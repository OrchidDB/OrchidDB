package io.crabgraph.gremlin;

import java.util.*;
import java.util.function.*;
import javax.script.SimpleBindings;
import org.apache.tinkerpop.gremlin.groovy.jsr223.GremlinGroovyScriptEngine;
import org.apache.tinkerpop.gremlin.structure.Vertex;
import org.junit.*;
import static org.junit.Assert.*;

public class CrabCallbackRegistryTest {
    @Test public void originalJavaFunctionsUseBorrowedGraphAndPropagateFailure() throws Exception {
        String nativeStore=System.getenv("CRABGRAPH_JVM_STORE");
        Assume.assumeNotNull(nativeStore);
        try(CrabGraph graph=CrabGraph.open(nativeStore);CrabCallbackRegistry registry=new CrabCallbackRegistry()) {
            Vertex vertex=graph.addVertex("name","alice");
            var groovy=new GremlinGroovyScriptEngine();
            var bindings=new SimpleBindings(new HashMap<>(Map.of("graph",graph,"vertex",vertex)));
            var invocations=new java.util.concurrent.atomic.AtomicInteger();
            String supplier=registry.register((Supplier<Map<String,Object>>)()->new HashMap<>(Map.of("count",invocations.incrementAndGet())));
            Object first=groovy.eval("("+supplier+").call()",bindings);
            Object second=groovy.eval("("+supplier+").call()",bindings);
            assertEquals(Map.of("count",1),first);assertEquals(Map.of("count",2),second);
            bindings.put("sack",first);
            String update=registry.register((BiFunction<Map<String,Object>,Vertex,Map<String,Object>>)(sack,v)->{sack.put("name",v.value("name"));return sack;});
            assertEquals(Map.of("count",1,"name","alice"),groovy.eval("("+update+").call(sack,vertex)",bindings));
            String failure=registry.register((Function<Object,Object>)value->{throw new IllegalStateException("original callback failed");});
            try {groovy.eval("("+failure+").call(1)",bindings);fail("Failure must reach caller");}
            catch(javax.script.ScriptException expected){assertTrue(expected.toString(),expected.toString().contains("original callback failed"));}
            assertEquals(Map.of("count",3),groovy.eval("("+supplier+").call()",bindings));
        }
    }
}
