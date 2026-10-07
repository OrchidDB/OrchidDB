package io.orchiddb;

import com.fasterxml.jackson.databind.ObjectMapper;
import java.sql.SQLException;
import java.util.*;

/**
 * Shared JSON SQL-island execution. Source data is bound into the final SELECT; no database objects
 * are created.
 */
public final class FederatedQuery {
  private FederatedQuery() {}

  public static QueryResult query(
      NativeSqlCompiler compiler, String request, Map<String, ExecutionEngine> engines)
      throws SQLException {
    var plan = compiler.compileJson(request);
    var resources = new ArrayList<AutoCloseable>();
    var sessions = new HashMap<String, ExecutionEngine.Session>();
    try {
      while (true) {
        var metadata = readJson(plan.diagnosticsJson());
        var transfers = metadata.path("transfers");
        if (transfers.isMissingNode() || transfers.isEmpty()) break;
        var transfer = transfers.get(0);
        var operation =
            transfer.hasNonNull("request") ? transfer.get("request") : transfer.get("operation");
        if (operation == null || operation.isNull()) operation = transfer.get("search");
        if (transfer.hasNonNull("request") && transfer.hasNonNull("operation"))
          throw new SQLException("Transfer has both SQL and request operations");
        var inputColumns =
            operation == null || operation.isNull()
                ? transfer.path("columns")
                : operation.path("input_columns");
        com.fasterxml.jackson.databind.node.ArrayNode values;
        if (transfer.path("sql").asText().isBlank()) {
          if (operation == null || operation.isNull() || !inputColumns.isEmpty())
            throw new SQLException("Empty source SQL is only valid for a closed operation");
          values = JSON.createArrayNode().add(JSON.createArrayNode());
        } else {
          var source =
              session(
                  transfer.path("source_engine").asText(),
                  transfer.path("source_dialect").asText(),
                  engines,
                  sessions,
                  resources);
          try (var rows =
              source.execute(
                  sqlPlan(
                      transfer.path("source_engine").asText(),
                      transfer.path("source_dialect").asText(),
                      transfer.path("sql").asText(),
                      inputColumns))) {
            values = collect(rows, inputColumns);
          }
        }
        String relation = transfer.path("target_relation").asText();
        if (operation != null && !operation.isNull()) {
          var command =
              JSON.createObjectNode().put("op", "bind_operation").put("relation", relation);
          command.set("plan", metadata);
          command.set("rows", values);
          var bound = readJson(compiler.command(command.toString()));
          String engineId = bound.path("engine").asText();
          boolean requestOperation = bound.has("requests");
          String dialect = bound.path(requestOperation ? "adapter" : "dialect").asText();
          var destination = session(engineId, dialect, engines, sessions, resources);
          if (requestOperation) {
            var response =
                readJson(
                    destination.executeRequests(
                        bound.get("requests").toString(), bound.path("columns").toString()));
            values = checkedRows(response, transfer.path("columns").size());
          } else {
            values = JSON.createArrayNode();
            for (var sql : bound.path("sql")) {
              try (var rows =
                  destination.execute(
                      sqlPlan(engineId, dialect, sql.asText(), transfer.path("columns")))) {
                values.addAll(collect(rows, transfer.path("columns")));
              }
            }
          }
        }
        plan = bindRows(compiler, plan, relation, values);
      }
      var target = session(plan.engine(), plan.dialect().id(), engines, sessions, resources);
      var rows = target.execute(plan);
      resources.add(rows);
      return new QueryResult() {
        private boolean closed;

        public List<String> columns() {
          return rows.columns();
        }

        public boolean next() throws SQLException {
          return rows.next();
        }

        public Object get(int column) throws SQLException {
          return rows.get(column);
        }

        public void close() throws SQLException {
          if (!closed) {
            closed = true;
            closeAll(resources);
          }
        }
      };
    } catch (SQLException | RuntimeException | Error e) {
      try {
        closeAll(resources);
      } catch (SQLException cleanup) {
        e.addSuppressed(cleanup);
      }
      throw e;
    }
  }

  private static final ObjectMapper JSON =
      new ObjectMapper()
          .enable(com.fasterxml.jackson.databind.DeserializationFeature.USE_BIG_INTEGER_FOR_INTS)
          .enable(com.fasterxml.jackson.databind.DeserializationFeature.USE_BIG_DECIMAL_FOR_FLOATS);

  private static com.fasterxml.jackson.databind.JsonNode readJson(String value)
      throws SQLException {
    try {
      return JSON.readTree(value);
    } catch (java.io.IOException e) {
      throw new SQLException("Invalid federation protocol JSON", e);
    }
  }

  private static ExecutionEngine.Session session(
      String id,
      String dialect,
      Map<String, ExecutionEngine> engines,
      Map<String, ExecutionEngine.Session> sessions,
      List<AutoCloseable> resources)
      throws SQLException {
    var engine = engines.get(id);
    if (engine == null || !engine.dialect().id().equals(dialect))
      throw new SQLException("Missing engine or dialect mismatch: " + id);
    var existing = sessions.get(id);
    if (existing != null) return existing;
    var opened = engine.openSession();
    sessions.put(id, opened);
    resources.add(opened);
    return opened;
  }

  private static CompiledQuery sqlPlan(
      String engine, String dialect, String sql, com.fasterxml.jackson.databind.JsonNode columns) {
    var fields = new ArrayList<String>();
    columns.forEach(c -> fields.add(c.path("name").asText()));
    return new CompiledQuery(engine, new SqlDialect(dialect), sql, fields);
  }

  private static com.fasterxml.jackson.databind.node.ArrayNode checkedRows(
      com.fasterxml.jackson.databind.JsonNode rows, int width) throws SQLException {
    if (!rows.isArray()) throw new SQLException("Request adapter must return a JSON array of rows");
    for (var row : rows)
      if (!row.isArray() || row.size() != width)
        throw new SQLException("Request adapter result row width mismatch");
    return (com.fasterxml.jackson.databind.node.ArrayNode) rows;
  }

  private static com.fasterxml.jackson.databind.node.ArrayNode collect(
      QueryResult rows, com.fasterxml.jackson.databind.JsonNode columns) throws SQLException {
    if (rows.columns().size() != columns.size())
      throw new SQLException("Exchange result column count mismatch");
    var values = JSON.createArrayNode();
    while (rows.next()) {
      var row = JSON.createArrayNode();
      for (int i = 0; i < columns.size(); i++)
        row.addPOJO(jsonValue(rows.get(i + 1), columns.get(i).path("data_type").asText()));
      values.add(row);
    }
    return values;
  }

  static void requireSingleEngine(CompiledQuery query) throws SQLException {
    try {
      var transfers = new ObjectMapper().readTree(query.diagnosticsJson()).path("transfers");
      if (!transfers.isMissingNode() && !transfers.isEmpty())
        throw new SQLException("Use FederatedQuery for a multi-engine plan");
    } catch (java.io.IOException e) {
      throw new SQLException("Invalid compiled plan metadata", e);
    }
  }

  private static void closeAll(List<AutoCloseable> resources) throws SQLException {
    SQLException failure = null;
    for (int i = resources.size() - 1; i >= 0; i--) {
      try {
        resources.get(i).close();
      } catch (Exception e) {
        if (failure == null) failure = new SQLException("Federation cleanup failed", e);
        else failure.addSuppressed(e);
      }
    }
    if (failure != null) throw failure;
  }

  private static CompiledQuery bindRows(
      NativeSqlCompiler compiler,
      CompiledQuery plan,
      String relation,
      com.fasterxml.jackson.databind.node.ArrayNode values)
      throws SQLException {
    var command =
        JSON.createObjectNode()
            .put("op", "bind")
            .put("relation", relation)
            .put("dialect", plan.dialect().id())
            .put("execution_engine", plan.engine());
    command.set("plan", readJson(plan.diagnosticsJson()));
    command.set("rows", values);
    return compiler.compileJson(command.toString());
  }

  private static Object jsonValue(Object value, String type) throws SQLException {
    if (value == null) return null;
    if (type.equals("json")) {
      // JDBC drivers expose JSON as String, PGobject, or a driver JSON wrapper.
      // Keep JSON null as the text "null", distinct from a SQL null reference.
      String text = value.toString();
      readJson(text);
      return text;
    }
    if (type.startsWith("list:")) {
      String itemType = type.substring(5);
      if (value instanceof java.sql.Array array) {
        try {
          return jsonValue(array.getArray(), type);
        } finally {
          array.free();
        }
      }
      var items = new ArrayList<Object>();
      if (value instanceof Iterable<?> iterable) {
        for (var item : iterable) items.add(jsonValue(item, itemType));
      } else if (value.getClass().isArray()) {
        for (int i = 0; i < java.lang.reflect.Array.getLength(value); i++)
          items.add(jsonValue(java.lang.reflect.Array.get(value, i), itemType));
      } else {
        // PostgreSQL transports nested list cells as JSONB values.
        try {
          var decoded = JSON.readValue(value.toString(), Object.class);
          if (!(decoded instanceof List<?>))
            throw new SQLException("Expected a list exchange cell");
          return jsonValue(decoded, type);
        } catch (java.io.IOException e) {
          throw new SQLException("Invalid list exchange cell", e);
        }
      }
      return items;
    }
    return jsonValue(value);
  }

  private static Object jsonValue(Object value) throws SQLException {
    if (value == null || value instanceof String || value instanceof Boolean) return value;
    if (value instanceof byte[] bytes) return Base64.getEncoder().encodeToString(bytes);
    if (value instanceof java.sql.Array array) {
      try {
        if (array.getBaseTypeName().equalsIgnoreCase("jsonb")
            || array.getBaseTypeName().equalsIgnoreCase("json")) {
          var json =
              new ObjectMapper()
                  .enable(
                      com.fasterxml.jackson.databind.DeserializationFeature
                          .USE_BIG_DECIMAL_FOR_FLOATS)
                  .enable(
                      com.fasterxml.jackson.databind.DeserializationFeature
                          .USE_BIG_INTEGER_FOR_INTS);
          var items = array.getArray();
          var decoded = new ArrayList<Object>();
          for (int i = 0; i < java.lang.reflect.Array.getLength(items); i++) {
            var item = java.lang.reflect.Array.get(items, i);
            try {
              decoded.add(item == null ? null : json.readValue(item.toString(), Object.class));
            } catch (java.io.IOException e) {
              throw new SQLException("Invalid JSON array cell", e);
            }
          }
          return jsonValue(decoded);
        }
        return jsonValue(array.getArray());
      } finally {
        array.free();
      }
    }
    if (value instanceof Iterable<?> items) {
      var result = new ArrayList<Object>();
      for (var item : items) result.add(jsonValue(item));
      return result;
    }
    if (value.getClass().isArray()) {
      var result = new ArrayList<Object>();
      for (int i = 0; i < java.lang.reflect.Array.getLength(value); i++)
        result.add(jsonValue(java.lang.reflect.Array.get(value, i)));
      return result;
    }
    if (value instanceof Number
        || value instanceof java.util.Date
        || value instanceof java.time.temporal.TemporalAccessor) return value.toString();
    throw new SQLException("Unsupported exchange value: " + value.getClass().getName());
  }
}
