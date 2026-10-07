#include "../examples/duckdb_engine.hpp"
#include <iostream>
#define REQUIRE(x) do { if(!(x)) throw std::runtime_error("failed: " #x); } while(false)
void sql(duckdb_connection c,const char* q) {
  duckdb_result result{};
  const auto status=duckdb_query(c,q,&result);
  std::string error=status==DuckDBSuccess?"":duckdb_result_error(&result);
  duckdb_destroy_result(&result);
  if(status!=DuckDBSuccess) throw std::runtime_error(error);
}
class ExchangeEngine : public orchiddb::ExecutionEngine {
  duckdb_connection connection_;
  DuckDBEngine delegate_;
public:
  explicit ExchangeEngine(duckdb_connection c) : connection_(c), delegate_(c) {}
  std::string id() const override { return "exchange"; }
  std::string dialect() const override { return "duckdb"; }
  orchiddb::ArrowResult execute(const orchiddb::CompiledQuery& q) override { return delegate_.execute(q); }

};
int main() {
  duckdb_database db{}; duckdb_connection connection{};
  REQUIRE(duckdb_open(nullptr,&db)==DuckDBSuccess);
  REQUIRE(duckdb_connect(db,&connection)==DuckDBSuccess);
  try {
    sql(connection,"CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (9007199254740993,'Orchid'),(2,NULL); BEGIN; INSERT INTO people VALUES (3,'transaction')");
    auto request=orchiddb::Json::parse(R"({"version":1,"dialect":"duckdb","language":"cypher","query":"MATCH (p:Person) RETURN p.id AS id, p.name AS name ORDER BY p.id","tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],"nodes":[{"label":"Person","table":"people","id":"id","properties":{"id":"id","name":"name"}}]})");
    orchiddb::Compiler compiler; DuckDBEngine engine(connection);
    auto result=orchiddb::query(compiler,request,engine);
    auto schema=result.schema(); REQUIRE(schema.get()->n_children==2);
    REQUIRE(std::string(schema.get()->children[0]->format)=="l");
    auto batch=result.next(); REQUIRE(batch.get()->length==3);
    const auto* ids=static_cast<const int64_t*>(batch.get()->children[0]->buffers[1]);
    REQUIRE(ids[0]==2 && ids[1]==3 && ids[2]==9007199254740993LL);
    REQUIRE(batch.get()->children[1]->null_count==1);
    REQUIRE(!result.next());
    result.close(); result.close();
    // C Data ownership survives closing the result stream.
    REQUIRE(ids[2]==9007199254740993LL);
    sql(connection,"ROLLBACK");
    request["query"]="MATCH (p:Person) RETURN count(p) AS n";
    auto count=orchiddb::query(compiler,request,engine);
    auto count_batch=count.next();
    REQUIRE(static_cast<const int64_t*>(count_batch.get()->children[0]->buffers[1])[0]==2);
    count.close();
    // A second real connection receives an aggregate island result.
    duckdb_connection target_connection{};
    REQUIRE(duckdb_connect(db, &target_connection) == DuckDBSuccess);
    {
      ExchangeEngine target(target_connection);
      auto routed = request;
      routed["engines"] = {{"source", {{"dialect", "duckdb"}}}, {"target", {{"dialect", "duckdb"}}}};
      routed["execution_engine"] = "target";
      routed["tables"][0]["engine"] = "source";
      orchiddb::query_federated(compiler, routed, {{"source", &engine}, {"target", &target}}, [](orchiddb::ArrowResult& r) {
        auto b = r.next();
        REQUIRE(b.get()->length == 1);
        REQUIRE(static_cast<const int64_t*>(b.get()->children[0]->buffers[1])[0] == 2);
      });
    }
    duckdb_disconnect(&target_connection);
    orchiddb::Statistics statistics(compiler);
    orchiddb::generate_statistics(statistics, request, engine);
    REQUIRE(!statistics.snapshot().is_null());
    REQUIRE(!statistics.report().is_null());
    REQUIRE(statistics.snapshot().at("sources").at("people").at("sample_rows") == 2);
    REQUIRE(statistics.report().at("skipped").empty());
    auto estimated = statistics.compile(request);
    REQUIRE(estimated.diagnostics.contains("statistics_usage"));
    REQUIRE(estimated.diagnostics.contains("plan_estimates"));
    auto snapshot = statistics.snapshot();
    statistics.clear();
    statistics.install(snapshot);
    REQUIRE(statistics.compile(request).sql == estimated.sql);
    auto measured_count = orchiddb::query(statistics, request, engine);
    auto measured_batch = measured_count.next();
    REQUIRE(static_cast<const int64_t*>(measured_batch.get()->children[0]->buffers[1])[0]==2);
    measured_count.close();
    statistics.generate(request, [&](orchiddb::Json work) {
      if (work.at("kind") != "metadata") work["max_bytes"] = 1;
      return engine.collect_statistics(work);
    });
    REQUIRE(statistics.snapshot().at("sources").at("people").at("sample_rows") == 0);
    REQUIRE(statistics.snapshot().at("sources").at("people").at("method") != "complete bounded read");
    REQUIRE(statistics.report().at("complete") == false);
    statistics.clear();
    sql(connection,"CREATE TABLE documents(id BIGINT, project_id BIGINT, title VARCHAR); INSERT INTO documents VALUES (1,10,'direct'),(2,20,'project'),(3,30,'denied'); CREATE TABLE effective_grants(resource_type VARCHAR, resource_rel VARCHAR, resource_id VARCHAR, subject_type VARCHAR, subject_rel VARCHAR, subject_id VARCHAR); INSERT INTO effective_grants VALUES ('document','view','1','user','','alice'),('project','view','20','user','','alice'),('project','view','30','user','','bob')");
    auto direct=orchiddb::permission_relation("effective_grants","document","view");
    auto project=orchiddb::permission_relation("effective_grants","project","view");
    auto protected_request=orchiddb::Json::parse(R"({"version":1,"dialect":"duckdb","language":"cypher","query":"MATCH (d:Document) RETURN d.id AS id ORDER BY id","tables":[{"name":"documents","columns":[{"name":"id","data_type":"int64"},{"name":"project_id","data_type":"int64"},{"name":"title","data_type":"string"}]},{"name":"effective_grants","columns":[{"name":"resource_type","data_type":"string"},{"name":"resource_rel","data_type":"string"},{"name":"resource_id","data_type":"string"},{"name":"subject_type","data_type":"string"},{"name":"subject_rel","data_type":"string"},{"name":"subject_id","data_type":"string"}]}],"nodes":[{"label":"Document","table":"documents","id":"id","properties":{"id":"id","project_id":"project_id"}}]})");
    protected_request["authorization"]=orchiddb::authorization("user","alice");
    protected_request["nodes"][0]["permission_scopes"]={
      orchiddb::permission_scope("id",direct),
      orchiddb::permission_scope("project_id",project)
    };
    auto protected_result=orchiddb::query(compiler,protected_request,engine);
    auto protected_batch=protected_result.next(); REQUIRE(protected_batch.get()->length==2);
    const auto* protected_ids=static_cast<const int64_t*>(protected_batch.get()->children[0]->buffers[1]);
    REQUIRE(protected_ids[0]==1 && protected_ids[1]==2);
    protected_result.close();
    auto missing_principal=protected_request; missing_principal.erase("authorization");
    bool missing_failed=false;try {compiler.compile(missing_principal);}catch(const std::exception&){missing_failed=true;} REQUIRE(missing_failed);
    request["query"]="invalid graph query";
    bool failed=false;try {compiler.compile(request);}catch(const std::exception&){failed=true;} REQUIRE(failed);
    request["dialect"]="postgres";
    failed=false;try {orchiddb::query(compiler,request,engine);}catch(const std::invalid_argument&){failed=true;} REQUIRE(failed);
    sql(connection,"SELECT 42");
    std::cout<<"Native compiler, DuckDB "<<duckdb_library_version()<<", Arrow buffers, nulls, int64, ownership, rollback and errors passed\n";
  } catch (...) {duckdb_disconnect(&connection);duckdb_close(&db);throw;}
  duckdb_disconnect(&connection);duckdb_close(&db);
}
