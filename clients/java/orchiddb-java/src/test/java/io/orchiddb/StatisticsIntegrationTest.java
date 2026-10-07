package io.orchiddb;

import static org.junit.jupiter.api.Assertions.*;

import com.fasterxml.jackson.databind.ObjectMapper;
import java.nio.file.*;
import java.sql.*;
import java.util.*;
import org.junit.jupiter.api.Test;

class StatisticsIntegrationTest {
  @Test
  void generatesAndReusesOneCatalogAcrossAllLanguages() throws Exception {
    var compiler = NativeSqlCompiler.load(Path.of(System.getProperty("orchiddb.native.path")));
    try (var connection = DriverManager.getConnection("jdbc:duckdb:")) {
      try (var statement = connection.createStatement()) {
        statement.execute("CREATE TABLE people(id BIGINT, name VARCHAR, age BIGINT)");
        statement.execute("INSERT INTO people VALUES (1,'Ada',36),(2,'Grace',45),(3,'Linus',36)");
      }
      var source = Source.table("lake", "people");
      var mapping =
          new GraphMapping(
              List.of(
                  NodeMapping.node("Person", source, "id")
                      .property("name", "name")
                      .property("age", "age")),
              List.of());
      var ontology =
          new Ontology(
              List.of(new Ontology.ClassMapping("http://example.org/Person", "Person", null)),
              List.of(new Ontology.PropertyMapping("http://example.org/name", "Person", "name")),
              List.of());
      var queries =
          List.of(
              Query.cypher("MATCH (p:Person) WHERE p.name='Ada' RETURN p.name"),
              Query.gremlin("g.V().hasLabel('Person').has('name','Ada').values('name')"),
              Query.sparql(
                  "SELECT ?name WHERE { ?p a <http://example.org/Person> ; <http://example.org/name> ?name . FILTER(?name = 'Ada') }",
                  ontology));
      try (var graph =
          new OrchidDB(
                  compiler,
                  PlanCache.bounded(16),
                  JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection))
              .graph(mapping)) {
        for (var query : queries)
          assertEquals(List.of("Ada"), IntegrationTest.values(graph.query(query)));
        var report = new ObjectMapper().readTree(graph.generateStatistics());
        assertTrue(report.path("accepted_rows").asInt() > 0, report.toString());
        var snapshot = new ObjectMapper().readTree(graph.statisticsSnapshotJson());
        assertFalse(snapshot.path("sources").isEmpty());
        assertTrue(snapshot.path("sources").elements().next().path("sample_rows").asInt() > 0);
        for (var query : queries) {
          assertEquals(List.of("Ada"), IntegrationTest.values(graph.query(query)));
          var diagnostics = new ObjectMapper().readTree(graph.plan(query).diagnosticsJson());
          assertEquals(snapshot.path("revision"), diagnostics.path("statistics_usage"));
          assertFalse(diagnostics.path("plan_estimates").isEmpty());
          assertTrue(
              java.util.stream.StreamSupport.stream(
                      diagnostics.path("plan_estimates").spliterator(), false)
                  .anyMatch(e -> !e.path("estimated_rows").isNull()));
        }
        var path = Files.createTempFile("orchiddb-native-statistics", ".json");
        try {
          graph.saveStatistics(path);
          graph.clearStatistics();
          graph.loadStatistics(path);
          assertEquals(List.of("Ada"), IntegrationTest.values(graph.query(queries.get(0))));
        } finally {
          Files.deleteIfExists(path);
        }
      }
    }
  }
}
