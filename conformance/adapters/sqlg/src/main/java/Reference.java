import org.apache.tinkerpop.gremlin.tinkergraph.structure.TinkerGraph;
import org.apache.tinkerpop.gremlin.groovy.jsr223.GremlinGroovyScriptEngine;
import org.apache.tinkerpop.gremlin.process.traversal.Traversal;
import org.apache.tinkerpop.gremlin.structure.T;
import com.fasterxml.jackson.databind.ObjectMapper;
import javax.script.SimpleBindings;
import java.io.*;
import java.util.*;
public class Reference {
 public static void main(String[] args) throws Exception {
 var mapper=new ObjectMapper(); var engine=new GremlinGroovyScriptEngine();
 var input=new BufferedReader(new InputStreamReader(System.in));String line;
 System.out.println("{\"ready\":true}");System.out.flush();
 while((line=input.readLine())!=null) {try(var graph=TinkerGraph.open()) {
 var g=graph.traversal();
   var a=graph.addVertex(T.label,"Person","uid",1L,"name","Alice","age",30L);
   var b=graph.addVertex(T.label,"Person","uid",2L,"name","Bob","age",20L);
   var c=graph.addVertex(T.label,"Person","uid",3L,"name","Carol","age",40L);
   graph.addVertex(T.label,"Person","uid",4L,"name","Dave");
   a.addEdge("KNOWS",b,"weight",1L);b.addEdge("KNOWS",c,"weight",2L);a.addEdge("KNOWS",c,"weight",3L);c.addEdge("KNOWS",a,"weight",4L);

 var bindings=new SimpleBindings();bindings.put("g",g);
 try {var req=mapper.readTree(line);Object value=engine.eval(req.get("query").asText(),bindings);
 List<?> values=value instanceof Traversal<?,?> t?t.toList():Collections.singletonList(value);
 List<List<?>> rows=new ArrayList<>();for(Object v:values)rows.add(Collections.singletonList(v));
 System.out.println(mapper.writeValueAsString(Map.of("rows",rows)));
 }catch(Exception e){System.out.println(mapper.writeValueAsString(Map.of("error",String.valueOf(e.getMessage()))));}
 System.out.flush();
 }}
 }}
