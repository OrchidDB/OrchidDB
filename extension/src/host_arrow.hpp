// Included inside the extension namespace. Arrow owns the interchange format;
// DuckDB owns conversion, relation storage, planning and execution.
struct OrchidHostRelation {
    const char *name;
    ArrowArrayStream *stream;
};

struct HostRelationData : FunctionData {
    vector<string> names;
    shared_ptr<ColumnDataCollection> data;
    unique_ptr<FunctionData> Copy() const override {
        auto copy = make_uniq<HostRelationData>();
        copy->names = names;
        copy->data = data;
        return std::move(copy);
    }
    bool Equals(const FunctionData &other) const override {
        return data == other.Cast<HostRelationData>().data;
    }
};

shared_ptr<HostRelationData> ImportHostRelation(ClientContext &context, ArrowArrayStream &stream) {
    if (!stream.release || !stream.get_schema || !stream.get_next) {
        throw InvalidInputException("Released or invalid host Arrow stream");
    }
    auto check = [&](int code) {
        if (code) {
            auto error = stream.get_last_error ? stream.get_last_error(&stream) : nullptr;
            throw InvalidInputException("Host Arrow stream: %s", error ? error : "unknown error");
        }
    };
    ArrowSchemaWrapper schema;
    check(stream.get_schema(&stream, &schema.arrow_schema));
    ArrowTableSchema table;
    ArrowTableFunction::PopulateArrowTableSchema(context, table, schema.arrow_schema);
    auto relation = make_shared_ptr<HostRelationData>();
    relation->names = table.GetNames();
    relation->data = make_shared_ptr<ColumnDataCollection>(context, table.GetTypes());
    for (;;) {
        auto array = make_uniq<ArrowArrayWrapper>();
        check(stream.get_next(&stream, &array->arrow_array));
        if (!array->arrow_array.release) { break; }
        ArrowScanLocalState scan(std::move(array), context);
        auto length = NumericCast<idx_t>(scan.chunk->arrow_array.length);
        while (scan.chunk_offset < length) {
            DataChunk chunk;
            chunk.Initialize(Allocator::Get(context), table.GetTypes());
            chunk.SetCardinality(MinValue<idx_t>(STANDARD_VECTOR_SIZE, length - scan.chunk_offset));
            ArrowTableFunction::ArrowToDuckDB(scan, table.GetColumns(), chunk);
            relation->data->Append(chunk);
            scan.chunk_offset += chunk.size();
        }
    }
    return relation;
}

// A registry entry exists only while binding/executing one synchronous host
// request. Bound plans own their collection; no pointer is exposed in SQL.
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
    types = found->second->data->Types();
    names = found->second->names;
    return found->second->Copy();
}
struct HostRelationState : GlobalTableFunctionState { ColumnDataScanState scan; };
unique_ptr<GlobalTableFunctionState> HostRelationInit(ClientContext &, TableFunctionInitInput &input) {
    auto state = make_uniq<HostRelationState>();
    input.bind_data->Cast<HostRelationData>().data->InitializeScan(state->scan);
    return std::move(state);
}
void HostRelationScan(ClientContext &, TableFunctionInput &input, DataChunk &output) {
    input.bind_data->Cast<HostRelationData>().data->Scan(input.global_state->Cast<HostRelationState>().scan, output);
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

// All input streams are borrowed. The returned stream owns its materialized
// result and is consumed/released by Rust before this ClientContext scope ends.
// A null return means success; errors use the existing UpdateHostFree allocator.
char *TypedHostQuery(void *state, const char *sql, ArrowArrayStream *parameters,
                     OrchidHostRelation *relations, size_t relation_count, ArrowArrayStream *output) {
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
            if (values->data->Count() != 1) { throw InvalidInputException("Host parameters require exactly one row"); }
            DataChunk chunk;
            values->data->InitializeScanChunk(chunk);
            values->data->FetchChunk(0, chunk);
            for (idx_t col = 0; col < chunk.ColumnCount(); col++) { params.push_back(chunk.GetValue(col, 0)); }
        }
        HostRelationScope scope;
        for (size_t i = 0; i < relation_count; i++) {
            auto &ctes = HostCTEs(*statement).map;
            string name(relations[i].name);
            if (ctes.find(name) != ctes.end()) { throw BinderException("Duplicate host input relation: %s", name); }
            auto id = scope.Add(ImportHostRelation(context, *relations[i].stream));
            auto cte = make_uniq<CommonTableExpressionInfo>();
            cte->query = Select(context, "SELECT * FROM __orchid_host_relation(" + std::to_string(id) + "::UBIGINT)");
            ctes.insert(name, std::move(cte));
        }
        auto result = HostExecute(context, std::move(statement), params);
        auto stream = make_uniq<ResultArrowArrayStreamWrapper>(std::move(result), STANDARD_VECTOR_SIZE);
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
