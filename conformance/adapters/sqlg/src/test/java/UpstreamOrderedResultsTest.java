import java.util.*;
import org.apache.tinkerpop.gremlin.structure.util.reference.ReferenceVertex;
import org.junit.Test;
import static org.junit.Assert.*;

public class UpstreamOrderedResultsTest {
 @Test public void nativeResultsSupportUpstreamTraversalLifecycle() {
  var values=Arrays.asList("a","b","a",null,3L);
  var traversal=UpstreamGremlin.nativeResults(values);
  assertFalse(traversal.asAdmin().isLocked());
  assertEquals(values,traversal.toList());
  assertTrue(traversal.asAdmin().isLocked());
 }

 @Test public void separatedDuplicateScalarsKeepTheirPositions() {
  var values=List.of("java","lop","java","ripple");
  var traversal=UpstreamGremlin.orderedResults(values);
  var actual=new ArrayList<Object>();
  while(traversal.hasNext()) {
   var traverser=traversal.nextTraverser();
   assertEquals(1L,traverser.bulk());
   actual.add(traverser.get());
  }
  assertEquals(values,actual);
 }
 @Test public void elementsMapsNullsAndExpandedMultiplicitySurvive() {
  var vertex=new ReferenceVertex(7,"node");
  var map=Map.of("k",List.of(1,2));
  var values=Arrays.asList(vertex,map,null,vertex,null,map,3L,3L,3L);
  var traversal=UpstreamGremlin.orderedResults(values);
  var actual=new ArrayList<Object>();
  while(traversal.hasNext())actual.add(traversal.next());
  assertEquals(values,actual);
  assertFalse(UpstreamGremlin.orderedResults(List.of()).hasNext());
 }
}
