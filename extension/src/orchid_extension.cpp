#define DUCKDB_EXTENSION_MAIN
#include "duckdb.hpp"
#include "duckdb/common/types/blob.hpp"
#include "duckdb/common/vector_operations/vector_operations.hpp"
#include "duckdb/main/client_context.hpp"
#include "duckdb/main/client_context_state.hpp"
#include "duckdb/main/extension/extension_loader.hpp"
#include "duckdb/planner/expression/bound_function_expression.hpp"
#include "duckdb/function/scalar_function.hpp"
#include "duckdb/common/arrow/arrow_converter.hpp"
#include "duckdb/function/table/arrow.hpp"
#include "duckdb/transaction/meta_transaction.hpp"
#include "json.hpp"
#include <cmath>
#include <mutex>
extern "C" char *orchid_bridge(const char *);
extern "C" char *orchid_cypher_scalar_v1(const char *, ArrowArray *, const ArrowSchema *, ArrowArrayStream *);
extern "C" int orchid_cypher_to_float_v1(const uint8_t **, const size_t *, size_t, double *, uint8_t *);
extern "C" void orchid_bridge_free(char *);
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

Json Parameter(const Value &value, bool native = false) {
    if (value.IsNull()) { return nullptr; }
    if (value.type().IsJSONType()) { return Json::parse(value.GetValue<string>()); }
    if (native && (value.type().id()==LogicalTypeId::TINYINT || value.type().id()==LogicalTypeId::SMALLINT || value.type().id()==LogicalTypeId::INTEGER || value.type().id()==LogicalTypeId::BIGINT || value.type().id()==LogicalTypeId::HUGEINT || value.type().id()==LogicalTypeId::DECIMAL || value.type().id()==LogicalTypeId::FLOAT || value.type().id()==LogicalTypeId::DOUBLE)) {
        return {{"__orchiddb_number",Json::array({value.type().ToString(),value.ToString()})}};
    }
    if (native && value.type().id() == LogicalTypeId::DATE) {
        return {{"__orchiddb_temporal", Json::array({"date", value.ToString()})}};
    }
    if (native) {
        int64_t ticks;
        int64_t ticks_per_second = 1000000;
        bool zoned = false;
        bool timestamp = true;
        switch (value.type().id()) {
        case LogicalTypeId::TIMESTAMP: ticks = value.GetValue<timestamp_t>().value; break;
        case LogicalTypeId::TIMESTAMP_TZ: ticks = value.GetValue<timestamp_tz_t>().value; zoned = true; break;
        case LogicalTypeId::TIMESTAMP_SEC: ticks = value.GetValue<timestamp_sec_t>().value; ticks_per_second = 1; break;
        case LogicalTypeId::TIMESTAMP_MS: ticks = value.GetValue<timestamp_ms_t>().value; ticks_per_second = 1000; break;
        case LogicalTypeId::TIMESTAMP_NS: ticks = value.GetValue<timestamp_ns_t>().value; ticks_per_second = 1000000000; break;
        default: timestamp = false;
        }
        if (timestamp) { return {{"__orchiddb_timestamp", Json::array({ticks, ticks_per_second, zoned})}}; }
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

struct NativeClock : ClientContextState {
    timestamp_t statement = Timestamp::GetCurrentTimestamp();
    void QueryBegin(ClientContext &) override { statement = Timestamp::GetCurrentTimestamp(); }
};
#include "scalar_arrow.hpp"

struct TypedScalarInfo : ScalarFunctionInfo {
    string operation;
    explicit TypedScalarInfo(string operation) : operation(std::move(operation)) {}
};
void TypedCypherScalar(DataChunk &args, ExpressionState &state, Vector &result) {
    auto &context=state.GetContext();
    auto &expression=state.expr.Cast<BoundFunctionExpression>();
    auto &info=static_cast<TypedScalarInfo &>(*expression.function.function_info);
    auto properties=context.GetClientProperties();
    vector<string> names;
    for(idx_t column=0;column<args.ColumnCount();column++) {names.push_back("arg"+to_string(column));}
    ArrowSchemaWrapper schema;
    ArrowArrayWrapper input;
    ArrowConverter::ToArrowSchema(&schema.arrow_schema,args.GetTypes(),names,properties);
    ArrowConverter::ToArrowArray(args,&input.arrow_array,properties,{});
    ArrowArrayStreamWrapper output;
    std::unique_ptr<char,decltype(&orchid_bridge_free)> error(
        orchid_cypher_scalar_v1(info.operation.c_str(),&input.arrow_array,&schema.arrow_schema,&output.arrow_array_stream),orchid_bridge_free);
    if(error) {
        auto detail=Json::parse(error.get(),nullptr,false);
        auto message=detail.is_object() && detail.contains("error") && detail.at("error").is_string()
            ? detail.at("error").get<string>() : string(error.get());
        throw InvalidInputException(HostErrorMetadata(error.get()),message);
    }
    auto relation=ImportHostRelation(context,output.arrow_array_stream);
    HostRelationCursor cursor(relation->source);
    DataChunk chunk;
    chunk.Initialize(context,{result.GetType()});
    if(!cursor.Next(context,chunk) || chunk.size()!=args.size()) {throw InternalException("Typed scalar batch cardinality mismatch");}
    // Copy into DuckDB-owned buffers before releasing the incoming Arrow batch.
    VectorOperations::Copy(chunk.data[0],result,args.size(),0,0);
}

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

// SQL grouping keys use the same Cypher equivalence kernel as native values.
// Keep SQL NULL as NULL so COUNT(DISTINCT ...) excludes it.
void CypherKey(DataChunk &args, ExpressionState &, Vector &result) {
    auto rows = Json::array();
    for (idx_t row = 0; row < args.size(); row++) {
        rows.push_back({Parameter(args.GetValue(0, row), true), true});
    }
    auto values = Bridge({{"op", "native_key"}, {"rows", rows}}).at("values");
    result.SetVectorType(VectorType::FLAT_VECTOR);
    for (idx_t row = 0; row < args.size(); row++) {
        if (args.GetValue(0, row).IsNull()) { result.SetValue(row, Value(LogicalType::BLOB)); }
        else { result.SetValue(row, Value::BLOB_RAW(Blob::FromBase64(string_t(values[row].get<string>())))); }
    }
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

// Immutable, null-preserving, typed batch compatibility kernel. Unlike the
// generic native-value wrappers this never serializes a row or invokes a planner.
void CypherToFloat(DataChunk &args, ExpressionState &, Vector &result) {
    UnifiedVectorFormat input;
    args.data[0].ToUnifiedFormat(args.size(), input);
    auto strings = UnifiedVectorFormat::GetData<string_t>(input);
    vector<const uint8_t *> pointers(args.size());
    vector<size_t> lengths(args.size());
    vector<uint8_t> valid(args.size());
    for (idx_t row = 0; row < args.size(); row++) {
        auto index = input.sel->get_index(row);
        if (input.validity.RowIsValid(index)) {
            pointers[row] = reinterpret_cast<const uint8_t *>(strings[index].GetData());
            lengths[row] = strings[index].GetSize();
        } else { pointers[row] = nullptr; lengths[row] = 0; }
    }
    result.SetVectorType(VectorType::FLAT_VECTOR);
    auto values = FlatVector::GetData<double>(result);
    if (orchid_cypher_to_float_v1(pointers.data(), lengths.data(), args.size(), values, valid.data()) != 0) {
        throw InvalidInputException("Orchid string-to-float batch conversion failed");
    }
    auto &validity = FlatVector::Validity(result);
    for (idx_t row = 0; row < args.size(); row++) { validity.Set(row, valid[row] != 0); }
}

}
void LoadOrchid(ExtensionLoader &loader) {
    auto native_type = LogicalType::STRUCT({{"__orchiddb_value_v1", LogicalType::VARCHAR}});
    for(auto &entry:Bridge({{"op","typed_scalar_catalog"}})) {
        auto kind=entry.at("output").get<string>();
        auto output=kind=="boolean" ? LogicalType::BOOLEAN : kind=="integer" ? LogicalType::BIGINT :
            kind=="indices" ? LogicalType::LIST(LogicalType::BIGINT) : kind=="text" ? LogicalType::VARCHAR : kind=="list" ? LogicalType::LIST(native_type) : native_type;
        ScalarFunction function(entry.at("name").get<string>(),{},output,TypedCypherScalar);
        function.varargs=LogicalType::ANY;
        function.null_handling=FunctionNullHandling::SPECIAL_HANDLING;
        function.function_info=make_shared_ptr<TypedScalarInfo>(entry.at("operation").get<string>());
        loader.RegisterFunction(function);
    }

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
    ScalarFunction cypher_key("__orchiddb_lang_cypher_key", {LogicalType::ANY}, LogicalType::BLOB, CypherKey);
    cypher_key.null_handling = FunctionNullHandling::SPECIAL_HANDLING;
    cypher_key.SetStability(FunctionStability::CONSISTENT);
    loader.RegisterFunction(cypher_key);
    ScalarFunction cypher_float("__orchiddb_cypher_to_float_v1", {LogicalType::VARCHAR}, LogicalType::DOUBLE, CypherToFloat);
    cypher_float.null_handling = FunctionNullHandling::SPECIAL_HANDLING;
    cypher_float.SetStability(FunctionStability::CONSISTENT);
    loader.RegisterFunction(cypher_float);
}
}
extern "C" {
DUCKDB_CPP_EXTENSION_ENTRY(orchid, loader) { duckdb::LoadOrchid(loader); }
}
