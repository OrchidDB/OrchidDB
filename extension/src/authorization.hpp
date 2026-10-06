// Session identity and batched permission checks. DuckDB owns secrets and execution.
struct AuthorizationState : ClientContextState {
    Json subject = Json::object();
    std::mutex mutex;
    unordered_map<string, Json> providers;
    unordered_map<string, bool> decisions;
    unordered_set<string> authorized_programs;
    idx_t protected_depth = 0;
    void QueryBegin(ClientContext &) override { providers.clear(); decisions.clear(); authorized_programs.clear(); }
    void QueryEnd() override { providers.clear(); decisions.clear(); authorized_programs.clear(); }
};
shared_ptr<AuthorizationState> Authorization(ClientContext &context) {
    return context.registered_state->GetOrCreate<AuthorizationState>("orchid_authorization");
}
struct ProtectedBinding {
    shared_ptr<AuthorizationState> state;
    ProtectedBinding(ClientContext &context, bool enabled) {
        if (enabled) { state=Authorization(context); state->protected_depth++; }
    }
    ~ProtectedBinding() { if (state) {state->protected_depth--;} }
};
Json SpiceSecret(ClientContext &context, const string &name) {
    auto entry=SecretManager::Get(context).GetSecretByName(CatalogTransaction::GetSystemCatalogTransaction(context),name);
    if (!entry || entry->secret->GetType()!="spicedb") {throw BinderException("Missing SpiceDB secret: %s",name);}
    auto &secret=dynamic_cast<const KeyValueSecret &>(*entry->secret);
    return {{"endpoint",secret.TryGetValue("endpoint",true).GetValue<string>()},
            {"token",secret.TryGetValue("token",true).GetValue<string>()}};
}
unique_ptr<BaseSecret> CreateSpiceSecret(ClientContext &, CreateSecretInput &input) {
    for (auto key : {"endpoint","token"}) {
        auto found=input.options.find(key);
        if (found==input.options.end() || found->second.IsNull() || found->second.GetValue<string>().empty()) {
            throw InvalidInputException("SpiceDB requires a nonempty %s",key);
        }
    }
    auto secret=make_uniq<KeyValueSecret>(input.scope,input.type,input.provider,input.name);
    for (auto &option:input.options) {secret->secret_map[option.first]=option.second;}
    secret->redact_keys.insert("token");
    return std::move(secret);
}
void RequireAuthorization(ClientContext &context, const Json &definition) {
    if (!definition.contains("authorization") || definition.at("authorization").is_null()) {return;}
    auto state=Authorization(context);
    if (state->subject.empty()) {throw BinderException("Protected graph requires session authorization");}
    SpiceSecret(context,definition.at("authorization").at("provider").get<string>());
}
struct AuthBindData : FunctionData {
    Json subject;
    explicit AuthBindData(Json value) : subject(std::move(value)) {}
    unique_ptr<FunctionData> Copy() const override {return make_uniq<AuthBindData>(subject);}
    bool Equals(const FunctionData &other) const override {return subject==other.Cast<AuthBindData>().subject;}
};
struct AuthSetState : GlobalTableFunctionState {bool done=false;};
unique_ptr<FunctionData> AuthSetBind(ClientContext &, TableFunctionBindInput &input, vector<LogicalType> &types, vector<string> &names) {
    if (input.inputs[0].IsNull()) {throw InvalidInputException("Authorization context cannot be NULL");}
    Json subject=Json::parse(input.inputs[0].GetValue<string>());
    if (!subject.is_object()) {throw InvalidInputException("Authorization context must be an object");}
    for (auto it=subject.begin();it!=subject.end();++it) {
        if (it.key()!="subject_type" && it.key()!="subject_id" && it.key()!="context" && it.key()!="at_least_as_fresh") {
            throw InvalidInputException("Unknown authorization context option: %s",it.key());
        }
        if (it.key()=="context") {
            if (!it.value().is_object()) {throw InvalidInputException("Caveat context must be an object");}
        } else if (!it.value().is_string() || it.value().get<string>().empty()) {
            throw InvalidInputException("Authorization identity and token must be nonempty strings");
        }
    }
    if (!subject.empty() && (!subject.contains("subject_type") || !subject.contains("subject_id"))) {
        throw InvalidInputException("Authorization requires subject_type and subject_id");
    }
    types={LogicalType::BOOLEAN};names={"success"};
    return make_uniq<AuthBindData>(subject);
}
unique_ptr<GlobalTableFunctionState> AuthSetInit(ClientContext &, TableFunctionInitInput &) {return make_uniq<AuthSetState>();}
void AuthSetExecute(ClientContext &context, TableFunctionInput &input, DataChunk &output) {
    auto &scan=input.global_state->Cast<AuthSetState>();if(scan.done){return;}
    auto state=Authorization(context);std::lock_guard<std::mutex> lock(state->mutex);
    state->subject=input.bind_data->Cast<AuthBindData>().subject;
    state->providers.clear();state->decisions.clear();
    output.SetValue(0,0,Value(true));output.SetCardinality(1);scan.done=true;
}
string AlterAuthorization(ClientContext &context, const FunctionParameters &parameters) {
    auto name=QualifiedName::Parse(parameters.values[0].GetValue<string>());
    auto &entry=Catalog::GetEntry<ViewCatalogEntry>(context,name.catalog,name.schema,"__orchid_graph_"+name.name);
    Json graph=ReadDefinition(entry);
    if (graph.contains("managed_table")) {throw BinderException("Authorization requires a mapped property graph");}
    graph["authorization"]=Json::parse(parameters.values[1].GetValue<string>());
    return "CREATE OR REPLACE VIEW "+Quote(entry.catalog.GetName())+"."+Quote(entry.schema.name)+"."+Quote(entry.name)+
        " AS SELECT * FROM orchid_graph_definition("+Literal(graph.dump())+")";
}
void SpiceCheck(DataChunk &args, ExpressionState &expression, Vector &output) {
    auto &context=expression.GetContext();auto state=Authorization(context);
    std::lock_guard<std::mutex> lock(state->mutex);
    if (state->subject.empty()) {throw InvalidInputException("Protected graph requires session authorization");}
    // Deduplicate within a vector and across scans/aliases in this statement.
    std::map<string, std::map<string,Json>> pending;
    vector<string> keys(args.size());
    for(idx_t row=0;row<args.size();row++) {
        bool null=false;for(idx_t c=0;c<4;c++){null=null||args.GetValue(c,row).IsNull();}
        if(null || args.GetValue(3,row).GetValue<string>().empty()){continue;}
        auto provider=args.GetValue(0,row).GetValue<string>();
        Json item=Json::array({args.GetValue(1,row).GetValue<string>(),args.GetValue(2,row).GetValue<string>(),args.GetValue(3,row).GetValue<string>()});
        keys[row]=Json::array({provider,item}).dump();
        if(state->decisions.find(keys[row])==state->decisions.end()){pending[provider][keys[row]]=item;}
    }
    for(auto &group:pending) {
        auto found=state->providers.find(group.first);
        if(found==state->providers.end()){found=state->providers.emplace(group.first,SpiceSecret(context,group.first)).first;}
        Json request=found->second;request["op"]="spicedb_check";request["authorization"]=state->subject;
        request["items"]=Json::array();
        for(auto &item:group.second){request["items"].push_back(item.second);}
        auto response=Bridge(request);
        found->second["revision"]=response.at("revision");
        idx_t i=0;for(auto &item:group.second){state->decisions[item.first]=response.at("decisions").at(i++).get<bool>();}
    }
    output.SetVectorType(VectorType::FLAT_VECTOR);
    for(idx_t row=0;row<args.size();row++){output.SetValue(row,Value(!keys[row].empty() && state->decisions.at(keys[row])));}
}
void RegisterAuthorization(ExtensionLoader &loader) {
    SecretType type;type.name="spicedb";type.default_provider="config";type.extension="orchid";
    type.deserializer=KeyValueSecret::Deserialize<KeyValueSecret>;loader.RegisterSecretType(type);
    CreateSecretFunction secret;secret.secret_type="spicedb";secret.provider="config";secret.function=CreateSpiceSecret;
    secret.named_parameters["endpoint"]=LogicalType::VARCHAR;secret.named_parameters["token"]=LogicalType::VARCHAR;
    loader.RegisterFunction(secret);
    loader.RegisterFunction(TableFunction("orchid_set_authorization",{LogicalType::VARCHAR},AuthSetExecute,AuthSetBind,AuthSetInit));
    loader.RegisterFunction(PragmaFunction::PragmaCall("orchid_graph_authorization",AlterAuthorization,{LogicalType::VARCHAR,LogicalType::VARCHAR}));
    ScalarFunction check("__orchid_spicedb_check",vector<LogicalType>(4,LogicalType::VARCHAR),LogicalType::BOOLEAN,SpiceCheck);
    check.SetStability(FunctionStability::VOLATILE);check.null_handling=FunctionNullHandling::SPECIAL_HANDLING;
    loader.RegisterFunction(check);
}

// Inspect DuckDB's own macro ASTs, including transitive calls and defaults. A
// macro subquery could otherwise introduce a scan outside the graph policies.
void ValidateProtectedFunctions(ClientContext &context, SelectStatement &statement) {
    auto state=Authorization(context);
    if (!state->protected_depth && state->subject.empty()) {return;}
    unordered_set<const CatalogEntry *> visited;
    std::function<void(QueryNode &)> query;
    std::function<void(ParsedExpression &,bool)> expression;
    expression=[&](ParsedExpression &expr,bool macro) {
        if (expr.GetExpressionClass()==ExpressionClass::SUBQUERY) {
            if (macro) {throw BinderException("Macros in authorized queries cannot contain subqueries");}
            query(*expr.Cast<SubqueryExpression>().subquery->node);
        }
        if (expr.GetExpressionClass()==ExpressionClass::FUNCTION) {
            auto &function=expr.Cast<FunctionExpression>();
            auto entry=Catalog::GetEntry(context,function.catalog,function.schema,
                EntryLookupInfo(CatalogType::SCALAR_FUNCTION_ENTRY,function.function_name),OnEntryNotFound::RETURN_NULL);
            if (entry && entry->type==CatalogType::MACRO_ENTRY && visited.insert(entry.get()).second) {
                for (auto &definition:entry->Cast<ScalarMacroCatalogEntry>().macros) {
                    expression(*definition->Cast<ScalarMacroFunction>().expression,true);
                    for (auto &parameter:definition->default_parameters) {expression(*parameter.second,true);}
                }
            }
        }
        ParsedExpressionIterator::EnumerateChildren(expr,[&](ParsedExpression &child){expression(child,macro);});
    };
    query=[&](QueryNode &node){ParsedExpressionIterator::EnumerateQueryNodeChildren(node,[&](unique_ptr<ParsedExpression> &expr){expression(*expr,false);});};
    query(*statement.node);
}
