// Included inside the extension namespace. Arrow owns the interchange format;
// DuckDB owns conversion, relation storage, planning and execution.
struct OrchidHostRelation {
    const char *name;
    ArrowArrayStream *stream;
    uint64_t host_token=0;
};

char *TypedHostQuery(void *,const char *,ArrowArrayStream *,OrchidHostRelation *,size_t,ArrowArrayStream *);
void UpdateHostFree(char *);
extern "C" {
void *orchid_host_scope_enter(uint64_t,void *,decltype(&TypedHostQuery),decltype(&UpdateHostFree),bool (*)(void *));
void orchid_host_scope_free(void *);
}
bool HostArrowInterrupted(void *context) {return static_cast<ClientContext *>(context)->IsInterrupted();}
struct HostPullScope {
    void *scope=nullptr;
    HostPullScope(uint64_t token,ClientContext &context) {
        if(token) {scope=orchid_host_scope_enter(token,&context,TypedHostQuery,UpdateHostFree,HostArrowInterrupted);}
    }
    ~HostPullScope() {if(scope) {orchid_host_scope_free(scope);}}
};

// Own the Arrow C stream. Binding reads only its schema; no get_next call is
// permitted until DuckDB executes the scan. Moving ownership makes early LIMIT,
// cancellation, and exception paths release the remote producer exactly once.
struct HostArrowRelation {
    ArrowArrayStreamWrapper stream;
    ArrowTableSchema table;
    mutex lock;
    bool claimed = false;
    uint64_t host_token=0;
};
struct HostRelationData : FunctionData {
    shared_ptr<HostArrowRelation> source;
    unique_ptr<FunctionData> Copy() const override {
        auto copy = make_uniq<HostRelationData>();
        copy->source = source;
        return std::move(copy);
    }
    bool Equals(const FunctionData &other) const override {
        return source == other.Cast<HostRelationData>().source;
    }
};
// Preserve authoritative language diagnostics across nested Arrow/Rust error
// envelopes. Never infer a diagnosis from human-readable error text.
unordered_map<string,string> HostErrorMetadata(string message) {
    unordered_map<string,string> metadata;
    for(idx_t depth=0;depth<16;depth++) {
        auto start=message.find('{');
        if(start==string::npos) {break;}
        auto detail=Json::parse(message.substr(start),nullptr,false);
        if(detail.is_discarded() || !detail.is_object()) {break;}
        for(auto key:{"orchid_classification","orchid_diagnosis"}) {
            if(detail.contains(key) && !detail.at(key).is_null()) {
                metadata[key]=detail.at(key).is_string() ? detail.at(key).get<string>() : detail.at(key).dump();
            }
        }
        if(!metadata.empty()) {break;}
        if(detail.contains("exception_message") && detail.at("exception_message").is_string()) {message=detail.at("exception_message").get<string>();}
        else if(detail.contains("error") && detail.at("error").is_string()) {message=detail.at("error").get<string>();}
        else {break;}
    }
    return metadata;
}
void CheckHostArrow(ArrowArrayStream &stream, int code) {
    if (!code) { return; }
    auto error = stream.get_last_error ? stream.get_last_error(&stream) : nullptr;
    auto message=string(error ? error : "unknown error");
    throw InvalidInputException(HostErrorMetadata(message),"Host Arrow stream: "+message);
}
shared_ptr<HostRelationData> ImportHostRelation(ClientContext &context, ArrowArrayStream &stream) {
    if (!stream.release || !stream.get_schema || !stream.get_next) {
        throw InvalidInputException("Released or invalid host Arrow stream");
    }
    auto relation = make_shared_ptr<HostRelationData>();
    relation->source = make_shared_ptr<HostArrowRelation>();
    auto &owned = relation->source->stream.arrow_array_stream;
    owned = stream;
    stream.release = nullptr;
    ArrowSchemaWrapper schema;
    CheckHostArrow(owned, owned.get_schema(&owned, &schema.arrow_schema));
    ArrowTableFunction::PopulateArrowTableSchema(context, relation->source->table, schema.arrow_schema);
    return relation;
}
unique_ptr<ResultArrowArrayStreamWrapper> HostResultArrowStream(unique_ptr<QueryResult> result) {
    auto stream=make_uniq<ResultArrowArrayStreamWrapper>(std::move(result),STANDARD_VECTOR_SIZE);
    // DuckDB's default wrapper coalesces small chunks up to batch_size, which
    // can wait for another remote page before returning already available rows.
    stream->stream.get_next=[](ArrowArrayStream *input,ArrowArray *output)->int {
        output->release=nullptr;
        if(!input->release) {return -1;}
        auto &owner=*static_cast<ResultArrowArrayStreamWrapper *>(input->private_data);
        try {
            for(;;) {
                auto chunk=owner.result->Fetch();
                if(!chunk) {return 0;}
                if(!chunk->size()) {continue;}
                ArrowConverter::ToArrowArray(*chunk,output,owner.result->client_properties,owner.extension_types);
                return 0;
            }
        } catch(std::exception &error) {owner.last_error=ErrorData(error);owner.last_error.ConvertErrorToJSON();return -1;}
    };
    return stream;
}
shared_ptr<HostRelationData> ImportHostResult(ClientContext &context, unique_ptr<QueryResult> result) {
    auto owner = HostResultArrowStream(std::move(result));
    ArrowArrayStreamWrapper stream;
    stream.arrow_array_stream = owner->stream;
    owner.release(); // The moved Arrow release callback owns this wrapper.
    return ImportHostRelation(context, stream.arrow_array_stream);
}

// A cursor retains at most its current Arrow batch. DuckDB owns any relational
// materialization (hash builds, sorting, explicitly replayable CTEs).
struct HostRelationCursor {
    shared_ptr<HostArrowRelation> source;
    unique_ptr<ArrowScanLocalState> scan;
    bool done = false;
    explicit HostRelationCursor(shared_ptr<HostArrowRelation> source) : source(std::move(source)) {
        lock_guard<mutex> guard(this->source->lock);
        if (this->source->claimed) {
            throw InvalidInputException("Host Arrow input is single-pass; a repeated scan requires an explicitly materialized CTE");
        }
        this->source->claimed = true;
    }
    bool Next(ClientContext &context, DataChunk &output) {
        HostPullScope host_scope(source->host_token,context);
        if (done) { return false; }
        if (context.IsInterrupted()) { throw InterruptException(); }
        auto &stream = source->stream.arrow_array_stream;
        while (!scan || scan->chunk_offset >= NumericCast<idx_t>(scan->chunk->arrow_array.length)) {
            scan.reset();
            auto array = make_uniq<ArrowArrayWrapper>();
            CheckHostArrow(stream, stream.get_next(&stream, &array->arrow_array));
            if (context.IsInterrupted()) { throw InterruptException(); }
            if (!array->arrow_array.release) { done = true; return false; }
            if (array->arrow_array.length < 0) { throw InvalidInputException("Negative Arrow batch length"); }
            if (array->arrow_array.length == 0) { continue; }
            scan = make_uniq<ArrowScanLocalState>(std::move(array), context);
        }
        output.Reset();
        output.SetCardinality(MinValue<idx_t>(STANDARD_VECTOR_SIZE,
            NumericCast<idx_t>(scan->chunk->arrow_array.length) - scan->chunk_offset));
        ArrowTableFunction::ArrowToDuckDB(*scan, source->table.GetColumns(), output);
        scan->chunk_offset += output.size();
        return true;
    }
};

// Registry lookup is binding-only. Bound plans own their streams, without an
// exposed pointer in SQL or a dependence on the registry during execution.
thread_local unordered_map<uint64_t, shared_ptr<HostRelationData>> host_relations;
thread_local uint64_t next_host_relation = 1;
struct HostRelationScope {
    vector<uint64_t> ids;
    uint64_t Add(shared_ptr<HostRelationData> relation) {
        auto id = next_host_relation++;
        host_relations.emplace(id, std::move(relation));
        ids.push_back(id);
        return id;
    }
    ~HostRelationScope() { for (auto id : ids) { host_relations.erase(id); } }
};

unique_ptr<FunctionData> HostRelationBind(ClientContext &, TableFunctionBindInput &input,
                                        vector<LogicalType> &types, vector<string> &names) {
    auto found = host_relations.find(input.inputs[0].GetValue<uint64_t>());
    if (found == host_relations.end()) { throw BinderException("Host relation is outside its execution scope"); }
    types = found->second->source->table.GetTypes();
    names = found->second->source->table.GetNames();
    return found->second->Copy();
}
struct HostRelationState : GlobalTableFunctionState {
    unique_ptr<HostRelationCursor> cursor;
    idx_t MaxThreads() const override { return 1; }
};
unique_ptr<GlobalTableFunctionState> HostRelationInit(ClientContext &, TableFunctionInitInput &input) {
    auto state = make_uniq<HostRelationState>();
    state->cursor = make_uniq<HostRelationCursor>(input.bind_data->Cast<HostRelationData>().source);
    return std::move(state);
}
void HostRelationScan(ClientContext &context, TableFunctionInput &input, DataChunk &output) {
    input.global_state->Cast<HostRelationState>().cursor->Next(context, output);
}

CommonTableExpressionMap &HostCTEs(SQLStatement &statement) {
    switch (statement.type) {
    case StatementType::SELECT_STATEMENT: return statement.Cast<SelectStatement>().node->cte_map;
    case StatementType::INSERT_STATEMENT: return statement.Cast<InsertStatement>().cte_map;
    case StatementType::UPDATE_STATEMENT: return statement.Cast<UpdateStatement>().cte_map;
    case StatementType::DELETE_STATEMENT: return statement.Cast<DeleteStatement>().cte_map;
    default: throw BinderException("Host input relations require SELECT, INSERT, UPDATE or DELETE");
    }
}

// A repeated reference or a reference in a recursive term requires replay.
// Its single Arrow scan feeds DuckDB's spill-capable CTE materializer;
// other inputs remain streaming scans.
void ConfigureHostReplay(SQLStatement &statement) {
    case_insensitive_map_t<idx_t> uses;
    std::function<void(unique_ptr<ParsedExpression> &)> expression;
    std::function<void(TableRef &)> reference = [&](TableRef &ref) {
        if (ref.type == TableReferenceType::BASE_TABLE) {
            auto &base = ref.Cast<BaseTableRef>();
            if (base.catalog_name.empty() && base.schema_name.empty()) { uses[base.table_name]++; }
        }
    };
    expression = [&](unique_ptr<ParsedExpression> &expr) {
        if (expr->GetExpressionClass() == ExpressionClass::SUBQUERY) {
            auto &subquery = expr->Cast<SubqueryExpression>();
            ParsedExpressionIterator::EnumerateQueryNodeChildren(*subquery.subquery->node, expression, reference);
        }
        ParsedExpressionIterator::EnumerateChildren(*expr, expression);
    };
    auto query = [&](SelectStatement &select) {
        ParsedExpressionIterator::EnumerateQueryNodeChildren(*select.node, expression, reference);
    };
    switch (statement.type) {
    case StatementType::SELECT_STATEMENT: query(statement.Cast<SelectStatement>()); break;
    case StatementType::INSERT_STATEMENT: {
        auto &insert = statement.Cast<InsertStatement>();
        if (insert.select_statement) { query(*insert.select_statement); }
        break;
    }
    case StatementType::UPDATE_STATEMENT: {
        auto &update = statement.Cast<UpdateStatement>();
        if (update.from_table) { ParsedExpressionIterator::EnumerateTableRefChildren(*update.from_table, expression, reference); }
        if (update.set_info->condition) { expression(update.set_info->condition); }
        for (auto &value : update.set_info->expressions) { expression(value); }
        break;
    }
    case StatementType::DELETE_STATEMENT: {
        auto &remove = statement.Cast<DeleteStatement>();
        for (auto &ref : remove.using_clauses) { ParsedExpressionIterator::EnumerateTableRefChildren(*ref, expression, reference); }
        if (remove.condition) { expression(remove.condition); }
        break;
    }
    default: return;
    }
    auto &ctes = HostCTEs(statement).map;
    if (statement.type != StatementType::SELECT_STATEMENT) {
        for (auto &entry : ctes) { query(*entry.second->query); }
    }
    unordered_set<QueryNode *> visited;
    std::function<void(QueryNode &)> recursive_inputs;
    recursive_inputs = [&](QueryNode &node) {
        if (!visited.insert(&node).second) { return; }
        if (node.type == QueryNodeType::RECURSIVE_CTE_NODE) {
            auto &recursive = node.Cast<RecursiveCTENode>();
            // One textual reference can be scanned once per iteration.
            ParsedExpressionIterator::EnumerateQueryNodeChildren(*recursive.right, expression, reference);
            recursive_inputs(*recursive.left);
            recursive_inputs(*recursive.right);
        } else if (node.type == QueryNodeType::SET_OPERATION_NODE) {
            for (auto &child : node.Cast<SetOperationNode>().children) { recursive_inputs(*child); }
        }
        for (auto &entry : node.cte_map.map) { recursive_inputs(*entry.second->query->node); }
        std::function<void(unique_ptr<ParsedExpression> &)> nested_expression;
        nested_expression = [&](unique_ptr<ParsedExpression> &expr) {
            if (expr->GetExpressionClass() == ExpressionClass::SUBQUERY) {
                recursive_inputs(*expr->Cast<SubqueryExpression>().subquery->node);
            }
            ParsedExpressionIterator::EnumerateChildren(*expr, nested_expression);
        };
        ParsedExpressionIterator::EnumerateQueryNodeChildren(node, nested_expression, [&](TableRef &ref) {
            if (ref.type == TableReferenceType::SUBQUERY) {
                recursive_inputs(*ref.Cast<SubqueryRef>().subquery->node);
            }
        });
    };
    if (statement.type == StatementType::SELECT_STATEMENT) {
        recursive_inputs(*statement.Cast<SelectStatement>().node);
    }
    for (auto &entry : ctes) { recursive_inputs(*entry.second->query->node); }
    // Inner CTEs can hide the source dependency from the recursive term.
    // Materialize the referenced inner relation as well as top-level inputs.
    for (auto node : visited) {
        for (auto &entry : node->cte_map.map) {
            if (uses[entry.first] > 1) { entry.second->materialized = CTEMaterialize::CTE_MATERIALIZE_ALWAYS; }
        }
    }
    for (auto &entry : ctes) {
        if (uses[entry.first] > 1) { entry.second->materialized = CTEMaterialize::CTE_MATERIALIZE_ALWAYS; }
    }
}

// Input streams are moved (their release callbacks are cleared). The returned
// stream is consumed/released by Rust before this ClientContext scope ends.
// A null return means success; errors use the existing UpdateHostFree allocator.
char *TypedHostQueryImpl(void *state, const char *sql, ArrowArrayStream *parameters,
                     OrchidHostRelation *relations, size_t relation_count, ArrowArrayStream *output, bool describe) {
    output->release = nullptr;
    try {
        auto &context = *static_cast<ClientContext *>(state);
        Parser parser(context.GetParserOptions());
        parser.ParseQuery(sql);
        if (parser.statements.size() != 1) { throw BinderException("Host stages require exactly one statement"); }
        auto statement = std::move(parser.statements[0]);
        vector<Value> params;
        if (parameters) {
            auto values = ImportHostRelation(context, *parameters);
            HostRelationCursor cursor(values->source);
            DataChunk chunk;
            chunk.Initialize(Allocator::Get(context), values->source->table.GetTypes());
            if (!cursor.Next(context, chunk) || chunk.size() != 1) { throw InvalidInputException("Host parameters require exactly one row"); }
            for (idx_t col = 0; col < chunk.ColumnCount(); col++) { params.push_back(chunk.GetValue(col, 0)); }
            if (cursor.Next(context, chunk)) { throw InvalidInputException("Host parameters require exactly one row"); }
        }
        HostRelationScope scope;
        for (size_t i = 0; i < relation_count; i++) {
            auto &ctes = HostCTEs(*statement).map;
            string name(relations[i].name);
            if (ctes.find(name) != ctes.end()) { throw BinderException("Duplicate host input relation: %s", name); }
            auto relation=ImportHostRelation(context,*relations[i].stream);
            relation->source->host_token=relations[i].host_token;
            auto id=scope.Add(std::move(relation));
            auto cte = make_uniq<CommonTableExpressionInfo>();
            cte->query = Select(context, "SELECT * FROM __orchid_host_relation(" + std::to_string(id) + "::UBIGINT)");
            ctes.insert(name, std::move(cte));
        }
        ConfigureHostReplay(*statement);
        unique_ptr<QueryResult> result;
        if (describe) {
            if(statement->type!=StatementType::SELECT_STATEMENT) {throw BinderException("Federation output must be a SELECT");}
            ValidateProtectedFunctions(context,statement->Cast<SelectStatement>());
            Planner planner(context);
            for(idx_t i=0;i<params.size();i++) {planner.parameter_data[std::to_string(i+1)]=BoundParameterData(params[i]);}
            planner.CreatePlan(std::move(statement));
            // Describe performs no optimizer evaluation, executor initialization,
            // remote pulls, or modifying SELECT calls such as nextval().
            result=make_uniq<MaterializedQueryResult>(StatementType::SELECT_STATEMENT,planner.properties,planner.names,
                make_uniq<ColumnDataCollection>(context,planner.types),context.GetClientProperties());
        } else {result=HostExecuteStream(context,std::move(statement),params);}
        auto stream = HostResultArrowStream(std::move(result));
        *output = stream->stream;
        stream.release(); // The Arrow stream release callback deletes its owner.
        return nullptr;
    } catch (std::exception &error) {
        auto length = std::strlen(error.what()) + 1;
        auto out = static_cast<char *>(std::malloc(length));
        if (!out) { std::terminate(); }
        std::memcpy(out, error.what(), length);
        return out;
    } catch (...) {
        const char *error = "Host Arrow execution failed";
        auto out = static_cast<char *>(std::malloc(std::strlen(error) + 1));
        if (!out) { std::terminate(); }
        std::strcpy(out, error);
        return out;
    }
}

char *TypedHostQuery(void *state,const char *sql,ArrowArrayStream *parameters,OrchidHostRelation *relations,size_t count,ArrowArrayStream *output) {
    return TypedHostQueryImpl(state,sql,parameters,relations,count,output,false);
}
char *TypedHostSchema(void *state,const char *sql,ArrowArrayStream *parameters,OrchidHostRelation *relations,size_t count,ArrowArrayStream *output) {
    return TypedHostQueryImpl(state,sql,parameters,relations,count,output,true);
}

#ifdef ORCHID_TEST_ARROW_STREAM
// Compiled only into diagnostic builds. Exercises the real host bridge with a
// counted producer; production extensions expose no test-only SQL functions.
struct HostStreamTestCounts { idx_t pulls = 0; idx_t releases = 0; idx_t fail_after = 0; };
struct HostStreamTestProducer {
    ArrowArrayStream upstream;
    shared_ptr<HostStreamTestCounts> counts;
    static int Schema(ArrowArrayStream *stream, ArrowSchema *out) {
        auto &self = *static_cast<HostStreamTestProducer *>(stream->private_data);
        return self.upstream.get_schema(&self.upstream, out);
    }
    static int Next(ArrowArrayStream *stream, ArrowArray *out) {
        auto &self = *static_cast<HostStreamTestProducer *>(stream->private_data);
        if (++self.counts->pulls == self.counts->fail_after) { return 1; }
        return self.upstream.get_next(&self.upstream, out);
    }
    static const char *Error(ArrowArrayStream *) { return "injected Arrow source failure"; }
    static void Release(ArrowArrayStream *stream) {
        if (!stream->release) { return; }
        auto self = static_cast<HostStreamTestProducer *>(stream->private_data);
        stream->release = nullptr;
        self->counts->releases++;
        self->upstream.release(&self->upstream);
        delete self;
    }
};
void HostStreamSelfTest(DataChunk &args, ExpressionState &state, Vector &output) {
    auto &context = state.GetContext();
    Json report = Json::object();
    vector<string> scenarios = args.ColumnCount() ? vector<string>{args.GetValue(0,0).GetValue<string>()}
        : vector<string>{"limit", "projection", "replay", "recursive_replay", "stream_output"};
    for (const auto &scenario : scenarios) {
        auto counts = make_shared_ptr<HostStreamTestCounts>();
        counts->fail_after = scenario == "error" ? 2 : 0;
        bool failed = false;
        idx_t rows = 0;
        {
            auto result = HostExecute(context, Select(context, "SELECT i::BIGINT AS x, (i*2)::BIGINT AS y FROM range(" + string(scenario == "stream_output" ? "100000" : "10000") + ") t(i)"), {});
            auto owner = make_uniq<ResultArrowArrayStreamWrapper>(std::move(result), STANDARD_VECTOR_SIZE);
            auto producer = new HostStreamTestProducer {owner->stream, counts};
            owner.release();
            ArrowArrayStream stream {};
            stream.get_schema = HostStreamTestProducer::Schema;
            stream.get_next = HostStreamTestProducer::Next;
            stream.get_last_error = HostStreamTestProducer::Error;
            stream.release = HostStreamTestProducer::Release;
            stream.private_data = producer;
            auto relation = ImportHostRelation(context, stream);
            if (counts->pulls != 0 || stream.release) { throw InternalException("Arrow bind consumed data or did not take ownership"); }
            HostRelationScope scope;
            auto id = scope.Add(std::move(relation));
            string source = "__orchid_host_relation(" + std::to_string(id) + "::UBIGINT)";
            string query = scenario == "replay" ? "WITH input AS (SELECT * FROM " + source + ") SELECT count(*) FROM input a JOIN input b ON a.x=b.x"
                : scenario == "recursive_replay" ? "WITH RECURSIVE input AS (SELECT * FROM " + source + "), walk(n) AS (SELECT 0 UNION ALL SELECT n+1 FROM walk JOIN input ON input.x=walk.n WHERE n<2) SELECT sum(n)::BIGINT FROM walk"
                : scenario == "projection" ? "SELECT y, x FROM " + source + " WHERE x >= 9990 ORDER BY x"
                : scenario == "limit" ? "SELECT y FROM " + source + " LIMIT 1"
                : "SELECT x FROM " + source;
            auto statement = Select(context, query);
            ConfigureHostReplay(*statement);
            try {
                auto result = HostExecuteStream(context, std::move(statement), {});
                if (counts->pulls) { throw InternalException("Preparing a streaming host result executed input"); }
                while (auto chunk = result->Fetch()) {
                    rows += chunk->size();
                    if (scenario == "replay" && chunk->GetValue(0,0).GetValue<int64_t>() != 10000) { throw InternalException("Replay count changed"); }
                    if (scenario == "recursive_replay" && chunk->GetValue(0,0).GetValue<int64_t>() != 3) { throw InternalException("Recursive replay changed"); }
                    if (scenario == "projection" && chunk->GetValue(0,0).GetValue<int64_t>() != 19980) { throw InternalException("Projection/filter changed"); }
                    if (scenario == "stream_output") { break; }
                }
            } catch (std::exception &error) {
                std::fprintf(stderr, "Host Arrow scenario %s: %s\n", scenario.c_str(), error.what());
                if (scenario != "error" || string(error.what()).find("injected Arrow source failure") == string::npos) { throw; }
                throw; // The outer query must report the producer failure.

            }
        }
        report[scenario] = Json{{"pulls",counts->pulls},{"releases",counts->releases},{"rows",rows},{"failed",failed}};
    }
    auto text = report.dump();
    output.SetVectorType(VectorType::CONSTANT_VECTOR);
    ConstantVector::GetData<string_t>(output)[0] = StringVector::AddString(output, text);
}
#endif
