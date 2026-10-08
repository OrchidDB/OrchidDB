#include "../../../clients/cpp/examples/duckdb_engine.hpp"
#include <iostream>
#include <cstdlib>
int main() {
  using namespace orchiddb;
  auto env=[](const char* key){auto value=std::getenv(key);if(!value)throw std::runtime_error("Missing smoke environment");return std::string(value);};
  std::vector<CatalogAuth> auths={CatalogAuth::bearer(env("AUTH_SMOKE_TOKEN")),CatalogAuth::bearer(Credential::env("AUTH_SMOKE_TOKEN")),
    CatalogAuth::bearer(Credential::file(env("AUTH_SMOKE_TOKEN_FILE"))),CatalogAuth::client_credentials("admin",env("AUTH_SMOKE_SECRET")),
    CatalogAuth::client_credentials("admin",Credential::env("AUTH_SMOKE_SECRET")),CatalogAuth::client_credentials("admin",Credential::file(env("AUTH_SMOKE_SECRET_FILE"))),
    CatalogAuth::client_credentials("external-client",env("AUTH_SMOKE_IDP_SECRET")).issuer(env("AUTH_SMOKE_ISSUER")),
    CatalogAuth::token_exchange(Credential::value(env("AUTH_SMOKE_TOKEN")))};
  duckdb_database db{};duckdb_connection session{};
  if(duckdb_open(nullptr,&db)!=DuckDBSuccess || duckdb_connect(db,&session)!=DuckDBSuccess)throw std::runtime_error("DuckDB setup");
  duckdb_result setup{};
  if(duckdb_query(session,"CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Grace')",&setup)!=DuckDBSuccess)throw std::runtime_error("DuckDB fixture");
  duckdb_destroy_result(&setup);
  DuckDBEngine engine(session);
  for(const auto& auth:auths) {
    Catalog catalog(env("AUTH_SMOKE_URL"),"smoke","smoke",auth);
    if(catalog.discover().at("description")!="Auth smoke graph")throw std::runtime_error("Discovery");
    Connection graph(engine,catalog);
    auto result=graph.query("MATCH (p:Person)-[r:PEER {score:1, limit:1}]->(q:Person) RETURN count(r) AS n");
    auto schema=result.schema();auto batch=result.next();
    if(!batch || batch.get()->length!=1 || static_cast<const int64_t*>(batch.get()->children[0]->buffers[1])[0]!=2)throw std::runtime_error("Query result");
  }
  Catalog catalog(env("AUTH_SMOKE_URL"),"smoke","smoke",auths[3]);
  if(catalog.register_principal("cpp-workload",{"reader"}).at("version")!=1 || catalog.principal("cpp-workload").at("version")!=1)throw std::runtime_error("Principal registration");
  duckdb_disconnect(&session);duckdb_close(&db);
  std::cout<<"PASS C++: credential sources, OAuth/OIDC, exchange, principal management, DuckDB execution\n";
}
