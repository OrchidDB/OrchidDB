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
 @Test public void originalGraphCountAssertionReceivesItsReferencedParameters() throws Exception {
  UpstreamGremlin.backend="reference";var context=new UpstreamGremlin.Context();var steps=new StepDefinition(context);
  try {
   steps.givenTheXGraph("modern");UpstreamGremlin.backend="orchiddb-jvm";
   var json=UpstreamGremlin.json;
   UpstreamGremlin.step(steps,json.valueToTree(Map.of("text","using the parameter vid1 defined as \"v[marko].id\"")));
   UpstreamGremlin.step(steps,json.valueToTree(Map.of("text","using the parameter unused defined as \"c[it.get()]\"")));
   UpstreamGremlin.step(steps,json.valueToTree(Map.of("text","the graph should return 3 for count of \"g.V(vid1).outE()\"")));
   Map<?,?> originals=(Map<?,?>)UpstreamGremlin.field(steps,"stringParameters");
   assertTrue(originals.containsKey("vid1"));assertFalse(originals.containsKey("unused"));
   try {UpstreamGremlin.step(steps,json.valueToTree(Map.of("text","the graph should return 4 for count of \"g.V(vid1).outE()\"")));fail("Original assertion must reject an incorrect count");}
   catch(AssertionError expected){assertTrue(expected.getMessage().contains("expected:<4>"));}
  } finally {context.afterEachScenario();UpstreamGremlin.backend="reference";}
 }
 @Test public void capturedIdsSurviveElementDeletionBeforeOriginalCountAssertions() throws Exception {
  UpstreamGremlin.backend="reference";var context=new UpstreamGremlin.Context();var steps=new StepDefinition(context);
  try {
   steps.givenTheXGraph("modern");UpstreamGremlin.backend="orchiddb-jvm";
   UpstreamGremlin.step(steps,UpstreamGremlin.json.valueToTree(Map.of("text","using the parameter vid1 defined as \"v[marko].id\"")));
   context.graph.traversal().V(context.typedParameters.get("vid1")).drop().iterate();
   UpstreamGremlin.step(steps,UpstreamGremlin.json.valueToTree(Map.of("text","the graph should return 0 for count of \"g.V(vid1)\"")));
  } finally {context.afterEachScenario();UpstreamGremlin.backend="reference";}
 }
 @Test public void directionAliasesRemainTypedInsideRecursiveMapParameters() throws Exception {
  UpstreamGremlin.backend="reference";var context=new UpstreamGremlin.Context();var steps=new StepDefinition(context);
  try {
   steps.givenTheXGraph("modern");
   Map<?,?> values=(Map<?,?>)UpstreamGremlin.typedParameter(steps,"m[{\"D[from]\":\"v[marko]\",\"D[to]\":{\"nested\":[\"D[IN]\",\"D[from]\"]}}]");
   assertTrue(values.get(Direction.OUT) instanceof Vertex);
   assertEquals(List.of(Direction.IN,Direction.OUT),((Map<?,?>)values.get(Direction.IN)).get("nested"));
  } finally {context.afterEachScenario();}
 }
 @Test public void nullCompilerTypesSelectGremlinOverloadsWithoutChangingLiterals() throws Exception {
  UpstreamGremlin.backend="reference";var context=new UpstreamGremlin.Context();var steps=new StepDefinition(context);
  try {
   steps.givenTheXGraph("empty");
   assertEquals(1,UpstreamGremlin.jvmTraversal(steps,"g.mergeV(null).option(Merge.onCreate,null)").toList().size());
   assertEquals(1,UpstreamGremlin.jvmTraversal(steps,"g.mergeV([:]).option(Merge.onMatch,null)").toList().size());
   assertEquals(1,UpstreamGremlin.jvmTraversal(steps,"g.mergeV(__.constant([:]))").toList().size());
   assertEquals(Arrays.asList(null,null),UpstreamGremlin.jvmTraversal(steps,"g.inject(null,null)").toList());
   var injected=UpstreamGremlin.jvmTraversal(steps,"g.inject(1).inject(null,null)").toList();
   assertEquals(3,injected.size());assertEquals(2,Collections.frequency(injected,null));assertEquals(1,Collections.frequency(injected,1));
   assertEquals(List.of("mergeV(null)"),UpstreamGremlin.jvmTraversal(steps,"g.inject('mergeV(null)')").toList());
   try{UpstreamGremlin.jvmTraversal(steps,"g.mergeE(null)").toList();fail("An edge still requires endpoints");}
   catch(IllegalArgumentException expected){assertTrue(expected.getMessage().contains("Out Vertex not specified"));}
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
