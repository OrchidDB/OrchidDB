package io.orchiddb.gremlin;

import static org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.__.*;
import static org.junit.jupiter.api.Assertions.*;

import io.orchiddb.*;
import java.nio.file.Path;
import java.sql.*;
import java.util.*;
import java.util.function.Function;
import org.apache.tinkerpop.gremlin.process.traversal.Order;
import org.apache.tinkerpop.gremlin.process.traversal.P;
import org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.GraphTraversalSource;
import org.apache.tinkerpop.gremlin.structure.T;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.TinkerGraph;
import org.junit.jupiter.api.*;

class GremlinIntegrationTest {
  static NativeSqlCompiler compiler;
  Connection connection;
  OrchidDB.Graph graph;
  GraphTraversalSource g;
  TinkerGraph reference;
  GraphTraversalSource expected;
  static final String SPECIAL = "O'Reilly \"🪷\"\\test\nline";

  @BeforeAll
  static void load() {
    compiler = NativeSqlCompiler.load(Path.of(System.getProperty("orchiddb.native.path")));
  }

  @BeforeEach
  void setup() throws Exception {
    connection = DriverManager.getConnection("jdbc:duckdb:");
    try (var s = connection.createStatement()) {
      s.execute("CREATE TEMP TABLE people(id BIGINT, name VARCHAR, age BIGINT)");
      s.execute("INSERT INTO people VALUES (1,'Ada',36),(2,'Grace',45),(3,'Linus',28)");
      s.execute("CREATE TEMP TABLE knows(id BIGINT, src BIGINT, dst BIGINT)");
      s.execute("INSERT INTO knows VALUES (10,1,2),(11,1,2),(12,2,3)");
    }
    try (var s = connection.prepareStatement("INSERT INTO people VALUES (4,?,50)")) {
      s.setString(1, SPECIAL);
      s.executeUpdate();
    }
    graph =
        new OrchidDB(
                compiler,
                PlanCache.bounded(32),
                JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
            .graph(
                new GraphMapping(
                    List.of(
                        NodeMapping.node("Person", Source.table("lake", "people"), "id")
                            .property("name", "name")
                            .property("age", "age")),
                    List.of(
                        EdgeMapping.edge(
                            "KNOWS",
                            Source.table("lake", "knows"),
                            "id",
                            "src",
                            "dst",
                            "Person",
                            "Person"))));
    g = OrchidGremlin.traversal(graph);
    reference = TinkerGraph.open();
    var ada = reference.addVertex(T.id, 1L, T.label, "Person", "name", "Ada", "age", 36L);
    var grace = reference.addVertex(T.id, 2L, T.label, "Person", "name", "Grace", "age", 45L);
    var linus = reference.addVertex(T.id, 3L, T.label, "Person", "name", "Linus", "age", 28L);
    reference.addVertex(T.id, 4L, T.label, "Person", "name", SPECIAL, "age", 50L);
    ada.addEdge("KNOWS", grace, T.id, 10L);
    ada.addEdge("KNOWS", grace, T.id, 11L);
    grace.addEdge("KNOWS", linus, T.id, 12L);
    expected = reference.traversal();
  }

  @AfterEach
  void close() throws Exception {
    g.close();
    expected.close();
    reference.close();
    connection.close();
  }

  void matches(Function<GraphTraversalSource, List<?>> query) {
    assertEquals(query.apply(expected), query.apply(g));
  }

  @Test
  void fluentRelationshipTraversal() {
    matches(t -> t.V().hasLabel("Person").has("name", "Ada").out("KNOWS").values("name").toList());
  }

  @Test
  void idsLabelsAndEdges() {
    matches(t -> t.V(1L).outE("KNOWS").inV().id().order().toList());
    matches(t -> t.E().label().order().toList());
    matches(t -> t.E(12L).outV().values("name").toList());
  }

  @Test
  void scalarPredicates() {
    matches(t -> t.V().has("age", P.gte(30L).and(P.lt(50L))).values("name").order().toList());
    matches(t -> t.V().has("name", P.within("Ada", "Linus")).values("name").order().toList());
    matches(t -> t.V().values("age").is(P.gt(40L)).order().toList());
  }

  @Test
  void slicesAndOrdering() {
    matches(t -> t.V().values("age").order().by(Order.desc).range(1, 3).toList());
    matches(t -> t.V().order().by("age", Order.desc).values("name").limit(2).toList());
  }

  @Test
  void nestedFilters() {
    matches(t -> t.V().filter(out("KNOWS")).values("name").order().toList());
    matches(t -> t.V().not(out("KNOWS")).values("name").order().toList());
  }

  @Test
  void duplicatesAndCounts() {
    matches(t -> t.V(1L).out("KNOWS").count().toList());
    matches(t -> t.V(1L).out("KNOWS").values("name").dedup().toList());
    matches(t -> t.V().has("name", "absent").count().toList());
  }

  @Test
  void escapingAndInjectionRemainValues() {
    matches(t -> t.V().has("name", SPECIAL).values("name").toList());
    matches(t -> t.V().has("name", "\");g.V().drop();//").values("name").toList());
  }

  @Test
  void partialIterationReleasesBorrowedConnection() throws Exception {
    var traversal = g.V().values("name");
    assertTrue(traversal.hasNext());
    traversal.next();
    assertEquals(4L, g.V().count().next());
    traversal.close();
    g.close();
    assertFalse(connection.isClosed());
    try (var s = connection.createStatement();
        var r = s.executeQuery("SELECT count(*) FROM people")) {
      assertTrue(r.next());
      assertEquals(4, r.getInt(1));
    }
    assertThrows(IllegalStateException.class, () -> g.V().count().next());
  }

  @Test
  void boundedResultsFailWithoutTruncationAndReleaseLease() throws Exception {
    try (var small = OrchidGremlin.traversal(graph, 1)) {
      assertThrows(IllegalStateException.class, () -> small.V().values("name").toList());
      assertEquals(List.of("Ada"), small.V(1L).values("name").toList());
    }
  }

  @Test
  void rejectsUnsupportedSemantics() {
    assertThrows(UnsupportedOperationException.class, () -> g.V().toList());
    assertThrows(UnsupportedOperationException.class, () -> g.V().path().toList());
    assertThrows(UnsupportedOperationException.class, () -> g.addV("Person").iterate());
    assertThrows(UnsupportedOperationException.class, () -> g.V().drop().iterate());
    assertThrows(UnsupportedOperationException.class, () -> g.V().filter(t -> true).count().next());
    assertThrows(UnsupportedOperationException.class, () -> g.withBulk(false).V().count().next());
    assertEquals(4L, g.V().count().next());
  }

  @Test
  void callerTransactionIsPreserved() throws Exception {
    connection.setAutoCommit(false);
    try (var s = connection.createStatement()) {
      s.execute("DELETE FROM knows WHERE id=10");
    }
    assertEquals(2L, g.E().count().next());
    connection.rollback();
    assertEquals(3L, g.E().count().next());
    assertFalse(connection.getAutoCommit());
  }

  @Test
  void fluentGremlinUsesConfiguredArrowBackend() throws Exception {
    try (var allocator = new org.apache.arrow.memory.RootAllocator()) {
      var exports = new java.util.concurrent.atomic.AtomicInteger();
      var engine =
          JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection)
              .withArrow(
                  (rows, memory, size) -> {
                    exports.incrementAndGet();
                    return (org.apache.arrow.vector.ipc.ArrowReader)
                        rows.unwrap(org.duckdb.DuckDBResultSet.class)
                            .arrowExportStream(memory, size);
                  },
                  allocator,
                  128);
      var mapping =
          new GraphMapping(
              List.of(
                  NodeMapping.node("Person", Source.table("lake", "people"), "id")
                      .property("name", "name")),
              List.of());
      var arrowGraph = new OrchidDB(compiler, PlanCache.none(), engine).graph(mapping);
      try (var traversal = OrchidGremlin.traversal(arrowGraph)) {
        assertEquals(
            List.of("Ada", "Grace", "Linus", SPECIAL),
            traversal.V().order().by("name").values("name").toList());
      }
      assertEquals(1, exports.get());
      assertEquals(0, allocator.getAllocatedMemory());
      assertTrue(allocator.getChildAllocators().isEmpty());
      assertFalse(connection.isClosed());
    }
  }
}
