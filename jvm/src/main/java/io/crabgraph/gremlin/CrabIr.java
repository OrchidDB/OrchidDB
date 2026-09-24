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
        if(value instanceof Map.Entry<?,?> pair) { checkOwnership(pair.getKey(),graph); checkOwnership(pair.getValue(),graph); }
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
        GremlinGroovyScriptEngine engine=new GremlinGroovyScriptEngine();
        String line;
        while((line=input.readLine())!=null) {
        Map<String,Object> response=new LinkedHashMap<>();
        response.put("kind","result");
        CrabGraph borrowedGraph=null;
        try {
            Map<String,Object> request=mapper.readValue(line,Map.class);
            CrabGraph graph=CrabGraph.forIr(input,protocol);
            borrowedGraph=graph;
            graph.tx().open(); // The surrounding IR execution owns this transaction.

            {
                List<Object> output=new ArrayList<>();
                String mode=(String)request.get("mode");
                List<Map<String,Object>> rows=(List<Map<String,Object>>)request.get("rows");
                if("Computer".equals(mode)) {
                    response.put("rows",CrabComputerKernel.execute(request,graph,engine));
                } else if("Sort".equals(mode)) {
                    List<Map<String,Object>> decoded=new ArrayList<>();
                    for(Map<String,Object> row:rows) {
                        Map<String,Object> value=new LinkedHashMap<>();
                        row.forEach((k,v)->value.put(k,CrabCodec.decode(v,graph)));decoded.add(value);
                    }
                    List<Map<String,Object>> specs=mapper.readValue((String)request.get("script"),List.class);
                    List<groovy.lang.Closure<?>> callbacks=new ArrayList<>();
                    for(Map<String,Object> spec:specs) callbacks.add(spec.get("body")==null?null:
                        (groovy.lang.Closure<?>)engine.eval("{"+spec.get("body")+"}",new SimpleBindings(Map.of("graph",graph,"g",graph.traversal()))));
                    List<Integer> order=new ArrayList<>();for(int i=0;i<rows.size();i++)order.add(i);
                    order.sort((a,b)->{
                        for(int i=0;i<specs.size();i++) {
                            Map<String,Object> spec=specs.get(i);String key=(String)spec.get("key");
                            Object left=decoded.get(a).get(key),right=decoded.get(b).get(key);
                            int compared=callbacks.get(i)==null?org.apache.tinkerpop.gremlin.util.GremlinValueComparator.ORDERABILITY.compare(left,right):
                                ((Number)callbacks.get(i).call(left,right)).intValue();
                            if(compared!=0)return Boolean.TRUE.equals(spec.get("descending"))?-Integer.signum(compared):Integer.signum(compared);
                        }
                        return 0;
                    });
                    response.put("order",order);
                } else {
                int rowIndex=0;
                for(Map<String,Object> row:rows) {
                    SimpleBindings bindings=new SimpleBindings();
                    for(var entry:row.entrySet()) bindings.put(entry.getKey(),CrabCodec.decode(entry.getValue(),graph));
                    bindings.put("graph",graph); bindings.put("g",graph.traversal());
                    if(request.get("traversers") instanceof List<?> frames)
                        bindings.put("__crabgraph_traverser",CrabTraverser.create((Map<String,Object>)frames.get(rowIndex),graph));
                    rowIndex++;
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
                response.put("rows",output);
                }
                response.put("ok",true);
            }
        } catch(Throwable failure) {
            response.put("ok",false); response.put("error",failure.getClass().getName()+": "+failure.getMessage());
        }
        if(borrowedGraph!=null) borrowedGraph.abortFamily();
        protocol.write((mapper.writeValueAsString(response)+"\n").getBytes(StandardCharsets.UTF_8));
        protocol.flush();
        } // The native executor owns worker lifetime and cancellation.
    }
}
