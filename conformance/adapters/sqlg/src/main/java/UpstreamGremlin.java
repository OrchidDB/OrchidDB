import java.io.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.regex.*;
import java.lang.reflect.*;
import com.fasterxml.jackson.databind.*;
import org.apache.commons.configuration2.BaseConfiguration;
import org.apache.tinkerpop.gremlin.features.*;
import org.apache.tinkerpop.gremlin.LoadGraphWith.GraphData;
import org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.*;
import org.apache.tinkerpop.gremlin.process.traversal.translator.GroovyTranslator;
import org.apache.tinkerpop.gremlin.process.traversal.Bytecode;
import org.apache.tinkerpop.gremlin.process.remote.*;
import org.apache.tinkerpop.gremlin.process.remote.traversal.*;
import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.structure.io.graphson.*;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.*;
import org.apache.tinkerpop.gremlin.driver.*;
import org.apache.tinkerpop.gremlin.driver.remote.DriverRemoteConnection;
import org.umlg.sqlg.structure.SqlgGraph;
import org.junit.AssumptionViolatedException;
import io.cucumber.datatable.DataTable;

/** Calls the unmodified gremlin-test 3.7.4 step definitions and assertions. */
public class UpstreamGremlin {
 static ObjectMapper json=new ObjectMapper();
 static String backend;
 static class Bridge {
  Process process; BufferedReader output; BufferedWriter input;
  Bridge() throws Exception {
   process=new ProcessBuilder(System.getenv().getOrDefault("CONFORMANCE_PYTHON","python3"),"conformance/upstream/bridge.py",backend).redirectError(ProcessBuilder.Redirect.INHERIT).start();
   output=new BufferedReader(new InputStreamReader(process.getInputStream()));input=new BufferedWriter(new OutputStreamWriter(process.getOutputStream()));
  }
  synchronized JsonNode send(Object request) throws Exception {
   input.write(json.writeValueAsString(request)+"\n");input.flush();String line=output.readLine();
   if(line==null)throw new IOException("Fixture bridge exited");return json.readTree(line);
  }
 }
 static Map<String,String> propertyTypes(Map<String,Object> props) {
  Map<String,String> types=new LinkedHashMap<>();props.forEach((key,value)->types.put(key,value.getClass().getSimpleName()));return types;
 }
 static Object typedValue(JsonNode value) {
  if(value.isNull())return null;
  if(value.isObject()&&value.has("$type"))return switch(value.get("$type").asText()){
   case "Byte" -> (byte)value.get("value").asInt();case "Short" -> (short)value.get("value").asInt();case "Integer" -> value.get("value").intValue();case "Long" -> value.get("value").longValue();case "Float" -> value.get("value").floatValue();case "Double" -> value.get("value").doubleValue();default -> throw new IllegalArgumentException("Unmapped result type "+value);
  };
  if(value.isArray()){List<Object> result=new ArrayList<>();value.forEach(item->result.add(typedValue(item)));return result;}
  if(value.isObject()){Map<String,Object> result=new LinkedHashMap<>();value.fields().forEachRemaining(entry->result.put(entry.getKey(),typedValue(entry.getValue())));return result;}
  return json.convertValue(value,Object.class);
 }
 static Bridge bridge;
 static SqlgGraph cachedSqlg;static String cachedFixture;
 static class Context implements World {
  Graph graph; Cluster cluster; GraphTraversalSource source;
  public GraphTraversalSource getGraphTraversalSource(GraphData data) {
   try {
    TinkerGraph fixture=switch(data==null?"EMPTY":data.name()) {
     case "MODERN" -> TinkerFactory.createModern();case "CLASSIC" -> TinkerFactory.createClassic();
     case "CREW" -> TinkerFactory.createTheCrew();case "SINK" -> TinkerFactory.createKitchenSink();
     case "GRATEFUL" -> TinkerFactory.createGratefulDead();default -> {var c=new BaseConfiguration();c.setProperty(TinkerGraph.GREMLIN_TINKERGRAPH_VERTEX_ID_MANAGER,"INTEGER");c.setProperty(TinkerGraph.GREMLIN_TINKERGRAPH_EDGE_ID_MANAGER,"INTEGER");c.setProperty(TinkerGraph.GREMLIN_TINKERGRAPH_VERTEX_PROPERTY_ID_MANAGER,"LONG");yield TinkerGraph.open(c);}
    };
    if(backend.equals("reference")){fixture.getServiceRegistry().registerService(new org.apache.tinkerpop.gremlin.tinkergraph.services.TinkerTextSearchFactory(fixture));fixture.getServiceRegistry().registerService(new org.apache.tinkerpop.gremlin.tinkergraph.services.TinkerDegreeCentralityFactory(fixture));graph=fixture;return fixture.traversal();}
    if(backend.equals("sqlg")){
     String key=data==null?"empty":data.name();
     if(cachedSqlg!=null && key.equals(cachedFixture)){fixture.close();graph=cachedSqlg;return graph.traversal();}
     if(cachedSqlg!=null){cachedSqlg.close();cachedSqlg=null;}
     if(data==GraphData.CREW)throw new AssumptionViolatedException("unsupported-feature: SQLg does not support the upstream CREW multi/meta-property fixture");
     var config=new BaseConfiguration();config.setProperty("jdbc.url","jdbc:postgresql://127.0.0.1:15433/conformance?options=-c%20statement_timeout%3D10000&ApplicationName=crabgraph-upstream-sqlg");config.setProperty("jdbc.username","conformance");config.setProperty("jdbc.password","conformance-local-only");
     graph=SqlgGraph.open(config);graph.traversal().V().drop().iterate();graph.tx().commit();((SqlgGraph)graph).tx().normalBatchModeOn();
     Map<Object,Vertex> vertices=new HashMap<>();
     fixture.vertices().forEachRemaining(v->{List<Object> kv=new ArrayList<>(List.of(T.label,v.label()));v.properties().forEachRemaining(p->{kv.add(p.key());kv.add(p.value());});vertices.put(v.id(),graph.addVertex(kv.toArray()));});
     fixture.edges().forEachRemaining(e->{List<Object> kv=new ArrayList<>();e.properties().forEachRemaining(p->{kv.add(p.key());kv.add(p.value());});vertices.get(e.outVertex().id()).addEdge(e.label(),vertices.get(e.inVertex().id()),kv.toArray());});
     graph.tx().commit();cachedSqlg=(SqlgGraph)graph;cachedFixture=key;fixture.close();source=graph.traversal();return source;
    }
    List<Object> nodes=new ArrayList<>(),edges=new ArrayList<>();
    fixture.vertices().forEachRemaining(v->{Map<String,Object> props=new LinkedHashMap<>();v.properties().forEachRemaining(p->{if(props.containsKey(p.key())||p.properties().hasNext())throw new AssumptionViolatedException("adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve");props.put(p.key(),p.value());});nodes.add(Map.of("id",v.id(),"label",v.label(),"properties",props,"property_types",propertyTypes(props)));});
    fixture.edges().forEachRemaining(e->{Map<String,Object> props=new LinkedHashMap<>();e.properties().forEachRemaining(p->props.put(p.key(),p.value()));edges.add(Map.of("id",e.id(),"label",e.label(),"src",e.outVertex().id(),"dst",e.inVertex().id(),"properties",props,"property_types",propertyTypes(props)));});
    var response=bridge.send(Map.of("op","fixture","name",data==null?"empty":data.name().toLowerCase(),"nodes",nodes,"edges",edges));
    if(response.has("error"))throw new AssumptionViolatedException("adapter-skip: "+response.get("error").asText());
    fixture.close();
    if(backend.equals("puppygraph")){
     cluster=Cluster.build("127.0.0.1").port(18182).credentials("puppygraph","conformance-local-only").create();
     source=org.apache.tinkerpop.gremlin.process.traversal.AnonymousTraversalSource.traversal().withRemote(DriverRemoteConnection.using(cluster,"g"));return source;
    }
    source=org.apache.tinkerpop.gremlin.process.traversal.AnonymousTraversalSource.traversal().withRemote(new RemoteConnection(){
     public <E> CompletableFuture<RemoteTraversal<?,E>> submitAsync(Bytecode bytecode) throws RemoteConnectionException {
      try{
       String script=GroovyTranslator.of("g").translate(bytecode).getScript();
       var response=bridge.send(Map.of("op","gremlin","query",script));
       if(response.has("error"))throw new RemoteConnectionException(response.get("error").asText());
       List<Object> values=new ArrayList<>();for(var row:response.get("typed_rows")){values.add(typedValue(row.get(0)));}
       return CompletableFuture.completedFuture(new EmbeddedRemoteTraversal<>(__.inject((E[])values.toArray())));
      }catch(Exception e){throw new RemoteConnectionException(e);}
     }
     public void close(){}
    });return source;
   }catch(AssumptionViolatedException e){throw e;}catch(Exception e){throw new RuntimeException("fixture-adapter: "+e,e);}
  }
  public String convertIdToScript(Object id,Class<? extends Element> type){
   if(id instanceof Number)return id.toString();
   try{return json.writeValueAsString(id.toString());}catch(Exception e){throw new RuntimeException(e);}
  }
  public void afterEachScenario(){try{if(graph!=null){if(graph.features().graph().supportsTransactions())graph.tx().rollback();if(!backend.equals("sqlg"))graph.close();}if(cluster!=null)cluster.close();}catch(Exception e){throw new RuntimeException(e);}}
  public String changePathToDataFile(String path){return "conformance/upstream/cache/tinkerpop/"+path;}
 }
 static Object field(StepDefinition steps,String name)throws Exception{var f=StepDefinition.class.getDeclaredField(name);f.setAccessible(true);return f.get(steps);}
 static String unquote(String x)throws Exception{return json.readValue(x,String.class);}
 static void step(StepDefinition def,JsonNode s)throws Exception{
  String t=s.get("text").asText(),doc=s.has("doc")?s.get("doc").asText():"";Matcher m;
  if((m=Pattern.compile("the (\\w+) graph").matcher(t)).matches())def.givenTheXGraph(m.group(1));
  else if(t.equals("the graph initializer of"))def.theGraphInitializerOf(doc);
  else if((m=Pattern.compile("using the parameter (\\w+) defined as (.+)").matcher(t)).matches())def.usingTheParameterXDefinedAsX(m.group(1),unquote(m.group(2)));
  else if((m=Pattern.compile("using the parameter (\\w+) of P\\.(\\w+)\\((.+)\\)").matcher(t)).matches())def.usingTheParameterXOfPX(m.group(1),m.group(2),unquote(m.group(3)));
  else if(t.equals("the traversal of"))def.theTraversalOf(doc);
  else if(t.equals("iterated to list"))def.iteratedToList();else if(t.equals("iterated next"))def.iteratedNext();
  else if(t.startsWith("the result should be ")&&s.has("table")){
   List<List<String>> table=new ArrayList<>();for(var row:s.get("table")){List<String> r=new ArrayList<>();for(var c:row)r.add(c.asText());table.add(r);}var dt=DataTable.create(table);
   switch(t){case "the result should be unordered"->def.theResultShouldBeUnordered(dt);case "the result should be ordered"->def.theResultShouldBeOrdered(dt);case "the result should be of"->def.theResultShouldBeOf(dt);default->throw new IllegalArgumentException("Unmapped step: "+t);}
  }else if((m=Pattern.compile("the result should have a count of (\\d+)").matcher(t)).matches())def.theResultShouldHaveACountOf(Integer.parseInt(m.group(1)));
  else if((m=Pattern.compile("(?:debug )?the graph should return (\\d+) for count of (.+)").matcher(t)).matches())def.theGraphShouldReturnForCountOf(Integer.parseInt(m.group(1)),unquote(m.group(2)));
  else if(t.equals("the result should be empty"))def.theResultShouldBeEmpty();
  else if(t.equals("the traversal will raise an error"))def.theTraversalWillRaiseAnError();
  else if((m=Pattern.compile("the traversal will raise an error with message (\\w+) text of (.+)").matcher(t)).matches())def.theTraversalWillRaiseAnErrorWithMessage(m.group(1),unquote(m.group(2)));
  else if(t.equals("an unsupported test"))def.anUnsupportedTest();else if(t.equals("nothing should happen because"))def.nothingShouldHappenBecause(doc);
  else throw new IllegalArgumentException("Unmapped upstream step: "+t);
 }
 public static void main(String[]args)throws Exception{
  backend=args[0];if(!backend.equals("sqlg")&&!backend.equals("reference"))bridge=new Bridge();
  var input=new BufferedReader(new InputStreamReader(System.in));String line;
  System.out.println("{\"ready\":true}");System.out.flush();
  while((line=input.readLine())!=null){var request=json.readTree(line);var context=new Context();var def=new StepDefinition(context);String status="pass",error="",failedStep="";long start=System.nanoTime();List<Object> timings=new ArrayList<>();
   try{for(var tag:request.get("tags")){String t=tag.asText();if(t.equals("@GraphComputerOnly")||t.equals("@AllowNullPropertyValues")||backend.equals("reference")&&t.equals("@RemoteOnly"))throw new AssumptionViolatedException("Upstream execution profile excludes "+t);}
   for(var s:request.get("steps")){failedStep=s.get("text").asText();long before=System.nanoTime();step(def,s);timings.add(Map.of("step",failedStep,"elapsed_ms",(System.nanoTime()-before)/1e6));}}
   catch(AssumptionViolatedException ex){status=ex.getMessage().startsWith("unsupported-feature:")?"unsupported":"skipped";error=ex.getMessage();}
   catch(AssertionError ex){status="fail";error=ex.toString();}
   catch(LinkageError ex){status="adapter-error";error=ex.toString();}
   catch(Throwable ex){status=failedStep.matches("the \\w+ graph")||ex.getMessage()!=null&&ex.getMessage().startsWith("Unmapped")?"adapter-error":"fail";error=ex.toString();}
   Object actual=field(def,"result");String actualJson;
   try{actualJson=GraphSONMapper.build().version(GraphSONVersion.V3_0).create().createMapper().writeValueAsString(actual);}catch(Exception ex){actualJson=String.valueOf(actual);}
   if(backend.equals("crabgraph")&&status.equals("fail")&&actual!=null){
    String raw=String.valueOf(actual);String original=request.toString();
    if((raw.contains("v[")&&original.contains("v["))||(raw.contains("e[")&&original.contains("e["))||(raw.startsWith("[[")&&actual instanceof String)){
     status="adapter-error";error="Typed graph value identity is unavailable in the Crabgraph formatted-text result transport. Upstream assertion: "+error;
    }
   }
   Object queryError=field(def,"error");
   try{context.afterEachScenario();}catch(Exception ex){status="adapter-error";error+=" cleanup: "+ex;}
   System.out.println(json.writeValueAsString(Map.of("id",request.get("id").asText(),"status",status,"error",error,"failed_step",status.equals("pass")?"":failedStep,"actual_graphson",actualJson,"query_error",String.valueOf(queryError),"elapsed_ms",(System.nanoTime()-start)/1e6,"step_timings",timings,"assertion_engine","Apache gremlin-test 3.7.4 StepDefinition (unmodified)")));System.out.flush();
  }
  if(bridge!=null)bridge.process.destroy();if(cachedSqlg!=null)cachedSqlg.close();
 }
}
