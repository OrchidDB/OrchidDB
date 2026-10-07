package io.orchiddb;

import static org.junit.jupiter.api.Assertions.*;

import java.lang.reflect.*;
import java.sql.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicInteger;
import javax.sql.DataSource;
import org.junit.jupiter.api.*;

class LifecycleTest {
  static final Source SOURCE = Source.table("lake", "people");
  static final GraphMapping MAPPING =
      new GraphMapping(List.of(NodeMapping.node("Person", SOURCE, "id")), List.of());

  static Connection database() throws SQLException {
    var c = DriverManager.getConnection("jdbc:duckdb:");
    try (var s = c.createStatement()) {
      s.execute("CREATE TABLE people(id BIGINT); INSERT INTO people VALUES (1)");
    }
    return c;
  }

  static DataSource pool(Connection c, AtomicInteger closes) {
    var wrapped =
        (Connection)
            Proxy.newProxyInstance(
                LifecycleTest.class.getClassLoader(),
                new Class[] {Connection.class},
                (p, m, a) -> {
                  if (m.getName().equals("close")) closes.incrementAndGet();
                  try {
                    return m.invoke(c, a);
                  } catch (InvocationTargetException e) {
                    throw e.getCause();
                  }
                });
    return (DataSource)
        Proxy.newProxyInstance(
            LifecycleTest.class.getClassLoader(),
            new Class[] {DataSource.class},
            (p, m, a) -> {
              if (m.getName().equals("getConnection")) return wrapped;
              throw new UnsupportedOperationException(m.getName());
            });
  }

  static SqlCompiler sql(String sql) {
    return r -> new CompiledQuery(r.engine(), r.dialect(), sql, List.of("id"));
  }

  @Test
  void pooledConnectionLivesUntilCursorCloses() throws Exception {
    var closes = new AtomicInteger();
    try (var c = database()) {
      var g =
          new OrchidDB(
                  sql("SELECT id FROM people"),
                  PlanCache.none(),
                  JdbcEngine.pooled("lake", SqlDialect.DUCKDB, pool(c, closes)))
              .graph(MAPPING);
      var result = g.query(Query.cypher("ignored by test compiler"));
      assertTrue(result.next());
      assertEquals(1L, result.get(1));
      assertEquals(0, closes.get());
      result.close();
      result.close();
      assertEquals(1, closes.get());
      assertTrue(c.isClosed());
    }
  }

  @Test
  void failuresReturnPoolConnection() throws Exception {
    for (SqlCompiler compiler :
        List.of(
            sql("SELECT * FROM nonexistent"),
            (SqlCompiler)
                r -> {
                  throw new PlanningException("test failure");
                })) {
      var closes = new AtomicInteger();
      try (var c = database()) {
        var g =
            new OrchidDB(
                    compiler,
                    PlanCache.none(),
                    JdbcEngine.pooled("lake", SqlDialect.DUCKDB, pool(c, closes)))
                .graph(MAPPING);
        assertThrows(Exception.class, () -> g.query(Query.cypher("ignored")));
        assertEquals(1, closes.get());
      }
    }
  }

  @Test
  void schemaFailureReturnsPoolConnection() throws Exception {
    var closes = new AtomicInteger();
    try (var c = database()) {
      try (var s = c.createStatement()) {
        s.execute("DROP TABLE people");
      }
      var g =
          new OrchidDB(
                  sql("SELECT 1"),
                  PlanCache.none(),
                  JdbcEngine.pooled("lake", SqlDialect.DUCKDB, pool(c, closes)))
              .graph(MAPPING);
      assertThrows(SQLException.class, () -> g.plan(Query.cypher("ignored")));
      assertEquals(1, closes.get());
    }
  }

  @Test
  void cacheIncludesParamsAndSchemaButNeverResults() throws Exception {
    var compiles = new AtomicInteger();
    SqlCompiler compiler =
        r -> {
          compiles.incrementAndGet();
          return new CompiledQuery(
              r.engine(), r.dialect(), "SELECT count(*) FROM people", List.of("count"));
        };
    try (var c = database()) {
      var g =
          new OrchidDB(
                  compiler, PlanCache.bounded(3), JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, c))
              .graph(MAPPING);
      var a = Query.cypher("ignored", Map.of("x", 1));
      var b = Query.cypher("ignored", Map.of("x", 2));
      g.plan(a);
      g.plan(a);
      assertEquals(1, compiles.get());
      g.plan(b);
      assertEquals(2, compiles.get());
      try (var s = c.createStatement()) {
        s.execute("INSERT INTO people VALUES (2)");
      }
      assertEquals(List.of(2L), IntegrationTest.values(g.query(a)));
      assertEquals(2, compiles.get());
      try (var s = c.createStatement()) {
        s.execute("ALTER TABLE people ADD COLUMN name VARCHAR");
      }
      g.plan(a);
      assertEquals(2, compiles.get(), "Unmapped columns do not invalidate plans");
      try (var s = c.createStatement()) {
        s.execute("ALTER TABLE people ALTER COLUMN id TYPE INTEGER");
      }
      g.plan(a);
      assertEquals(3, compiles.get());
    }
  }

  @Test
  void engineRoutingAndFederationBoundary() throws Exception {
    var other = Source.table("other", "people");
    var otherMapping =
        new GraphMapping(List.of(NodeMapping.node("Person", other, "id")), List.of());
    try (var c = database();
        var d = database()) {
      try (var s = d.createStatement()) {
        s.execute("UPDATE people SET id=2");
      }
      var db =
          new OrchidDB(
              sql("SELECT id FROM people"),
              PlanCache.none(),
              JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, c),
              JdbcEngine.borrowed("other", SqlDialect.DUCKDB, d));
      assertEquals(
          List.of(1L), IntegrationTest.values(db.graph(MAPPING).query(Query.cypher("ignored"))));
      assertEquals(
          List.of(2L),
          IntegrationTest.values(db.graph(otherMapping).query(Query.cypher("ignored"))));
      var mixed =
          new GraphMapping(
              List.of(NodeMapping.node("A", SOURCE, "id"), NodeMapping.node("B", other, "id")),
              List.of());
      assertThrows(PlanningException.class, () -> db.graph(mixed));
      assertThrows(
          PlanningException.class,
          () ->
              db.graph(
                  new GraphMapping(
                      List.of(NodeMapping.node("X", Source.table("missing", "people"), "id")),
                      List.of())));
    }
  }

  @Test
  void incorrectEnginePlanFailsAndBorrowedConnectionIsReusable() throws Exception {
    try (var c = database()) {
      var engine = JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, c);
      var g =
          new OrchidDB(
                  r -> new CompiledQuery("other", SqlDialect.DUCKDB, "SELECT 1", List.of()),
                  PlanCache.none(),
                  engine)
              .graph(MAPPING);
      assertThrows(PlanningException.class, () -> g.query(Query.cypher("ignored")));
      try (var session = engine.openSession()) {
        assertThrows(
            SQLException.class,
            () ->
                session.execute(
                    new CompiledQuery("lake", SqlDialect.POSTGRES, "SELECT 1", List.of())));
      }
      assertFalse(c.isClosed());
    }
  }

  @Test
  void nanosecondMetadataIsNotSilentlyDescribedAsMicroseconds() throws Exception {
    try (var c = database()) {
      try (var s = c.createStatement()) {
        s.execute("ALTER TABLE people ADD COLUMN created_at TIMESTAMP_NS");
      }
      var g =
          new OrchidDB(
                  sql("SELECT 1"),
                  PlanCache.none(),
                  JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, c))
              .graph(
                  new GraphMapping(
                      List.of(
                          NodeMapping.node("Person", SOURCE, "id")
                              .property("created", "created_at")),
                      List.of()));
      assertThrows(SQLException.class, () -> g.plan(Query.cypher("ignored")));
      assertFalse(c.isClosed());
    }
  }
}
