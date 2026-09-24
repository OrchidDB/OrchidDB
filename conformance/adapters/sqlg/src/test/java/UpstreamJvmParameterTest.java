import java.util.*;
import groovy.lang.Closure;
import org.apache.tinkerpop.gremlin.features.StepDefinition;
import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.*;
import org.junit.Test;
import static org.junit.Assert.*;

/** Tests transport mechanics only; provider conformance uses the original scenarios. */
public class UpstreamJvmParameterTest {
 @Test public void originalObjectConverterPreservesEdgesAndNumericWidths() throws Exception {
  UpstreamGremlin.backend="reference";
  var context=new UpstreamGremlin.Context();var steps=new StepDefinition(context);
  try {
   steps.givenTheXGraph("modern");
   Object edge=UpstreamGremlin.typedParameter(steps,"e[marko-knows->vadas]");
   assertTrue(edge instanceof Edge);assertEquals("knows",((Edge)edge).label());
   Object edges=UpstreamGremlin.typedParameter(steps,"l[e[marko-knows->vadas], e[marko-knows->josh]]");
   assertEquals(2,((List<?>)edges).size());assertTrue(((List<?>)edges).get(1) instanceof Edge);
   assertTrue(UpstreamGremlin.typedParameter(steps,"d[1].i") instanceof Integer);
   assertTrue(UpstreamGremlin.typedParameter(steps,"d[1].l") instanceof Long);
   assertTrue(UpstreamGremlin.typedParameter(steps,"d[1].f") instanceof Float);
  } finally {context.afterEachScenario();}
 }
 @Test public void emptySetIsTypedMutableAndRetainsUniqueness() throws Exception {
  UpstreamGremlin.backend="reference";var context=new UpstreamGremlin.Context();var steps=new StepDefinition(context);
  Set<Object> set=(Set<Object>)UpstreamGremlin.typedParameter(steps,"s[]");
  assertTrue(set.add(1));assertFalse(set.add(1));assertEquals(Set.of(1),set);
 }
 @Test public void closuresCompileOnceWithRealCapturedBindingsAndNoNestedWrapper() throws Exception {
  UpstreamGremlin.backend="reference";var context=new UpstreamGremlin.Context();var steps=new StepDefinition(context);
  try {
   steps.givenTheXGraph("empty");context.typedParameters.put("offset",3);
   Closure<?> simple=(Closure<?>)UpstreamGremlin.typedParameter(steps,"c[it + offset]");
   Closure<?> braced=(Closure<?>)UpstreamGremlin.typedParameter(steps,"c[{ x -> x + offset }]");
   assertEquals(10,simple.call(7));assertEquals(10,braced.call(7));
  } finally {context.afterEachScenario();}
 }
 @Test public void fixtureCopyPreservesPublicIdsCardinalityAndMetadata() throws Exception {
  try(TinkerGraph fixture=TinkerFactory.createTheCrew();TinkerGraph copy=TinkerGraph.open()){
   UpstreamGremlin.copyFixture(fixture,copy);
   assertEquals(fixture.traversal().V().count().next(),copy.traversal().V().count().next());
   assertEquals(fixture.traversal().E().count().next(),copy.traversal().E().count().next());
   fixture.vertices().forEachRemaining(v->{
    Vertex target=copy.vertices(v.id()).next();assertEquals(v.label(),target.label());
    v.properties().forEachRemaining(p->{
     VertexProperty<?> cloned=(VertexProperty<?>)copy.traversal().V(v.id()).properties(p.key()).hasId(p.id()).next();
     assertEquals(p.value(),cloned.value());
     p.properties().forEachRemaining(meta->assertEquals(meta.value(),cloned.value(meta.key())));
    });
   });
  }
 }
}
