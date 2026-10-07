package io.orchiddb;

import static org.junit.jupiter.api.Assertions.*;

import java.io.IOException;
import java.math.BigDecimal;
import java.nio.file.Path;
import java.sql.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicInteger;
import org.apache.arrow.memory.RootAllocator;
import org.apache.arrow.vector.*;
import org.apache.arrow.vector.ipc.ArrowReader;
import org.apache.arrow.vector.types.pojo.Schema;
import org.duckdb.DuckDBResultSet;
import org.junit.jupiter.api.*;

class ArrowIntegrationTest {
  static final ArrowExporter EXPORT =
      (rows, allocator, size) ->
          (ArrowReader) rows.unwrap(DuckDBResultSet.class).arrowExportStream(allocator, size);
  static NativeSqlCompiler compiler;
  static final GraphMapping MAPPING =
      new GraphMapping(
          List.of(
              NodeMapping.node("Person", Source.table("lake", "people"), "id")
                  .property("name", "name")
                  .property("id", "id")),
          List.of());

  @BeforeAll
  static void load() {
    compiler = NativeSqlCompiler.load(Path.of(System.getProperty("orchiddb.native.path")));
  }

  static Connection database() throws SQLException {
    var properties = new Properties();
    properties.setProperty("jdbc_stream_results", "true");
    var db = DriverManager.getConnection("jdbc:duckdb:", properties);
    try (var s = db.createStatement()) {
      s.execute(
          "CREATE TABLE people AS SELECT i::BIGINT AS id, CASE WHEN i % 5 = 0 THEN NULL ELSE 'person-' || i END AS name FROM range(10003) t(i)");
    }
    return db;
  }

  static JdbcEngine engine(Connection db, RootAllocator allocator) {
    return JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, db).withArrow(EXPORT, allocator, 128);
  }

  static CompiledQuery sql(String sql) {
    return new CompiledQuery("lake", SqlDialect.DUCKDB, sql, List.of());
  }

  static void noLeaks(RootAllocator allocator) {
    assertEquals(0, allocator.getAllocatedMemory());
    assertTrue(allocator.getChildAllocators().isEmpty(), "query allocator leaked");
  }

  // Fail if the batch path silently degrades into JDBC per-row/per-cell iteration.
  static Connection forbidJdbcRowReads(Connection db) {
    return (Connection)
        java.lang.reflect.Proxy.newProxyInstance(
            ArrowIntegrationTest.class.getClassLoader(),
            new Class<?>[] {Connection.class},
            (p, m, a) -> {
              try {
                Object value = m.invoke(db, a);
                if (!m.getName().equals("createStatement")) return value;
                Statement statement = (Statement) value;
                return java.lang.reflect.Proxy.newProxyInstance(
                    ArrowIntegrationTest.class.getClassLoader(),
                    new Class<?>[] {Statement.class},
                    (sp, sm, sa) -> {
                      try {
                        Object result = sm.invoke(statement, sa);
                        if (!sm.getName().equals("executeQuery")) return result;
                        ResultSet rows = (ResultSet) result;
                        return java.lang.reflect.Proxy.newProxyInstance(
                            ArrowIntegrationTest.class.getClassLoader(),
                            new Class<?>[] {ResultSet.class},
                            (rp, rm, ra) -> {
                              if (rm.getName().equals("next") || rm.getName().equals("getObject"))
                                throw new AssertionError("JDBC row conversion on the Arrow path");
                              try {
                                return rm.invoke(rows, ra);
                              } catch (java.lang.reflect.InvocationTargetException e) {
                                throw e.getCause();
                              }
                            });
                      } catch (java.lang.reflect.InvocationTargetException e) {
                        throw e.getCause();
                      }
                    });
              } catch (java.lang.reflect.InvocationTargetException e) {
                throw e.getCause();
              }
            });
  }

  @Test
  void nativeCompilerAndDriverProduceMultipleTypedBatches() throws Exception {
    try (var db = database();
        var allocator = new RootAllocator(64 * 1024 * 1024)) {
      var graph =
          new OrchidDB(compiler, PlanCache.bounded(8), engine(db, allocator)).graph(MAPPING);
      try (var result =
          graph.queryArrow(
              Query.cypher("MATCH (p:Person) RETURN p.id AS id, p.name AS name ORDER BY id"))) {
        assertEquals(
            List.of("id", "name"),
            result.schema().getFields().stream().map(f -> f.getName()).toList());
        assertThrows(SQLException.class, result::batch);
        long expected = 0;
        int batches = 0;
        while (result.nextBatch()) {
          batches++;
          var batch = result.batch();
          var ids = (BigIntVector) batch.getVector("id");
          var names = (VarCharVector) batch.getVector("name");
          for (int row = 0; row < batch.getRowCount(); row++, expected++) {
            assertEquals(expected, ids.get(row));
            if (expected % 5 == 0) assertTrue(names.isNull(row));
            else assertEquals("person-" + expected, names.getObject(row).toString());
          }
        }
        assertEquals(10003, expected);
        assertTrue(batches > 1);
        assertFalse(result.nextBatch());
        assertThrows(SQLException.class, result::batch);
      }
      noLeaks(allocator);
      assertFalse(db.isClosed());
    }
  }

  @Test
  void emptyResultHasSchemaAndNoCurrentBatch() throws Exception {
    try (var db = database();
        var allocator = new RootAllocator();
        var session = engine(db, allocator).openSession()) {
      try (var result = session.executeArrow(sql("SELECT id,name FROM people WHERE false"))) {
        assertEquals(2, result.schema().getFields().size());
        assertFalse(result.nextBatch());
        assertFalse(result.nextBatch());
        assertThrows(SQLException.class, result::batch);
      }
      noLeaks(allocator);
    }
  }

  @Test
  void nativeArrowPreservesDecimalBinaryTimestampAndNestedTypes() throws Exception {
    try (var db = database();
        var allocator = new RootAllocator();
        var session = engine(db, allocator).openSession()) {
      try (var result =
          session.executeArrow(
              sql(
                  "SELECT 123.45::DECIMAL(12,2) AS amount, TIMESTAMP '2026-09-25 12:34:56.123456' AS time, blob 'hello' AS bytes, [1, NULL, 3] AS items"))) {
        assertTrue(result.nextBatch());
        var batch = result.batch();
        assertEquals(new BigDecimal("123.45"), batch.getVector("amount").getObject(0));
        assertEquals("2026-09-25T12:34:56.123456", batch.getVector("time").getObject(0).toString());
        assertArrayEquals(
            "hello".getBytes(java.nio.charset.StandardCharsets.UTF_8),
            (byte[]) batch.getVector("bytes").getObject(0));
        assertEquals(Arrays.asList(1, null, 3), batch.getVector("items").getObject(0));
      }
      noLeaks(allocator);
    }
  }

  @Test
  void partialCloseReleasesLeaseBuffersAndPreservesCallerTransaction() throws Exception {
    try (var db = database();
        var allocator = new RootAllocator()) {
      db.setAutoCommit(false);
      try (var statement = db.createStatement()) {
        statement.execute("INSERT INTO people VALUES (10004, 'uncommitted')");
      }
      var engine = engine(db, allocator);
      var graph = new OrchidDB(compiler, PlanCache.none(), engine).graph(MAPPING);
      var result = graph.queryArrow(Query.cypher("MATCH (p:Person) RETURN p.name ORDER BY p.id"));
      assertTrue(result.nextBatch());
      assertThrows(SQLException.class, engine::openSession);
      result.close();
      result.close();
      assertThrows(SQLException.class, result::nextBatch);
      assertThrows(SQLException.class, result::batch);
      noLeaks(allocator);
      assertFalse(db.isClosed());
      assertFalse(db.getAutoCommit());
      db.rollback();
      try (var session = engine.openSession();
          var rows = session.execute(sql("SELECT count(*) AS count FROM people"))) {
        assertTrue(rows.next());
        assertEquals(10003L, rows.get(1));
      }
      noLeaks(allocator);
    }
  }

  @Test
  void rowConvenienceUsesArrowAndHonorsCursorState() throws Exception {
    try (var db = database();
        var allocator = new RootAllocator()) {
      var exports = new AtomicInteger();
      var engine =
          JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, forbidJdbcRowReads(db))
              .withArrow(
                  (rows, a, n) -> {
                    exports.incrementAndGet();
                    return EXPORT.export(rows, a, n);
                  },
                  allocator,
                  128);
      var graph = new OrchidDB(compiler, PlanCache.none(), engine).graph(MAPPING);
      try (var rows =
          graph.query(
              Query.cypher("MATCH (p:Person) RETURN p.name AS name ORDER BY p.id LIMIT 2"))) {
        assertThrows(SQLException.class, () -> rows.get(1));
        assertTrue(rows.next());
        assertNull(rows.get(1));
        assertTrue(rows.next());
        assertEquals("person-1", rows.get("name"));
        assertThrows(SQLException.class, () -> rows.get(0));
        assertFalse(rows.next());
        assertFalse(rows.next());
        assertThrows(SQLException.class, () -> rows.get(1));
      }
      assertEquals(1, exports.get());
      noLeaks(allocator);
    }
  }

  @Test
  void pooledConnectionLivesUntilArrowResultCloses() throws Exception {
    var closes = new AtomicInteger();
    try (var db = database();
        var allocator = new RootAllocator()) {
      var engine =
          JdbcEngine.pooled("lake", SqlDialect.DUCKDB, LifecycleTest.pool(db, closes))
              .withArrow(EXPORT, allocator, 128);
      var graph = new OrchidDB(compiler, PlanCache.none(), engine).graph(MAPPING);
      var result = graph.queryArrow(Query.cypher("MATCH (p:Person) RETURN p.name"));
      assertTrue(result.nextBatch());
      assertEquals(0, closes.get());
      result.close();
      result.close();
      assertEquals(1, closes.get());
      noLeaks(allocator);
    }
  }

  @Test
  void exporterFailureReturnsPoolLeaseAndClosesQueryAllocator() throws Exception {
    var closes = new AtomicInteger();
    try (var db = database();
        var allocator = new RootAllocator()) {
      var engine =
          JdbcEngine.pooled("lake", SqlDialect.DUCKDB, LifecycleTest.pool(db, closes))
              .withArrow(
                  (r, a, n) -> {
                    throw new SQLException("export failed");
                  },
                  allocator,
                  128);
      var graph = new OrchidDB(compiler, PlanCache.none(), engine).graph(MAPPING);
      assertThrows(
          SQLException.class,
          () -> graph.queryArrow(Query.cypher("MATCH (p:Person) RETURN p.name")));
      assertEquals(1, closes.get());
      noLeaks(allocator);
    }
  }

  @Test
  void batchFailureClosesReaderAndReturnsPoolLease() throws Exception {
    var closes = new AtomicInteger();
    var readerCloses = new AtomicInteger();
    try (var db = database();
        var allocator = new RootAllocator()) {
      var engine =
          JdbcEngine.pooled("lake", SqlDialect.DUCKDB, LifecycleTest.pool(db, closes))
              .withArrow(
                  (r, a, n) ->
                      new ArrowReader(a) {
                        public boolean loadNextBatch() throws IOException {
                          throw new IOException("batch failed");
                        }

                        public long bytesRead() {
                          return 0;
                        }

                        protected Schema readSchema() {
                          return new Schema(List.of());
                        }

                        protected void closeReadSource() {
                          readerCloses.incrementAndGet();
                        }
                      },
                  allocator,
                  128);
      var graph = new OrchidDB(compiler, PlanCache.none(), engine).graph(MAPPING);
      var result = graph.queryArrow(Query.cypher("MATCH (p:Person) RETURN p.name"));
      assertThrows(SQLException.class, result::nextBatch);
      result.close();
      assertEquals(1, closes.get());
      assertEquals(1, readerCloses.get());
      noLeaks(allocator);
    }
  }

  @Test
  void sessionCloseAlsoClosesOutstandingArrowResults() throws Exception {
    try (var db = database();
        var allocator = new RootAllocator()) {
      var session = engine(db, allocator).openSession();
      var result = session.executeArrow(sql("SELECT * FROM people"));
      assertTrue(result.nextBatch());
      session.close();
      session.close();
      assertThrows(SQLException.class, result::nextBatch);
      result.close();
      noLeaks(allocator);
      assertFalse(db.isClosed());
    }
  }

  @Test
  void arrowCapabilityIsExplicitAndConfigurationSharesBorrowedLease() throws Exception {
    try (var db = database();
        var allocator = new RootAllocator()) {
      var plain = JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, db);
      var arrow = plain.withArrow(EXPORT, allocator, 128);
      assertThrows(IllegalArgumentException.class, () -> plain.withArrow(EXPORT, allocator, 0));
      try (var session = plain.openSession()) {
        assertThrows(
            SQLFeatureNotSupportedException.class, () -> session.executeArrow(sql("SELECT 1")));
        assertThrows(SQLException.class, arrow::openSession);
      }
      try (var session = arrow.openSession()) {
        assertThrows(
            SQLException.class,
            () ->
                session.executeArrow(
                    new CompiledQuery("other", SqlDialect.DUCKDB, "SELECT 1", List.of())));
      }
      noLeaks(allocator);
    }
  }

  @Test
  void readerSchemaFailureClosesAllAcquiredResources() throws Exception {
    var closes = new AtomicInteger();
    var readerCloses = new AtomicInteger();
    try (var db = database();
        var allocator = new RootAllocator()) {
      var engine =
          JdbcEngine.pooled("lake", SqlDialect.DUCKDB, LifecycleTest.pool(db, closes))
              .withArrow(
                  (r, a, n) ->
                      new ArrowReader(a) {
                        public boolean loadNextBatch() {
                          return false;
                        }

                        public long bytesRead() {
                          return 0;
                        }

                        protected Schema readSchema() throws IOException {
                          throw new IOException("schema failed");
                        }

                        protected void closeReadSource() {
                          readerCloses.incrementAndGet();
                        }
                      },
                  allocator,
                  128);
      var graph = new OrchidDB(compiler, PlanCache.none(), engine).graph(MAPPING);
      assertThrows(
          SQLException.class,
          () -> graph.queryArrow(Query.cypher("MATCH (p:Person) RETURN p.name")));
      assertEquals(1, closes.get());
      assertEquals(1, readerCloses.get());
      noLeaks(allocator);
    }
  }

  @Test
  void readerCloseFailureStillReleasesAllocatorAndPoolLease() throws Exception {
    var closes = new AtomicInteger();
    try (var db = database();
        var allocator = new RootAllocator()) {
      var engine =
          JdbcEngine.pooled("lake", SqlDialect.DUCKDB, LifecycleTest.pool(db, closes))
              .withArrow(
                  (r, a, n) ->
                      new ArrowReader(a) {
                        public boolean loadNextBatch() {
                          return false;
                        }

                        public long bytesRead() {
                          return 0;
                        }

                        protected Schema readSchema() {
                          return new Schema(List.of());
                        }

                        protected void closeReadSource() throws IOException {
                          throw new IOException("close failed");
                        }
                      },
                  allocator,
                  128);
      var graph = new OrchidDB(compiler, PlanCache.none(), engine).graph(MAPPING);
      var result = graph.queryArrow(Query.cypher("MATCH (p:Person) RETURN p.name"));
      assertThrows(SQLException.class, result::close);
      result.close();
      assertEquals(1, closes.get());
      noLeaks(allocator);
    }
  }
}
