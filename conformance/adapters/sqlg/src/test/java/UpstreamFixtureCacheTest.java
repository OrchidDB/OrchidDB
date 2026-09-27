import java.io.IOException;
import java.util.*;
import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.node.ArrayNode;
import org.apache.tinkerpop.gremlin.LoadGraphWith.GraphData;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.TinkerGraph;
import org.junit.Test;
import static org.junit.Assert.*;

public class UpstreamFixtureCacheTest {
 @Test public void repeatedFixtureSendsOnlyResetAndNullPolicyHasSeparateIdentity() throws Exception {
  var session=new UpstreamGremlin.FixtureSession();
  List<JsonNode> requests=new ArrayList<>();
  UpstreamGremlin.FixtureTransport transport=request->{
   requests.add(UpstreamGremlin.json.readTree(request));
   return UpstreamGremlin.json.createObjectNode().put("ok",true);
  };
  session.load(GraphData.MODERN,false,transport);
  session.load(GraphData.MODERN,false,transport);
  session.load(GraphData.MODERN,true,transport);
  session.load(GraphData.MODERN,true,transport);
  assertEquals("fixture",requests.get(0).get("op").asText());
  assertEquals(6,requests.get(0).get("nodes").size());
  assertEquals(6,requests.get(0).get("edges").size());
  assertFalse(requests.get(0).get("allow_null_property_values").asBoolean());
  assertEquals("fixture-reset",requests.get(1).get("op").asText());
  assertEquals(2,requests.get(1).size());
  assertEquals(requests.get(0).get("fixture_key"),requests.get(1).get("fixture_key"));
  assertEquals("fixture",requests.get(2).get("op").asText());
  assertTrue(requests.get(2).get("allow_null_property_values").asBoolean());
  assertNotEquals(requests.get(0).get("fixture_key"),requests.get(2).get("fixture_key"));
  assertEquals("fixture-reset",requests.get(3).get("op").asText());
  new UpstreamGremlin.FixtureSession().load(GraphData.MODERN,false,transport);
  assertEquals("fixture",requests.get(4).get("op").asText());
 }

 @Test public void failedRegistrationRetriesFullPayloadAndMissingResetFails() throws Exception {
  var session=new UpstreamGremlin.FixtureSession();
  List<String> operations=new ArrayList<>();
  UpstreamGremlin.FixtureTransport failure=request->{
   operations.add(UpstreamGremlin.json.readTree(request).get("op").asText());
   return UpstreamGremlin.json.createObjectNode().put("error","missing fixture");
  };
  try {session.load(null,false,failure);fail("Registration must fail");}
  catch(IOException expected){assertTrue(expected.getMessage().contains("missing fixture"));}
  session.load(null,false,request->{
   operations.add(UpstreamGremlin.json.readTree(request).get("op").asText());
   return UpstreamGremlin.json.createObjectNode().put("ok",true);
  });
  try {session.load(null,false,failure);fail("Reset must fail");}
  catch(IOException expected){assertTrue(expected.getMessage().contains("missing fixture"));}
  assertEquals(List.of("fixture","fixture","fixture-reset"),operations);
 }

 @Test public void missingOrFalseAcknowledgementCannotRegisterOrResetFixture() throws Exception {
  for(String reply:List.of("{}","{\"ok\":false}")) {
   var session=new UpstreamGremlin.FixtureSession();
   List<String> operations=new ArrayList<>();
   UpstreamGremlin.FixtureTransport failure=request->{
    operations.add(UpstreamGremlin.json.readTree(request).get("op").asText());
    return UpstreamGremlin.json.readTree(reply);
   };
   try {session.load(null,false,failure);fail("Registration requires acknowledgement");}
   catch(IOException expected){assertTrue(expected.getMessage().contains("acknowledgement"));}
   session.load(null,false,request->{
    operations.add(UpstreamGremlin.json.readTree(request).get("op").asText());
    return UpstreamGremlin.json.createObjectNode().put("ok",true);
   });
   try {session.load(null,false,failure);fail("Reset requires acknowledgement");}
   catch(IOException expected){assertTrue(expected.getMessage().contains("acknowledgement"));}
   assertEquals(List.of("fixture","fixture","fixture-reset"),operations);
  }
 }

 @Test public void cachedBytesPreserveCrewRecordsAndAreIsolatedFromGraphMutation() throws Exception {
  var cache=new UpstreamGremlin.FixturePayloadCache();
  String payload=cache.payload(GraphData.CREW,false);
  List<Object> nodes=new ArrayList<>(),edges=new ArrayList<>();
  try(TinkerGraph graph=UpstreamGremlin.standardFixture(GraphData.CREW)) {
   graph.vertices().forEachRemaining(v->nodes.add(UpstreamGremlin.fixtureNode(v,"orchiddb")));
   graph.edges().forEachRemaining(e->edges.add(UpstreamGremlin.fixtureEdge(e)));
   graph.traversal().V().drop().iterate();
  }
  JsonNode encoded=UpstreamGremlin.json.readTree(payload);
  assertEquals(UpstreamGremlin.json.readTree(UpstreamGremlin.json.writeValueAsString(nodes)),encoded.get("nodes"));
  assertEquals(UpstreamGremlin.json.readTree(UpstreamGremlin.json.writeValueAsString(edges)),encoded.get("edges"));
  ((ArrayNode)encoded.get("nodes")).removeAll();
  assertSame(payload,cache.payload(GraphData.CREW,false));
  assertEquals(nodes.size(),UpstreamGremlin.json.readTree(cache.payload(GraphData.CREW,false)).get("nodes").size());
 }
}
