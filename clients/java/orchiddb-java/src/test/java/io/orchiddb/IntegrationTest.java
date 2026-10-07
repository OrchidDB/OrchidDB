package io.orchiddb;

import static org.junit.jupiter.api.Assertions.*;

import java.nio.file.Path;
import java.sql.*;
import java.util.*;
import org.junit.jupiter.api.*;

class IntegrationTest {
  static final Source PEOPLE = Source.table("lake", "people");
  static final Source KNOWS = Source.table("lake", "knows");
  static final GraphMapping MAPPING =
      new GraphMapping(
          List.of(
              NodeMapping.node("Person", PEOPLE, "id")
                  .property("name", "name")
                  .property("age", "age")),
          List.of(EdgeMapping.edge("KNOWS", KNOWS, "id", "src", "dst", "Person", "Person")));
  static NativeSqlCompiler compiler;
  Connection connection;
  OrchidDB.Graph graph;

  @BeforeAll
  static void load() {
    compiler = NativeSqlCompiler.load(Path.of(System.getProperty("orchiddb.native.path")));
  }

  @BeforeEach
  void setup() throws Exception {
    connection = DriverManager.getConnection("jdbc:duckdb:");
    try (var s = connection.createStatement()) {
      s.execute("CREATE TEMP TABLE people(id BIGINT, name VARCHAR, age BIGINT)");
      s.execute(
          "INSERT INTO people VALUES (1, 'Ada', 36), (2, 'Grace', 45), (3, 'O''Reilly 🪷', 28)");
      s.execute("CREATE TEMP TABLE knows(id BIGINT, src BIGINT, dst BIGINT)");
      s.execute("INSERT INTO knows VALUES (10,1,2),(11,2,3)");
    }
    graph =
        new OrchidDB(
                compiler,
                PlanCache.bounded(16),
                JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
            .graph(MAPPING);
  }

  @AfterEach
  void teardown() throws Exception {
    connection.close();
  }

  static List<Object> values(QueryResult result) throws Exception {
    try (result) {
      var rows = new ArrayList<Object>();
      while (result.next()) rows.add(result.get(1));
      return rows;
    }
  }

  @Test
  void ignoresUnmappedComplexColumnsInExistingTables() throws Exception {
    try (var statement = connection.createStatement()) {
      statement.execute("ALTER TABLE people ADD COLUMN payload STRUCT(tags VARCHAR[])");
    }
    assertEquals(
        List.of("Grace"),
        values(graph.query(Query.cypher("MATCH (p:Person) WHERE p.name='Grace' RETURN p.name"))));
  }

  @Test
  void sameConnectionJoin() throws Exception {
    assertEquals(
        List.of("Grace"),
        values(
            graph.query(
                Query.cypher(
                    "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.name = $name RETURN b.name AS friend",
                    Map.of("name", "Ada")))));
    assertFalse(connection.isClosed());
  }

  @Test
  void permissionMembershipFiltersAtNodeSourceAndDeduplicatesGrants() throws Exception {
    try (var statement = connection.createStatement()) {
      statement.execute(
          "CREATE TEMP TABLE grants(resource_type VARCHAR, resource_rel VARCHAR, resource_id VARCHAR, subject_type VARCHAR, subject_rel VARCHAR, subject_id VARCHAR)");
      statement.execute(
          "INSERT INTO grants VALUES "
              + "('document','view','1','user','','alice'), "
              + "('document','view','1','user','','alice'), "
              + "('document','view','2','user','','bob'), "
              + "('document','view','2','user','member','alice'), "
              + "('document','edit','3','user','','alice'), "
              + "('document','view','3','user','','alice'), "
              + "('document','view','03','user','','alice')");
    }
    var grants = PermissionRelation.flat(Source.table("lake", "grants"), "document", "view");
    var protectedMapping =
        new GraphMapping(
            List.of(
                NodeMapping.node("Person", PEOPLE, "id").property("id", "id").protectWith(grants)),
            List.of());
    var protectedGraph =
        new OrchidDB(
                compiler,
                PlanCache.none(),
                JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
            .graph(protectedMapping);
    var query =
        Query.cypher("MATCH (p:Person) RETURN p.id AS id ORDER BY id")
            .as(new Authorization("user", "alice"));
    assertEquals(List.of(1L, 3L), values(protectedGraph.query(query)));
    assertThrows(
        PlanningException.class,
        () -> protectedGraph.plan(Query.cypher("MATCH (p:Person) RETURN p.id")));
  }

  @Test
  void permissionScopesCombineDirectAndCoarseResourceGrants() throws Exception {
    try (var statement = connection.createStatement()) {
      statement.execute(
          "CREATE TEMP TABLE scoped_docs(id BIGINT, project_id BIGINT, title VARCHAR)");
      statement.execute(
          "INSERT INTO scoped_docs VALUES (1, 10, 'direct'), (2, 20, 'project'), (3, 30, 'denied')");
      statement.execute(
          "CREATE TEMP TABLE scoped_grants(resource_type VARCHAR, resource_rel VARCHAR, resource_id VARCHAR, subject_type VARCHAR, subject_rel VARCHAR, subject_id VARCHAR)");
      statement.execute(
          "INSERT INTO scoped_grants VALUES "
              + "('document','view','1','user','','alice'), "
              + "('document','view','2','user','','alice'), "
              + "('project','view','20','user','','alice'), "
              + "('project','view','10','user','','alice'), "
              + "('project','view','30','user','','bob')");
    }
    var direct = PermissionRelation.flat(Source.table("lake", "scoped_grants"), "document", "view");
    var project = PermissionRelation.flat(Source.table("lake", "scoped_grants"), "project", "view");
    var mapping =
        new GraphMapping(
            List.of(
                NodeMapping.node("Document", Source.table("lake", "scoped_docs"), "id")
                    .property("id", "id")
                    .property("title", "title")
                    .property("project_id", "project_id")
                    .protectWith(direct)
                    .protectWith("project_id", project)),
            List.of());
    var protectedGraph =
        new OrchidDB(
                compiler,
                PlanCache.none(),
                JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
            .graph(mapping);
    var query =
        Query.cypher("MATCH (d:Document) RETURN d.title AS title ORDER BY title")
            .as(new Authorization("user", "alice"));
    assertEquals(List.of("direct", "project"), values(protectedGraph.query(query)));
  }

  @Test
  void permissionRelationSupportsCallerDefinedColumnNames() throws Exception {
    try (var statement = connection.createStatement()) {
      statement.execute(
          "CREATE TEMP TABLE custom_grants(object_kind VARCHAR, action_name VARCHAR, object_key VARCHAR, principal_kind VARCHAR, via VARCHAR, principal_key VARCHAR)");
      statement.execute(
          "INSERT INTO custom_grants VALUES "
              + "('person','read','1','account','','alice'), "
              + "('person','read','2','account','','bob')");
    }
    var grants =
        PermissionRelation.flat(
            Source.table("lake", "custom_grants"),
            "person",
            "read",
            "object_kind",
            "action_name",
            "object_key",
            "principal_kind",
            "via",
            "principal_key");
    var mapping =
        new GraphMapping(
            List.of(
                NodeMapping.node("Person", PEOPLE, "id").property("id", "id").protectWith(grants)),
            List.of());
    var protectedGraph =
        new OrchidDB(
                compiler,
                PlanCache.none(),
                JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
            .graph(mapping);
    var query =
        Query.cypher("MATCH (p:Person) RETURN p.id AS id ORDER BY id")
            .as(new Authorization("account", "alice"));
    assertEquals(List.of(1L), values(protectedGraph.query(query)));
  }

  @Test
  void literalParametersAreBoundStructurally() throws Exception {
    assertEquals(
        List.of("O'Reilly 🪷"),
        values(
            graph.query(
                Query.cypher(
                    "MATCH (p:Person) WHERE p.name = $name RETURN p.name",
                    Map.of("name", "O'Reilly 🪷")))));
    assertEquals(
        List.of(),
        values(
            graph.query(
                Query.cypher(
                    "MATCH (p:Person) WHERE p.name = $name RETURN p.name",
                    Map.of("name", "' OR true --")))));
    assertEquals(
        List.of("Ada"),
        values(
            graph.query(
                Query.cypher(
                    "MATCH (p:Person) WHERE p.name = $name RETURN p.name",
                    Map.of("name", "Ada")))));
  }

  @Test
  void gremlin() throws Exception {
    assertEquals(
        List.of("Grace"),
        values(
            graph.query(
                Query.gremlin(
                    "g.V().hasLabel('Person').has('name','Ada').out('KNOWS').values('name')"))));
  }

  @Test
  void callerOwnsTransactions() throws Exception {
    connection.setAutoCommit(false);
    try (var s = connection.createStatement()) {
      s.executeUpdate("INSERT INTO people VALUES (4,'Linus',55)");
    }
    assertEquals(
        List.of("Linus"),
        values(graph.query(Query.cypher("MATCH (p:Person) WHERE p.name = 'Linus' RETURN p.name"))));
    assertFalse(connection.getAutoCommit());
    connection.rollback();
    assertEquals(
        List.of(),
        values(graph.query(Query.cypher("MATCH (p:Person) WHERE p.name = 'Linus' RETURN p.name"))));
  }

  @Test
  void customFunctionUsesCallerSession() throws Exception {
    try (var s = connection.createStatement()) {
      s.execute("CREATE TEMP MACRO add_ten(x) AS x + 10");
    }
    var withFunction =
        new OrchidDB(
                compiler,
                PlanCache.none(),
                JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
            .graph(
                MAPPING,
                List.of(FunctionSignature.scalar("boostAge", "add_ten", "int64", "int64")));
    assertEquals(
        List.of(46L),
        values(
            withFunction.query(
                Query.cypher(
                    "MATCH (p:Person) WHERE p.name = 'Ada' RETURN boostAge(p.age) AS boosted"))));
  }

  @Test
  void mutationsAndMissingParamsFailBeforeExecution() {
    assertThrows(
        PlanningException.class, () -> graph.query(Query.cypher("MATCH (p:Person) DELETE p")));
    assertThrows(
        PlanningException.class,
        () -> graph.query(Query.cypher("MATCH (p:Person) WHERE p.name=$missing RETURN p.name")));
  }

  @Test
  void cursorLeaseFailsFastAndReleases() throws Exception {
    var cursor = graph.query(Query.cypher("MATCH (p:Person) RETURN p.name"));
    assertThrows(SQLException.class, () -> graph.plan(Query.cypher("RETURN 1")));
    cursor.close();
    cursor.close();
    assertEquals(List.of(1), values(graph.query(Query.cypher("RETURN 1"))));
  }

  @Test
  void sparqlMappedOntology() throws Exception {
    var ontology =
        new Ontology(
            List.of(new Ontology.ClassMapping("http://example.org/Person", "Person", null)),
            List.of(new Ontology.PropertyMapping("http://example.org/name", "Person", "name")),
            List.of());
    assertEquals(
        List.of("Ada", "Grace", "O'Reilly 🪷"),
        values(
            graph.query(
                Query.sparql(
                    "SELECT ?name WHERE { ?p a <http://example.org/Person> ; <http://example.org/name> ?name . } ORDER BY ?name",
                    ontology))));
  }

  @Test
  void offlinePostgresAndUnsupportedDialect() {
    var schemas =
        Map.of(
            PEOPLE,
            List.of(
                new Column("id", "int64", false),
                new Column("name", "string", true),
                new Column("age", "int64", true)),
            KNOWS,
            List.of(
                new Column("id", "int64", false),
                new Column("src", "int64", false),
                new Column("dst", "int64", false)));
    var plan =
        compiler.compile(
            new Compilation(
                "lake",
                SqlDialect.POSTGRES,
                MAPPING,
                schemas,
                List.of(),
                Query.cypher("MATCH (p:Person) RETURN p.name")));
    assertTrue(plan.sql().contains("people"));
    assertThrows(
        PlanningException.class,
        () ->
            compiler.compile(
                new Compilation(
                    "lake",
                    new SqlDialect("clickhouse"),
                    MAPPING,
                    schemas,
                    List.of(),
                    Query.cypher("RETURN 1"))));
  }

  @Test
  void javaUdfUsesExactCallerConnection() throws Exception {
    org.duckdb.DuckDBFunctions.scalarFunction()
        .withName("java_plus_one")
        .withParameter(long.class)
        .withReturnType(long.class)
        .withLongFunction(x -> x + 1)
        .register(connection);
    var g =
        new OrchidDB(
                compiler,
                PlanCache.none(),
                JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
            .graph(
                MAPPING,
                List.of(FunctionSignature.scalar("nextAge", "java_plus_one", "int64", "int64")));
    assertEquals(
        List.of(37L),
        values(g.query(Query.cypher("MATCH (p:Person) WHERE p.name='Ada' RETURN nextAge(p.age)"))));
  }

  @Test
  void quotedQualifiedIdentifiers() throws Exception {
    try (var s = connection.createStatement()) {
      s.execute(
          "CREATE SCHEMA \"Graph Data\"; CREATE TABLE \"Graph Data\".\"People Set\"(\"Node ID\" BIGINT, \"Display Name\" VARCHAR); INSERT INTO \"Graph Data\".\"People Set\" VALUES (1,'Ada')");
    }
    var source = Source.table("lake", "Graph Data", "People Set");
    var mapping =
        new GraphMapping(
            List.of(NodeMapping.node("Person", source, "Node ID").property("name", "Display Name")),
            List.of());
    var g =
        new OrchidDB(
                compiler,
                PlanCache.none(),
                JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
            .graph(mapping);
    assertEquals(List.of("Ada"), values(g.query(Query.cypher("MATCH (p:Person) RETURN p.name"))));
  }

  @Test
  void nullAndListParameters() throws Exception {
    var parameters = new HashMap<String, Object>();
    parameters.put("name", null);
    assertEquals(
        List.of(),
        values(
            graph.query(
                Query.cypher("MATCH (p:Person) WHERE p.name=$name RETURN p.name", parameters))));
    assertEquals(
        List.of("Ada", "Grace"),
        values(
            graph.query(
                Query.cypher(
                    "MATCH (p:Person) WHERE p.name IN $names RETURN p.name ORDER BY p.name",
                    Map.of("names", List.of("Ada", "Grace"))))));
  }

  @Test
  void membershipKeepsNullAndEmptyListSemantics() throws Exception {
    var withNull = new ArrayList<Object>();
    withNull.add("Ada");
    withNull.add(null);
    assertEquals(
        List.of("Ada"),
        values(
            graph.query(
                Query.cypher(
                    "MATCH (p:Person) WHERE p.name IN $names RETURN p.name",
                    Map.of("names", withNull)))));
    assertEquals(
        List.of(),
        values(
            graph.query(
                Query.cypher(
                    "MATCH (p:Person) WHERE p.name IN $names RETURN p.name",
                    Map.of("names", List.of())))));
  }

  @Test
  void edgePropertyAggregationKeepsGraphBindingIdentity() throws Exception {
    try (var statement = connection.createStatement()) {
      statement.execute("ALTER TABLE knows ADD COLUMN weight DOUBLE DEFAULT 1.5");
    }
    var mapping =
        new GraphMapping(
            MAPPING.nodes(),
            List.of(
                EdgeMapping.edge("KNOWS", KNOWS, "id", "src", "dst", "Person", "Person")
                    .property("weight", "weight")));
    var g =
        new OrchidDB(
                compiler,
                PlanCache.none(),
                JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
            .graph(mapping);
    try (var result =
        g.query(
            Query.cypher(
                "MATCH (p:Person)-[r:KNOWS]->(:Person) RETURN p.name AS name, sum(r.weight) AS weight ORDER BY name"))) {
      assertTrue(result.next());
      assertEquals("Ada", result.get("name"));
      assertEquals(1.5, result.get("weight"));
      assertTrue(result.next());
      assertEquals("Grace", result.get("name"));
      assertEquals(1.5, result.get("weight"));
      assertFalse(result.next());
    }
  }

  @Test
  void rdfRulesUseApplicationColumnsWithoutTypeRoot() throws Exception {
    var rule =
        Map.<String, Object>of(
            "table", PEOPLE.sql(SqlDialect.DUCKDB),
            "subject",
                Map.of("kind", "template", "prefix", "urn:person:", "columns", List.of("id")),
            "predicate", Map.of("kind", "constant", "value", "urn:name"),
            "object", Map.of("kind", "literal", "column", "name"));
    assertEquals(
        List.of("Ada", "Grace", "O'Reilly 🪷"),
        values(
            graph.query(
                Query.sparql(
                    "SELECT ?name WHERE {?s <urn:name> ?name} ORDER BY ?name",
                    List.of(rule),
                    "default"))));
  }
}
