// DuckDB schedules the existing compiled kernels as physical operators. This
// adapter handles batches and dependencies; language semantics remain in Rust.
using OrchidHostQuery = decltype(&TypedHostQuery);
using OrchidHostFree = void (*)(char *);
using OrchidCatalogQuery = decltype(&CatalogQuery);
using OrchidNested = char *(*)(void *, const void *, void *, ArrowArrayStream *);
extern "C" {
char *orchid_program_new(const char *, void *, OrchidCatalogQuery, OrchidHostFree);
char *orchid_program_manifest(const void *);
char *orchid_program_schema(const void *, uint64_t, ArrowSchema *);
void orchid_program_free(void *);
char *orchid_program_state_new(const void *, void *, OrchidHostQuery, OrchidHostFree, OrchidNested, OrchidCatalogQuery,
                              int64_t, int64_t, void **, void **);
char *orchid_program_scope_enter(void *, void *, OrchidHostQuery, OrchidHostFree, OrchidNested, OrchidCatalogQuery, void **);
void orchid_program_scope_free(void *);
void orchid_program_state_free(void *);
void *orchid_program_cancel_token(const void *);
void orchid_program_cancel(const void *);
void orchid_program_cancel_token_free(void *);
char *orchid_program_invoke(const void *, uint64_t, void *, ArrowArrayStream **, size_t, ArrowArrayStream *);
char *orchid_program_source_open(const void *, uint64_t, void **);
char *orchid_program_source_next(void *, void *, size_t, ArrowArrayStream *, bool *);
void orchid_program_source_free(void *);
char *orchid_program_finish(const void *, void *, ArrowArrayStream *, ArrowArrayStream *);
char *orchid_managed(const char *, void *, OrchidHostQuery, OrchidHostFree);
}
constexpr uint64_t ORCHID_RESULT = UINT64_MAX;

unique_ptr<FunctionData> ManagedBind(ClientContext &context, TableFunctionBindInput &input,
                                    vector<LogicalType> &types, vector<string> &names) {
    auto operation=input.table_function.name.substr(string("orchid_graph_").size());
    auto name=QualifiedName::Parse(input.inputs[0].GetValue<string>());
    if (name.catalog.empty()) {name.catalog=DatabaseManager::GetDefaultDatabase(context);}
    if (name.schema.empty()) {name.schema=DEFAULT_SCHEMA;}
    auto table=Quote(name.catalog)+"."+Quote(name.schema)+"."+Quote(name.name);
    Json request={{"op",operation},{"table",table}};
    if (operation=="create") {
        request["definition"]={{"version",1},{"name",Json::array({name.catalog,name.schema,name.name})},{"managed_table",table},{"vertices",Json::array()},{"edges",Json::array()}};
        request["view"]=Quote(name.catalog)+"."+Quote(name.schema)+"."+Quote("__orchid_graph_"+name.name);
    }
    if (operation=="import") {request["fixture"]=Json::parse(input.inputs.at(1).GetValue<string>());}
    if (input.binder) {
        auto &properties=input.binder->GetStatementProperties(); properties.always_require_rebind=true;
        if (operation!="snapshot") {
            auto &catalog=Catalog::GetCatalog(context,name.catalog);
            properties.RegisterDBModify(catalog,context,DatabaseModificationType::INSERT_DATA|DatabaseModificationType::DELETE_DATA|
                DatabaseModificationType::UPDATE_DATA|DatabaseModificationType::CREATE_CATALOG_ENTRY);
        }
    }
    types={LogicalType::VARCHAR}; names={"result"};
    return make_uniq<ProgramBindData>(request.dump());
}
void ManagedExecute(ClientContext &context, TableFunctionInput &input, DataChunk &output) {
    auto &state=input.global_state->Cast<ProgramState>();
    if (state.done) {return;}
    const auto &request=input.bind_data->Cast<ProgramBindData>().request;
    std::unique_ptr<char,decltype(&orchid_bridge_free)> response(orchid_managed(request.c_str(),&context,TypedHostQuery,UpdateHostFree),orchid_bridge_free);
    if (!response) {throw InternalException("Managed graph API returned no response");}
    auto envelope=Json::parse(response.get());
    if (!envelope.at("ok").get<bool>()) {throw InvalidInputException("Orchid managed graph: %s",envelope.at("error").get<string>());}
    auto result=envelope.at("result");
    auto operation=Json::parse(request);
    if (operation.at("op")=="create") {
        HostQuery(context,{{"sql","CREATE VIEW IF NOT EXISTS "+operation.at("view").get<string>()+
            " AS SELECT * FROM orchid_graph_definition("+Literal(operation.at("definition").dump())+")"}});
    }
    if (result.contains("native_snapshot")) {result=result.at("native_snapshot");}
    output.SetValue(0,0,Value(result.dump())); output.SetCardinality(1); state.done=true;
}

void ProgramCheck(char *error) {
    if (!error) { return; }
    std::unique_ptr<char, decltype(&orchid_bridge_free)> owned(error, orchid_bridge_free);
    auto detail=Json::parse(error,nullptr,false);
    if (!detail.is_discarded() && detail.contains("error")) {
        unordered_map<string,string> metadata;
        if (detail.contains("classification") && !detail.at("classification").is_null()) {metadata["orchid_classification"]=detail.at("classification").dump();}
        if (detail.contains("diagnosis") && !detail.at("diagnosis").is_null()) {metadata["orchid_diagnosis"]=detail.at("diagnosis").dump();}
        throw InvalidInputException(metadata,detail.at("error").get<string>());
    }
    throw InvalidInputException("Orchid program: %s", error);
}
Json ProgramJson(char *text) {
    std::unique_ptr<char, decltype(&orchid_bridge_free)> owned(text, orchid_bridge_free);
    if (!text) { throw InternalException("Orchid program returned no response"); }
    auto response = Json::parse(text);
    if (!response.at("ok").get<bool>()) {
        unordered_map<string,string> metadata;
        if (response.contains("classification") && !response.at("classification").is_null()) {metadata["orchid_classification"]=response.at("classification").dump();}
        throw BinderException(metadata,response.at("error").get<string>());
    }
    return response.at("result");
}
char *ExecuteNested(void *, const void *, void *, ArrowArrayStream *);

struct NativeProgram {
    void *handle;
    Json manifest;
    bool owned;
    NativeProgram(void *handle, Json manifest, bool owned) : handle(handle), manifest(std::move(manifest)), owned(owned) {}
    ~NativeProgram() { if (owned) { orchid_program_free(handle); } }
    const Json &Node(uint64_t id) const {
        for (const auto &node : manifest.at("nodes")) { if (node.at("id").get<uint64_t>() == id) { return node; } }
        throw InternalException("Missing compiled Orchid node");
    }
    ArrowTableSchema Schema(ClientContext &context, uint64_t node) const {
        ArrowSchemaWrapper schema;
        ProgramCheck(orchid_program_schema(handle, node, &schema.arrow_schema));
        ArrowTableSchema result;
        ArrowTableFunction::PopulateArrowTableSchema(context, result, schema.arrow_schema);
        return result;
    }
};
// The watcher owns only an independent atomic Rust token. It never borrows
// KernelState or invokes a language kernel from a second thread.
struct NativeInterrupt {
    std::unique_ptr<void, decltype(&orchid_program_cancel_token_free)> token;
    std::atomic<bool> stopped {false};
    std::mutex lifecycle;
    std::thread worker;
    NativeInterrupt(ClientContext &context, void *token_p)
        : token(token_p, orchid_program_cancel_token_free) {
        if (!token) { throw InternalException("Missing Orchid cancellation token"); }
        worker=std::thread([this, &context] {
            while (!stopped.load(std::memory_order_acquire)) {
                if (context.interrupted.load(std::memory_order_relaxed)) {
                    orchid_program_cancel(token.get());
                    return;
                }
                std::this_thread::sleep_for(std::chrono::milliseconds(2));
            }
        });
    }
    void Stop() {
        std::lock_guard<std::mutex> guard(lifecycle);
        stopped.store(true,std::memory_order_release);
        if (worker.joinable()) {worker.join();}
    }
    ~NativeInterrupt() {Stop();}
};
struct NativeInterrupts : ClientContextState {
    std::mutex lock;
    vector<shared_ptr<NativeInterrupt>> watchers;
    void Add(shared_ptr<NativeInterrupt> watcher) {
        std::lock_guard<std::mutex> guard(lock);
        watchers.push_back(std::move(watcher));
    }
    void QueryEnd() override {
        vector<shared_ptr<NativeInterrupt>> finished;
        {
            std::lock_guard<std::mutex> guard(lock);
            finished.swap(watchers);
        }
        for (auto &watcher : finished) {watcher->Stop();}
    }
    ~NativeInterrupts() override {QueryEnd();}
};
struct NativeExecution;
struct NativeCachedPlan {
    shared_ptr<NativeExecution> execution;
    unique_ptr<PreparedStatementData> prepared;
};
// Query ownership avoids a cycle between cached physical operators and their
// execution state. Reentrant calls take a cache slot until execution completes.
struct NativePlans : ClientContextState {
    vector<shared_ptr<NativeCachedPlan>> plans;
    void QueryEnd() override {plans.clear();}
};
struct NativeExecution {
    shared_ptr<NativeProgram> program;
    void *execution = nullptr;
    void *kernel_state = nullptr;
    bool owned = true;
    shared_ptr<NativeInterrupt> interrupt;
    unordered_map<const void *, weak_ptr<NativeCachedPlan>> nested;
    explicit NativeExecution(shared_ptr<NativeProgram> program) : program(std::move(program)) {}
    ~NativeExecution() {
        if (interrupt) {interrupt->Stop();}
        if (owned && execution) {orchid_program_state_free(execution);}
    }
    void Ensure(ClientContext &context) {
        if (kernel_state) { return; }
        auto clock = context.registered_state->GetOrCreate<NativeClock>("orchid_native_clock");
        ProgramCheck(orchid_program_state_new(program->handle, &context, TypedHostQuery, UpdateHostFree, ExecuteNested, CatalogQuery,
            clock->statement.value, MetaTransaction::Get(context).start_timestamp.value, &execution, &kernel_state));
        interrupt=make_shared_ptr<NativeInterrupt>(context,orchid_program_cancel_token(execution));
        context.registered_state->GetOrCreate<NativeInterrupts>("orchid_native_interrupts")->Add(interrupt);
    }
};
thread_local NativeExecution *active_native_execution=nullptr;
// A scope lasts for exactly one synchronous FFI invocation, including its
// nested calls. No ClientContext pointer is stored in a compiled program.
struct NativeScope {
    void *scope = nullptr;
    NativeExecution *previous=nullptr;
    explicit NativeScope(ClientContext &context, NativeExecution &execution) {
        execution.Ensure(context);
        if (execution.execution) {
            ProgramCheck(orchid_program_scope_enter(execution.execution, &context, TypedHostQuery, UpdateHostFree, ExecuteNested, CatalogQuery, &scope));
        }
        previous=active_native_execution; active_native_execution=&execution;
    }
    ~NativeScope() { active_native_execution=previous; if (scope) { orchid_program_scope_free(scope); } }
};

unique_ptr<ArrowArrayStreamWrapper> ExportKernelInput(ClientContext &context,
        unique_ptr<ColumnDataCollection> collection, const vector<string> &names) {
    auto result = make_uniq<MaterializedQueryResult>(StatementType::SELECT_STATEMENT, StatementProperties(), names,
        std::move(collection), context.GetClientProperties());
    auto owner = make_uniq<ResultArrowArrayStreamWrapper>(std::move(result), STANDARD_VECTOR_SIZE);
    auto output = make_uniq<ArrowArrayStreamWrapper>();
    output->arrow_array_stream = owner->stream;
    owner.release();
    return output;
}
struct KernelOutput {
    shared_ptr<HostRelationData> relation;
    ColumnDataScanState scan;
    void Set(ClientContext &context, ArrowArrayStream &stream) {
        relation = ImportHostRelation(context, stream);
        relation->data->InitializeScan(scan);
    }
    bool Next(DataChunk &chunk) {
        if (!relation) { return false; }
        if (relation->data->Scan(scan, chunk)) { return true; }
        relation.reset();
        return false;
    }
};
struct KernelGlobalOperator : GlobalOperatorState {
    idx_t MaxThreads(idx_t) override { return 1; }
};
struct KernelOperatorState : OperatorState { KernelOutput output; };
struct KernelSourceState : GlobalSourceState {
    KernelOutput output;
    void *cursor = nullptr;
    bool done = false;
    ~KernelSourceState() override { if (cursor) { orchid_program_source_free(cursor); } }
};
struct KernelSinkState : GlobalSinkState {
    vector<unique_ptr<ColumnDataCollection>> inputs;
    KernelOutput output;
    idx_t MaxThreads(idx_t) override { return 1; }
};

class PhysicalOrchidKernel : public PhysicalOperator {
public:
    shared_ptr<NativeExecution> execution;
    uint64_t node;
    string name;
    bool streaming, source, finish;
    vector<vector<LogicalType>> input_types;
    vector<vector<string>> input_names;
    PhysicalOrchidKernel(PhysicalPlan &plan, vector<LogicalType> types, shared_ptr<NativeExecution> execution,
                        uint64_t node, string name, bool streaming, bool source,
                        vector<vector<LogicalType>> input_types, vector<vector<string>> input_names)
        : PhysicalOperator(plan, PhysicalOperatorType::EXTENSION, std::move(types), 1000), execution(std::move(execution)),
          node(node), name(std::move(name)), streaming(streaming), source(source), finish(node == ORCHID_RESULT),
          input_types(std::move(input_types)), input_names(std::move(input_names)) {}
    string GetName() const override { return "ORCHID_" + name; }
    bool IsSink() const override { return !streaming && !input_types.empty(); }
    bool IsSource() const override { return !streaming || input_types.empty(); }
    bool SinkOrderDependent() const override { return true; }
    unique_ptr<GlobalOperatorState> GetGlobalOperatorState(ClientContext &) const override { return make_uniq<KernelGlobalOperator>(); }
    unique_ptr<OperatorState> GetOperatorState(ExecutionContext &) const override { return make_uniq<KernelOperatorState>(); }
    unique_ptr<GlobalSourceState> GetGlobalSourceState(ClientContext &) const override { return make_uniq<KernelSourceState>(); }
    unique_ptr<GlobalSinkState> GetGlobalSinkState(ClientContext &context) const override {
        auto state = make_uniq<KernelSinkState>();
        for (const auto &types : input_types) { state->inputs.push_back(make_uniq<ColumnDataCollection>(context, types)); }
        return std::move(state);
    }
    void Invoke(ClientContext &context, vector<unique_ptr<ColumnDataCollection>> inputs, KernelOutput &output) const {
        NativeScope scope(context, *execution);
        vector<unique_ptr<ArrowArrayStreamWrapper>> owners;
        vector<ArrowArrayStream *> streams;
        for (idx_t i = 0; i < inputs.size(); i++) {
            owners.push_back(ExportKernelInput(context, std::move(inputs[i]), input_names[i]));
            streams.push_back(&owners.back()->arrow_array_stream);
        }
        ArrowArrayStreamWrapper result;
        if (finish) {
            ProgramCheck(orchid_program_finish(execution->program->handle, execution->execution, streams.at(0), &result.arrow_array_stream));
        } else {
            ProgramCheck(orchid_program_invoke(execution->program->handle, node, execution->kernel_state,
                streams.data(), streams.size(), &result.arrow_array_stream));
        }
        output.Set(context, result.arrow_array_stream);
    }
    OperatorResultType Execute(ExecutionContext &context, DataChunk &input, DataChunk &output,
                               GlobalOperatorState &, OperatorState &state_p) const override {
        auto &state = state_p.Cast<KernelOperatorState>();
        if (state.output.relation) {
            if (state.output.Next(output)) { return OperatorResultType::HAVE_MORE_OUTPUT; }
            return OperatorResultType::NEED_MORE_INPUT;
        }
        vector<unique_ptr<ColumnDataCollection>> inputs;
        auto collection = make_uniq<ColumnDataCollection>(context.client, input_types.at(0));
        collection->Append(input);
        inputs.push_back(std::move(collection));
        Invoke(context.client, std::move(inputs), state.output);
        return state.output.Next(output) ? OperatorResultType::HAVE_MORE_OUTPUT : OperatorResultType::NEED_MORE_INPUT;
    }
    SinkResultType Sink(ExecutionContext &, DataChunk &chunk, OperatorSinkInput &input) const override {
        auto &state = input.global_state.Cast<KernelSinkState>();
        if (state.inputs.size() == 1) { state.inputs[0]->Append(chunk); }
        else {
            // Multi-input kernels receive a tagged, ordered DuckDB UNION. Tags
            // route batches only; the original kernel still performs the join.
            auto index = chunk.GetValue(chunk.ColumnCount() - 1, 0).GetValue<uint64_t>();
            DataChunk projected;
            vector<column_t> columns;
            for (idx_t i = 0; i + 1 < chunk.ColumnCount(); i++) { columns.push_back(i); }
            projected.InitializeEmpty(input_types.at(index));
            projected.ReferenceColumns(chunk, columns);
            state.inputs.at(index)->Append(projected);
        }
        return SinkResultType::NEED_MORE_INPUT;
    }
    SinkFinalizeType Finalize(Pipeline &, Event &, ClientContext &context, OperatorSinkFinalizeInput &input) const override {
        auto &state = input.global_state.Cast<KernelSinkState>();
        Invoke(context, std::move(state.inputs), state.output);
        return SinkFinalizeType::READY;
    }
    SourceResultType GetDataInternal(ExecutionContext &context, DataChunk &output, OperatorSourceInput &input) const override {
        if (IsSink()) {
            return sink_state->Cast<KernelSinkState>().output.Next(output) ? SourceResultType::HAVE_MORE_OUTPUT : SourceResultType::FINISHED;
        }
        auto &state = input.global_state.Cast<KernelSourceState>();
        if (state.output.Next(output)) { return SourceResultType::HAVE_MORE_OUTPUT; }
        while (!state.done) {
            NativeScope scope(context.client, *execution);
            if (source) {
                if (!state.cursor) { ProgramCheck(orchid_program_source_open(execution->program->handle, node, &state.cursor)); }
                ArrowArrayStreamWrapper result;
                ProgramCheck(orchid_program_source_next(state.cursor, execution->kernel_state, STANDARD_VECTOR_SIZE,
                    &result.arrow_array_stream, &state.done));
                if (result.arrow_array_stream.release) { state.output.Set(context.client, result.arrow_array_stream); }
            } else {
                state.done = true;
                // NativeScope is already active; invoke the no-input kernel
                // directly, preserving the single-row/empty-frontier contract.
                ArrowArrayStreamWrapper result;
                ProgramCheck(orchid_program_invoke(execution->program->handle, node, execution->kernel_state,
                    nullptr, 0, &result.arrow_array_stream));
                state.output.Set(context.client, result.arrow_array_stream);
            }
            if (state.output.Next(output)) { return SourceResultType::HAVE_MORE_OUTPUT; }
        }
        return SourceResultType::FINISHED;
    }
};

unique_ptr<QueryResult> ExecuteProgramInput(ClientContext &, shared_ptr<NativeExecution>, uint64_t);
struct KernelInputsState : GlobalSourceState {
    idx_t input=0;
    KernelOutput output;
    idx_t MaxThreads() override {return 1;}
};
// Stateful branches share a single kernel context. DuckDB's UNION orders rows,
// but may run the branches' blocking pipelines concurrently. Execute each
// already-bound input with the existing host executor before starting the next.
// This preserves branch effects and avoids overlapping mutable Rust borrows.
class PhysicalKernelInputs : public PhysicalOperator {
public:
    shared_ptr<NativeExecution> execution;
    vector<uint64_t> inputs;
    PhysicalKernelInputs(PhysicalPlan &plan, vector<LogicalType> types,
                         shared_ptr<NativeExecution> execution, vector<uint64_t> inputs)
        : PhysicalOperator(plan,PhysicalOperatorType::EXTENSION,std::move(types),1000),
          execution(std::move(execution)),inputs(std::move(inputs)) {}
    string GetName() const override {return "ORCHID_ORDERED_INPUTS";}
    bool IsSource() const override {return true;}
    unique_ptr<GlobalSourceState> GetGlobalSourceState(ClientContext &) const override {return make_uniq<KernelInputsState>();}
    SourceResultType GetDataInternal(ExecutionContext &context, DataChunk &output, OperatorSourceInput &input) const override {
        auto &state=input.global_state.Cast<KernelInputsState>();
        while (state.input<inputs.size()) {
            if (!state.output.relation) {
                auto result=ExecuteProgramInput(context.client,execution,inputs[state.input]);
                ResultArrowArrayStreamWrapper stream(std::move(result),STANDARD_VECTOR_SIZE);
                state.output.Set(context.client,stream.stream);
            }
            DataChunk chunk;
            auto input_types=types; input_types.pop_back();
            chunk.Initialize(Allocator::Get(context.client),input_types);
            if (state.output.Next(chunk)) {
                for (idx_t i=0;i<chunk.ColumnCount();i++) {output.data[i].Reference(chunk.data[i]);}
                output.data.back().Reference(Value::UBIGINT(state.input));
                output.SetCardinality(chunk.size());
                return SourceResultType::HAVE_MORE_OUTPUT;
            }
            state.input++;
        }
        return SourceResultType::FINISHED;
    }
};

class LogicalOrchidKernel : public LogicalExtensionOperator {
public:
    shared_ptr<NativeExecution> execution;
    uint64_t node;
    idx_t table_index;
    vector<LogicalType> result_types;
    vector<vector<string>> input_names;
    LogicalOrchidKernel(shared_ptr<NativeExecution> execution, uint64_t node, idx_t table_index, vector<LogicalType> types)
        : execution(std::move(execution)), node(node), table_index(table_index), result_types(std::move(types)) {}
    string GetName() const override { return node==ORCHID_RESULT ? "ORCHID_RESULT" : "ORCHID_"+execution->program->Node(node).at("name").get<string>(); }
    string GetExtensionName() const override { return "orchid"; }
    bool SupportSerialization() const override { return false; }
    vector<ColumnBinding> GetColumnBindings() override { return GenerateColumnBindings(table_index, result_types.size()); }
    vector<idx_t> GetTableIndex() const override { return {table_index}; }
    void ResolveTypes() override { types = result_types; }
    void AddInput(BoundStatement bound) {
        auto bindings=bound.plan->GetColumnBindings();
        for (idx_t i=0;i<bound.types.size();i++) {
            expressions.push_back(make_uniq<BoundColumnRefExpression>(bound.types[i],bindings[i]));
        }
        input_names.push_back(bound.names);
        children.push_back(std::move(bound.plan));
    }
    void ResolveColumnBindings(ColumnBindingResolver &resolver, vector<ColumnBinding> &bindings) override {
        idx_t expression=0;
        for (idx_t child=0;child<children.size();child++) {
            resolver.VisitOperator(*children[child]);
            for (idx_t i=0;i<input_names[child].size();i++) {resolver.VisitExpression(&expressions[expression++]);}
        }
        bindings=GetColumnBindings();
    }
    PhysicalOperator &CreatePlan(ClientContext &, PhysicalPlanGenerator &planner) override {
        vector<vector<LogicalType>> child_types;
        vector<reference<PhysicalOperator>> inputs;
        for (auto &child : children) { auto &physical=planner.CreatePlan(*child); child_types.push_back(physical.types); inputs.push_back(physical); }
        bool finish=node==ORCHID_RESULT;
        const auto &description=finish ? Json::object() : execution->program->Node(node);
        auto &physical=planner.Make<PhysicalOrchidKernel>(result_types,execution,node,finish ? "RESULT" : description.at("name").get<string>(),
            !finish && description.value("streaming",false) && inputs.size()==1, !finish && description.value("source",false),child_types,input_names);
        if (inputs.size()==1) { physical.children.push_back(inputs[0]); }
        else if (!inputs.empty()) {
            auto tagged_types=child_types[0]; tagged_types.push_back(LogicalType::UBIGINT);
            vector<uint64_t> ids;
            for (idx_t i=0; i<inputs.size(); i++) {
                if (child_types[i]!=child_types[0]) {throw InternalException("Compiled kernel inputs require the same traverser schema");}
                ids.push_back(description.at("inputs").at(i).get<uint64_t>());
            }
            auto &joined=planner.Make<PhysicalKernelInputs>(tagged_types,execution,std::move(ids));
            physical.children.push_back(joined);
        }
        return physical;
    }
};

thread_local unordered_map<uint64_t, shared_ptr<NativeExecution>> active_programs;
thread_local uint64_t next_program_id=1;
struct ProgramBindScope {
    uint64_t id;
    explicit ProgramBindScope(shared_ptr<NativeExecution> execution) : id(next_program_id++) { active_programs.emplace(id,std::move(execution)); }
    ~ProgramBindScope() { active_programs.erase(id); }
    string SQL(uint64_t node) const { return "SELECT * FROM __orchid_kernel("+std::to_string(id)+"::UBIGINT,"+std::to_string(node)+"::UBIGINT)"; }
};
unique_ptr<QueryResult> ExecuteProgramInput(ClientContext &context, shared_ptr<NativeExecution> execution, uint64_t node) {
    NativeScope native(context,*execution);
    ProgramBindScope binding(execution);
    return HostExecute(context,Select(context,binding.SQL(node)),{});
}
unique_ptr<LogicalOperator> BindProgramNode(ClientContext &context, TableFunctionBindInput &input,
                                          shared_ptr<NativeExecution> execution, uint64_t node,
                                          idx_t index, vector<string> &names, uint64_t program_id) {
    const auto &description=execution->program->Node(node);
    auto child_sql=[&](uint64_t child) {return "SELECT * FROM __orchid_kernel("+std::to_string(program_id)+"::UBIGINT,"+std::to_string(child)+"::UBIGINT)";};
    if (description.at("kind")=="sql") {
        auto statement=Select(context,description.at("sql").get<string>());
        for (const auto &dependency:description.value("inputs",Json::array())) {
            auto cte=make_uniq<CommonTableExpressionInfo>();
            cte->materialized=CTEMaterialize::CTE_MATERIALIZE_ALWAYS;
            cte->query=Select(context,child_sql(dependency.at("node").get<uint64_t>()));
            statement->node->cte_map.map.insert(dependency.at("relation").get<string>(),std::move(cte));
        }
        auto binder=Binder::CreateBinder(context,input.binder);
        auto bound=binder->Bind(static_cast<SQLStatement &>(*statement));
        names=bound.names;
        // Table bind operators must expose the caller's binding index. A
        // standard projection handles column rebinding around this SQL island.
        vector<unique_ptr<Expression>> expressions;
        auto bindings=bound.plan->GetColumnBindings();
        for (idx_t i=0;i<bound.types.size();i++) {expressions.push_back(make_uniq<BoundColumnRefExpression>(bound.types[i],bindings[i]));}
        auto projection=make_uniq<LogicalProjection>(index,std::move(expressions));
        projection->children.push_back(std::move(bound.plan));
        return std::move(projection);
    }
    auto schema=execution->program->Schema(context,node); names=schema.GetNames();
    auto result=make_uniq<LogicalOrchidKernel>(execution,node,index,schema.GetTypes());
    for (const auto &child:description.value("inputs",Json::array())) {
        auto binder=Binder::CreateBinder(context,input.binder);
        auto statement=Select(context,child_sql(child.get<uint64_t>()));
        auto bound=binder->Bind(static_cast<SQLStatement &>(*statement));
        result->AddInput(std::move(bound));
    }
    return std::move(result);
}
unique_ptr<LogicalOperator> KernelBindOperator(ClientContext &context, TableFunctionBindInput &input, idx_t index, vector<string> &names) {
    auto id=input.inputs[0].GetValue<uint64_t>();
    auto found=active_programs.find(id);
    if (found==active_programs.end()) {throw BinderException("Compiled Orchid program is outside its binding scope");}
    return BindProgramNode(context,input,found->second,input.inputs[1].GetValue<uint64_t>(),index,names,id);
}
unique_ptr<LogicalOperator> NativeProgramBind(ClientContext &context, TableFunctionBindInput &input, idx_t index, vector<string> &names) {
    auto request=BoundRequest(context,input);
    auto compiled=ProgramJson(orchid_program_new(request.dump().c_str(), &context, CatalogQuery, UpdateHostFree));
    auto program=make_shared_ptr<NativeProgram>(reinterpret_cast<void *>(compiled.at("handle").get<uintptr_t>()),compiled.at("manifest"),true);
    if (input.binder) {
        auto &properties=input.binder->GetStatementProperties();
        properties.always_require_rebind=true;
        if (compiled.value("mutating",false)) {
            for (const auto &target:compiled.at("targets")) {
                auto name=QualifiedName::Parse(target.get<string>());
                auto catalog_name=name.catalog.empty()?DatabaseManager::GetDefaultDatabase(context):name.catalog;
                auto &catalog=Catalog::GetCatalog(context,catalog_name);
                properties.RegisterDBModify(catalog,context,DatabaseModificationType::INSERT_DATA|DatabaseModificationType::DELETE_DATA|DatabaseModificationType::UPDATE_DATA);
            }
        }
    }
    auto execution=make_shared_ptr<NativeExecution>(program);
    ProgramBindScope scope(execution);
    auto schema=program->Schema(context,ORCHID_RESULT); names=schema.GetNames();
    auto result=make_uniq<LogicalOrchidKernel>(execution,ORCHID_RESULT,index,schema.GetTypes());
    auto binder=Binder::CreateBinder(context,input.binder);
    auto statement=Select(context,scope.SQL(program->manifest.at("root").get<uint64_t>()));
    auto bound=binder->Bind(static_cast<SQLStatement &>(*statement));
    result->AddInput(std::move(bound));
    return std::move(result);
}
char *ExecuteNested(void *context_p, const void *program_p, void *state, ArrowArrayStream *output) {
    output->release=nullptr;
    try {
        auto &context=*static_cast<ClientContext *>(context_p);
        if (!active_native_execution) {throw InternalException("Nested program has no active execution");}
        auto &parent=*active_native_execution;
        auto found=parent.nested.find(program_p);
        shared_ptr<NativeCachedPlan> cached;
        if (found!=parent.nested.end()) {cached=found->second.lock();parent.nested.erase(found);}
        if (!cached) {
            auto metadata=ProgramJson(orchid_program_manifest(program_p));
            auto program=make_shared_ptr<NativeProgram>(const_cast<void *>(program_p),metadata.at("manifest"),false);
            cached=make_shared_ptr<NativeCachedPlan>();
            cached->execution=make_shared_ptr<NativeExecution>(program);
            cached->execution->owned=false;
            cached->execution->execution=parent.execution;
            context.registered_state->GetOrCreate<NativePlans>("orchid_native_plans")->plans.push_back(cached);
        }
        auto execution=cached->execution;
        execution->kernel_state=state;
        ProgramBindScope scope(execution);
        auto result=HostExecute(context,Select(context,scope.SQL(execution->program->manifest.at("root").get<uint64_t>())),{},&cached->prepared);
        execution->kernel_state=nullptr;
        parent.nested[program_p]=cached;
        auto stream=make_uniq<ResultArrowArrayStreamWrapper>(std::move(result),STANDARD_VECTOR_SIZE);
        *output=stream->stream; stream.release(); return nullptr;
    } catch (std::exception &error) {
        ErrorData details(error);
        Json envelope={{"error",details.RawMessage()},{"classification",nullptr},{"diagnosis",nullptr}};
        for (const auto &key:{"classification","diagnosis"}) {
            auto found=details.ExtraInfo().find("orchid_"+string(key));
            if (found!=details.ExtraInfo().end()) {envelope[key]=Json::parse(found->second,nullptr,false);}
        }
        auto text=envelope.dump();
        auto out=static_cast<char *>(std::malloc(text.size()+1));
        if (!out) {std::terminate();} std::memcpy(out,text.c_str(),text.size()+1); return out;
    } catch (...) {
        const char *message="Nested Orchid program failed";
        auto out=static_cast<char *>(std::malloc(std::strlen(message)+1));
        if (!out) {std::terminate();} std::strcpy(out,message); return out;
    }
}
