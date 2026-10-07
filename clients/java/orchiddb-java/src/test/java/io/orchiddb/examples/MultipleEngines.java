package io.orchiddb.examples;

import io.orchiddb.*;
import java.sql.*;
import java.util.*;

/** Separate engine routing today; this example does not claim cross-engine joins. */
public final class MultipleEngines {
  public static void main(String[] args) throws Exception {
    var compiler = NativeSqlCompiler.load();
    try (var east = DriverManager.getConnection("jdbc:duckdb:");
        var west = DriverManager.getConnection("jdbc:duckdb:")) {
      try (var s = east.createStatement()) {
        s.execute(
            "CREATE TABLE people(id BIGINT,name VARCHAR); INSERT INTO people VALUES (1,'Ada')");
      }
      try (var s = west.createStatement()) {
        s.execute(
            "CREATE TABLE people(id BIGINT,name VARCHAR); INSERT INTO people VALUES (1,'Grace')");
      }
      var db =
          new OrchidDB(
              compiler,
              PlanCache.bounded(16),
              JdbcEngine.borrowed("east", SqlDialect.DUCKDB, east),
              JdbcEngine.borrowed("west", SqlDialect.DUCKDB, west));
      for (String engine : List.of("east", "west")) {
        var graph =
            db.graph(
                new GraphMapping(
                    List.of(
                        NodeMapping.node("Person", Source.table(engine, "people"), "id")
                            .property("name", "name")),
                    List.of()));
        try (var result = graph.query(Query.cypher("MATCH (p:Person) RETURN p.name AS name"))) {
          while (result.next()) System.out.println(engine + ": " + result.get("name"));
        }
      }
    }
  }
}
