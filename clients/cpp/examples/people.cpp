#include "duckdb_engine.hpp"
#include <iostream>

int main() {
  duckdb_database database{};
  duckdb_connection connection{};
  if (duckdb_open(nullptr, &database) != DuckDBSuccess) return 1;
  if (duckdb_connect(database, &connection) != DuckDBSuccess) { duckdb_close(&database); return 1; }
  int status = 0;
  try {
    duckdb_result setup{};
    auto state = duckdb_query(connection,
        "CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Grace')", &setup);
    std::string error = state == DuckDBSuccess ? "" : duckdb_result_error(&setup);
    duckdb_destroy_result(&setup);
    if (state != DuckDBSuccess) throw std::runtime_error(error);
    DuckDBEngine engine(connection);
    auto schema = orchiddb::Json::parse(R"({
      "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],
      "nodes":[{"label":"Person","table":"people","id":"id","properties":{"id":"id","name":"name"}}]
    })");
    orchiddb::Connection graph(engine, schema);
    for (auto name : {"Ada", "Grace"}) {
      auto result = graph.query("MATCH (p:Person) WHERE p.name=$name RETURN p.id AS id", {{"name", name}});
      while (auto batch = result.next()) {
        auto column = batch.get()->children[0];
        auto ids = static_cast<const int64_t*>(column->buffers[1]);
        for (int64_t row = 0; row < batch.get()->length; ++row)
          std::cout << name << ": " << ids[column->offset + row] << '\n';
      }
    }
  } catch (const std::exception& error) { std::cerr << error.what() << '\n'; status = 1; }
  duckdb_disconnect(&connection);
  duckdb_close(&database);
  return status;
}
