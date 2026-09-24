package io.crabgraph.gremlin;

import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;
import javax.script.SimpleBindings;
import org.apache.tinkerpop.gremlin.groovy.jsr223.GremlinGroovyScriptEngine;
import org.apache.tinkerpop.gremlin.process.traversal.Traversal;

/** One IR operator. Native callbacks use the caller's staged graph; no independent commit. */
public final class CrabIr {
    private CrabIr() { }
    private static Object encode(Object value,CrabGraph graph) {
        checkOwnership(value,graph);
        return CrabCodec.encode(value,graph);
    }
    private static void checkOwnership(Object value,CrabGraph graph) {
        if(value instanceof org.apache.tinkerpop.gremlin.structure.Element element && element.graph()!=graph)
            throw new IllegalArgumentException("Project execution-local graph elements to values before crossing the JVM IR boundary");
        if(value instanceof org.apache.tinkerpop.gremlin.structure.Property<?> property) checkOwnership(property.element(),graph);
        if(value instanceof org.apache.tinkerpop.gremlin.process.traversal.Path path) for(Object item:path.objects()) checkOwnership(item,graph);
        if(value instanceof Map<?,?> map) for(var entry:map.entrySet()) { checkOwnership(entry.getKey(),graph); checkOwnership(entry.getValue(),graph); }
        if(value instanceof Collection<?> values) for(Object item:values) checkOwnership(item,graph);
    }
    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws Exception {
        OutputStream protocol=System.out;
        // Application println must not corrupt native callback framing.
        System.setOut(System.err);
        ObjectMapper mapper=new ObjectMapper();
        BufferedReader input=new BufferedReader(new InputStreamReader(System.in,StandardCharsets.UTF_8));
        Map<String,Object> response=new LinkedHashMap<>();
        response.put("kind","result");
        try {
            Map<String,Object> request=mapper.readValue(input.readLine(),Map.class);
            CrabGraph graph=CrabGraph.forIr(input,protocol);
            graph.tx().open(); // The surrounding IR execution owns this transaction.
            GremlinGroovyScriptEngine engine=new GremlinGroovyScriptEngine();
            {
                List<Object> output=new ArrayList<>();
                String mode=(String)request.get("mode");
                List<Map<String,Object>> rows=(List<Map<String,Object>>)request.get("rows");
                for(Map<String,Object> row:rows) {
                    SimpleBindings bindings=new SimpleBindings();
                    for(var entry:row.entrySet()) bindings.put(entry.getKey(),CrabCodec.decode(entry.getValue(),graph));
                    bindings.put("graph",graph); bindings.put("g",graph.traversal());
                    Object value=engine.eval((String)request.get("script"),bindings);
                    List<Object> values=new ArrayList<>();
                    if("FlatMap".equals(mode)) {
                        if(value instanceof Traversal<?,?> traversal) {
                            try(traversal) { while(traversal.hasNext()) values.add(encode(traversal.next(),graph)); }
                        } else if(value instanceof Iterable<?> iterable) {
                            for(Object item:iterable) values.add(encode(item,graph));
                        } else if(value instanceof Iterator<?> iterator) {
                            while(iterator.hasNext()) values.add(encode(iterator.next(),graph));
                        } else throw new IllegalArgumentException("JVM flatMap must return an iterable or traversal");
                    } else if("Map".equals(mode)||"Filter".equals(mode)) {
                        if("Filter".equals(mode)&&!(value instanceof Boolean)) throw new IllegalArgumentException("JVM filter must return Boolean");
                        values.add(encode(value,graph));
                    } else throw new IllegalArgumentException("Unknown JVM IR mode: "+mode);
                    output.add(values);
                }
                response.put("rows",output); response.put("ok",true);
            }
        } catch(Throwable failure) {
            response.put("ok",false); response.put("error",failure.getClass().getName()+": "+failure.getMessage());
        }
        protocol.write((mapper.writeValueAsString(response)+"\n").getBytes(StandardCharsets.UTF_8));
        protocol.flush();
        // Callback reader is daemon; the Rust parent owns worker lifetime and cancellation.
    }
}
