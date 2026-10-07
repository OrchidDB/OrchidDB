package io.orchiddb;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import io.orchiddb.internal.NativeBridge;
import java.sql.SQLException;
import java.sql.SQLFeatureNotSupportedException;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Caller-owned Quickwit or Elasticsearch HTTP client. Reuse it across federated queries and close
 * it when finished. Query sessions borrow this client's connection pool and mapping cache.
 */
public final class RemoteEngine implements ExecutionEngine, AutoCloseable {
  private static final ObjectMapper JSON =
      new ObjectMapper()
          .enable(com.fasterxml.jackson.databind.DeserializationFeature.USE_BIG_INTEGER_FOR_INTS)
          .enable(com.fasterxml.jackson.databind.DeserializationFeature.USE_BIG_DECIMAL_FOR_FLOATS);
  private final String id;
  private final SqlDialect adapter;
  private final String handle;
  private boolean closed;

  public static RemoteEngine quickwit(String id, String endpoint) throws SQLException {
    return new RemoteEngine(id, "quickwit", Map.of("endpoint", endpoint));
  }

  public static RemoteEngine elasticsearch(String id, String endpoint) throws SQLException {
    return new RemoteEngine(id, "elasticsearch", Map.of("endpoint", endpoint));
  }

  /**
   * Options follow the shared remote transport protocol. They include endpoint, authentication,
   * page_size, batch_size, max_rows, and max_response_bytes. Credentials stay in this session;
   * never put them in compilation requests.
   */
  public RemoteEngine(String id, String adapter, Map<String, ?> options) throws SQLException {
    this.id = Checks.name(id);
    if (!adapter.equals("quickwit") && !adapter.equals("elasticsearch"))
      throw new IllegalArgumentException("Unsupported remote adapter: " + adapter);
    this.adapter = new SqlDialect(adapter);
    NativeSqlCompiler.ensureLoaded();
    var request = JSON.createObjectNode().put("op", "open").put("adapter", adapter);
    request.set("options", JSON.valueToTree(options));
    this.handle = invoke(request).path("id").asText();
    if (handle.isEmpty()) throw new SQLException("Remote session returned no handle");
  }

  public String id() {
    return id;
  }

  public SqlDialect dialect() {
    return adapter;
  }

  public synchronized Session openSession() throws SQLException {
    requireOpen();
    return new Session() {
      private boolean sessionClosed;

      public Map<Source, List<Column>> schemas(Set<Source> sources) throws SQLException {
        throw new SQLFeatureNotSupportedException("Remote graph mappings require explicit schemas");
      }

      public String executeRequests(String requestsJson, String columnsJson) throws SQLException {
        synchronized (RemoteEngine.this) {
          requireOpen();
          if (sessionClosed) throw new SQLException("Remote query session is closed");
          var request =
              JSON.createObjectNode().put("op", "execute").put("id", handle).put("format", "rows");
          try {
            request.set("requests", JSON.readTree(requestsJson));
            request.set("columns", JSON.readTree(columnsJson));
          } catch (java.io.IOException e) {
            throw new SQLException("Invalid remote operation JSON", e);
          }
          var response = invoke(request);
          if (!response.path("rows").isArray())
            throw new SQLException("Remote session returned no rows array");
          return response.get("rows").toString();
        }
      }

      public void close() {
        synchronized (RemoteEngine.this) {
          sessionClosed = true;
        }
      }
    };
  }

  /** Refresh mapping validation after changing the remote index configuration. */
  public synchronized void clearMetadataCache() throws SQLException {
    requireOpen();
    invoke(JSON.createObjectNode().put("op", "clear_metadata_cache").put("id", handle));
  }

  public synchronized void close() throws SQLException {
    if (!closed) {
      invoke(JSON.createObjectNode().put("op", "close").put("id", handle));
      closed = true;
    }
  }

  private void requireOpen() throws SQLException {
    if (closed) throw new SQLException("Remote engine is closed");
  }

  private static JsonNode invoke(JsonNode request) throws SQLException {
    try {
      return JSON.readTree(NativeBridge.remoteJson(request.toString()));
    } catch (java.io.IOException | PlanningException e) {
      throw new SQLException("Remote engine command failed", e);
    }
  }

  @Override
  public String toString() {
    return "RemoteEngine[id=" + id + ", adapter=" + adapter.id() + "]";
  }
}
