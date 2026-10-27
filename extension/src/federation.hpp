// Query-scoped federation, with the host transaction retained by every SQL
// island. Source credentials are session state and never enter compiled plans.
extern "C" {
char *orchid_federation_schema(const char *, void *, decltype(&TypedHostQuery), decltype(&UpdateHostFree), ArrowArrayStream *);
char *orchid_federation_open(const char *, const char *, void *, decltype(&TypedHostQuery), decltype(&UpdateHostFree), bool (*)(void *), void **);
char *orchid_federation_next(void *, void *, decltype(&TypedHostQuery), decltype(&UpdateHostFree), bool (*)(void *), ArrowArrayStream *, bool *);
void orchid_federation_free(void *);
}
bool FederationInterrupted(void *context) {return static_cast<ClientContext *>(context)->IsInterrupted();}
void FederationCheck(char *error) {
    if (!error) { return; }
    std::unique_ptr<char, decltype(&orchid_bridge_free)> owned(error, orchid_bridge_free);
    throw InvalidInputException(HostErrorMetadata(string(error)),"Orchid federation: "+string(error));
}
struct RegisteredSources : ClientContextState { std::map<string, Json> sources; };
unique_ptr<FunctionData> RegisterSourceBind(ClientContext &, TableFunctionBindInput &input, vector<LogicalType> &types, vector<string> &names) {
    types={LogicalType::BOOLEAN}; names={"registered"};
    for (auto &value:input.inputs) { if (value.IsNull()) { throw BinderException("Source registration arguments cannot be NULL"); } }
    auto name=input.inputs[0].GetValue<string>();
    auto adapter=input.inputs[1].GetValue<string>();
    if (name.empty()) { throw BinderException("Source name cannot be empty"); }
    if (adapter!="weaviate" && adapter!="quickwit" && adapter!="elasticsearch" && adapter!="prometheus" && adapter!="loki") {
        throw BinderException("Expected a supported remote request source adapter");
    }
    auto options=Json::parse(input.inputs[2].GetValue<string>());
    if (!options.is_object()) { throw BinderException("Source options must be a JSON object"); }
    return make_uniq<ProgramBindData>(Json({{"name",name},{"adapter",adapter},{"options",options}}).dump());
}
void RegisterSourceExecute(ClientContext &context, TableFunctionInput &input, DataChunk &output) {
    auto &state=input.global_state->Cast<ProgramState>();
    if (state.done) { return; }
    auto entry=Json::parse(input.bind_data->Cast<ProgramBindData>().request);
    context.registered_state->GetOrCreate<RegisteredSources>("orchid_sources")->sources[entry.at("name").get<string>()]=entry;
    output.SetValue(0,0,Value::BOOLEAN(true));output.SetCardinality(1);state.done=true;
}
unique_ptr<FunctionData> FederationBind(ClientContext &context, TableFunctionBindInput &input, vector<LogicalType> &types, vector<string> &names) {
    if (input.inputs[0].IsNull()) { throw BinderException("Missing federation plan"); }
    auto plan=input.inputs[0].GetValue<string>();
    ArrowArrayStreamWrapper schema;
    FederationCheck(orchid_federation_schema(plan.c_str(),&context,TypedHostSchema,UpdateHostFree,&schema.arrow_array_stream));
    auto relation=ImportHostRelation(context,schema.arrow_array_stream);
    types=relation->source->table.GetTypes();
    names=relation->source->table.GetNames();
    return make_uniq<ProgramBindData>(std::move(plan));
}
struct FederationState : GlobalTableFunctionState {
    void *execution=nullptr;
    unique_ptr<HostRelationCursor> cursor;
    bool done=false;
    ~FederationState() override { cursor.reset(); if(execution) {orchid_federation_free(execution);} }
    idx_t MaxThreads() const override {return 1;}
};
unique_ptr<GlobalTableFunctionState> FederationInit(ClientContext &,TableFunctionInitInput &) {return make_uniq<FederationState>();}
void FederationScan(ClientContext &context,TableFunctionInput &input,DataChunk &output) {
    auto &state=input.global_state->Cast<FederationState>();
    if (state.done) {return;}
    if(!state.execution) {
        const auto &plan=input.bind_data->Cast<ProgramBindData>().request;
        Json sources=context.registered_state->GetOrCreate<RegisteredSources>("orchid_sources")->sources;
        FederationCheck(orchid_federation_open(plan.c_str(),sources.dump().c_str(),&context,TypedHostQuery,UpdateHostFree,FederationInterrupted,&state.execution));
    }
    for(;;) {
        if(context.IsInterrupted()) {throw InterruptException();}
        if(state.cursor && state.cursor->Next(context,output)) {return;}
        state.cursor.reset();
        ArrowArrayStreamWrapper stream;
        FederationCheck(orchid_federation_next(state.execution,&context,TypedHostQuery,UpdateHostFree,FederationInterrupted,&stream.arrow_array_stream,&state.done));
        if(state.done) {return;}
        auto relation=ImportHostRelation(context,stream.arrow_array_stream);
        state.cursor=make_uniq<HostRelationCursor>(relation->source);
    }
}
unique_ptr<TableRef> CompiledSubquery(ClientContext &context,const Json &compiled) {
    if(compiled.contains("transfers") && !compiled.at("transfers").empty()) {
        return Subquery(context,"SELECT * FROM __orchid_federated("+Literal(compiled.dump())+")");
    }
    return Subquery(context,compiled.at("sql").get<string>());
}
void ValidateHostEngines(const Json &schema) {
    auto engines=schema.value("engines",Json::object());
    auto target=schema.value("execution_engine",Json());
    if(!target.is_null() && (!engines.contains(target.get<string>()) || engines.at(target.get<string>()).value("dialect","")!="duckdb")) {
        throw BinderException("Orchid extension execution_engine must name the caller's DuckDB engine");
    }
    for(auto it=engines.begin();it!=engines.end();++it) {
        auto dialect=it.value().value("dialect","");
        if(dialect!="duckdb" && dialect!="trino" && dialect!="weaviate" && dialect!="quickwit" &&
           dialect!="elasticsearch" && dialect!="prometheus" && dialect!="loki") {
            throw BinderException("Orchid extension queries must execute in the host DuckDB; unsupported engine: %s",dialect);
        }
        if(it.value().value("dialect","")=="duckdb" && (target.is_null() || it.key()!=target.get<string>())) {
            throw BinderException("Orchid extension supports one caller-owned DuckDB execution engine");
        }
    }
}
