package io.orchiddb;

import static org.junit.jupiter.api.Assertions.*;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.DriverManager;
import java.sql.SQLException;
import java.util.*;
import org.junit.jupiter.api.Test;

class RemoteEngineIntegrationTest {
  private static final ObjectMapper JSON =
      new ObjectMapper()
          .enable(com.fasterxml.jackson.databind.DeserializationFeature.USE_BIG_INTEGER_FOR_INTS)
          .enable(com.fasterxml.jackson.databind.DeserializationFeature.USE_BIG_DECIMAL_FOR_FLOATS);

  static JsonNode fixture() throws Exception {
    String path = System.getenv("ORCHIDDB_REMOTE_FIXTURE");
    assumeTrue(
        path != null && !path.isBlank(), "ORCHIDDB_REMOTE_FIXTURE enables live HTTP engine tests");
    return JSON.readTree(Files.readString(Path.of(path)));
  }

  @Test
  void realRemoteRequestsComposeWithDuckdbAndPreserveClientOwnership() throws Exception {
    var fixture = fixture();
    assertFalse(fixture.path("cases").isEmpty(), "Fixture must contain live cases");
    var compiler = NativeSqlCompiler.load();
    var adapters = new HashSet<String>();
    for (var test : fixture.path("cases")) {
      String adapter = test.path("adapter").asText();
      adapters.add(adapter);
      try (var duck = DriverManager.getConnection("jdbc:duckdb:");
          var remote =
              new RemoteEngine(
                  "search",
                  adapter,
                  Map.of(
                      "endpoint",
                      test.path("endpoint").asText(),
                      "page_size",
                      2,
                      "batch_size",
                      2))) {
        for (var sql : test.path("setup_sql")) {
          try (var statement = duck.createStatement()) {
            statement.execute(sql.asText());
          }
        }
        var request = test.get("request");
        var routes = new LinkedHashMap<String, ExecutionEngine>();
        request
            .path("engines")
            .fields()
            .forEachRemaining(
                entry -> {
                  if (entry.getValue().path("dialect").asText().equals(adapter)) {
                    routes.put(entry.getKey(), remote);
                  } else if (entry.getValue().path("dialect").asText().equals("duckdb")) {
                    routes.put(
                        entry.getKey(),
                        JdbcEngine.borrowed(entry.getKey(), SqlDialect.DUCKDB, duck));
                  } else throw new IllegalArgumentException("Unsupported fixture coordinator");
                });
        for (int repeat = 0; repeat < 2; repeat++) {
          var actual = JSON.createArrayNode();
          try (var rows = FederatedQuery.query(compiler, request.toString(), routes)) {
            while (rows.next()) {
              var row = JSON.createArrayNode();
              for (int i = 1; i <= rows.columns().size(); i++) row.addPOJO(rows.get(i));
              actual.add(row);
            }
          }
          assertEquals(
              normalize(test.path("expected_rows")), normalize(actual), test.path("name").asText());
          assertFalse(duck.isClosed(), "Federation must retain caller-owned JDBC connections");
          remote.clearMetadataCache();
        }
        var borrowed = remote.openSession();
        borrowed.close();
        assertThrows(SQLException.class, () -> borrowed.executeRequests("[]", "[]"));
        try (var another = remote.openSession()) {
          assertThrows(SQLException.class, () -> another.executeRequests("[{}]", "[]"));
          assertEquals("[]", another.executeRequests("[]", "[]"));
        }
        remote.close();
        remote.close();
        assertThrows(SQLException.class, remote::openSession);
      }
    }
    assertEquals(Set.of("quickwit", "elasticsearch"), adapters, "Exercise both real engines");
  }

  @Test
  void failedRequestClosesFederationSessionsAndPreservesCallerConnection() throws Exception {
    var test = fixture().path("cases").get(0);
    var compiler = NativeSqlCompiler.load();
    try (var duck = DriverManager.getConnection("jdbc:duckdb:")) {
      for (var sql : test.path("setup_sql")) {
        try (var statement = duck.createStatement()) {
          statement.execute(sql.asText());
        }
      }
      var routes = new LinkedHashMap<String, ExecutionEngine>();
      var opened = new ArrayList<TrackingEngine>();
      test.path("request")
          .path("engines")
          .fields()
          .forEachRemaining(
              entry -> {
                String dialect = entry.getValue().path("dialect").asText();
                var engine =
                    new TrackingEngine(
                        entry.getKey(),
                        dialect,
                        dialect.equals("duckdb")
                            ? JdbcEngine.borrowed(entry.getKey(), SqlDialect.DUCKDB, duck)
                            : null);
                routes.put(entry.getKey(), engine);
                opened.add(engine);
              });
      var error =
          assertThrows(
              SQLException.class,
              () -> FederatedQuery.query(compiler, test.get("request").toString(), routes));
      assertEquals("deliberate request failure", error.getMessage());
      assertTrue(opened.stream().anyMatch(e -> e.requests > 0));
      for (var engine : opened) assertEquals(engine.opens, engine.closes);
      assertFalse(duck.isClosed());
    }
  }

  private static JsonNode normalize(JsonNode node) throws Exception {
    // Serialize POJO JDBC numbers first, then use decimal equality across integer JDBC widths.
    node = JSON.readTree(node.toString());
    if (node.isNumber())
      return JSON.getNodeFactory().numberNode(node.decimalValue().stripTrailingZeros());
    if (node.isArray()) {
      var result = JSON.createArrayNode();
      for (var item : node) result.add(normalize(item));
      return result;
    }
    return node;
  }

  private static final class TrackingEngine implements ExecutionEngine {
    final String id;
    final SqlDialect dialect;
    final ExecutionEngine delegate;
    int opens, closes, requests;

    TrackingEngine(String id, String dialect, ExecutionEngine delegate) {
      this.id = id;
      this.dialect = new SqlDialect(dialect);
      this.delegate = delegate;
    }

    public String id() {
      return id;
    }

    public SqlDialect dialect() {
      return dialect;
    }

    public Session openSession() throws SQLException {
      opens++;
      var inner = delegate == null ? null : delegate.openSession();
      return new Session() {
        public Map<Source, List<Column>> schemas(Set<Source> sources) throws SQLException {
          return inner.schemas(sources);
        }

        public QueryResult execute(CompiledQuery query) throws SQLException {
          return inner.execute(query);
        }

        public String executeRequests(String requestsJson, String columnsJson) throws SQLException {
          requests++;
          throw new SQLException("deliberate request failure");
        }

        public void close() throws SQLException {
          closes++;
          if (inner != null) inner.close();
        }
      };
    }
  }
}
