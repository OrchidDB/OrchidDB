import java.util.*;
import org.junit.Test;
import org.junit.AssumptionViolatedException;
import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.*;
import static org.junit.Assert.*;

public class UpstreamFixturePropertyTransportTest {
 @Test public void crewFixtureRetainsEveryPropertyIdentityAndMetaProperty() throws Exception {
  try(TinkerGraph graph=TinkerFactory.createTheCrew()) {
   Iterator<Vertex> vertices=graph.vertices();
   int totalRecords=0,totalMeta=0;
   while(vertices.hasNext()) {
    Vertex vertex=vertices.next();
    Map<String,Object> encoded=UpstreamGremlin.fixtureNode(vertex,"crabgraph");
    assertEquals(vertex.id(),encoded.get("id"));
    assertEquals(vertex.id().getClass().getSimpleName(),encoded.get("id_type"));
    List<?> records=(List<?>)encoded.get("property_records");
    Iterator<VertexProperty<Object>> expected=vertex.properties();
    Map<?,?> scalars=(Map<?,?>)encoded.get("properties");
    int recordIndex=0;
    while(expected.hasNext()) {
     VertexProperty<?> property=expected.next();
     Map<?,?> record=(Map<?,?>)records.get(recordIndex++);
     assertEquals(property.id(),record.get("id"));
     assertEquals(property.id().getClass().getSimpleName(),record.get("id_type"));
     assertEquals(property.key(),record.get("key"));
     assertEquals(property.value(),record.get("value"));
     assertEquals(property.value().getClass().getSimpleName(),record.get("type"));
     Map<?,?> meta=(Map<?,?>)record.get("meta"),types=(Map<?,?>)record.get("meta_types");
     Iterator<Property<Object>> metaProperties=property.properties();
     int metaCount=0;
     while(metaProperties.hasNext()) {
      Property<?> item=metaProperties.next();
      assertEquals(item.value(),meta.get(item.key()));
      assertEquals(item.value().getClass().getSimpleName(),types.get(item.key()));
      metaCount++;
     }
     assertEquals(metaCount,meta.size());
     totalMeta+=metaCount;
    }
    assertEquals(recordIndex,records.size());
    assertTrue(records.size()>=scalars.size());
    totalRecords+=recordIndex;
    // Verify the actual JSON shape, including numeric property IDs.
    var json=UpstreamGremlin.json.readTree(UpstreamGremlin.json.writeValueAsString(encoded));
    assertEquals(records.size(),json.get("property_records").size());
   }
   assertTrue(totalRecords>4);
   assertTrue(totalMeta>0);
  }
 }
 @Test public void modernFixtureRetainsElementIdTypesAndScalarProperties() throws Exception {
  try(TinkerGraph graph=TinkerFactory.createModern()) {
   Vertex vertex=graph.vertices(1).next();
   Map<String,Object> node=UpstreamGremlin.fixtureNode(vertex,"crabgraph");
   assertEquals("Integer",node.get("id_type"));
   assertEquals("marko",((Map<?,?>)node.get("properties")).get("name"));
   Edge edge=graph.edges(7).next();
   Map<String,Object> encoded=UpstreamGremlin.fixtureEdge(edge);
   assertEquals(edge.id(),encoded.get("id"));
   assertEquals(edge.id().getClass().getSimpleName(),encoded.get("id_type"));
   assertEquals(edge.outVertex().id(),encoded.get("src"));
   assertEquals(edge.inVertex().id(),encoded.get("dst"));
   Map<String,Object> puppyNode=UpstreamGremlin.fixtureNode(vertex,"puppygraph");
   assertFalse(puppyNode.containsKey("property_records"));
   assertEquals(node.get("properties"),puppyNode.get("properties"));
  }
 }
 @Test public void puppyGraphKeepsItsExplicitUnsupportedFixtureBoundary() throws Exception {
  try(TinkerGraph graph=TinkerFactory.createTheCrew()) {
   try {
    UpstreamGremlin.fixtureNode(graph.traversal().V().has("name","marko").next(),"puppygraph");
    fail("PuppyGraph cannot preserve this fixture's metadata/cardinality");
   } catch(AssumptionViolatedException expected) {
    assertTrue(expected.getMessage().contains("PuppyGraph"));
   }
  }
 }
}
