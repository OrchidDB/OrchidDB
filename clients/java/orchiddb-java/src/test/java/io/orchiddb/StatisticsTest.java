package io.orchiddb;

import static org.junit.jupiter.api.Assertions.*;

import com.fasterxml.jackson.databind.*;
import java.nio.file.*;
import java.sql.*;
import java.util.*;
import org.junit.jupiter.api.Test;

class StatisticsTest {
  static final ObjectMapper JSON = new ObjectMapper();
  static final Source PEOPLE = Source.table("lake", "people");
  static final GraphMapping MAPPING =
      new GraphMapping(List.of(NodeMapping.node("Person", PEOPLE, "id")), List.of());
  static final Map<Source, List<Column>> SCHEMAS =
      Map.of(PEOPLE, List.of(new Column("id", "int64", true)));

  static final class Compiler implements SqlCompiler {
    final List<JsonNode> commands = new ArrayList<>();
    final List<Compilation> compilations = new ArrayList<>();
    int revision;
    boolean failFinish;

    public CompiledQuery compile(Compilation request) {
      compilations.add(request);
      return new CompiledQuery(
          request.engine(), request.dialect(), "SELECT id FROM people", List.of("id"));
    }

    public String statisticsCommand(String input) {
      try {
        var c = JSON.readTree(input);
        commands.add(c);
        return switch (c.path("op").asText()) {
          case "begin" ->
              "{\"id\":\"analysis\",\"request\":{\"id\":\"read\",\"source\":\"people\",\"sql\":\"SELECT id FROM people LIMIT 2\",\"max_rows\":2,\"max_bytes\":1000,\"timeout_ms\":1000}}";
          case "submit" -> "{\"id\":\"analysis\",\"request\":null}";
          case "finish" -> {
            if (failFinish) throw new PlanningException("finish failed");
            yield "{\"catalog_id\":\"catalog-"
                + (++revision)
                + "\",\"snapshot\":{\"revision\":"
                + revision
                + "},\"report\":{\"complete\":true}}";
          }
          case "install" -> "{\"catalog_id\":\"catalog-" + (++revision) + "\"}";
          default -> "{}";
        };
      } catch (java.io.IOException e) {
        throw new AssertionError(e);
      }
    }
  }

  @Test
  void graphRetainsSnapshotAndInvalidatesPlanCacheOnGenerationLoadAndClear() throws Exception {
    try (var connection = DriverManager.getConnection("jdbc:duckdb:")) {
      connection.createStatement().execute("CREATE TABLE people AS SELECT * FROM range(4) t(id)");
      var compiler = new Compiler();
      try (var graph =
          new OrchidDB(
                  compiler,
                  PlanCache.bounded(8),
                  JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
              .graph(MAPPING)) {
        var query = Query.cypher("MATCH (p:Person) RETURN p.id");
        graph.plan(query);
        graph.generateStatistics();
        graph.plan(query);
        graph.plan(query);
        assertEquals(2, compiler.compilations.size());
        assertNull(compiler.compilations.get(0).statisticsCatalog());
        assertEquals("catalog-1", compiler.compilations.get(1).statisticsCatalog());
        var submitted =
            compiler.commands.stream()
                .filter(c -> c.path("op").asText().equals("submit"))
                .findFirst()
                .orElseThrow();
        assertFalse(submitted.has("error"), submitted.toString());
        assertEquals(2, submitted.path("rows").size());
        var path = Files.createTempFile("orchiddb-statistics", ".json");
        try {
          graph.saveStatistics(path);
          graph.loadStatistics(path);
          graph.plan(query);
          assertEquals("catalog-2", compiler.compilations.get(2).statisticsCatalog());
        } finally {
          Files.deleteIfExists(path);
        }
        String before = graph.statisticsSnapshotJson();
        compiler.failFinish = true;
        assertThrows(PlanningException.class, graph::generateStatistics);
        assertEquals(before, graph.statisticsSnapshotJson());
        assertTrue(
            compiler.commands.stream().anyMatch(c -> c.path("op").asText().equals("cancel")));
        graph.clearStatistics();
        assertNull(graph.statisticsSnapshotJson());
      }
      assertFalse(connection.isClosed());
    }
  }

  @Test
  void jdbcCancelsExpensiveCollectionAndReleasesTheSession() throws Exception {
    try (var connection = DriverManager.getConnection("jdbc:duckdb:")) {
      var engine = JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection);
      long started = System.nanoTime();
      try (var session = engine.openSession()) {
        assertThrows(
            SQLException.class,
            () -> {
              try (var rows =
                  session.readStatistics(
                      new StatisticsRead(
                          "SELECT sum(sin(i::DOUBLE)) FROM range(1000000000000) t(i)",
                          1,
                          1000,
                          30))) {
                rows.next();
              }
            });
      }
      assertTrue((System.nanoTime() - started) / 1_000_000 < 5000);
      try (var session = engine.openSession();
          var rows = session.readStatistics(new StatisticsRead("SELECT 42", 1, 1000, 1000))) {
        assertTrue(rows.next());
        assertEquals(42, ((Number) rows.get(1)).intValue());
      }
    }
  }

  @Test
  void jdbcCollectionValuesPreserveStructFieldNames() throws Exception {
    var compiler = new Compiler();
    try (var connection = DriverManager.getConnection("jdbc:duckdb:");
        var jdbc = JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection).openSession()) {
      var session =
          new ExecutionEngine.Session() {
            public Map<Source, List<Column>> schemas(Set<Source> sources) {
              return SCHEMAS;
            }

            public QueryResult readStatistics(StatisticsRead request) throws SQLException {
              return jdbc.readStatistics(
                  new StatisticsRead(
                      "SELECT [{'sku':'a','quantity':2}] AS items",
                      request.maxRows(),
                      request.maxBytes(),
                      request.timeoutMillis()));
            }

            public void close() {}
          };
      try (var statistics =
          Statistics.generate(
              compiler,
              new Compilation(
                  "lake", SqlDialect.DUCKDB, MAPPING, SCHEMAS, List.of(), Query.cypher("RETURN 1")),
              session)) {
        var submission =
            compiler.commands.stream().filter(c -> c.has("rows")).findFirst().orElseThrow();
        assertEquals("a", submission.path("rows").get(0).path("items").get(0).path("sku").asText());
      }
    }
  }

  @Test
  void byteBudgetPreservesAcceptedRowsAndMarksPartialCoverage() throws Exception {
    var compiler = new Compiler();
    var session =
        new ExecutionEngine.Session() {
          public Map<Source, List<Column>> schemas(Set<Source> sources) {
            return SCHEMAS;
          }

          public QueryResult readStatistics(StatisticsRead request) {
            return new QueryResult() {
              int row;

              public List<String> columns() {
                return List.of("id");
              }

              public boolean next() {
                return ++row <= 2;
              }

              public Object get(int column) {
                return row == 1 ? "small" : "x".repeat(2000);
              }

              public void close() {}
            };
          }

          public void close() {}
        };
    try (var statistics =
        Statistics.generate(
            compiler,
            new Compilation(
                "lake", SqlDialect.DUCKDB, MAPPING, SCHEMAS, List.of(), Query.cypher("RETURN 1")),
            session)) {
      var partial = compiler.commands.stream().filter(c -> c.has("done")).findFirst().orElseThrow();
      assertFalse(partial.path("done").asBoolean());
      assertEquals(1, partial.path("rows").size());
      assertTrue(compiler.commands.stream().anyMatch(c -> c.has("error")));
    }
  }

  @Test
  void unsupportedReaderReportsCoverageInsteadOfUnboundedFallback() throws Exception {
    var compiler = new Compiler();
    var session =
        new ExecutionEngine.Session() {
          public Map<Source, List<Column>> schemas(Set<Source> sources) {
            return SCHEMAS;
          }

          public void close() {}
        };
    try (var statistics =
        Statistics.generate(
            compiler,
            new Compilation(
                "lake", SqlDialect.DUCKDB, MAPPING, SCHEMAS, List.of(), Query.cypher("RETURN 1")),
            session)) {
      assertNotNull(statistics.snapshotJson());
      assertTrue(compiler.commands.stream().anyMatch(c -> c.has("error")));
    }
  }

  @Test
  void interruptionCancelsWithoutInstallingPartialCatalog() throws Exception {
    var compiler = new Compiler();
    var session =
        new ExecutionEngine.Session() {
          public Map<Source, List<Column>> schemas(Set<Source> sources) {
            return SCHEMAS;
          }

          public void close() {}
        };
    Thread.currentThread().interrupt();
    try {
      assertThrows(
          SQLException.class,
          () ->
              Statistics.generate(
                  compiler,
                  new Compilation(
                      "lake",
                      SqlDialect.DUCKDB,
                      MAPPING,
                      SCHEMAS,
                      List.of(),
                      Query.cypher("RETURN 1")),
                  session));
    } finally {
      Thread.interrupted();
    }
    assertTrue(compiler.commands.stream().anyMatch(c -> c.path("op").asText().equals("cancel")));
    assertFalse(compiler.commands.stream().anyMatch(c -> c.path("op").asText().equals("finish")));
  }
}
