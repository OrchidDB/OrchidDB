import java.math.BigDecimal;
import org.junit.Test;
import static org.junit.Assert.*;

public class UpstreamLiteralTransportTest {
 @Test public void decimalAndDoubleHaveDistinctNativeLiterals() {
  var translator=new UpstreamGremlin.GrammarTypeTranslator();
  assertEquals("1M",translator.getSyntax(new BigDecimal("1")));
  assertEquals("1.0D",translator.getSyntax(Double.valueOf(1)));
  assertEquals("12345678901234567890.00000001M",translator.getSyntax(new BigDecimal("12345678901234567890.00000001")));
  assertEquals("NaN",translator.getSyntax(Double.NaN));
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
}
