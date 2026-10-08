package io.orchiddb;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.util.List;
import java.util.Map;

public final class Catalog {
  private static final ObjectMapper JSON = new ObjectMapper();
  private final ObjectNode reference;

  public Catalog(String endpoint, String scope, String graph) {
    reference = JSON.createObjectNode().put("endpoint", endpoint).put("scope", scope)
        .put("graph", graph).put("token_env", "ORCHID_CATALOG_TOKEN");
  }

  public Catalog(String endpoint, String scope, String graph, CatalogAuth auth) {
    this(endpoint, scope, graph);
    reference.set("auth", auth.configuration());
  }

  public Catalog authenticatedWith(CatalogAuth auth) {
    var next = reference.deepCopy();
    next.set("auth", auth.configuration());
    return new Catalog(next);
  }

  private Catalog(ObjectNode reference) { this.reference = reference; }

  public Catalog atRevision(long revision) {
    if (revision < 1) throw new IllegalArgumentException("revision must be positive");
    return new Catalog(reference.deepCopy().put("revision", revision));
  }

  public Catalog tokenEnvironment(String name) {
    var next = reference.deepCopy();
    next.remove("auth");
    return new Catalog(next.put("token_env", name));
  }

  String schemaJson() {
    return JSON.createObjectNode().set("catalog", reference).toString();
  }

  private JsonNode command(String action, Map<String, ?> values) {
    var request = JSON.createObjectNode().put("op", "catalog").put("action", action);
    request.set("catalog", reference);
    values.forEach((name, value) -> request.set(name, JSON.valueToTree(value)));
    try {
      return JSON.readTree(NativeSqlCompiler.load().command(request.toString()));
    } catch (java.io.IOException e) {
      throw new IllegalStateException("Invalid catalog response", e);
    }
  }

  public JsonNode discover() { return command("discover", Map.of()); }
  public JsonNode discover(String search) { return command("discover", Map.of("search", search)); }
  public JsonNode edges() { return command("edges", Map.of()).get("objects"); }
  public JsonNode edges(String search) { return command("edges", Map.of("search", search)).get("objects"); }
  public JsonNode object(String id) { return command("object", Map.of("id", id)); }
  public JsonNode draft() { return command("draft", Map.of()); }

  public JsonNode principals() { return command("principals", Map.of()); }
  public JsonNode principal(String id) { return command("principal", Map.of("id", id)); }
  public JsonNode registerPrincipal(String id, Principal principal) {
    return registerPrincipal(id, principal, 0, true);
  }
  public JsonNode registerPrincipal(String id, Principal principal, long expectedVersion, boolean enabled) {
    return command("register_principal", Map.of("id", id, "principal", principal,
        "expected_version", expectedVersion, "enabled", enabled));
  }
  public JsonNode grants() { return command("grants", Map.of()); }
  public JsonNode setGrants(List<String> discover, List<String> execute, long expectedVersion) {
    return command("set_grants", Map.of("expected_version", expectedVersion,
        "definition", Map.of("discover", discover, "execute", execute)));
  }
  public record Principal(String subject, List<String> roles, boolean admin, String tenant) {
    public Principal(String subject, List<String> roles) { this(subject, roles, false, null); }
    public Principal { roles = List.copyOf(roles); }
  }

  public JsonNode registerEdge(String id, CypherEdge edge) { return registerEdge(id, edge, 0); }

  public JsonNode registerEdge(String id, CypherEdge edge, long expectedVersion) {
    return command("register_edge", Map.of("id", id, "expected_version", expectedVersion,
        "definition", edge.definition()));
  }

  public JsonNode registerGraph(List<String> objects, String description, long expectedVersion) {
    return registerGraph(objects, description, expectedVersion, null);
  }

  public JsonNode registerGraph(List<String> objects, String description, long expectedVersion,
      String executionConnector) {
    var definition = JSON.createObjectNode().put("description", description);
    definition.set("objects", JSON.valueToTree(objects));
    if (executionConnector != null) definition.put("execution_connector", executionConnector);
    return command("register_graph", Map.of("expected_version", expectedVersion, "definition", definition));
  }

  public JsonNode publish(long expectedRevision, long graphVersion, Map<String, Long> objectVersions) {
    return command("publish", Map.of("publication", Map.of("expected_revision", expectedRevision,
        "graph_version", graphVersion, "object_versions", objectVersions)));
  }

  public record CypherEdge(String name, String source, String target, String cypher,
      String description, String targetColumn, Map<String, ?> properties, List<Map<String, ?>> parameters) {
    public CypherEdge(String name, String source, String target, String cypher, String description) {
      this(name, source, target, cypher, description, "target", Map.of(), List.of());
    }
    public CypherEdge {
      properties = Map.copyOf(properties);
      parameters = List.copyOf(parameters);
    }
    public CypherEdge withProperty(String name, Map<String, ?> schema) {
      var next = new java.util.LinkedHashMap<String, Object>(properties);
      next.put(name, Map.copyOf(schema));
      return new CypherEdge(this.name, source, target, cypher, description, targetColumn, next, parameters);
    }
    Map<String, ?> definition() {
      return Map.of("kind", "cypher_relationship", "name", name, "source", source,
          "target", target, "cypher", cypher, "description", description, "parameters", parameters,
          "returns", Map.of("target", targetColumn, "properties", properties));
    }
  }
}
