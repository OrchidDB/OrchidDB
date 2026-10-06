#define DUCKDB_EXTENSION_MAIN
#include "duckdb.hpp"
#include "duckdb/catalog/catalog.hpp"
#include "duckdb/catalog/catalog_entry/view_catalog_entry.hpp"
#include "duckdb/catalog/catalog_search_path.hpp"
#include "duckdb/main/client_context.hpp"
#include "duckdb/main/client_context_state.hpp"
#include "duckdb/main/client_data.hpp"
#include "duckdb/main/config.hpp"
#include "duckdb/main/database_manager.hpp"
#include "duckdb/main/extension/extension_loader.hpp"
#include "duckdb/parser/expression/constant_expression.hpp"
#include "duckdb/parser/expression/function_expression.hpp"
#include "duckdb/parser/parser.hpp"
#include "duckdb/parser/parser_extension.hpp"
#include "duckdb/parser/query_node/select_node.hpp"
#include "duckdb/parser/statement/select_statement.hpp"
#include "duckdb/parser/tableref/subqueryref.hpp"
#include "duckdb/parser/tableref/table_function_ref.hpp"
#include "duckdb/planner/binder.hpp"
#include "duckdb/planner/operator/logical_get.hpp"
#include "duckdb/function/scalar_function.hpp"
#include "duckdb/planner/planner.hpp"
#include "duckdb/optimizer/optimizer.hpp"
#include "duckdb/execution/executor.hpp"
#include "duckdb/execution/expression_executor.hpp"
#include "duckdb/execution/physical_plan_generator.hpp"
#include "duckdb/execution/operator/helper/physical_result_collector.hpp"
#include "duckdb/main/prepared_statement_data.hpp"
#include "duckdb/main/query_profiler.hpp"
#include "duckdb/main/attached_database.hpp"
#include "duckdb/planner/operator/logical_extension_operator.hpp"
#include "duckdb/planner/bound_statement.hpp"
#include "duckdb/planner/operator/logical_projection.hpp"
#include "duckdb/planner/expression/bound_columnref_expression.hpp"
#include "duckdb/execution/physical_operator_states.hpp"
#include "duckdb/execution/column_binding_resolver.hpp"
#include "duckdb/execution/operator/set/physical_union.hpp"
#include "duckdb/main/materialized_query_result.hpp"
#include "duckdb/common/arrow/arrow_converter.hpp"
#include "duckdb/common/arrow/result_arrow_wrapper.hpp"
#include "duckdb/common/types/column/column_data_collection.hpp"
#include "duckdb/function/table/arrow.hpp"
#include "duckdb/parser/statement/insert_statement.hpp"
#include "duckdb/parser/statement/delete_statement.hpp"
#include "duckdb/parser/statement/update_statement.hpp"
#include "json.hpp"
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstdlib>
#include <cstring>
#include <mutex>
#include <thread>

extern "C" char *orchid_bridge(const char *);
extern "C" void orchid_bridge_free(char *);
extern "C" char *orchid_update(const char *, void *, char *(*)(void *, const char *), void (*)(char *));

namespace duckdb {
namespace {
using Json = nlohmann::json;

Json Bridge(const Json &request) {
    auto input = request.dump();
    std::unique_ptr<char, decltype(&orchid_bridge_free)> response(orchid_bridge(input.c_str()), orchid_bridge_free);
    if (!response) { throw InternalException("Orchid compiler returned no response"); }
    auto result = Json::parse(response.get());
    if (!result.at("ok").get<bool>()) {
        throw BinderException("Orchid: %s", result.at("error").get<string>());
    }
    return result.at("result");
}

string Quote(const string &value) {
    return "\"" + StringUtil::Replace(value, "\"", "\"\"") + "\"";
}
string Literal(const string &value) {
    return "'" + StringUtil::Replace(value, "'", "''") + "'";
}
string Path(const Json &parts) {
    string out;
    for (const auto &part : parts) {
        if (!out.empty()) { out += "."; }
        out += Quote(part.get<string>());
    }
    return out;
}

unique_ptr<SelectStatement> Select(ClientContext &context, const string &sql) {
    Parser parser(context.GetParserOptions());
    parser.ParseQuery(sql);
    if (parser.statements.size() != 1 || parser.statements[0]->type != StatementType::SELECT_STATEMENT) {
        throw BinderException("Orchid expected one read-only SQL query");
    }
    return unique_ptr_cast<SQLStatement, SelectStatement>(std::move(parser.statements[0]));
}
unique_ptr<TableRef> Subquery(ClientContext &context, const string &sql) {
    return make_uniq<SubqueryRef>(Select(context, sql));
}

// Do not infer a source schema from rows. Bind its actual DuckDB relation,
// including extension-provided catalogs and views, without executing a scan.
string CompilerType(const LogicalType &type) {
    if (type.HasAlias() && type.GetAlias() == "JSON") { return "json"; }
    switch (type.id()) {
    case LogicalTypeId::BOOLEAN: return "boolean";
    case LogicalTypeId::TINYINT: return "int8";
    case LogicalTypeId::SMALLINT: return "int16";
    case LogicalTypeId::INTEGER: return "int32";
    case LogicalTypeId::BIGINT: return "int64";
    case LogicalTypeId::UTINYINT: return "uint8";
    case LogicalTypeId::USMALLINT: return "uint16";
    case LogicalTypeId::UINTEGER: return "uint32";
    case LogicalTypeId::UBIGINT: return "uint64";
    case LogicalTypeId::FLOAT: return "float32";
    case LogicalTypeId::DOUBLE: return "float64";
    case LogicalTypeId::VARCHAR: return "string";
    case LogicalTypeId::BLOB: return "binary";
    case LogicalTypeId::DATE: return "date";
    case LogicalTypeId::TIME: return "time";
    case LogicalTypeId::TIMESTAMP: return "timestamp";
    case LogicalTypeId::INTERVAL: return "interval";
    case LogicalTypeId::DECIMAL:
        return "decimal:" + std::to_string(DecimalType::GetWidth(type)) + ":" + std::to_string(DecimalType::GetScale(type));
    case LogicalTypeId::LIST: return "list:" + CompilerType(ListType::GetChildType(type));
    case LogicalTypeId::ARRAY: return "list:" + CompilerType(ArrayType::GetChildType(type));
    case LogicalTypeId::STRUCT: {
        auto fields = Json::array();
        for (const auto &child : StructType::GetChildTypes(type)) {
            fields.push_back(Json::array({child.first, CompilerType(child.second)}));
        }
        return "struct_fields:" + fields.dump();
    }
    default:
        throw BinderException("Orchid does not yet support source type %s; cast it in a source view", type.ToString());
    }
}

struct BoundGraph {
    Json definition;
    Json tables = Json::array();
    vector<string> sources;
};

BoundGraph BindGraph(ClientContext &context, Json definition, const string &catalog, const string &schema) {
    BoundGraph graph;
    graph.definition = std::move(definition);
    if (graph.definition.contains("managed_table") && !graph.definition.at("managed_table").is_null()) {
        auto source=graph.definition.at("managed_table").get<string>();
        auto statement=Select(context,"SELECT * FROM "+source+" LIMIT 0");
        auto binder=Binder::CreateBinder(context);
        binder->Bind(static_cast<SQLStatement &>(*statement));
        graph.sources.push_back(source);
        return graph;
    }
    for (auto group : {"vertices", "edges"}) {
        for (const auto &element : graph.definition.at(group)) {
            auto parts = element.at("source");
            // Relative sources belong to the graph's catalog/schema. Attached
            // catalogs should be explicitly named as catalog.schema.table.
            if (parts.size() == 1) { parts.insert(parts.begin(), schema); }
            if (parts.size() == 2) { parts.insert(parts.begin(), catalog); }
            auto source = Path(parts);
            auto statement = Select(context, "SELECT * FROM " + source + " LIMIT 0");
            auto binder = Binder::CreateBinder(context);
            auto bound = binder->Bind(static_cast<SQLStatement &>(*statement));
            auto columns = Json::array();
            for (idx_t i = 0; i < bound.names.size(); i++) {
                bool needed = element.at("properties").is_null();
                auto includes = [&](const Json &names) {
                    for (const auto &name : names) {
                        if (StringUtil::CIEquals(name.get<string>(), bound.names[i])) { return true; }
                    }
                    return false;
                };
                needed = needed || includes(element.at("key"));
                if (!element.at("properties").is_null()) { needed = needed || includes(element.at("properties")); }
                for (auto endpoint : {"from", "to"}) {
                    if (!element.at(endpoint).is_null()) { needed = needed || includes(element.at(endpoint).at("columns")); }
                }
                if (!needed) { continue; }
                columns.push_back({{"name", bound.names[i]}, {"data_type", CompilerType(bound.types[i])}, {"nullable", true}});
            }
            graph.tables.push_back({{"name", source}, {"columns", columns}});
            graph.sources.push_back(source);
        }
    }
    Bridge({{"op", "validate"}, {"graph", graph.definition}, {"tables", graph.tables}});
    return graph;
}

Json ReadDefinition(ViewCatalogEntry &entry) {
    auto &query = entry.GetQuery();
    if (query.node->type != QueryNodeType::SELECT_NODE) { throw BinderException("Not an Orchid property graph"); }
    auto &node = query.node->Cast<SelectNode>();
    if (!node.from_table || node.from_table->type != TableReferenceType::TABLE_FUNCTION) {
        throw BinderException("Not an Orchid property graph");
    }
    auto &expr = node.from_table->Cast<TableFunctionRef>().function->Cast<FunctionExpression>();
    if (expr.function_name != "orchid_graph_definition" || expr.children.size() != 1 ||
        expr.children[0]->GetExpressionClass() != ExpressionClass::CONSTANT) {
        throw BinderException("Invalid Orchid property graph definition");
    }
    return Json::parse(expr.children[0]->Cast<ConstantExpression>().value.GetValue<string>());
}

BoundGraph LoadGraph(ClientContext &context, const string &name) {
    auto qualified = QualifiedName::Parse(name);
    qualified.name = "__orchid_graph_" + qualified.name;
    auto &entry = Catalog::GetEntry<ViewCatalogEntry>(context, qualified.catalog, qualified.schema, qualified.name);
    return BindGraph(context, ReadDefinition(entry), entry.catalog.GetName(), entry.schema.name);
}

unique_ptr<TableRef> DefinitionBind(ClientContext &context, TableFunctionBindInput &input) {
    auto definition = Json::parse(input.inputs[0].GetValue<string>());
    auto name = QualifiedName::Parse(Path(definition.at("name")));
    auto &search = *ClientData::Get(context).catalog_search_path;
    auto catalog = name.catalog.empty() ? DatabaseManager::GetDefaultDatabase(context) : name.catalog;
    auto schema = name.schema.empty() ? search.GetDefaultSchema(context, catalog) : name.schema;
    BindGraph(context, definition, catalog, schema);
    // The original function expression is persisted by DuckDB's CREATE VIEW,
    // while this replacement validates sources during binding, without writes.
    return Subquery(context, "SELECT " + Literal(definition.dump()) + " AS definition");
}

Json Parameter(const Value &value, bool native = false) {
    if (value.IsNull()) { return nullptr; }
    if (native && (value.type().id()==LogicalTypeId::TINYINT || value.type().id()==LogicalTypeId::SMALLINT || value.type().id()==LogicalTypeId::INTEGER || value.type().id()==LogicalTypeId::BIGINT || value.type().id()==LogicalTypeId::HUGEINT || value.type().id()==LogicalTypeId::FLOAT || value.type().id()==LogicalTypeId::DOUBLE)) {
        return {{"__orchiddb_number",Json::array({value.type().ToString(),value.ToString()})}};
    }
    switch (value.type().id()) {
    case LogicalTypeId::BOOLEAN: return value.GetValue<bool>();
    case LogicalTypeId::TINYINT:
    case LogicalTypeId::SMALLINT:
    case LogicalTypeId::INTEGER:
    case LogicalTypeId::BIGINT: return value.GetValue<int64_t>();
    case LogicalTypeId::UTINYINT:
    case LogicalTypeId::USMALLINT:
    case LogicalTypeId::UINTEGER:
    case LogicalTypeId::UBIGINT: return value.GetValue<uint64_t>();
    case LogicalTypeId::DECIMAL:
    case LogicalTypeId::FLOAT:
    case LogicalTypeId::DOUBLE: {
        auto number = value.GetValue<double>();
        if (!std::isfinite(number)) {
            if (native) { return {{"__orchiddb_float", std::isnan(number) ? "NaN" : number > 0 ? "Infinity" : "-Infinity"}}; }
            throw BinderException("Cypher parameters must be finite");
        }
        return number;
    }
    case LogicalTypeId::VARCHAR: return value.GetValue<string>();
    case LogicalTypeId::STRUCT: {
        auto out = Json::object();
        const auto &children = StructValue::GetChildren(value);
        for (idx_t i = 0; i < children.size(); i++) { out[StructType::GetChildName(value.type(), i)] = Parameter(children[i], native); }
        return out;
    }
    case LogicalTypeId::LIST:
    case LogicalTypeId::ARRAY: {
        auto out = Json::array();
        auto &children = value.type().id() == LogicalTypeId::LIST ? ListValue::GetChildren(value) : ArrayValue::GetChildren(value);
        for (auto &child : children) { out.push_back(Parameter(child, native)); }
        return out;
    }
    default: throw BinderException("Unsupported Cypher parameter type %s", value.type().ToString());
    }
}

// Execute generated program stages using DuckDB's planner and pipeline executor
// in the caller's transaction. No Connection or independent transaction is made.
unique_ptr<QueryResult> HostExecute(ClientContext &context, unique_ptr<SQLStatement> statement, const vector<Value> &params, unique_ptr<PreparedStatementData> *cached=nullptr) {
    auto statement_type = statement->type;
    auto &client = ClientData::Get(context);
    struct ProfilerGuard {
        ClientData &client;
        shared_ptr<QueryProfiler> previous;
        ProfilerGuard(ClientData &client, ClientContext &context) : client(client), previous(client.profiler) {
            client.profiler = make_shared_ptr<QueryProfiler>(context);
        }
        ~ProfilerGuard() { client.profiler = std::move(previous); }
    } guard(client, context);
    unique_ptr<PreparedStatementData> local;
    auto &prepared=cached ? *cached : local;
    if (!prepared) {
    Planner planner(context);
    for (idx_t i = 0; i < params.size(); i++) {
        planner.parameter_data[std::to_string(i + 1)] = BoundParameterData(params[i]);
    }
    planner.CreatePlan(std::move(statement));
    for (auto &entry : planner.properties.modified_databases) {
        auto db = DatabaseManager::Get(context).GetDatabase(context, entry.first);
        if (!db || db->IsReadOnly()) { throw BinderException("Update target database is missing or read-only"); }
        MetaTransaction::Get(context).ModifyDatabase(*db, entry.second.modifications);
    }
    Optimizer optimizer(*planner.binder, context);
    auto logical = optimizer.Optimize(std::move(planner.plan));
    prepared=make_uniq<PreparedStatementData>(statement_type);
    prepared->names = planner.names;
    prepared->types = planner.types;
    prepared->properties = planner.properties;
    prepared->output_type = QueryResultOutputType::FORCE_MATERIALIZED;
    prepared->memory_type = QueryResultMemoryType::IN_MEMORY;
    PhysicalPlanGenerator generator(context);
    prepared->physical_plan = generator.Plan(std::move(logical));
    }
    Executor executor(context);
    executor.Initialize(PhysicalResultCollector::GetResultCollector(context, *prepared));
    // A completed pipeline can still have scheduler-owned task references.
    // Drain them before destroying this executor or restoring its profiler.
    struct TaskGuard {
        Executor &executor;
        ~TaskGuard() { executor.CancelTasks(); }
    } tasks {executor};
    for (;;) {
        auto status = executor.ExecuteTask();
        if (status == PendingExecutionResult::RESULT_READY || status == PendingExecutionResult::EXECUTION_FINISHED) { break; }
        if (status == PendingExecutionResult::EXECUTION_ERROR) { executor.ThrowException(); }
        if (status == PendingExecutionResult::BLOCKED || status == PendingExecutionResult::NO_TASKS_AVAILABLE) { executor.WaitForTask(); }
    }
    auto result = executor.GetResult();
    if (result->HasError()) { result->ThrowError(); }
    return result;
}

Json HostQuery(ClientContext &context, const Json &request) {
    Parser parser(context.GetParserOptions());
    parser.ParseQuery(request.at("sql").get<string>());
    if (parser.statements.size() != 1) { throw BinderException("Program stages require one SQL statement"); }
    vector<Value> params;
    for (const auto &param : request.value("parameters", Json::array())) {
        params.push_back(param.is_null() ? Value(LogicalType::VARCHAR) : Value(param.get<string>()));
    }
    auto result = HostExecute(context, std::move(parser.statements[0]), params);
    auto rows = Json::array();
    while (auto chunk = result->Fetch()) {
        for (idx_t row = 0; row < chunk->size(); row++) {
            auto cells = Json::array();
            for (idx_t col = 0; col < chunk->ColumnCount(); col++) { cells.push_back(Parameter(chunk->GetValue(col, row))); }
            rows.push_back(std::move(cells));
        }
    }
    return {{"columns", result->names}, {"rows", rows}};
}

#include "host_arrow.hpp"

Json BoundRequest(ClientContext &context, TableFunctionBindInput &input);
char *UpdateHostQuery(void *state, const char *input) {
    Json result;
    try { result = {{"ok", true}, {"result", HostQuery(*static_cast<ClientContext *>(state), Json::parse(input))}}; }
    catch (std::exception &error) { result = {{"ok", false}, {"error", error.what()}}; }
    catch (...) { result = {{"ok", false}, {"error", "Host SQL execution failed"}}; }
    auto text = result.dump();
    auto output = static_cast<char *>(std::malloc(text.size() + 1));
    if (output) { std::memcpy(output, text.c_str(), text.size() + 1); }
    return output;
}
void UpdateHostFree(char *output) { std::free(output); }

struct ProgramBindData : FunctionData {
    string request;
    explicit ProgramBindData(string request) : request(std::move(request)) {}
    unique_ptr<FunctionData> Copy() const override { return make_uniq<ProgramBindData>(request); }
    bool Equals(const FunctionData &other) const override { return request == other.Cast<ProgramBindData>().request; }
};
struct ProgramState : GlobalTableFunctionState { bool done = false; };
unique_ptr<FunctionData> ProgramBind(ClientContext &context, TableFunctionBindInput &input, vector<LogicalType> &types, vector<string> &names) {
    types = {LogicalType::VARCHAR}; names = {"result"};
    if (input.binder) { input.binder->GetStatementProperties().always_require_rebind = true; }
    auto request = BoundRequest(context, input);
    if (request.at("language") != "sparql") { throw BinderException("Expected a SPARQL update request"); }
    // Declare effects during binding, before executing any generated program stage.
    auto mark = [&](const string &source) {
        auto name = QualifiedName::Parse(source);
        auto catalog_name = name.catalog.empty() ? DatabaseManager::GetDefaultDatabase(context) : name.catalog;
        auto &catalog = Catalog::GetCatalog(context, catalog_name);
        if (!input.binder) { throw InternalException("SPARQL updates require a binder"); }
        input.binder->GetStatementProperties().RegisterDBModify(catalog, context,
            DatabaseModificationType::INSERT_DATA | DatabaseModificationType::DELETE_DATA | DatabaseModificationType::UPDATE_DATA);
    };
    for (auto group : {"rdf", "rdf_sources"}) {
        for (auto &source : request.value(group, Json::array())) {
            if (source.value("writable", false)) { mark(source.at("table").get<string>()); }
        }
    }
    auto names_mapping = request.value("rdf_graph_names", Json());
    if (!names_mapping.is_null() && names_mapping.value("writable", false)) { mark(names_mapping.at("table").get<string>()); }
    Json program = {{"request", request}};
    auto base = input.named_parameters.find("base");
    if (base != input.named_parameters.end() && !base->second.IsNull()) { program["base"] = base->second.GetValue<string>(); }
    // Parse during bind, but never execute update effects during EXPLAIN/PREPARE.
    Bridge({{"op", "sparql_syntax"}, {"query", request.at("query")}, {"base", program.value("base", Json())}, {"update", true}});
    return make_uniq<ProgramBindData>(program.dump());
}
unique_ptr<GlobalTableFunctionState> ProgramInit(ClientContext &, TableFunctionInitInput &) {
    return make_uniq<ProgramState>();
}
void ProgramExecute(ClientContext &context, TableFunctionInput &input, DataChunk &output) {
    auto &state = input.global_state->Cast<ProgramState>();
    if (state.done) { return; }
    const auto &request = input.bind_data->Cast<ProgramBindData>().request;
    std::unique_ptr<char, decltype(&orchid_bridge_free)> response(
        orchid_update(request.c_str(), &context, UpdateHostQuery, UpdateHostFree), orchid_bridge_free);
    if (!response) { throw InternalException("Orchid update returned no result"); }
    auto result = Json::parse(response.get());
    if (!result.at("ok").get<bool>()) { throw InvalidInputException("Orchid update: %s", result.at("error").get<string>()); }
    output.SetValue(0, 0, Value(result.at("result").dump()));
    output.SetCardinality(1);
    state.done = true;
}

unique_ptr<TableRef> LanguageBind(ClientContext &context, TableFunctionBindInput &input, const string &language) {
    // Source schemas and graph definitions can change independently, especially
    // in attached catalogs. Rebind prepared queries instead of caching stale
    // mappings or parameter-specialized SQL.
    if (input.binder) { input.binder->GetStatementProperties().always_require_rebind = true; }
    auto graph = LoadGraph(context, input.inputs[0].GetValue<string>());
    auto parameters = Json::object();
    auto found = input.named_parameters.find("parameters");
    if (found != input.named_parameters.end()) {
        if (found->second.type().id() != LogicalTypeId::STRUCT || found->second.IsNull()) {
            throw BinderException("Cypher parameters must be a non-null STRUCT");
        }
        parameters = Parameter(found->second);
    }
    auto compiled = Bridge({{"op", "compile"}, {"graph", graph.definition}, {"tables", graph.tables},
                            {"query", input.inputs[1].GetValue<string>()}, {"parameters", parameters}, {"language", language}});
    return Subquery(context, compiled.at("sql").get<string>());
}

unique_ptr<TableRef> CypherBind(ClientContext &context, TableFunctionBindInput &input) {
    return LanguageBind(context, input, "cypher");
}
unique_ptr<TableRef> GremlinBind(ClientContext &context, TableFunctionBindInput &input) {
    return LanguageBind(context, input, "gremlin");
}

// Advanced mappings use the existing compiler protocol. The host binder is the
// authority for schemas; caller-supplied column types are never trusted.
Json BoundRequest(ClientContext &context, TableFunctionBindInput &input) {
    if (input.binder) { input.binder->GetStatementProperties().always_require_rebind = true; }
    auto request = Json::parse(input.inputs[0].GetValue<string>());
    if (request.value("dialect", "duckdb") != "duckdb" ||
        !request.value("engines", Json::object()).empty() || !request.value("execution_engine", Json()).is_null()) {
        throw BinderException("Orchid extension queries must execute in the host DuckDB");
    }
    request["dialect"] = "duckdb";
    for (auto &table : request.at("tables")) {
        auto name = QualifiedName::Parse(table.at("name").get<string>());
        string source;
        if (!name.catalog.empty()) { source += Quote(name.catalog) + "."; }
        if (!name.schema.empty()) { source += Quote(name.schema) + "."; }
        source += Quote(name.name);
        auto statement = Select(context, "SELECT * FROM " + source + " LIMIT 0");
        auto binder = Binder::CreateBinder(context);
        auto bound = binder->Bind(static_cast<SQLStatement &>(*statement));
        auto columns = Json::array();
        for (idx_t i = 0; i < bound.names.size(); i++) {
            columns.push_back({{"name", bound.names[i]}, {"data_type", CompilerType(bound.types[i])}, {"nullable", true}});
        }
        table["columns"] = std::move(columns);
    }
    return request;
}
Json CompileRequest(ClientContext &context, TableFunctionBindInput &input, bool inspect = false) {
    return Bridge({{"op", "compile_request"}, {"request", BoundRequest(context, input)}, {"inspect", inspect}});
}
unique_ptr<TableRef> QueryBind(ClientContext &context, TableFunctionBindInput &input) {
    return Subquery(context, CompileRequest(context, input).at("sql").get<string>());
}
unique_ptr<TableRef> CompileBind(ClientContext &context, TableFunctionBindInput &input) {
    return Subquery(context, "SELECT " + Literal(CompileRequest(context, input, true).dump()) + " AS compilation");
}
unique_ptr<TableRef> SparqlSyntaxBind(ClientContext &context, TableFunctionBindInput &input) {
    Bridge({{"op", "sparql_syntax"}, {"query", input.inputs[0].GetValue<string>()},
            {"base", input.inputs[1].IsNull() ? Json() : Json(input.inputs[1].GetValue<string>())},
            {"update", input.inputs[2].GetValue<bool>()}});
    return Subquery(context, "SELECT true AS parsed");
}

struct NativeClock : ClientContextState {
    timestamp_t statement = Timestamp::GetCurrentTimestamp();
    void QueryBegin(ClientContext &) override { statement = Timestamp::GetCurrentTimestamp(); }
};
#include "program.hpp"

void NativeScalar(DataChunk &args, ExpressionState &state, Vector &result, bool predicate, bool json) {
    auto &context=state.GetContext();
    auto clock=context.registered_state->GetOrCreate<NativeClock>("orchid_native_clock");
    auto rows = Json::array();
    for (idx_t row = 0; row < args.size(); row++) {
        auto cells = Json::array();
        for (idx_t col = 0; col < args.ColumnCount(); col++) { cells.push_back(Parameter(args.GetValue(col, row), true)); }
        rows.push_back(std::move(cells));
    }
    auto response = Bridge({{"op", "native_scalar"}, {"rows", rows}, {"predicate", predicate}, {"json", json}, {"statement_micros",clock->statement.value}, {"transaction_micros",MetaTransaction::Get(context).start_timestamp.value}});
    if (response.contains("execution_error")) {
        unordered_map<string,string> details;
        if (!response.at("classification").is_null()) {details["orchid_classification"]=response.at("classification").dump();}
        throw InvalidInputException(details,response.at("execution_error").get<string>());
    }
    auto values=response.at("values");
    result.SetVectorType(VectorType::FLAT_VECTOR);
    for (idx_t row = 0; row < args.size(); row++) {
        if (json) { result.SetValue(row, Value(values[row].dump())); }
        else if (values[row].is_null()) { result.SetValue(row, Value(result.GetType())); }
        else if (predicate) { result.SetValue(row, Value(values[row].get<bool>())); }
        else { result.SetValue(row, Value::STRUCT({{"__orchiddb_value_v1", Value(values[row].at("__orchiddb_value_v1").get<string>())}})); }
    }
}
void NativeValue(DataChunk &args, ExpressionState &state, Vector &result) { NativeScalar(args,state,result,false,false); }
void NativePredicate(DataChunk &args, ExpressionState &state, Vector &result) { NativeScalar(args,state,result,true,false); }
void NativeJson(DataChunk &args, ExpressionState &state, Vector &result) { NativeScalar(args,state,result,false,true); }
void NativeKey(DataChunk &args, ExpressionState &, Vector &result) {
    auto rows=Json::array();
    for (idx_t row=0;row<args.size();row++) { rows.push_back({Parameter(args.GetValue(0,row),true),Parameter(args.GetValue(1,row))}); }
    auto values=Bridge({{"op","native_key"},{"rows",rows}}).at("values");
    result.SetVectorType(VectorType::FLAT_VECTOR);
    for (idx_t row=0;row<args.size();row++) { result.SetValue(row,Value(values[row].get<string>())); }
}

void NativeCollection(DataChunk &args, ExpressionState &state, Vector &result, bool aggregate, bool procedure = false, bool project = false) {
    auto rows=Json::array();
    for (idx_t row=0;row<args.size();row++) {
        auto cells=Json::array();
        for (idx_t col=0;col<args.ColumnCount();col++) {cells.push_back(Parameter(args.GetValue(col,row),true));}
        rows.push_back(std::move(cells));
    }
    auto &context=state.GetContext();
    auto clock=context.registered_state->GetOrCreate<NativeClock>("orchid_native_clock");
    auto response=Bridge({{"op","native_collection"},{"rows",rows},{"aggregate",aggregate},{"procedure",procedure},{"project",project},
        {"statement_micros",clock->statement.value},{"transaction_micros",MetaTransaction::Get(context).start_timestamp.value}});
    if(response.contains("execution_error")) {
        unordered_map<string,string> details;
        if(!response.at("classification").is_null()){details["orchid_classification"]=response.at("classification").dump();}
        throw InvalidInputException(details,response.at("execution_error").get<string>());
    }
    auto values=response.at("values");
    auto type=LogicalType::STRUCT({{"__orchiddb_value_v1",LogicalType::VARCHAR}});
    auto value=[&](const Json &item) {return item.is_null()?Value(type):Value::STRUCT({{"__orchiddb_value_v1",Value(item.at("__orchiddb_value_v1").get<string>())}});};
    result.SetVectorType(VectorType::FLAT_VECTOR);
    for (idx_t row=0;row<args.size();row++) {
        if (aggregate) {result.SetValue(row,value(values[row]));}
        else {
            vector<Value> items;
            for (auto &item:values[row]) {items.push_back(value(item));}
            result.SetValue(row,Value::LIST(type,std::move(items)));
        }
    }
}
void NativeProcedure(DataChunk &args,ExpressionState &state,Vector &result){NativeCollection(args,state,result,false,true);}
void NativeProject(DataChunk &args,ExpressionState &state,Vector &result){NativeCollection(args,state,result,false,false,true);}
void NativeItems(DataChunk &args, ExpressionState &state, Vector &result) {NativeCollection(args,state,result,false);}
void NativeAggregate(DataChunk &args, ExpressionState &state, Vector &result) {NativeCollection(args,state,result,true);}

unique_ptr<FunctionData> NativeSortBind(ClientContext &, ScalarFunction &function, vector<unique_ptr<Expression>> &arguments) {
    if (arguments[1]->return_type.id()!=LogicalTypeId::LIST) {throw BinderException("Native sort expects a list of rows");}
    function.return_type=arguments[1]->return_type;
    return nullptr;
}
void NativeSort(DataChunk &args, ExpressionState &, Vector &result) {
    auto rows=Json::array();
    for (idx_t row=0;row<args.size();row++) {rows.push_back({Parameter(args.GetValue(0,row)),Parameter(args.GetValue(1,row),true)});}
    auto permutations=Bridge({{"op","native_sort"},{"rows",rows}}).at("values");
    result.SetVectorType(VectorType::FLAT_VECTOR);
    for (idx_t row=0;row<args.size();row++) {
        auto source=args.GetValue(1,row);
        vector<Value> sorted;
        if (!source.IsNull()) {
            const auto &children=ListValue::GetChildren(source);
            for (auto &index:permutations[row]) {sorted.push_back(children.at(index.get<idx_t>()));}
        }
        result.SetValue(row,Value::LIST(ListType::GetChildType(result.GetType()),std::move(sorted)));
    }
}

void SparqlScalar(DataChunk &args, ExpressionState &, Vector &result) {
    auto rows = Json::array();
    for (idx_t row = 0; row < args.size(); row++) {
        auto cells = Json::array();
        for (idx_t col = 0; col < args.ColumnCount(); col++) {
            auto value = args.GetValue(col, row);
            cells.push_back(value.IsNull() ? Json() : Json(value.GetValue<string>()));
        }
        rows.push_back(std::move(cells));
    }
    auto values = Bridge({{"op", "sparql_scalar"}, {"rows", rows}}).at("values");
    if (values.size() != args.size()) { throw InternalException("Invalid SPARQL scalar batch size"); }
    result.SetVectorType(VectorType::FLAT_VECTOR);
    for (idx_t row = 0; row < args.size(); row++) {
        result.SetValue(row, values[row].is_null() ? Value(LogicalType::VARCHAR) : Value(values[row].get<string>()));
    }
}

unique_ptr<TableRef> InfoBind(ClientContext &context, TableFunctionBindInput &input) {
    auto graph = LoadGraph(context, input.inputs[0].GetValue<string>());
    if (graph.definition.contains("managed_table")) {
        return Subquery(context,"SELECT 'managed' AS kind,"+Literal(graph.definition.at("managed_table").get<string>())+" AS source");
    }
    string sql;
    idx_t i = 0;
    for (auto group : {"vertices", "edges"}) {
        for (const auto &e : graph.definition.at(group)) {
            if (!sql.empty()) { sql += " UNION ALL "; }
            sql += "SELECT " + Literal(group) + " AS kind, " + Literal(e.at("label").get<string>()) + " AS label, " +
                   Literal(graph.sources[i++]) + " AS source, " + Literal(e.at("key").dump()) + " AS key_columns, " +
                   Literal(e.at("from").dump()) + " AS source_endpoint, " + Literal(e.at("to").dump()) + " AS destination_endpoint";
        }
    }
    if (graph.definition.contains("computed_relationships")) {
        for (const auto &e : graph.definition.at("computed_relationships")) {
            if (!sql.empty()) { sql += " UNION ALL "; }
            sql += "SELECT 'computed_edges' AS kind, " + Literal(e.at("name").get<string>()) +
                   " AS label, NULL::VARCHAR AS source, NULL::VARCHAR AS key_columns, " +
                   Literal(e.at("source").get<string>()) + " AS source_endpoint, " +
                   Literal(e.at("target").get<string>()) + " AS destination_endpoint";
        }
    }
    return Subquery(context, sql);
}

// Rewrite only Orchid statements. Let DuckDB and other parser extensions parse
// all ordinary SQL, including Lance DDL. The guard prevents recursive rewriting.
thread_local bool rewriting = false;
ParserOverrideResult Rewrite(ParserExtensionInfo *, const string &query, ParserOptions &options) {
    if (rewriting) { return ParserOverrideResult(); }
    auto upper = StringUtil::Upper(query);
    if (!StringUtil::Contains(upper, "CYPHER") && !StringUtil::Contains(upper, "GREMLIN") &&
        !(StringUtil::Contains(upper, "PROPERTY") && StringUtil::Contains(upper, "GRAPH"))) {
        return ParserOverrideResult();
    }
    struct Guard { Guard() { rewriting = true; } ~Guard() { rewriting = false; } } guard;
    try {
        auto result = Bridge({{"op", "rewrite"}, {"sql", query}});
        if (result.at("sql").is_null()) { return ParserOverrideResult(); }
        Parser parser(options);
        parser.ParseQuery(result.at("sql").get<string>());
        return ParserOverrideResult(std::move(parser.statements));
    } catch (std::exception &error) {
        return ParserOverrideResult(error);
    }
}
} // namespace

void LoadOrchid(ExtensionLoader &loader) {
    for (auto operation : {"create","reset","snapshot","import"}) {
        vector<LogicalType> arguments={LogicalType::VARCHAR};
        if (string(operation)=="import") {arguments.push_back(LogicalType::VARCHAR);}
        loader.RegisterFunction(TableFunction("orchid_graph_"+string(operation),arguments,ManagedExecute,ManagedBind,ProgramInit));
    }
    TableFunction kernel("__orchid_kernel", {LogicalType::UBIGINT,LogicalType::UBIGINT}, nullptr, nullptr);
    kernel.bind_operator=KernelBindOperator;
    loader.RegisterFunction(kernel);
    TableFunction native_program("orchid_program", {LogicalType::VARCHAR}, nullptr, nullptr);
    native_program.bind_operator=NativeProgramBind;
    loader.RegisterFunction(native_program);
    loader.RegisterFunction(TableFunction("__orchid_host_relation", {LogicalType::UBIGINT}, HostRelationScan, HostRelationBind, HostRelationInit));
    TableFunction program("orchid_sparql_update", {LogicalType::VARCHAR}, ProgramExecute, ProgramBind, ProgramInit);
    program.named_parameters["base"] = LogicalType::VARCHAR;
    loader.RegisterFunction(program);
    auto native_type = LogicalType::STRUCT({{"__orchiddb_value_v1", LogicalType::VARCHAR}});
    for (auto function : {
        ScalarFunction("__orchiddb_value", {LogicalType::VARCHAR, LogicalType::ANY}, native_type, NativeValue),
        ScalarFunction("__orchiddb_predicate", {LogicalType::VARCHAR, LogicalType::ANY}, LogicalType::BOOLEAN, NativePredicate),
        ScalarFunction("__orchiddb_value_json", {LogicalType::ANY}, LogicalType::VARCHAR, NativeJson),
        ScalarFunction("__orchiddb_value_key", {LogicalType::ANY, LogicalType::BOOLEAN}, LogicalType::VARCHAR, NativeKey),
        ScalarFunction("__orchiddb_value_procedure", {LogicalType::VARCHAR,LogicalType::ANY},LogicalType::LIST(native_type),NativeProcedure),
        ScalarFunction("__orchiddb_value_project", {LogicalType::VARCHAR,LogicalType::ANY},LogicalType::LIST(native_type),NativeProject),
        ScalarFunction("__orchiddb_value_items", {LogicalType::ANY}, LogicalType::LIST(native_type), NativeItems),
        ScalarFunction("__orchiddb_value_aggregate", {LogicalType::VARCHAR,LogicalType::ANY},native_type,NativeAggregate)}) {
        function.null_handling = FunctionNullHandling::SPECIAL_HANDLING;
        function.SetStability(FunctionStability::VOLATILE);
        loader.RegisterFunction(function);
    }
    ScalarFunction native_sort("__orchiddb_value_sort",{LogicalType::VARCHAR,LogicalType::ANY},LogicalType::ANY,NativeSort,NativeSortBind);
    native_sort.null_handling=FunctionNullHandling::SPECIAL_HANDLING;
    loader.RegisterFunction(native_sort);
    ScalarFunction sparql_scalar("__orchiddb_sparql_scalar", vector<LogicalType>(5, LogicalType::VARCHAR),
                                 LogicalType::VARCHAR, SparqlScalar);
    sparql_scalar.null_handling = FunctionNullHandling::SPECIAL_HANDLING;
    loader.RegisterFunction(sparql_scalar);
    TableFunction definition("orchid_graph_definition", {LogicalType::VARCHAR}, nullptr, nullptr);
    definition.bind_replace = DefinitionBind;
    loader.RegisterFunction(definition);
    TableFunction cypher("orchid_cypher", {LogicalType::VARCHAR, LogicalType::VARCHAR}, nullptr, nullptr);
    cypher.named_parameters["parameters"] = LogicalType::ANY;
    cypher.bind_replace = CypherBind;
    loader.RegisterFunction(cypher);
    TableFunction gremlin("orchid_gremlin", {LogicalType::VARCHAR, LogicalType::VARCHAR}, nullptr, nullptr);
    gremlin.bind_replace = GremlinBind;
    loader.RegisterFunction(gremlin);
    TableFunction query("orchid_query", {LogicalType::VARCHAR}, nullptr, nullptr);
    query.bind_replace = QueryBind;
    loader.RegisterFunction(query);
    TableFunction compile("orchid_compile", {LogicalType::VARCHAR}, nullptr, nullptr);
    compile.bind_replace = CompileBind;
    loader.RegisterFunction(compile);
    TableFunction syntax("orchid_sparql_syntax", {LogicalType::VARCHAR, LogicalType::VARCHAR, LogicalType::BOOLEAN}, nullptr, nullptr);
    syntax.bind_replace = SparqlSyntaxBind;
    loader.RegisterFunction(syntax);
    TableFunction info("orchid_graph_info", {LogicalType::VARCHAR}, nullptr, nullptr);
    info.bind_replace = InfoBind;
    loader.RegisterFunction(info);
    ParserExtension parser;
    parser.parser_override = Rewrite;
    auto &config = DBConfig::GetConfig(loader.GetDatabaseInstance());
    ParserExtension::Register(config, std::move(parser));
    config.SetOptionByName("allow_parser_override_extension", Value("fallback"));
}
} // namespace duckdb

extern "C" {
DUCKDB_CPP_EXTENSION_ENTRY(orchid, loader) { duckdb::LoadOrchid(loader); }
}
