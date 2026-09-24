import java.io.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.regex.*;
import java.lang.reflect.*;
import javax.script.*;
import groovy.lang.Closure;
import io.crabgraph.gremlin.CrabGraph;
import io.crabgraph.gremlin.CrabJvmExecutor;
import org.apache.tinkerpop.gremlin.groovy.jsr223.GremlinGroovyScriptEngine;
import org.apache.tinkerpop.gremlin.process.traversal.Traversal;
import com.fasterxml.jackson.databind.*;
import org.apache.commons.configuration2.BaseConfiguration;
import org.apache.tinkerpop.gremlin.features.*;
import org.apache.tinkerpop.gremlin.LoadGraphWith.GraphData;
import org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.*;
import org.apache.tinkerpop.gremlin.process.traversal.translator.GroovyTranslator;
import org.apache.tinkerpop.gremlin.process.traversal.Bytecode;
import org.apache.tinkerpop.gremlin.process.traversal.Traverser;
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
 static boolean jvmProfile(){return "crabgraph-jvm".equals(backend)||"crabgraph-computer".equals(backend);}
 static class Bridge {
  Process process; BufferedReader output; BufferedWriter input;
  Bridge() throws Exception {
   var builder=new ProcessBuilder(System.getenv().getOrDefault("CONFORMANCE_PYTHON","python3"),"conformance/upstream/bridge.py",backend).redirectError(ProcessBuilder.Redirect.INHERIT);
   builder.environment().put("CRABGRAPH_GREMLIN_IO_JAVA",System.getProperty("java.home")+"/bin/java");
   builder.environment().put("CRABGRAPH_GREMLIN_IO_CLASSPATH",System.getProperty("java.class.path"));
   process=builder.start();
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
 // Rows already carry the engine's expanded multiplicity and ordered sequence.
 // InjectStep uses a TraverserSet and merges separated duplicates; applying it
 // here would silently reorder an ordered remote result.
 static <E> RemoteTraversal<?,E> orderedResults(List<E> values) {
  return new AbstractRemoteTraversal<Object,E>() {
   final Iterator<E> rows=values.iterator();
   @Override public void close(){}
   @Override public boolean hasNext(){return rows.hasNext();}
   @Override public E next(){return rows.next();}
   @Override public Traverser.Admin<E> nextTraverser(){return new DefaultRemoteTraverser<>(rows.next(),1L);}
  };
 }
 static SqlgGraph cachedSqlg;static String cachedFixture;
 static class Context implements World {
  boolean allowNullPropertyValues;
  final Map<String,Object> typedParameters=new LinkedHashMap<>();
  final Map<String,String> parameterDefinitions=new LinkedHashMap<>(),predicateDefinitions=new LinkedHashMap<>();
  GremlinGroovyScriptEngine scriptEngine; CrabJvmExecutor executor;
  Graph graph; Cluster cluster; GraphTraversalSource source;List<Object> queryTransports=new ArrayList<>();String currentStep="";
  public GraphTraversalSource getGraphTraversalSource(GraphData data) {
   try {
    TinkerGraph fixture=switch(data==null?"EMPTY":data.name()) {
     case "MODERN" -> TinkerFactory.createModern();case "CLASSIC" -> TinkerFactory.createClassic();
     case "CREW" -> TinkerFactory.createTheCrew();case "SINK" -> TinkerFactory.createKitchenSink();
     case "GRATEFUL" -> TinkerFactory.createGratefulDead();default -> {var c=new BaseConfiguration();c.setProperty(TinkerGraph.GREMLIN_TINKERGRAPH_VERTEX_ID_MANAGER,"INTEGER");c.setProperty(TinkerGraph.GREMLIN_TINKERGRAPH_EDGE_ID_MANAGER,"INTEGER");c.setProperty(TinkerGraph.GREMLIN_TINKERGRAPH_VERTEX_PROPERTY_ID_MANAGER,"LONG");yield TinkerGraph.open(c);}
    };
    if(backend.equals("reference")){fixture.getServiceRegistry().registerService(new org.apache.tinkerpop.gremlin.tinkergraph.services.TinkerTextSearchFactory(fixture));fixture.getServiceRegistry().registerService(new org.apache.tinkerpop.gremlin.tinkergraph.services.TinkerDegreeCentralityFactory(fixture));graph=fixture;return fixture.traversal();}
    if(jvmProfile()){
     graph=CrabGraph.open();
     ((CrabGraph)graph).atomicMutation(()->copyFixture(fixture,graph));fixture.close();
     if(graph.features().graph().supportsTransactions())graph.tx().commit();
     source=backend.equals("crabgraph-computer")?graph.traversal().withComputer():graph.traversal();
     return source;
    }
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
    var response=bridge.send(Map.of("op","fixture","name",data==null?"empty":data.name().toLowerCase(),"nodes",nodes,"edges",edges,"allow_null_property_values",allowNullPropertyValues));
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
       return CompletableFuture.completedFuture(orderedResults((List<E>)(List<?>)values));
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
  public void afterEachScenario(){try{if(executor!=null)executor.close();if(scriptEngine!=null)scriptEngine.reset();if(source!=null)source.close();if(graph!=null&&executor==null){if(graph.features().graph().supportsTransactions())graph.tx().rollback();if(!backend.equals("sqlg"))graph.close();}if(cluster!=null)cluster.close();}catch(Exception e){throw new RuntimeException(e);}}
  public String changePathToDataFile(String path){return new File("conformance/upstream/cache/tinkerpop",path).getAbsolutePath();}
 }
 /** Copy only upstream input data; all evaluated traversals use the native provider. */
 static void copyFixture(Graph fixture,Graph target) {
  Map<Object,Vertex> vertices=new HashMap<>();
  fixture.vertices().forEachRemaining(v->{
   Vertex copy=target.addVertex(T.id,v.id(),T.label,v.label());vertices.put(v.id(),copy);
   v.properties().forEachRemaining(p->{
    List<Object> meta=new ArrayList<>(List.of(T.id,p.id()));
    p.properties().forEachRemaining(m->{meta.add(m.key());meta.add(m.value());});
    copy.property(VertexProperty.Cardinality.list,p.key(),p.value(),meta.toArray());
   });
  });
  fixture.edges().forEachRemaining(e->{
   List<Object> properties=new ArrayList<>(List.of(T.id,e.id()));
   e.properties().forEachRemaining(p->{properties.add(p.key());properties.add(p.value());});
   vertices.get(e.outVertex().id()).addEdge(e.label(),vertices.get(e.inVertex().id()),properties.toArray());
  });
 }
 static void setField(StepDefinition steps,String name,Object value)throws Exception{
  var f=StepDefinition.class.getDeclaredField(name);f.setAccessible(true);f.set(steps,value);
 }
 static Context context(StepDefinition steps)throws Exception{return (Context)field(steps,"world");}
 static Bindings bindings(StepDefinition steps,boolean remote)throws Exception{
  Context context=context(steps);Bindings bindings=new SimpleBindings();bindings.putAll(context.typedParameters);
  Object source=field(steps,"g");
  if(remote){
   if(context.executor==null)context.executor=new CrabJvmExecutor((CrabGraph)context.graph);
   source=org.apache.tinkerpop.gremlin.process.traversal.AnonymousTraversalSource.traversal().withRemote(context.executor.remoteConnection());
  }
  bindings.put("g",source);return bindings;
 }
 static GremlinGroovyScriptEngine scriptEngine(StepDefinition steps)throws Exception{
  Context context=context(steps);if(context.scriptEngine==null)context.scriptEngine=new GremlinGroovyScriptEngine();return context.scriptEngine;
 }
 static Object typedParameter(StepDefinition steps,String value)throws Exception{
  if(value.startsWith("c[")&&value.endsWith("]")){
   String body=value.substring(2,value.length()-1).trim();
   if(body.startsWith("{")&&body.endsWith("}"))body=body.substring(1,body.length()-1);
   Object closure=scriptEngine(steps).eval("return {"+body+"}",bindings(steps,false));
   if(!(closure instanceof Closure))throw new IllegalArgumentException("Lambda parameter did not compile to a Closure");
   return closure;
  }
  var converter=StepDefinition.class.getDeclaredMethod("convertToObject",Object.class);converter.setAccessible(true);
  Object converted;
  try{converted=converter.invoke(steps,value);}catch(InvocationTargetException error){throw (Exception)error.getCause();}
  // Upstream uses immutable Collections.emptySet; side effect seeds are mutable.
  return converted instanceof Set<?> set?new LinkedHashSet<>(set):converted;
 }
 /** Count assertions retain the original parser and parameter substitution. */
 static void prepareAssertionParameters(StepDefinition steps,String script)throws Exception{
  Context context=context(steps);
  for(var parameter:context.parameterDefinitions.entrySet()){
   String name=parameter.getKey();
   if(!Pattern.compile("\\b"+Pattern.quote(name)+"\\b").matcher(script).find())continue;
   String predicate=context.predicateDefinitions.get(name);
   if(predicate==null)steps.usingTheParameterXDefinedAsX(name,parameter.getValue());
   else steps.usingTheParameterXOfPX(name,predicate,parameter.getValue());
  }
 }
 static Traversal<?,?> jvmTraversal(StepDefinition steps,String script)throws Exception{
  var path=StepDefinition.class.getDeclaredMethod("tryUpdateDataFilePath",String.class);path.setAccessible(true);
  String updated=(String)path.invoke(steps,script);boolean remote=hasInlineLambda(updated);
  Object traversal=scriptEngine(steps).eval(updated,bindings(steps,remote));
  if(!(traversal instanceof Traversal<?,?> result))throw new IllegalArgumentException("Script did not return a traversal");
  context(steps).queryTransports.add(Map.of("step",context(steps).currentStep,"query",updated,"backend",backend,"submission",remote?"JVM remote bytecode":"JVM provider traversal","typed_parameters",true));
  return result;
 }
 static Object field(StepDefinition steps,String name)throws Exception{var f=StepDefinition.class.getDeclaredField(name);f.setAccessible(true);return f.get(steps);}
 static String unquote(String x)throws Exception{return json.readValue(x,String.class);}
 static boolean hasInlineLambda(String script) {
  // Ignore string contents: a property value containing "Lambda.function(...)"
  // does not require a remote execution capability.
  String code=script.replaceAll("'([^'\\\\]|\\\\.)*'|\"([^\"\\\\]|\\\\.)*\"", "");
  return Pattern.compile("\\bLambda\\s*\\.\\s*\\w+\\s*\\(").matcher(code).find();
 }
 static void defineTraversal(StepDefinition def,String script)throws Exception {
  if(jvmProfile()){
   try{setField(def,"traversal",jvmTraversal(def,script));}
   catch(Exception error){setField(def,"error",error);}
   return;
  }
  def.theTraversalOf(script);
  if(field(def,"traversal")==null&&field(def,"error")!=null&&hasInlineLambda(script))
   throw new AssumptionViolatedException("unsupported-feature: remote-lambda: the gremlin-language execution profile cannot compile inline Lambda expressions; compiler diagnostic: "+field(def,"error"), (Throwable)field(def,"error"));
 }
 static void iterate(StepDefinition def,boolean next)throws Exception {
  // StepDefinition stores compilation errors; calling iteration on its absent
  // traversal would replace the useful diagnostic with NullPointerException.
  // Leave the error available to the original upstream assertions.
  if(field(def,"traversal")==null&&field(def,"error")!=null)return;
  if(next)def.iteratedNext();else def.iteratedToList();
 }
 static void step(StepDefinition def,JsonNode s)throws Exception{
  String t=s.get("text").asText(),doc=s.has("doc")?s.get("doc").asText():"";Matcher m;
  if((m=Pattern.compile("the (\\w+) graph").matcher(t)).matches())def.givenTheXGraph(m.group(1));
  else if(t.equals("the graph initializer of")){if(jvmProfile())jvmTraversal(def,doc).iterate();else def.theGraphInitializerOf(doc);}
  else if((m=Pattern.compile("using the parameter (\\w+) defined as (.+)").matcher(t)).matches()){if(jvmProfile()){String raw=unquote(m.group(2));context(def).typedParameters.put(m.group(1),typedParameter(def,raw));context(def).parameterDefinitions.put(m.group(1),raw);}else def.usingTheParameterXDefinedAsX(m.group(1),unquote(m.group(2)));}
  else if((m=Pattern.compile("using the parameter (\\w+) of P\\.(\\w+)\\((.+)\\)").matcher(t)).matches()){if(jvmProfile()){
   String raw=unquote(m.group(3));context(def).parameterDefinitions.put(m.group(1),raw);context(def).predicateDefinitions.put(m.group(1),m.group(2));
   Bindings values=bindings(def,false);values.put("__value",typedParameter(def,raw));
   context(def).typedParameters.put(m.group(1),scriptEngine(def).eval("P."+m.group(2)+"(__value)",values));
  }else def.usingTheParameterXOfPX(m.group(1),m.group(2),unquote(m.group(3)));}
  else if(t.equals("the traversal of"))defineTraversal(def,doc);
  else if(t.equals("iterated to list"))iterate(def,false);else if(t.equals("iterated next"))iterate(def,true);
  else if(t.startsWith("the result should be ")&&s.has("table")){
   List<List<String>> table=new ArrayList<>();for(var row:s.get("table")){List<String> r=new ArrayList<>();for(var c:row)r.add(c.asText());table.add(r);}var dt=DataTable.create(table);
   switch(t){case "the result should be unordered"->def.theResultShouldBeUnordered(dt);case "the result should be ordered"->def.theResultShouldBeOrdered(dt);case "the result should be of"->def.theResultShouldBeOf(dt);default->throw new IllegalArgumentException("Unmapped step: "+t);}
  }else if((m=Pattern.compile("the result should have a count of (\\d+)").matcher(t)).matches())def.theResultShouldHaveACountOf(Integer.parseInt(m.group(1)));
  else if((m=Pattern.compile("(?:debug )?the graph should return (\\d+) for count of (.+)").matcher(t)).matches()){String script=unquote(m.group(2));if(jvmProfile())prepareAssertionParameters(def,script);def.theGraphShouldReturnForCountOf(Integer.parseInt(m.group(1)),script);}
  else if(t.equals("the result should be empty"))def.theResultShouldBeEmpty();
  else if(t.equals("the traversal will raise an error"))def.theTraversalWillRaiseAnError();
  else if((m=Pattern.compile("the traversal will raise an error with message (\\w+) text of (.+)").matcher(t)).matches())def.theTraversalWillRaiseAnErrorWithMessage(m.group(1),unquote(m.group(2)));
  else if(t.equals("an unsupported test"))def.anUnsupportedTest();else if(t.equals("nothing should happen because"))def.nothingShouldHappenBecause(doc);
  else throw new IllegalArgumentException("Unmapped upstream step: "+t);
 }
 public static void main(String[]args)throws Exception{
  backend=args[0];if(!backend.equals("sqlg")&&!backend.equals("reference")&&!jvmProfile())bridge=new Bridge();
  var input=new BufferedReader(new InputStreamReader(System.in));String line;
  System.out.println("{\"ready\":true}");System.out.flush();
  while((line=input.readLine())!=null){var request=json.readTree(line);var context=new Context();var def=new StepDefinition(context);String status="pass",error="",failedStep="";long start=System.nanoTime();List<Object> timings=new ArrayList<>();
   try{for(var tag:request.get("tags"))if(tag.asText().equals("@AllowNullPropertyValues"))context.allowNullPropertyValues=true;
   for(var tag:request.get("tags")){String t=tag.asText();if(t.equals("@GraphComputerOnly")&&!backend.equals("crabgraph-computer")||t.equals("@AllowNullPropertyValues")&&!jvmProfile()&&!backend.equals("crabgraph")||t.equals("@DisallowNullPropertyValues")&&jvmProfile()||backend.equals("reference")&&t.equals("@RemoteOnly"))throw new AssumptionViolatedException("Upstream execution profile excludes "+t);}
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
