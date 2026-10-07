package io.orchiddb;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.sql.SQLException;
import java.util.*;

/** Immutable statistics snapshot backed by a shared native catalog. */
public final class Statistics implements AutoCloseable {
  private static final ObjectMapper JSON = new ObjectMapper();
  private final SqlCompiler compiler;
  private final String catalog;
  private final String snapshot;
  private final String report;
  private boolean closed;

  private Statistics(SqlCompiler compiler, JsonNode result, JsonNode snapshot) {
    this.compiler = compiler;
    this.catalog = result.path("catalog_id").asText();
    this.snapshot = snapshot.toString();
    this.report =
        (result.has("report") ? result.path("report") : snapshot.path("report")).toString();
    if (catalog.isEmpty()) throw new PlanningException("Missing statistics catalog handle");
  }

  static JsonNode command(SqlCompiler compiler, Object command) {
    try {
      return JSON.readTree(compiler.statisticsCommand(JSON.writeValueAsString(command)));
    } catch (IOException e) {
      throw new PlanningException("Invalid statistics protocol response", e);
    }
  }

  /** Execute the shared collection plan on an existing caller-owned session. */
  public static Statistics generate(
      SqlCompiler compiler, Compilation metadata, ExecutionEngine.Session session)
      throws SQLException {
    var state =
        command(compiler, Map.of("op", "begin", "request", NativeSqlCompiler.request(metadata)));
    String id = state.path("id").asText();
    boolean finished = false;
    try {
      while (!state.path("request").isNull() && !state.path("request").isMissingNode()) {
        if (Thread.currentThread().isInterrupted())
          throw new SQLException("Statistics generation cancelled");
        var request = state.path("request");
        var submission = new LinkedHashMap<String, Object>();
        submission.put("op", "submit");
        submission.put("id", id);
        submission.put("request_id", request.path("id").asText());
        var rows = new ArrayList<Map<String, Object>>();
        try {
          var read =
              new StatisticsRead(
                  request.path("sql").asText(),
                  request.path("max_rows").asLong(),
                  request.path("max_bytes").asLong(),
                  request.path("timeout_ms").asLong());
          long deadline = System.nanoTime() + read.timeoutMillis() * 1_000_000L;
          long bytes = 2;
          try (var result = session.readStatistics(read)) {
            while (rows.size() < read.maxRows() && result.next()) {
              if (Thread.currentThread().isInterrupted())
                throw new SQLException("Statistics generation cancelled");
              if (System.nanoTime() >= deadline)
                throw new SQLException("Statistics read timed out");
              var row = new LinkedHashMap<String, Object>();
              for (int i = 0; i < result.columns().size(); i++)
                row.put(result.columns().get(i), value(result.get(i + 1)));
              long size = JSON.writeValueAsBytes(row).length + 1;
              if (bytes + size > read.maxBytes()) {
                throw new SQLException("Statistics response exceeded the byte budget");
              }
              rows.add(row);
              bytes += size;
            }
          }
          submission.put("rows", rows);
        } catch (SQLException | IOException e) {
          if (Thread.currentThread().isInterrupted())
            throw new SQLException("Statistics generation cancelled", e);
          submission.put(
              "error", e.getMessage() == null ? e.getClass().getSimpleName() : e.getMessage());
        }
        // Keep already acquired rows when a later row exceeds the work budget.
        // The final error marks this as partial, never as a complete relation.
        if (submission.containsKey("error") && !rows.isEmpty()) {
          var partial = new LinkedHashMap<String, Object>(submission);
          partial.remove("error");
          partial.put("rows", rows);
          partial.put("done", false);
          command(compiler, partial);
        }
        state = command(compiler, submission);
      }
      if (Thread.currentThread().isInterrupted())
        throw new SQLException("Statistics generation cancelled");
      var result = command(compiler, Map.of("op", "finish", "id", id));
      var statistics = new Statistics(compiler, result, result.path("snapshot"));
      finished = true;
      return statistics;
    } finally {
      if (!finished) command(compiler, Map.of("op", "cancel", "id", id));
    }
  }

  private static Object value(Object value) throws SQLException {
    if (value instanceof java.sql.Array array) {
      try {
        return value(array.getArray());
      } finally {
        array.free();
      }
    }
    if (value instanceof Object[] array) {
      var result = new ArrayList<Object>();
      for (var element : array) result.add(value(element));
      return result;
    }
    if (value instanceof java.sql.Struct struct) {
      // JDBC does not expose struct field names; drivers may offer a named map.
      try {
        return value(struct.getClass().getMethod("getMap").invoke(struct));
      } catch (ReflectiveOperationException e) {
        throw new SQLException("The JDBC driver cannot expose named struct fields", e);
      }
    }
    if (value instanceof Map<?, ?> map) {
      var result = new LinkedHashMap<String, Object>();
      for (var entry : map.entrySet()) {
        if (!(entry.getKey() instanceof String key))
          throw new SQLException("Statistics map keys must be strings");
        result.put(key, value(entry.getValue()));
      }
      return result;
    }
    if (value instanceof List<?> list) {
      var result = new ArrayList<Object>();
      for (var element : list) result.add(value(element));
      return result;
    }
    if (value instanceof CharSequence text) return text.toString();
    if (value instanceof java.sql.Date
        || value instanceof java.sql.Timestamp
        || value instanceof java.time.temporal.TemporalAccessor) return value.toString();
    if (value instanceof Number
        || value instanceof Boolean
        || value instanceof String
        || value instanceof byte[]
        || value == null) return value;
    throw new SQLException("Unsupported statistics value " + value.getClass().getName());
  }

  public static Statistics load(SqlCompiler compiler, Path path) throws IOException {
    return install(compiler, Files.readString(path));
  }

  public static Statistics install(SqlCompiler compiler, String snapshotJson) throws IOException {
    var snapshot = JSON.readTree(snapshotJson);
    var result = command(compiler, Map.of("op", "install", "snapshot", snapshot));
    return new Statistics(compiler, result, snapshot);
  }

  public String snapshotJson() {
    return snapshot;
  }

  public String reportJson() {
    return report;
  }

  public void save(Path path) throws IOException {
    Path target = path.toAbsolutePath();
    Path temporary = Files.createTempFile(target.getParent(), ".orchiddb-statistics-", ".json");
    try {
      Files.writeString(temporary, snapshot);
      Files.move(
          temporary,
          target,
          java.nio.file.StandardCopyOption.ATOMIC_MOVE,
          java.nio.file.StandardCopyOption.REPLACE_EXISTING);
    } finally {
      Files.deleteIfExists(temporary);
    }
  }

  public synchronized String catalogId() {
    if (closed) throw new IllegalStateException("Statistics catalog is closed");
    return catalog;
  }

  @Override
  public synchronized void close() {
    if (!closed) {
      command(compiler, Map.of("op", "release", "catalog_id", catalog));
      closed = true;
    }
  }
}
