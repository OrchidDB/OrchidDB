#include "../examples/duckdb_engine.hpp"
#include <iostream>
#include <cstring>
#define REQUIRE(x) do { if(!(x)) throw std::runtime_error("failed: " #x); } while(false)
using orchiddb::Json;
void sql(duckdb_connection connection,const std::string& query) {
  duckdb_result result{};
  const auto status=duckdb_query(connection,query.c_str(),&result);
  const std::string error=status==DuckDBSuccess?"":duckdb_result_error(&result);
  duckdb_destroy_result(&result);
  if(status!=DuckDBSuccess) throw std::runtime_error(error);
}
struct Database {
  duckdb_database database{};duckdb_connection connection{};
  Database() {REQUIRE(duckdb_open(nullptr,&database)==DuckDBSuccess);REQUIRE(duckdb_connect(database,&connection)==DuckDBSuccess);}
  ~Database() {duckdb_disconnect(&connection);duckdb_close(&database);}
};
bool valid(const ArrowArray* a,int64_t row) {
  row+=a->offset;
  auto bitmap=static_cast<const uint8_t*>(a->buffers[0]);
  return !bitmap || ((bitmap[row/8]>>(row%8))&1);
}
Json cell(const ArrowSchema* schema,const ArrowArray* a,int64_t row) {
  if(!valid(a,row)) return nullptr;
  const auto i=row+a->offset;
  const std::string format=schema->format;
  if(format=="l") return static_cast<const int64_t*>(a->buffers[1])[i];
  if(format=="i") return static_cast<const int32_t*>(a->buffers[1])[i];
  if(format=="L") return static_cast<const uint64_t*>(a->buffers[1])[i];
  if(format=="g") return static_cast<const double*>(a->buffers[1])[i];
  if(format=="f") return static_cast<const float*>(a->buffers[1])[i];
  if(format=="b") return bool((static_cast<const uint8_t*>(a->buffers[1])[i/8]>>(i%8))&1);
  if(format=="u") {const auto* offsets=static_cast<const int32_t*>(a->buffers[1]);return std::string(static_cast<const char*>(a->buffers[2])+offsets[i],offsets[i+1]-offsets[i]);}
  if(format=="U") {const auto* offsets=static_cast<const int64_t*>(a->buffers[1]);return std::string(static_cast<const char*>(a->buffers[2])+offsets[i],offsets[i+1]-offsets[i]);}
  throw std::runtime_error("Unsupported test Arrow type: "+format);
}
Json rows(orchiddb::ArrowResult& result) {
  auto schema=result.schema();Json values=Json::array();
  while(auto batch=result.next()) {
    for(int64_t row=0;row<batch.get()->length;++row) {
      Json value=Json::array();
      for(int64_t col=0;col<schema.get()->n_children;++col) value.push_back(cell(schema.get()->children[col],batch.get()->children[col],row));
      values.push_back(std::move(value));
    }
  }
  return values;
}
void dependent_sql(const orchiddb::Compiler& compiler) {
  Database database;DuckDBEngine local(database.connection);
  auto plan=Json::parse(R"JSON({"version":1,"dialect":"duckdb","execution_engine":"local","fields":["value"],
    "sql":"SELECT value FROM stage2 ORDER BY value","transfers":[
      {"source_engine":"local","source_dialect":"duckdb","sql":"SELECT value AS seed FROM (VALUES (2::BIGINT),(5::BIGINT)) t(value)",
       "target_relation":"stage1","columns":[{"name":"value","data_type":"int64","nullable":false}],
       "operation":{"engine":"local","input_columns":[{"name":"seed","data_type":"int64","nullable":false}],
        "template":{"dialect":"duckdb","parameters":1,"sql":"SELECT CAST($1 + 1 AS BIGINT) AS value"}}},
      {"source_engine":"local","source_dialect":"duckdb","sql":"SELECT value * 2 AS value FROM stage1", "target_relation":"stage2",
       "columns":[{"name":"value","data_type":"int64","nullable":false}]}
    ]})JSON");
  orchiddb::execute_federated(compiler,orchiddb::Compiler::from_plan(plan),{{"local",&local}},[](orchiddb::ArrowResult& result) {REQUIRE(rows(result)==Json::parse("[[6],[12]]"));});
  plan["transfers"][0]["sql"]="SELECT 2::BIGINT AS seed WHERE FALSE";
  orchiddb::execute_federated(compiler,orchiddb::Compiler::from_plan(plan),{{"local",&local}},[](orchiddb::ArrowResult& result) {REQUIRE(rows(result).empty());});
  sql(database.connection,"SELECT 1");
}
int main() {
  orchiddb::Compiler compiler;
  dependent_sql(compiler);
  const auto* fixture=std::getenv("ORCHIDDB_REMOTE_FIXTURE");
  if(!fixture) {std::cout<<"Dependent SQL passed; live HTTP fixture not supplied\n";return 0;}
  std::ifstream file(fixture);
  const auto cases=file?Json::parse(file):Json::parse(fixture);
  for(const auto& test:cases.at("cases")) {
    Database database;DuckDBEngine local(database.connection);
    if(test.at("setup_sql").is_array()) for(const auto& statement:test.at("setup_sql")) sql(database.connection,statement);
    else sql(database.connection,test.at("setup_sql"));
    auto remote=[&] {
      orchiddb::Compiler scoped_compiler;
      orchiddb::RemoteEngine opened(scoped_compiler,test.at("adapter"),{{"endpoint",test.at("endpoint")},{"page_size",1},{"batch_size",2}});
      return orchiddb::RemoteEngine(std::move(opened));
    }(); // The moved session retains its compiler after the original scope ends.

    std::map<std::string,orchiddb::ExecutionEngine*> engines;
    for(auto it=test.at("request").at("engines").begin();it!=test.at("request").at("engines").end();++it)
      engines[it.key()]=it.value().at("dialect")=="duckdb"?static_cast<orchiddb::ExecutionEngine*>(&local):static_cast<orchiddb::ExecutionEngine*>(&remote);
    auto consume=[&](orchiddb::ArrowResult& result){const auto actual=rows(result);if(actual!=test.at("expected_rows"))throw std::runtime_error("Unexpected rows: "+actual.dump()+" expected "+test.at("expected_rows").dump());};
    orchiddb::query_federated(compiler,test.at("request"),engines,consume);
    remote.clear_metadata_cache();
    bool malformed_failed=false;
    try {remote.execute_requests(Json::array({Json{{"invalid",true}}}),Json::array());}
    catch(const std::exception&) {malformed_failed=true;}
    REQUIRE(malformed_failed);
    bool failed=false;
    try {orchiddb::query_federated(compiler,test.at("request"),engines,[](orchiddb::ArrowResult&){throw std::runtime_error("consumer failed");});}
    catch(const std::runtime_error& error) {failed=std::string(error.what())=="consumer failed";}
    REQUIRE(failed);
    orchiddb::query_federated(compiler,test.at("request"),engines,consume);
    remote.close();remote.close();
    failed=false;try {remote.execute_requests(Json::array(),Json::array());}catch(const std::runtime_error&){failed=true;}REQUIRE(failed);
    sql(database.connection,"SELECT 1");
  }
  std::cout<<"Dependent SQL, live HTTP federation, lifecycle, and consumer failures passed\n";
}
