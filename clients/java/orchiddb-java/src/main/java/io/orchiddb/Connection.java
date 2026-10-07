package io.orchiddb;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.nio.file.*;
import java.sql.SQLException;
import java.util.*;

/** Registered JSON graph schema and execution on caller-owned engines. */
public final class Connection {
  private static final ObjectMapper JSON = new ObjectMapper();
  private final NativeSqlCompiler runtime = NativeSqlCompiler.load();
  private final ObjectNode schema;
  private final Map<String, ExecutionEngine> engines;
  private final ExecutionEngine primary;

  public Connection(String schemaJson, ExecutionEngine primary, ExecutionEngine... others) {
    this.primary = Objects.requireNonNull(primary);
    try {
      var input = JSON.createObjectNode().put("op", "validate_schema");
      input.set("schema", JSON.readTree(schemaJson));
      schema = (ObjectNode) JSON.readTree(runtime.command(input.toString()));
    } catch (Exception e) {
      throw new IllegalArgumentException("Invalid graph schema", e);
    }
    var registry = new HashMap<String, ExecutionEngine>();
    registry.put(primary.id(), primary);
    for (var engine : others)
      if (registry.putIfAbsent(engine.id(), engine) != null)
        throw new IllegalArgumentException("Duplicate engine " + engine.id());
    engines = Map.copyOf(registry);
  }

  public static Connection fromSchemaFile(
      Path path, ExecutionEngine primary, ExecutionEngine... others) throws java.io.IOException {
    return new Connection(Files.readString(path), primary, others);
  }

  public QueryResult query(String text) throws SQLException {
    return query(Query.cypher(text));
  }

  public QueryResult query(Query query) throws SQLException {
    var request = schema.deepCopy();
    request
        .put("version", 1)
        .put("dialect", primary.dialect().id())
        .put("query", query.text())
        .put("language", query.language());
    request.set("parameters", JSON.valueToTree(query.parameters()));
    if (query.authorization() != null)
      request.set(
          "authorization",
          JSON.valueToTree(
              Map.of(
                  "subject_type",
                  query.authorization().subjectType(),
                  "subject_id",
                  query.authorization().subjectId())));
    if (!request.has("engines")) {
      request.putObject("engines").putObject(primary.id()).put("dialect", primary.dialect().id());
      request.put("execution_engine", primary.id());
      request
          .withArray("tables")
          .forEach(table -> ((ObjectNode) table).put("engine", primary.id()));
    }
    return FederatedQuery.query(runtime, request.toString(), engines);
  }
}
