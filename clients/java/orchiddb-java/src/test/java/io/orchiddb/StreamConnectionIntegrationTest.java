package io.orchiddb;

import static org.junit.jupiter.api.Assertions.*;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import com.fasterxml.jackson.databind.*;
import com.fasterxml.jackson.databind.node.*;
import java.net.*;
import java.net.http.*;
import java.nio.file.*;
import java.util.*;
import org.junit.jupiter.api.Test;

class StreamConnectionIntegrationTest {
  private static final ObjectMapper JSON = new ObjectMapper();

  private static JsonNode change(String key, int version) {
    ObjectNode c =
        JSON.createObjectNode().put("table", "docs").put("key", key).put("version", version);
    c.putObject("data").put("name", key + "-" + version);
    return c;
  }

  @Test
  void kafkaSpiceDbRoute() throws Exception {
    String path = System.getenv("ORCHIDDB_STREAM_FIXTURE");
    assumeTrue(path != null);
    JsonNode f = JSON.readTree(Files.readString(Path.of(path)));
    HttpClient http = HttpClient.newHttpClient();
    StreamConnection.Authorizer auth =
        (subject, checks) -> {
          ObjectNode v = JSON.createObjectNode();
          v.set("subject", subject);
          v.set("checks", checks);
          var request =
              HttpRequest.newBuilder(URI.create(f.path("auth_url").asText()))
                  .header("Content-Type", "application/json")
                  .POST(HttpRequest.BodyPublishers.ofString(v.toString()))
                  .build();
          var response = http.send(request, HttpResponse.BodyHandlers.ofString());
          assertEquals(200, response.statusCode());
          return JSON.readValue(response.body(), boolean[].class);
        };
    Map<String, String> kafka =
        JSON.convertValue(
            f.get("kafka"),
            new com.fasterxml.jackson.core.type.TypeReference<Map<String, String>>() {});
    JsonNode subject = f.get("subject");
    try (var c =
        new StreamConnection(
            f.get("schema"),
            kafka,
            f.path("topic_prefix").asText() + "-java",
            f.path("source").asText(),
            auth)) {
      var denied = assertThrows(Exception.class, () -> c.insert(change("blocked", 1), subject));
      assertTrue(denied.getMessage().contains("denied"));
      c.insert(change("a", 1), subject);
      c.insertBatch(
          JSON.createArrayNode().add(change("b", 1)).add(change("hidden", 1)).add(change("a", 2)),
          subject);
      JsonNode query = c.query(f.path("query").asText(), subject, Map.of());
      ObjectNode snapshot = JSON.createObjectNode().put("source", f.path("source").asText());
      snapshot.set("offsets", f.get("offsets"));
      snapshot.putArray("changes").add(change("a", 1)).add(change("b", 1));
      JsonNode hybrid = c.queryHybrid(f.path("query").asText(), subject, Map.of(), snapshot);
      c.subscribe(f.path("stream_query").asText(), subject, Map.of(), Map.of(0, 0L));
      ArrayNode frames = JSON.createArrayNode();
      ArrayNode rawBatches = JSON.createArrayNode();
      JsonNode offsets = null;
      for (int i = 0; i < 20; i++) {
        JsonNode b = c.nextBatch(16, 100);
        rawBatches.add(b);
        frames.addAll((ArrayNode) b.get("frames"));
        offsets = b.get("offsets");
        if (offsets.path("0").asInt() == 4) break;
      }
      assertEquals(3, frames.size());
      assertEquals(4, offsets.path("0").asInt());
      c.unsubscribe();
      ObjectNode report = JSON.createObjectNode();
      report.set("query", query);
      report.set("hybrid", hybrid);
      report.set("frames", frames);
      report.set("raw_batches", rawBatches);
      report.set("offsets", offsets);
      Files.writeString(Path.of("/tmp/orchid-stream-java-result.json"), report.toString());
    }
    try (var c =
        new StreamConnection(
            f.get("schema"),
            kafka,
            f.path("topic_prefix").asText() + "-java",
            f.path("source").asText(),
            (s, checks) -> new boolean[0])) {
      assertThrows(IllegalArgumentException.class, () -> c.insert(change("a", 3), subject));
      assertThrows(
          IllegalStateException.class, () -> c.query(f.path("query").asText(), subject, Map.of()));
    }
  }
}
