package io.orchiddb.gremlin;

import io.orchiddb.*;
import java.sql.*;
import java.util.*;

/** Run with scripts/run-gremlin-example.sh. */
public final class GremlinExample {
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
      // Optional TinkerPop adapter; no Gremlin Server and no new database connection.
      try (var g = OrchidGremlin.traversal(graph)) {
        var names =
            g.V()
                .hasLabel("Person")
                .has("name", "Ada")
                .out("KNOWS")
                .<String>values("name")
                .toList();
        if (!names.equals(List.of("Grace"))) throw new AssertionError(names);
        System.out.println("Ada knows " + names);
        System.out.println("People: " + g.V().count().next());
      }
      // Closing results has not closed or committed the caller's connection.
      System.out.println("Caller connection remains open: " + !connection.isClosed());
    }
  }
}
