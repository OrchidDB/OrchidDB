package io.orchiddb;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import java.io.ByteArrayInputStream;
import java.sql.SQLException;
import java.util.*;
import org.apache.arrow.memory.RootAllocator;
import org.apache.arrow.vector.ipc.ArrowStreamReader;

/** Source-first graph execution. All SQL sessions and drivers are application-owned. */
public final class SourceConnection {
  public enum Engine {
    NONE,
    DUCKDB
  }

  public interface SqlSource {
    String dialect();

    QueryResult query(String sql) throws SQLException;
  }

  public interface RequestSource extends SqlSource {
    QueryResult request(List<JsonNode> requests, JsonNode columns) throws SQLException;

    default QueryResult query(String sql) throws SQLException {
      throw new SQLException("Source does not support SQL");
    }
  }

  public interface DuckDbCompletion {
    QueryResult query(String sql, Map<String, QueryResult> inputs) throws SQLException;
  }

  @FunctionalInterface
  public interface LocalFunction {
    Object call(List<Object> arguments) throws SQLException;
  }

  private static final ObjectMapper JSON = new ObjectMapper();
  private final NativeSqlCompiler runtime = NativeSqlCompiler.load();
  private final ObjectNode schema;
  private final Engine engine;
  private final Map<String, SqlSource> sources;
  private final Map<String, LocalFunction> functions;
  private final DuckDbCompletion completion;

  public SourceConnection(
      String schemaJson,
      Engine engine,
      Map<String, SqlSource> sources,
      Map<String, LocalFunction> functions,
      DuckDbCompletion completion) {
    this.engine = Objects.requireNonNull(engine);
    this.sources = Map.copyOf(sources);
    this.functions = Map.copyOf(functions);
    this.completion = completion;
    try {
      var command = JSON.createObjectNode().put("op", "validate_schema");
      command.set("schema", JSON.readTree(schemaJson));
      schema = (ObjectNode) JSON.readTree(runtime.command(command.toString()));
    } catch (Exception e) {
      throw new IllegalArgumentException("Invalid graph schema", e);
    }
  }

  public QueryResult query(String text) throws SQLException {
    return query(Query.cypher(text));
  }

  public QueryResult query(Query query) throws SQLException {
    try {
      var request = schema.deepCopy();
      request
          .put("version", 1)
          .put("dialect", "duckdb")
          .put("engine", engine == Engine.NONE ? "none" : "duckdb")
          .put("language", query.language())
          .put("query", query.text());
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
      var compiled = JSON.readTree(runtime.command(request.toString()));
      if (compiled.path("version").asInt() != 2)
        throw new SQLException("Expected source-plan protocol");
      return execute(compiled.path("execution_plan"));
    } catch (SQLException e) {
      throw e;
    } catch (Exception e) {
      throw new SQLException("Source-plan execution failed", e);
    }
  }

  private void checkExpr(JsonNode expr, int width) throws SQLException {
    switch (expr.path("kind").asText()) {
      case "column" -> {
        int i = expr.path("index").asInt(-1);
        if (i < 0 || i >= width) throw new SQLException("Column out of bounds");
      }
      case "literal" -> literal(expr);
      case "cast", "graph_element" -> checkExpr(expr.path("input"), width);
      case "call" -> {
        if (!functions.containsKey(expr.path("function").asText()))
          throw new SQLException("Missing local function " + expr.path("function").asText());
        for (var arg : expr.path("arguments")) checkExpr(arg, width);
      }
      default -> throw new SQLException("Unsupported local expression");
    }
  }

  private Object literal(JsonNode expr) throws SQLException {
    byte[] bytes = new byte[expr.path("ipc").size()];
    for (int i = 0; i < bytes.length; i++) bytes[i] = (byte) expr.path("ipc").get(i).asInt();
    try (var allocator = new RootAllocator();
        var reader = new ArrowStreamReader(new ByteArrayInputStream(bytes), allocator)) {
      if (!reader.loadNextBatch()
          || reader.getVectorSchemaRoot().getRowCount() != 1
          || reader.getVectorSchemaRoot().getFieldVectors().size() != 1)
        throw new SQLException("Invalid literal");
      Object value = reader.getVectorSchemaRoot().getVector(0).getObject(0);
      return value instanceof org.apache.arrow.vector.util.Text ? value.toString() : value;
    } catch (java.io.IOException e) {
      throw new SQLException("Invalid literal", e);
    }
  }

  private Object eval(JsonNode expr, QueryResult input) throws SQLException {
    return switch (expr.path("kind").asText()) {
      case "column" -> input.get(expr.path("index").asInt() + 1);
      case "literal" -> literal(expr);
      case "cast" -> eval(expr.path("input"), input);
      case "graph_element" -> {
        try {
          var command =
              JSON.createObjectNode()
                  .put("op", "decode_graph_values")
                  .put("data_type", expr.path("data_type").asText());
          command
              .putArray("values")
              .add(JSON.valueToTree(graphDescriptor(eval(expr.path("input"), input))));
          var values = JSON.readTree(runtime.command(command.toString()));
          if (!values.isArray() || values.size() != 1)
            throw new SQLException("Invalid graph decoder row count");
          yield JSON.convertValue(values.get(0), Object.class);
        } catch (Exception e) {
          throw new SQLException("Graph descriptor decoding failed", e);
        }
      }
      case "call" -> {
        var arguments = new ArrayList<Object>();
        for (var arg : expr.path("arguments")) arguments.add(eval(arg, input));
        Object value =
            functions
                .get(expr.path("function").asText())
                .call(Collections.unmodifiableList(arguments));
        validateValue(value, expr.path("returns").asText());
        yield value;
      }
      default -> throw new SQLException("Unsupported local expression");
    };
  }

  private static Object graphDescriptor(Object value)
      throws ReflectiveOperationException, SQLException {
    if (value instanceof java.sql.Struct row) return Arrays.asList(row.getAttributes());
    // Keep the JDBC driver optional. Trino's ROW facade is not java.sql.Struct;
    // its ordered fields correspond to the shared descriptor schema.
    if (value != null && value.getClass().getName().equals("io.trino.jdbc.Row")) {
      var fields = (List<?>) value.getClass().getMethod("getFields").invoke(value);
      var result = new ArrayList<Object>();
      for (var field : fields) result.add(field.getClass().getMethod("getValue").invoke(field));
      return result;
    }
    return value;
  }

  private static void validateValue(Object value, String kind) throws SQLException {
    if (value == null) return;
    boolean valid =
        switch (kind) {
          case "int64" -> value instanceof Long;
          case "int32" -> value instanceof Integer;
          case "float64" -> value instanceof Double;
          case "float32" -> value instanceof Float;
          case "boolean" -> value instanceof Boolean;
          case "string" -> value instanceof CharSequence;
          case "binary" -> value instanceof byte[];
          default -> true;
        };
    if (!valid)
      throw new SQLException(
          "Local function expected " + kind + ", received " + value.getClass().getName());
    if (kind.startsWith("list<") && kind.endsWith(">")) {
      if (!(value instanceof List<?>))
        throw new SQLException("Local function must return a List for " + kind);
      for (Object item : (List<?>) value) validateValue(item, kind.substring(5, kind.length() - 1));
    }
  }

  private static JsonNode bind(JsonNode template, List<Object> values) throws SQLException {
    if (values.size() != template.path("parameters").asInt())
      throw new SQLException("Request parameter count mismatch");
    JsonNode request = template.path("request").deepCopy();
    for (var binding : template.path("bindings")) {
      int index = binding.path("parameter").asInt();
      Object value = values.get(index);
      JsonNode cell =
          JSON.valueToTree(
              binding.path("encoding").asText().equals("sql_null") ? value == null : value);
      String pointer = binding.path("pointer").asText();
      if (pointer.isEmpty()) {
        request = cell;
        continue;
      }
      int slash = pointer.lastIndexOf('/');
      JsonNode parent = request.at(pointer.substring(0, slash));
      String key = pointer.substring(slash + 1).replace("~1", "/").replace("~0", "~");
      if (parent.isArray())
        ((com.fasterxml.jackson.databind.node.ArrayNode) parent).set(Integer.parseInt(key), cell);
      else if (parent.isObject() && parent.has(key)) ((ObjectNode) parent).set(key, cell);
      else throw new SQLException("Invalid request binding pointer");
    }
    return request;
  }

  private static List<String> fields(JsonNode node) {
    var names = new ArrayList<String>();
    for (var c : node.path("columns")) names.add(c.path("name").asText());
    return List.copyOf(names);
  }

  private QueryResult execute(JsonNode plan) throws SQLException {
    var nodes = plan.path("nodes");
    int result = plan.path("result").asInt(-1);
    if (plan.path("version").asInt() != 1 || result < 0 || result >= nodes.size())
      throw new SQLException("Invalid execution plan");
    for (int i = 0; i < nodes.size(); i++) {
      var node = nodes.get(i);
      var deps = new ArrayList<Integer>();
      switch (node.path("kind").asText()) {
        case "source_sql" -> {
          var source = sources.get(node.path("source").asText());
          if (source == null || !source.dialect().equals(node.path("dialect").asText()))
            throw new SQLException("Missing source or dialect mismatch");
        }
        case "source_request" -> {
          var source = sources.get(node.path("source").asText());
          if (!(source instanceof RequestSource)
              || !source.dialect().equals(node.path("template").path("adapter").asText()))
            throw new SQLException("Missing request source");
          if (!node.path("input").isMissingNode() && !node.path("input").isNull())
            deps.add(node.path("input").asInt(-1));
        }
        case "local_project" -> {
          int input = node.path("input").asInt(-1);
          deps.add(input);
          if (input < 0 || input >= i) throw new SQLException("Invalid projection dependency");
          for (var expr : node.path("expressions"))
            checkExpr(expr, nodes.get(input).path("columns").size());
        }
        case "duckdb" -> {
          if (engine != Engine.DUCKDB || completion == null)
            throw new SQLException("DuckDB completion binding required");
          for (var input : node.path("inputs")) deps.add(input.path("node").asInt(-1));
        }
        case "input" -> {}
        default ->
            throw new SQLException(
                "Unsupported source-plan operation " + node.path("kind").asText());
      }
      for (int dep : deps)
        if (dep < 0 || dep >= i) throw new SQLException("Invalid plan dependency");
    }
    var outputs = new ArrayList<QueryResult>();
    // All bindings are checked before any source is opened. Deferred rows keep
    // source acquisition on the actual consumption path.
    for (var node : nodes) {
      var columns = fields(node);
      QueryResult rows =
          switch (node.path("kind").asText()) {
            case "source_sql" ->
                new Deferred(
                    columns,
                    () ->
                        sources.get(node.path("source").asText()).query(node.path("sql").asText()));
            case "source_request" ->
                new QueryResult() {
                  final QueryResult input =
                      node.path("input").isNull() ? null : outputs.get(node.path("input").asInt());
                  QueryResult current;
                  boolean once;
                  boolean done;

                  public List<String> columns() {
                    return columns;
                  }

                  public boolean next() throws SQLException {
                    while (!done) {
                      if (current != null && current.next()) return true;
                      if (current != null) {
                        current.close();
                        current = null;
                      }
                      var requests = new ArrayList<JsonNode>();
                      if (input == null) {
                        if (once) {
                          done = true;
                          return false;
                        }
                        once = true;
                        requests.add(bind(node.path("template"), List.of()));
                      } else {
                        for (int count = 0; count < 64 && input.next(); count++) {
                          var values = new ArrayList<Object>();
                          for (int c = 1; c <= input.columns().size(); c++)
                            values.add(input.get(c));
                          requests.add(bind(node.path("template"), values));
                        }
                        if (requests.isEmpty()) {
                          done = true;
                          return false;
                        }
                      }
                      current =
                          ((RequestSource) sources.get(node.path("source").asText()))
                              .request(requests, node.path("columns"));
                    }
                    return false;
                  }

                  public Object get(int column) throws SQLException {
                    if (current == null) throw new SQLException("Cursor not positioned");
                    return current.get(column);
                  }

                  public void close() throws SQLException {
                    done = true;
                    if (current != null) current.close();
                  }
                };
            case "duckdb" ->
                new Deferred(
                    columns,
                    () -> {
                      var inputs = new LinkedHashMap<String, QueryResult>();
                      for (var input : node.path("inputs"))
                        inputs.put(
                            input.path("name").asText(), outputs.get(input.path("node").asInt()));
                      return completion.query(
                          node.path("sql").asText(), Collections.unmodifiableMap(inputs));
                    });
            case "input" ->
                new QueryResult() {
                  boolean available = node.path("one_row").asBoolean();

                  public List<String> columns() {
                    return columns;
                  }

                  public boolean next() {
                    boolean result = available;
                    available = false;
                    return result;
                  }

                  public Object get(int column) throws SQLException {
                    throw new SQLException("Unit row has no columns");
                  }

                  public void close() {
                    available = false;
                  }
                };
            default ->
                new QueryResult() {
                  final QueryResult input = outputs.get(node.path("input").asInt());
                  Object[] values;

                  public List<String> columns() {
                    return columns;
                  }

                  public boolean next() throws SQLException {
                    if (!input.next()) {
                      values = null;
                      return false;
                    }
                    values = new Object[columns.size()];
                    for (int i = 0; i < values.length; i++)
                      values[i] = eval(node.path("expressions").get(i), input);
                    return true;
                  }

                  public Object get(int column) throws SQLException {
                    if (values == null || column < 1 || column > values.length)
                      throw new SQLException("Invalid cursor/column");
                    return values[column - 1];
                  }

                  public void close() {
                    values = null;
                  }
                };
          };
      boolean reentry = false;
      for (int later = outputs.size() + 1; later < nodes.size(); later++) {
        var next = nodes.get(later);
        if (node.path("kind").asText().equals("duckdb")
            && next.path("kind").asText().equals("duckdb")) reentry = true;
        if (node.has("source") && node.path("source").equals(next.path("source"))) reentry = true;
      }
      outputs.add(reentry ? new Replay(rows) : rows);
    }
    return new QueryResult() {
      final QueryResult terminal = outputs.get(result);
      boolean closed;

      public List<String> columns() {
        return terminal.columns();
      }

      public boolean next() throws SQLException {
        if (closed) return false;
        try {
          if (terminal.next()) return true;
          close();
          return false;
        } catch (SQLException | RuntimeException e) {
          try {
            close();
          } catch (SQLException cleanup) {
            e.addSuppressed(cleanup);
          }
          throw e;
        }
      }

      public Object get(int column) throws SQLException {
        if (closed) throw new SQLException("Result closed");
        return terminal.get(column);
      }

      public void close() throws SQLException {
        if (closed) return;
        closed = true;
        SQLException failure = null;
        for (int i = outputs.size() - 1; i >= 0; i--)
          try {
            outputs.get(i).close();
          } catch (SQLException e) {
            if (failure == null) failure = e;
            else failure.addSuppressed(e);
          }
        if (failure != null) throw failure;
      }
    };
  }

  private static final class Replay implements QueryResult {
    final QueryResult input;
    java.nio.file.Path path;
    java.io.ObjectInputStream reader;
    Object[] row;
    boolean closed;

    Replay(QueryResult input) {
      this.input = input;
    }

    public List<String> columns() {
      return input.columns();
    }

    public boolean next() throws SQLException {
      if (closed) return false;
      try {
        if (reader == null) {
          path = java.nio.file.Files.createTempFile("orchid-rows-", ".bin");
          try (var writer =
              new java.io.ObjectOutputStream(java.nio.file.Files.newOutputStream(path))) {
            while (input.next()) {
              Object[] values = new Object[columns().size()];
              for (int i = 0; i < values.length; i++) values[i] = input.get(i + 1);
              writer.writeUnshared(values);
              writer.reset();
            }
          }
          input.close();
          reader = new java.io.ObjectInputStream(java.nio.file.Files.newInputStream(path));
        }
        row = (Object[]) reader.readUnshared();
        return true;
      } catch (java.io.EOFException end) {
        row = null;
        return false;
      } catch (java.io.IOException | ClassNotFoundException e) {
        throw new SQLException("Cannot replay source rows; adapter values must be serializable", e);
      }
    }

    public Object get(int column) throws SQLException {
      if (row == null || column < 1 || column > row.length)
        throw new SQLException("Cursor not positioned");
      return row[column - 1];
    }

    public void close() throws SQLException {
      if (closed) return;
      closed = true;
      try {
        input.close();
      } finally {
        try {
          if (reader != null) reader.close();
        } catch (java.io.IOException e) {
          throw new SQLException("Closing replay", e);
        } finally {
          try {
            if (path != null) java.nio.file.Files.deleteIfExists(path);
          } catch (java.io.IOException e) {
            throw new SQLException("Removing replay", e);
          }
        }
      }
    }
  }

  @FunctionalInterface
  private interface Open {
    QueryResult open() throws SQLException;
  }

  private static final class Deferred implements QueryResult {
    final List<String> columns;
    final Open open;
    QueryResult rows;
    boolean closed;

    Deferred(List<String> columns, Open open) {
      this.columns = columns;
      this.open = open;
    }

    public List<String> columns() {
      return columns;
    }

    public boolean next() throws SQLException {
      if (closed) return false;
      if (rows == null) rows = open.open();
      return rows.next();
    }

    public Object get(int column) throws SQLException {
      if (rows == null || closed) throw new SQLException("Cursor not positioned");
      return rows.get(column);
    }

    public void close() throws SQLException {
      if (!closed) {
        closed = true;
        if (rows != null) rows.close();
      }
    }
  }
}
