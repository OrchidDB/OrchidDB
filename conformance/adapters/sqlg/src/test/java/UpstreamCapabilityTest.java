import org.apache.tinkerpop.gremlin.features.StepDefinition;
import org.junit.AssumptionViolatedException;
import org.junit.Test;
import static org.junit.Assert.*;

public class UpstreamCapabilityTest {
 @Test public void inlineLambdasAreDistinguishedFromStringValues() {
  assertTrue(UpstreamGremlin.hasInlineLambda("g.V().map(Lambda.function(\"it.get()\"))"));
  assertTrue(UpstreamGremlin.hasInlineLambda("g.V().map(Lambda . function ('it.get()'))"));
  assertFalse(UpstreamGremlin.hasInlineLambda("g.inject('Lambda.function(\"x\")')"));
  assertFalse(UpstreamGremlin.hasInlineLambda("g.inject(\"Lambda.function('x')\")"));
 }
 @Test public void failedCompilationKeepsItsErrorThroughIteration() throws Exception {
  UpstreamGremlin.backend="reference";
  var context=new UpstreamGremlin.Context();
  var def=new StepDefinition(context);
  try {
   def.givenTheXGraph("empty");
   UpstreamGremlin.defineTraversal(def,"g.invalidTraversal(");
   Object error=UpstreamGremlin.field(def,"error");
   assertNotNull(error);
   UpstreamGremlin.iterate(def,false);
   assertSame(error,UpstreamGremlin.field(def,"error"));
   UpstreamGremlin.iterate(def,true);
   assertSame(error,UpstreamGremlin.field(def,"error"));
   def.theTraversalWillRaiseAnError();
  } finally {context.afterEachScenario();}
 }
 @Test public void inlineLambdaFailureIdentifiesUnsupportedCapability() throws Exception {
  UpstreamGremlin.backend="reference";
  var context=new UpstreamGremlin.Context();
  var def=new StepDefinition(context);
  try {
   def.givenTheXGraph("modern");
   try {
    UpstreamGremlin.defineTraversal(def,"g.V(1).out().map(Lambda.function(\"it.get().value('name')\")).map(Lambda.function(\"it.get().toString().length()\"))");
    fail("The gremlin-language profile must report its missing lambda capability");
   } catch(AssumptionViolatedException ex) {
    assertTrue(ex.getMessage().startsWith("unsupported-feature: remote-lambda:"));
    assertNotNull(UpstreamGremlin.field(def,"error"));
    assertFalse(UpstreamGremlin.field(def,"error") instanceof NullPointerException);
   }
  } finally {context.afterEachScenario();}
 }
}
