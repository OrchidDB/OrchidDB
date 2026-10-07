package io.orchiddb.examples;

import io.orchiddb.*;
import java.sql.*;
import java.util.*;

/** Run with scripts/run-example.sh BringYourOwnDuckDb. */
public final class BringYourOwnDuckDb {
  public static void main(String[] args) throws Exception {
    var compiler = NativeSqlCompiler.load();
    // The application chooses the driver, database, extensions, credentials,
    // session configuration and transaction policy.
    try (var connection = DriverManager.getConnection("jdbc:duckdb:")) {
      try (var sql = connection.createStatement()) {
        sql.execute("SET memory_limit='512MB'");
        sql.execute("CREATE TEMP TABLE people(id BIGINT, name VARCHAR, age BIGINT)");
        sql.execute("INSERT INTO people VALUES (1,'Ada',36),(2,'Grace',45)");
        sql.execute("CREATE TEMP TABLE knows(id BIGINT, src BIGINT, dst BIGINT)");
        sql.execute("INSERT INTO knows VALUES (10,1,2)");
      }
      var people = Source.table("lake", "people");
      var knows = Source.table("lake", "knows");
      var mapping =
          new GraphMapping(
              List.of(
                  NodeMapping.node("Person", people, "id")
                      .property("name", "name")
                      .property("age", "age")),
              List.of(EdgeMapping.edge("KNOWS", knows, "id", "src", "dst", "Person", "Person")));
      var db =
          new OrchidDB(
              compiler,
              PlanCache.bounded(128),
              JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection));
      var graph = db.graph(mapping);
      var query =
          Query.cypher(
              "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.name=$name RETURN b.name AS friend",
              Map.of("name", "Ada"));
      var plan = graph.plan(query);
      System.out.println("Generated SQL (may contain parameter values):\n" + plan.sql());
      try (var result = graph.query(query)) {
        while (result.next()) System.out.println("Ada knows " + result.get("friend"));
      }
      // Or execute the plan yourself with your own Statement settings:
      try (var sql = connection.createStatement();
          var result = sql.executeQuery(plan.sql())) {
        while (result.next()) System.out.println("Direct JDBC: " + result.getString(1));
      }
      // Closing results has not closed or committed the caller's connection.
      System.out.println("Caller connection remains open: " + !connection.isClosed());
    }
  }
}
