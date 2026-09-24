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
 /** Preserve the fixture's property records instead of flattening cardinality or metadata. */
 static Map<String,Object> fixtureNode(Vertex vertex,String targetBackend) {
  Map<String,Object> properties=new LinkedHashMap<>();
  List<Object> records=new ArrayList<>();
  vertex.properties().forEachRemaining(property->{
   Map<String,Object> meta=new LinkedHashMap<>();
   property.properties().forEachRemaining(p->meta.put(p.key(),p.value()));
   if(targetBackend.equals("puppygraph")&&(properties.containsKey(property.key())||!meta.isEmpty()))
    throw new AssumptionViolatedException("adapter-skip: PuppyGraph fixture mapping cannot preserve multi/meta-properties");
   properties.putIfAbsent(property.key(),property.value());
   if(targetBackend.equals("crabgraph"))records.add(Map.of(
    "id",property.id(),"id_type",property.id().getClass().getSimpleName(),
    "key",property.key(),"value",property.value(),"type",property.value().getClass().getSimpleName(),
    "meta",meta,"meta_types",propertyTypes(meta)));
  });
  Map<String,Object> result=new LinkedHashMap<>();
  result.put("id",vertex.id());result.put("id_type",vertex.id().getClass().getSimpleName());
  result.put("label",vertex.label());result.put("properties",properties);result.put("property_types",propertyTypes(properties));
  if(targetBackend.equals("crabgraph"))result.put("property_records",records);
  return result;
 }
 static Map<String,Object> fixtureEdge(Edge edge) {
  Map<String,Object> properties=new LinkedHashMap<>();
  edge.properties().forEachRemaining(property->properties.put(property.key(),property.value()));
  return Map.of("id",edge.id(),"id_type",edge.id().getClass().getSimpleName(),"label",edge.label(),
   "src",edge.outVertex().id(),"dst",edge.inVertex().id(),"properties",properties,"property_types",propertyTypes(properties));
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
 /** Keep upstream literal translation; omit Groovy's inject(null) overload cast. */
 static class GrammarTypeTranslator extends GroovyTranslator.LanguageTypeTranslator {
  GrammarTypeTranslator(){super(false);}
  @Override protected String getSyntax(Number value) {
   // The upstream Groovy renderer uses D for both Double and BigDecimal.
   // Gremlin-language uses M for exact decimals; preserve the Java input type.
   if(value instanceof java.math.BigDecimal decimal)return decimal.toString()+"M";
   return super.getSyntax(value);
  }
  @Override protected org.apache.tinkerpop.gremlin.process.traversal.Script produceScript(Set<?> value) {
   script.append("{");int i=0;for(Object item:value){if(i++>0)script.append(",");convertToScript(item);}return script.append("}");
  }
  @Override protected org.apache.tinkerpop.gremlin.process.traversal.Script produceScript(org.apache.tinkerpop.gremlin.process.traversal.strategy.TraversalStrategyProxy<?> value) {
   if(!value.getStrategyClass().equals(org.apache.tinkerpop.gremlin.process.traversal.strategy.decoration.PartitionStrategy.class))return super.produceScript(value);
   if(value.getConfiguration().isEmpty())return produceScript(value.getStrategyClass());
   script.append("new ");produceScript(value.getStrategyClass());script.append("(");int i=0;
   for(var entry:org.apache.commons.configuration2.ConfigurationConverter.getMap(value.getConfiguration()).entrySet()) {
    if(i++>0)script.append(", ");script.append(entry.getKey().toString()).append(": ");
    Object argument=entry.getValue();
    // Strategy grammar defines readPartitions as a string list; it is a set in Java configuration.
    if(entry.getKey().equals("readPartitions")&&argument instanceof Set<?> items)argument=new ArrayList<>(items);
    convertToScript(argument);
   }
   return script.append(")");
  }
  @Override protected org.apache.tinkerpop.gremlin.process.traversal.Script produceScript(Map<?,?> value) {
   // The upstream Groovy renderer emits [] for both empty maps and lists.
   // Gremlin-language needs the map literal [:]; use actual Java type identity.
   return value.isEmpty()?script.append("[:]"):super.produceScript(value);
  }
  @Override protected org.apache.tinkerpop.gremlin.process.traversal.Script produceScript(String traversalSource,Bytecode bytecode) {
   script.append(traversalSource);
   for(Bytecode.Instruction instruction:bytecode.getInstructions()) {
    script.append(".").append(instruction.getOperator()).append("(");
    if(instruction.getOperator().equals(GraphTraversalSource.Symbols.tx)&&instruction.getArguments().length>0) {
     script.append(").").append(instruction.getArguments()[0].toString()).append("(");
    } else {
     Object[] args=instruction.getArguments();
     for(int i=0;i<args.length;i++){if(i>0)script.append(",");convertToScript(args[i]);}
    }
    script.append(")");
   }
   return script;
  }
 }
 static String nativeFloatingLiteral(JsonNode value) {
  return switch(value.asText()) {
   case "inf", "+inf", "+Infinity" -> "Infinity";
   case "-inf" -> "-Infinity";
   case "nan", "NaN" -> "NaN";
   default -> value.asText();
  };
 }
 static Object nativeValue(JsonNode v) {
  String type=v.path("type").asText();JsonNode x=v.get("value");
  return switch(type) {
   case "token" -> T.valueOf(x.asText());case "direction" -> Direction.valueOf(x.asText());
   case "null" -> null;case "boolean" -> x.asBoolean();case "string" -> x.asText();
   case "byte" -> (byte)x.asInt();case "short" -> (short)x.asInt();case "int" -> {long n=x.longValue();if(n<Integer.MIN_VALUE||n>Integer.MAX_VALUE)throw new IllegalArgumentException("Unmapped out-of-range native int: "+v);yield (int)n;}case "long" -> x.longValue();
   case "float" -> Float.valueOf(nativeFloatingLiteral(x));case "double" -> Double.valueOf(nativeFloatingLiteral(x));
   case "bigint","uint64","uint128" -> new java.math.BigInteger(x.asText());case "bigdecimal" -> new java.math.BigDecimal(x.asText());
   case "uint8","uint16" -> x.intValue();case "uint32" -> x.longValue();
   case "datetime" -> org.apache.tinkerpop.gremlin.util.DatetimeHelper.parse(x.asText());
   case "bulkset" -> {var values=new org.apache.tinkerpop.gremlin.process.traversal.step.util.BulkSet<Object>();x.forEach(item->values.add(nativeValue(item)));yield values;}
   case "list","set" -> {List<Object> values=new ArrayList<>();x.forEach(item->values.add(nativeValue(item)));yield type.equals("set")?new LinkedHashSet<>(values):values;}
   case "entry" -> new AbstractMap.SimpleImmutableEntry<>(nativeValue(x.get(0)),nativeValue(x.get(1)));
   case "map" -> {Map<Object,Object> values=new LinkedHashMap<>();x.forEach(pair->values.put(nativeValue(pair.get(0)),nativeValue(pair.get(1))));yield values;}
   case "vertex" -> new org.apache.tinkerpop.gremlin.structure.util.reference.ReferenceVertex(nativeValue(v.get("id")),v.get("label").asText());
   case "edge" -> new org.apache.tinkerpop.gremlin.structure.util.reference.ReferenceEdge(nativeValue(v.get("id")),v.get("label").asText(),new org.apache.tinkerpop.gremlin.structure.util.reference.ReferenceVertex(nativeValue(v.get("inV")),v.get("inVLabel").asText()),new org.apache.tinkerpop.gremlin.structure.util.reference.ReferenceVertex(nativeValue(v.get("outV")),v.get("outVLabel").asText()));
   case "vertex_property" -> nativeVertexProperty(v);
   case "property" -> nativeProperty(v);
   case "path" -> {var path=org.apache.tinkerpop.gremlin.process.traversal.step.util.MutablePath.make();x.forEach(item->path.extend(nativeValue(item),Set.of()));yield path;}
   default -> throw new IllegalArgumentException("Unmapped native result type "+v);
  };
 }
 /** Native properties retain their owners; maps with similar fields are still maps. */
 static VertexProperty<Object> nativeVertexProperty(JsonNode value) {
  Object owner=nativeValue(value.get("owner"));
  if(!(owner instanceof Vertex vertex))throw new IllegalArgumentException("Vertex property owner must be a vertex: "+value);
  Map<String,Object> properties=new LinkedHashMap<>();
  for(JsonNode property:value.path("properties")) {
   if(property.has("type")&&!property.path("type").asText().equals("property"))throw new IllegalArgumentException("Invalid native meta-property: "+property);
   // The enclosing vertex property supplies the owner. This also avoids
   // requiring an infinitely recursive owner->properties->owner wire value.
   properties.put(property.get("key").asText(),nativeValue(property.get("value")));
  }
  return new org.apache.tinkerpop.gremlin.structure.util.detached.DetachedVertexProperty<>(
   nativeValue(value.get("id")),value.get("key").asText(),nativeValue(value.get("value")),properties,vertex);
 }
 static Property<Object> nativeProperty(JsonNode value) {
  Object owner=nativeValue(value.get("owner"));
  if(!(owner instanceof Edge)&&!(owner instanceof VertexProperty<?>))throw new IllegalArgumentException("Property owner must be an edge or vertex property: "+value);
  return new org.apache.tinkerpop.gremlin.structure.util.detached.DetachedProperty<>(
   value.get("key").asText(),nativeValue(value.get("value")),(Element)owner);
 }
 static Bridge bridge;
 static SqlgGraph cachedSqlg;static String cachedFixture;
 static class Context implements World {
  Graph graph; Cluster cluster; GraphTraversalSource source;List<Object> queryTransports=new ArrayList<>();String currentStep="";
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
    fixture.vertices().forEachRemaining(v->nodes.add(fixtureNode(v,backend)));
    fixture.edges().forEachRemaining(e->edges.add(fixtureEdge(e)));
    var response=bridge.send(Map.of("op","fixture","name",data==null?"empty":data.name().toLowerCase(),"nodes",nodes,"edges",edges));
    if(response.has("error"))throw new IOException("fixture-adapter: "+response.get("error").asText());
    fixture.close();
    if(backend.equals("puppygraph")){
     cluster=Cluster.build("127.0.0.1").port(18182).credentials("puppygraph","conformance-local-only").create();
     source=org.apache.tinkerpop.gremlin.process.traversal.AnonymousTraversalSource.traversal().withRemote(DriverRemoteConnection.using(cluster,"g"));return source;
    }
    source=org.apache.tinkerpop.gremlin.process.traversal.AnonymousTraversalSource.traversal().withRemote(new RemoteConnection(){
     public <E> CompletableFuture<RemoteTraversal<?,E>> submitAsync(Bytecode bytecode) throws RemoteConnectionException {
      try{
       String script=GroovyTranslator.of("g", new GrammarTypeTranslator()).translate(bytecode).getScript();
       var response=bridge.send(Map.of("op","gremlin","query",script));
       queryTransports.add(Map.of("step",currentStep,"query",script,"backend",response.path("backend").asText(""),"native_rows",response.hasNonNull("native_rows"),"error",response.path("error").asText("")));
       if(response.has("error"))throw new RemoteConnectionException((response.path("timeout").asBoolean()?"adapter-timeout: ":response.path("adapter_error").asBoolean()?"adapter-error: ":"")+response.get("error").asText());
       boolean nativeRows=response.hasNonNull("native_rows");List<Object> values=new ArrayList<>();for(var row:response.get(nativeRows?"native_rows":"typed_rows")){values.add(nativeRows?nativeValue(row.get(0)):typedValue(row.get(0)));}
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
   for(var s:request.get("steps")){failedStep=s.get("text").asText();context.currentStep=failedStep;long before=System.nanoTime();step(def,s);timings.add(Map.of("step",failedStep,"elapsed_ms",(System.nanoTime()-before)/1e6));}}
   catch(AssumptionViolatedException ex){status=ex.getMessage().startsWith("unsupported-feature:")?"unsupported":"skipped";error=ex.getMessage();}
   catch(AssertionError ex){status="fail";error=ex.toString();}
   catch(LinkageError ex){status="adapter-error";error=ex.toString();}
   catch(Throwable ex){status=failedStep.matches("the \\w+ graph")||ex.getMessage()!=null&&ex.getMessage().startsWith("Unmapped")?"adapter-error":"fail";error=ex.toString();}
   Object actual=field(def,"result");String actualJson;
   try{actualJson=GraphSONMapper.build().version(GraphSONVersion.V3_0).create().createMapper().writeValueAsString(actual);}catch(Exception ex){actualJson=String.valueOf(actual);}
   Object queryError=field(def,"error");
   if(String.valueOf(queryError).contains("adapter-timeout: "))status="timeout";
   else if(String.valueOf(queryError).contains("adapter-error: "))status="adapter-error";
   try{context.afterEachScenario();}catch(Exception ex){status="adapter-error";error+=" cleanup: "+ex;}
   System.out.println(json.writeValueAsString(Map.of("id",request.get("id").asText(),"status",status,"error",error,"failed_step",status.equals("pass")?"":failedStep,"actual_graphson",actualJson,"query_error",String.valueOf(queryError),"elapsed_ms",(System.nanoTime()-start)/1e6,"step_timings",timings,"query_transports",context.queryTransports,"assertion_engine","Apache gremlin-test 3.7.4 StepDefinition (unmodified)")));System.out.flush();
  }
  if(bridge!=null)bridge.process.destroy();if(cachedSqlg!=null)cachedSqlg.close();
 }
}
