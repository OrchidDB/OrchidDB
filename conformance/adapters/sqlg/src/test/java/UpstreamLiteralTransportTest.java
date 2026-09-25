import java.math.BigDecimal;
import org.junit.Test;
import static org.junit.Assert.*;

public class UpstreamLiteralTransportTest {
 @Test public void decimalAndDoubleHaveDistinctNativeLiterals() throws Exception {
  try(var callbacks=new io.orchiddb.gremlin.OrchidCallbackRegistry()) {
   var translator=new io.orchiddb.gremlin.OrchidBytecodeTranslator(callbacks);
   var code=new org.apache.tinkerpop.gremlin.process.traversal.Bytecode();
   code.addStep("inject",new BigDecimal("1"),Double.valueOf(1),new BigDecimal("12345678901234567890.00000001"),Double.NaN);
   assertEquals("g.inject(1M,1.0D,12345678901234567890.00000001M,NaN)",org.apache.tinkerpop.gremlin.process.traversal.translator.GroovyTranslator.of("g",translator).translate(code).getScript());
  }
 }
 @Test public void nativeEntriesRemainDistinctFromMaps() throws Exception {
  var entry=UpstreamGremlin.nativeValue(UpstreamGremlin.json.readTree("{\"type\":\"entry\",\"value\":[{\"type\":\"long\",\"value\":42},{\"type\":\"string\",\"value\":\"x\"}]}"));
  assertTrue(entry instanceof java.util.Map.Entry<?,?>);
  assertEquals(Long.valueOf(42),((java.util.Map.Entry<?,?>)entry).getKey());
  assertEquals("x",((java.util.Map.Entry<?,?>)entry).getValue());
  var map=UpstreamGremlin.nativeValue(UpstreamGremlin.json.readTree("{\"type\":\"map\",\"value\":[[{\"type\":\"string\",\"value\":\"key\"},{\"type\":\"string\",\"value\":\"x\"}]]}"));
  assertTrue(map instanceof java.util.Map<?,?>);
  assertFalse(map instanceof java.util.Map.Entry<?,?>);
 }
 @Test public void nativeCollectionKindsAndElementWidthsRemainDistinct() throws Exception {
  String items="[{\"type\":\"int\",\"value\":1},{\"type\":\"long\",\"value\":1},{\"type\":\"int\",\"value\":1}]";
  var set=UpstreamGremlin.nativeValue(UpstreamGremlin.json.readTree("{\"type\":\"set\",\"value\":"+items+"}"));
  var list=UpstreamGremlin.nativeValue(UpstreamGremlin.json.readTree("{\"type\":\"list\",\"value\":"+items+"}"));
  var bulk=UpstreamGremlin.nativeValue(UpstreamGremlin.json.readTree("{\"type\":\"bulkset\",\"value\":"+items+"}"));
  assertTrue(set instanceof java.util.Set<?>);
  assertEquals(java.util.Set.of(Integer.valueOf(1),Long.valueOf(1)),set);
  assertEquals(java.util.List.of(Integer.valueOf(1),Long.valueOf(1),Integer.valueOf(1)),list);
  assertTrue(bulk instanceof org.apache.tinkerpop.gremlin.process.traversal.step.util.BulkSet<?>);
  assertEquals(2L,((org.apache.tinkerpop.gremlin.process.traversal.step.util.BulkSet<Object>)bulk).get(Integer.valueOf(1)));
  var empty=UpstreamGremlin.nativeValue(UpstreamGremlin.json.readTree("{\"type\":\"set\",\"value\":[]}"));
  assertEquals(java.util.Set.of(),empty);
  assertFalse(empty instanceof java.util.List<?>);
 }
}
