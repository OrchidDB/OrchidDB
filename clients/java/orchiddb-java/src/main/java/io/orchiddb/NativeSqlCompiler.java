package io.orchiddb;

import com.fasterxml.jackson.databind.ObjectMapper;
import io.orchiddb.internal.NativeBridge;
import io.orchiddb.internal.NativeLibrary;
import java.nio.file.Path;
import java.util.*;

/** Shared graph compiler and statistics coordinator. */
public final class NativeSqlCompiler implements SqlCompiler {
  private static final ObjectMapper JSON =
      new ObjectMapper()
          .enable(com.fasterxml.jackson.databind.DeserializationFeature.USE_BIG_INTEGER_FOR_INTS)
          .enable(com.fasterxml.jackson.databind.DeserializationFeature.USE_BIG_DECIMAL_FOR_FLOATS);
  private static Path loadedPath;

  private NativeSqlCompiler() {}

  /**
   * Loads the explicitly configured path, or the matching native classifier JAR on the classpath.
   */
  public static NativeSqlCompiler load() {
    String override = System.getProperty("orchiddb.native.path");
    return load(
        override == null || override.isBlank() ? NativeLibrary.extract() : Path.of(override));
  }

  public static synchronized NativeSqlCompiler load(Path library) {
    Path path = library.toAbsolutePath().normalize();
    if (loadedPath != null && !loadedPath.equals(path))
      throw new IllegalStateException("A different native compiler is already loaded");
    if (loadedPath == null) {
      try {
        System.load(path.toString());
      } catch (UnsatisfiedLinkError e) {
        throw new PlanningException(
            "Cannot load OrchidDB compiler at "
                + path
                + ". Run scripts/build.sh for this platform.",
            e);
      }
      loadedPath = path;
    }
    return new NativeSqlCompiler();
  }

  static synchronized void ensureLoaded() {
    if (loadedPath == null) load();
  }

  /** Execute a shared compiler protocol command, including dependent operation binding. */
  public String command(String command) {
    return NativeBridge.compileJson(command);
  }

  static Map<String, Object> request(Compilation r) {
    var request = new LinkedHashMap<String, Object>();
    request.put("version", 1);
    request.put("dialect", r.dialect().id());
    request.put("language", r.query().language());
    request.put("query", r.query().text());
    request.put("parameters", r.query().parameters());
    request.put("ontology", r.query().ontology());
    if (r.query().authorization() != null) {
      var principal = r.query().authorization();
      request.put(
          "authorization",
          Map.of("subject_type", principal.subjectType(), "subject_id", principal.subjectId()));
    }
    if (!r.query().rdf().isEmpty()) request.put("rdf", r.query().rdf());
    if (!r.query().dataset().equals("default")) request.put("dataset", r.query().dataset());
    request.put(
        "tables",
        r.schemas().entrySet().stream()
            .map(
                e ->
                    Map.of(
                        "name",
                        e.getKey().sql(r.dialect()),
                        "columns",
                        e.getValue().stream()
                            .map(
                                c ->
                                    Map.of(
                                        "name",
                                        c.name(),
                                        "data_type",
                                        c.dataType(),
                                        "nullable",
                                        c.nullable()))
                            .toList()))
            .toList());
    if (r.mapping().nodes().stream().anyMatch(n -> !n.permissionScopes().isEmpty())
        && r.query().authorization() == null)
      throw new PlanningException(
          "Query requires a principal because the graph contains protected node mappings");
    request.put(
        "nodes",
        r.mapping().nodes().stream()
            .map(
                n -> {
                  String table = n.source().sql(r.dialect());
                  var node = new LinkedHashMap<String, Object>();
                  node.put("label", n.label());
                  node.put("table", table);
                  node.put("id", n.id());
                  node.put("properties", n.properties());
                  if (!n.permissionScopes().isEmpty())
                    node.put(
                        "permission_scopes",
                        n.permissionScopes().stream()
                            .map(
                                scope -> {
                                  var p = scope.relation();
                                  if (!p.source().engine().equals(r.engine()))
                                    throw new PlanningException(
                                        "Permission relation is bound to a different engine");
                                  return Map.of(
                                      "resource_column",
                                      scope.resourceColumn(),
                                      "relation",
                                      Map.of(
                                          "table",
                                          p.source().sql(r.dialect()),
                                          "resource_type",
                                          p.resourceType(),
                                          "permission",
                                          p.permission(),
                                          "resource_type_column",
                                          p.resourceTypeColumn(),
                                          "permission_column",
                                          p.permissionColumn(),
                                          "resource_id_column",
                                          p.resourceIdColumn(),
                                          "subject_type_column",
                                          p.subjectTypeColumn(),
                                          "subject_relation_column",
                                          p.subjectRelationColumn(),
                                          "subject_id_column",
                                          p.subjectIdColumn()));
                                })
                            .toList());
                  return node;
                })
            .toList());
    request.put(
        "edges",
        r.mapping().edges().stream()
            .map(
                e ->
                    Map.of(
                        "label",
                        e.label(),
                        "table",
                        e.source().sql(r.dialect()),
                        "id",
                        e.id(),
                        "source",
                        e.sourceColumn(),
                        "target",
                        e.targetColumn(),
                        "source_label",
                        e.sourceLabel(),
                        "target_label",
                        e.targetLabel(),
                        "properties",
                        e.properties()))
            .toList());
    request.put("functions", r.functions());
    return request;
  }

  /** Compile the shared JSON protocol, including engine and table routing. */
  public CompiledQuery compileJson(String requestJson) {
    try {
      var request = JSON.readTree(requestJson);
      var result = JSON.readTree(NativeBridge.compileJson(requestJson));
      if (result.path("version").asInt() != 1
          || !result.path("dialect").asText().equals(request.path("dialect").asText()))
        throw new PlanningException("Native compiler protocol mismatch");
      var fields = new ArrayList<String>();
      result.path("fields").forEach(f -> fields.add(f.asText()));
      return new CompiledQuery(
          request.path("execution_engine").asText("default"),
          new SqlDialect(result.path("dialect").asText()),
          result.path("sql").asText(),
          fields,
          result.toString());
    } catch (java.io.IOException e) {
      throw new PlanningException("Invalid compiler response", e);
    }
  }

  public String statisticsCommand(String command) {
    return NativeBridge.statisticsJson(command);
  }

  public CompiledQuery compile(Compilation r) {
    try {
      var request = request(r);
      var result =
          JSON.readTree(
              r.statisticsCatalog() == null
                  ? NativeBridge.compileJson(JSON.writeValueAsString(request))
                  : statisticsCommand(
                      JSON.writeValueAsString(
                          Map.of(
                              "op",
                              "compile",
                              "catalog_id",
                              r.statisticsCatalog(),
                              "request",
                              request))));
      if (result.path("version").asInt() != 1
          || !result.path("dialect").asText().equals(r.dialect().id()))
        throw new PlanningException("Native compiler protocol mismatch");
      var fields = new ArrayList<String>();
      result.path("fields").forEach(f -> fields.add(f.asText()));
      return new CompiledQuery(
          r.engine(), r.dialect(), result.path("sql").asText(), fields, result.toString());
    } catch (java.io.IOException e) {
      throw new PlanningException("Invalid compiler response", e);
    }
  }
}
