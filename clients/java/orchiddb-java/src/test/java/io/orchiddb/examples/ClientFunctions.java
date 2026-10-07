package io.orchiddb.examples;

import io.orchiddb.*;
import java.sql.*;
import java.util.*;
import org.duckdb.DuckDBFunctions;

/** DuckDB's own Java UDF API runs on the application's existing session. */
public final class ClientFunctions {
  public static void main(String[] args) throws Exception {
    var compiler = NativeSqlCompiler.load();
    try (var connection = DriverManager.getConnection("jdbc:duckdb:")) {
      try (var sql = connection.createStatement()) {
        sql.execute(
            "CREATE TEMP TABLE people(id BIGINT, age BIGINT); INSERT INTO people VALUES (1,36)");
      }
      DuckDBFunctions.scalarFunction()
          .withName("java_next_age")
          .withParameter(long.class)
          .withReturnType(long.class)
          .withLongFunction(age -> age + 1)
          .register(connection);
      var mapping =
          new GraphMapping(
              List.of(
                  NodeMapping.node("Person", Source.table("lake", "people"), "id")
                      .property("age", "age")),
              List.of());
      var graph =
          new OrchidDB(
                  compiler,
                  PlanCache.none(),
                  JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
              .graph(
                  mapping,
                  List.of(FunctionSignature.scalar("nextAge", "java_next_age", "int64", "int64")));
      try (var result =
          graph.query(Query.cypher("MATCH (p:Person) RETURN nextAge(p.age) AS nextAge"))) {
        while (result.next()) System.out.println("Java UDF result: " + result.get("nextAge"));
      }
    }
  }
}
