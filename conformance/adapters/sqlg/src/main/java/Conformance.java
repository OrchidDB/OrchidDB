import org.umlg.sqlg.structure.SqlgGraph;
import org.apache.commons.configuration2.BaseConfiguration;
import org.apache.tinkerpop.gremlin.groovy.jsr223.GremlinGroovyScriptEngine;
import org.apache.tinkerpop.gremlin.process.traversal.Traversal;
import org.apache.tinkerpop.gremlin.structure.T;
import com.fasterxml.jackson.databind.ObjectMapper;
import javax.script.SimpleBindings;
import java.io.*;
import java.util.*;
public class Conformance {
 public static void main(String[] args) throws Exception {
  var config=new BaseConfiguration();
  config.addProperty("jdbc.url",System.getenv().getOrDefault("SQLG_JDBC_URL","jdbc:postgresql://127.0.0.1:15433/conformance"));
  config.addProperty("jdbc.username","conformance");config.addProperty("jdbc.password","conformance-local-only");
  try(var graph=SqlgGraph.open(config)) {
   var g=graph.traversal();g.V().drop().iterate();graph.tx().commit();
   var a=graph.addVertex(T.label,"Person","uid",1L,"name","Alice","age",30L);
   var b=graph.addVertex(T.label,"Person","uid",2L,"name","Bob","age",20L);
   var c=graph.addVertex(T.label,"Person","uid",3L,"name","Carol","age",40L);
   graph.addVertex(T.label,"Person","uid",4L,"name","Dave");
   a.addEdge("KNOWS",b,"weight",1L);b.addEdge("KNOWS",c,"weight",2L);a.addEdge("KNOWS",c,"weight",3L);c.addEdge("KNOWS",a,"weight",4L);graph.tx().commit();
   var mapper=new ObjectMapper();var engine=new GremlinGroovyScriptEngine();var bindings=new SimpleBindings();bindings.put("g",g);
   var input=new BufferedReader(new InputStreamReader(System.in));String line;
   System.out.println("{\"ready\":true}");System.out.flush();
   while((line=input.readLine())!=null) {
    try {
     var req=mapper.readTree(line);Object value=engine.eval(req.get("query").asText(),bindings);
     List<?> values=value instanceof Traversal<?,?> t?t.toList():List.of(value);
     List<List<?>> rows=new ArrayList<>();for(Object v:values) rows.add(Collections.singletonList(v));
     System.out.println(mapper.writeValueAsString(Map.of("rows",rows)));graph.tx().rollback();
    } catch(Exception e) {graph.tx().rollback();System.out.println(mapper.writeValueAsString(Map.of("error",String.valueOf(e.getMessage()))));}
    System.out.flush();
   }
  }
 }
}
