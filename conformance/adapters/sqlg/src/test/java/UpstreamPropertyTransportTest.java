import com.fasterxml.jackson.databind.JsonNode;
import org.junit.Test;
import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.TinkerGraph;
import static org.junit.Assert.*;

public class UpstreamPropertyTransportTest {
 private static final String VERTEX="""
  {"type":"vertex","id":{"type":"long","value":37},"label":"document"}
  """;
 private static final String VP="""
  {"type":"vertex_property","id":{"type":"long","value":81},
   "owner":%s,"key":"title","value":{"type":"string","value":"atlas"},
   "properties":[{"key":"revision","value":{"type":"int","value":2}}]}
  """.formatted(VERTEX);
 private static JsonNode parse(String value) throws Exception {
  return UpstreamGremlin.json.readTree(value);
 }
 @Test public void vertexPropertiesPreserveIdentityOwnerValueAndMetaProperties() throws Exception {
  Object decoded=UpstreamGremlin.nativeValue(parse(VP));
  assertTrue(decoded instanceof VertexProperty<?>);
  VertexProperty<?> property=(VertexProperty<?>)decoded;
  assertEquals(81L,property.id());
  assertEquals("title",property.key());
  assertEquals("atlas",property.value());
  assertEquals(37L,property.element().id());
  assertEquals("document",property.element().label());
  Property<?> meta=property.property("revision");
  assertEquals(2,meta.value());
  assertSame(property,meta.element());
  try(TinkerGraph graph=TinkerGraph.open()) {
   Vertex vertex=graph.addVertex(T.id,37L,T.label,"document");
   VertexProperty<?> expected=vertex.property(VertexProperty.Cardinality.list,"title","atlas",T.id,81L,"revision",2);
   assertEquals(expected,property);
   assertEquals(expected.hashCode(),property.hashCode());
   assertEquals(expected.property("revision"),meta);
   assertNotEquals(property,UpstreamGremlin.nativeValue(parse(VP.replace("\"value\":81","\"value\":82"))));
  }
 }
 @Test public void standaloneMetaPropertyPreservesTheOwningVertexProperty() throws Exception {
  String wire="""
   {"type":"property","owner":%s,"key":"revision","value":{"type":"int","value":2}}
   """.formatted(VP);
  Property<?> property=(Property<?>)UpstreamGremlin.nativeValue(parse(wire));
  assertTrue(property.element() instanceof VertexProperty<?>);
  VertexProperty<?> owner=(VertexProperty<?>)property.element();
  assertEquals(81L,owner.id());
  assertEquals(37L,owner.element().id());
  assertEquals(owner.property("revision"),property);
 }
 @Test public void edgePropertiesPreserveTheirActualOwnerAndNativeEquality() throws Exception {
  String wire="""
   {"type":"property","owner":{"type":"edge","id":{"type":"long","value":19},"label":"links",
    "inV":{"type":"long","value":38},"inVLabel":"document","outV":{"type":"long","value":37},"outVLabel":"document"},
    "key":"weight","value":{"type":"double","value":0.5}}
   """;
  Property<?> property=(Property<?>)UpstreamGremlin.nativeValue(parse(wire));
  assertFalse(property instanceof VertexProperty<?>);
  assertTrue(property.element() instanceof Edge);
  Edge owner=(Edge)property.element();
  assertEquals(19L,owner.id());
  assertEquals(37L,owner.outVertex().id());
  assertEquals(38L,owner.inVertex().id());
  try(TinkerGraph graph=TinkerGraph.open()) {
   Vertex out=graph.addVertex(T.id,37L,T.label,"document");
   Vertex in=graph.addVertex(T.id,38L,T.label,"document");
   Property<?> expected=out.addEdge("links",in,T.id,19L,"weight",0.5).property("weight");
   assertEquals(expected,property);
   assertEquals(expected.hashCode(),property.hashCode());
  }
 }
 @Test public void ordinaryMapsCannotMasqueradeAsProperties() throws Exception {
  String wire="""
   {"type":"map","value":[[{"type":"string","value":"key"},{"type":"string","value":"title"}],
    [{"type":"string","value":"value"},{"type":"string","value":"atlas"}]]}
   """;
  Object value=UpstreamGremlin.nativeValue(parse(wire));
  assertTrue(value instanceof java.util.Map<?,?>);
  assertFalse(value instanceof Property<?>);
 }
}
