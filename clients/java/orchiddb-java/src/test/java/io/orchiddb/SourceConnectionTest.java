package io.orchiddb;

import static org.junit.jupiter.api.Assertions.*;

import java.nio.file.Path;
import java.util.*;
import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.*;

class SourceConnectionTest {
  @BeforeAll
  static void load() {
    NativeSqlCompiler.load(Path.of(System.getProperty("orchiddb.native.path")));
  }

  @Test
  void constantsAndLocalFunctionsDoNotInitializeCompletion() throws Exception {
    var graph =
        new SourceConnection(
            "{\"functions\":[{\"name\":\"twice\",\"target\":\"twice\",\"parameters\":[\"int64\"],\"returns\":\"int64\",\"local\":true}]}",
            SourceConnection.Engine.DUCKDB,
            Map.of(),
            Map.of("twice", args -> ((Number) args.get(0)).longValue() * 2),
            (sql, inputs) -> {
              throw new AssertionError("DuckDB completion must not run");
            });
    try (var rows = graph.query("RETURN twice(21) AS answer")) {
      assertTrue(rows.next());
      assertEquals(42L, rows.get(1));
      assertFalse(rows.next());
    }
  }

  @Test
  void sourceOnlyUsesApplicationCallback() throws Exception {
    String schema =
        "{\"sources\":{\"db\":{\"adapter\":\"postgres\"}},\"tables\":[{\"name\":\"people\",\"source\":\"db\",\"columns\":[{\"name\":\"id\",\"data_type\":\"int64\"}]}],\"nodes\":[{\"label\":\"Person\",\"table\":\"people\",\"id\":\"id\",\"properties\":{\"id\":\"id\"}}]}";
    var calls = new AtomicInteger();
    var closed = new AtomicInteger();
    var source =
        new SourceConnection.SqlSource() {
          public String dialect() {
            return "postgres";
          }

          public QueryResult query(String sql) {
            calls.incrementAndGet();
            assertTrue(sql.contains("people"));
            return new QueryResult() {
              int row;
              boolean done;

              public List<String> columns() {
                return List.of("id");
              }

              public boolean next() {
                return ++row <= 2;
              }

              public Object get(int col) {
                return (long) row;
              }

              public void close() {
                if (!done) {
                  done = true;
                  closed.incrementAndGet();
                }
              }
            };
          }
        };
    var graph =
        new SourceConnection(
            schema, SourceConnection.Engine.NONE, Map.of("db", source), Map.of(), null);
    try (var rows = graph.query("MATCH (p:Person) RETURN p.id AS id")) {
      assertTrue(rows.next());
      assertEquals(1L, rows.get(1));
    }
    assertEquals(1, calls.get());
    assertEquals(1, closed.get());
  }
}
