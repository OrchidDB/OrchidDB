package io.orchiddb;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import io.orchiddb.internal.NativeBridge;
import java.util.Map;
import java.util.Objects;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Kafka execution with application-owned authorization, including SpiceDB. Results contain base64
 * Arrow IPC. Subscription batches contain replacement frames; persist their offsets only after
 * processing the entire batch. Use separate connections for concurrent operations. Close cancels
 * pending work.
 */
public final class StreamConnection implements AutoCloseable {
  @FunctionalInterface
  public interface Authorizer {
    /** Return one boolean per check, in order. The application owns cache expiry. */
    boolean[] check(JsonNode subject, JsonNode checks) throws Exception;
  }

  private static final ObjectMapper JSON = new ObjectMapper();
  private final String id;
  private final Authorizer authorizer;
  private final AtomicBoolean busy = new AtomicBoolean();
  private final AtomicBoolean closed = new AtomicBoolean();

  public StreamConnection(
      JsonNode schema,
      Map<String, String> kafka,
      String topic,
      String source,
      Authorizer authorizer)
      throws Exception {
    this.authorizer = Objects.requireNonNull(authorizer, "authorizer required");
    NativeSqlCompiler.ensureLoaded();
    ObjectNode request =
        JSON.createObjectNode().put("op", "stream_open").put("topic", topic).put("source", source);
    request.set("schema", schema);
    request.set("kafka", JSON.valueToTree(kafka));
    id = invoke(request).path("id").asText();
    if (id.isEmpty()) throw new IllegalStateException("Missing stream handle");
  }

  private static JsonNode invoke(ObjectNode request) throws Exception {
    return JSON.readTree(NativeBridge.compileJson(request.toString()));
  }

  private JsonNode command(String op, ObjectNode values) throws Exception {
    if (closed.get()) throw new IllegalStateException("Stream connection is closed");
    return invoke(values.put("op", op).put("id", id));
  }

  private JsonNode run(String action, ObjectNode values) throws Exception {
    if (!busy.compareAndSet(false, true))
      throw new IllegalStateException("Stream connection is busy");
    try {
      command("stream_start", values.put("action", action));
      for (; ; ) {
        if (Thread.currentThread().isInterrupted()) {
          close();
          throw new InterruptedException("Stream operation cancelled");
        }
        JsonNode event = command("stream_poll", JSON.createObjectNode());
        if (event.path("state").asText().equals("done")) return event.get("result");
        if (event.path("state").asText().equals("authorize")) {
          try {
            boolean[] decisions = authorizer.check(event.get("subject"), event.get("checks"));
            if (decisions == null || decisions.length != event.get("checks").size())
              throw new IllegalArgumentException("authorizer must return one boolean per check");
            ObjectNode reply = JSON.createObjectNode().put("nonce", event.path("nonce").asText());
            reply.set("decisions", JSON.valueToTree(decisions));
            command("stream_authorize", reply);
          } catch (Exception | Error e) {
            close();
            throw e;
          }
        }
      }
    } finally {
      busy.set(false);
    }
  }

  public JsonNode insert(JsonNode change, JsonNode subject) throws Exception {
    ObjectNode v = JSON.createObjectNode();
    v.set("change", change);
    v.set("subject", subject);
    return run("insert", v);
  }

  public JsonNode insertBatch(JsonNode changes, JsonNode subject) throws Exception {
    ObjectNode v = JSON.createObjectNode();
    v.set("changes", changes);
    v.set("subject", subject);
    return run("insert_batch", v);
  }

  private ObjectNode queryValues(String text, JsonNode subject, Map<String, ?> parameters) {
    ObjectNode v = JSON.createObjectNode().put("text", text);
    v.set("subject", subject);
    v.set("parameters", JSON.valueToTree(parameters));
    return v;
  }

  public JsonNode query(String text, JsonNode subject, Map<String, ?> parameters) throws Exception {
    return run("query", queryValues(text, subject, parameters));
  }

  public JsonNode queryHybrid(
      String text, JsonNode subject, Map<String, ?> parameters, JsonNode snapshot)
      throws Exception {
    ObjectNode v = queryValues(text, subject, parameters);
    v.set("snapshot", snapshot);
    return run("hybrid", v);
  }

  public void subscribe(
      String text, JsonNode subject, Map<String, ?> parameters, Map<Integer, Long> offsets)
      throws Exception {
    ObjectNode v = queryValues(text, subject, parameters);
    v.set("offsets", JSON.valueToTree(offsets));
    run("subscribe", v);
  }

  public JsonNode nextBatch(int maxRecords, int waitMs) throws Exception {
    return run(
        "next", JSON.createObjectNode().put("max_records", maxRecords).put("wait_ms", waitMs));
  }

  public void unsubscribe() throws Exception {
    run("unsubscribe", JSON.createObjectNode());
  }

  @Override
  public void close() throws Exception {
    if (closed.compareAndSet(false, true))
      invoke(JSON.createObjectNode().put("op", "stream_close").put("id", id));
  }
}
