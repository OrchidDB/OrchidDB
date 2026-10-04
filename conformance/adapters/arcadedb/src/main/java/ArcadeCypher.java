import com.arcadedb.database.*;
import com.arcadedb.graph.*;
import com.arcadedb.query.opencypher.Labels;
import com.arcadedb.query.opencypher.traversal.TraversalPath;
import com.arcadedb.query.sql.executor.Result;
import com.arcadedb.query.sql.executor.ResultInternal;
import com.arcadedb.query.sql.executor.CommandContext;
import com.arcadedb.query.opencypher.ast.*;
import com.arcadedb.query.opencypher.parser.Cypher25AntlrParser;
import com.arcadedb.query.opencypher.procedures.*;
import com.arcadedb.query.opencypher.temporal.CypherTemporalValue;
import com.fasterxml.jackson.databind.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;

/** JSONL transport only. Expectations and comparisons remain in the upstream TCK runner. */
public class ArcadeCypher {
  static final ObjectMapper JSON = new ObjectMapper();
  static Database database;
  static Path root;
  static final Set<String> fixtureProcedures=new HashSet<>();
  static Object normalize(Object value) {
    if (value == null || value instanceof String || value instanceof Boolean) return value;
    if (value instanceof Double d && !Double.isFinite(d))
      return Map.of("$float", d.isNaN() ? "NaN" : d > 0 ? "Infinity" : "-Infinity");
    if (value instanceof Float f && !Float.isFinite(f)) return normalize(f.doubleValue());
    if (value instanceof Number) return value;
    if (value instanceof CypherTemporalValue || value instanceof java.time.temporal.TemporalAccessor || value instanceof java.time.Duration || value instanceof java.time.Period) return temporalText(value);
    if (value instanceof RID rid) return normalize(rid.getRecord());
    if (value instanceof Vertex v)
      return Map.of("$node", Map.of("labels", Labels.getLabels(v).stream().sorted().toList(), "properties", properties(v)));
    if (value instanceof Edge e)
      return Map.of("$relationship", Map.of("type", e.getTypeName(), "properties", properties(e)));
    if (value instanceof TraversalPath path) {
      var segments = new ArrayList<>();
      for (int i=0; i<path.getEdges().size(); i++) {
        var edge=path.getEdges().get(i);
        segments.add(Map.of("direction", edge.getOut().equals(path.getVertices().get(i).getIdentity()) ? "forward" : "backward",
            "relationship", normalize(edge), "node", normalize(path.getVertices().get(i+1))));
      }
      return Map.of("$path", Map.of("start", normalize(path.getStartVertex()), "segments", segments));
    }
    if (value instanceof Result r) {
      if (r.isElement()) return normalize(r.getElement().orElseThrow());
      var map=new LinkedHashMap<String,Object>();
      for (var key:r.getPropertyNames()) map.put(key, normalize(r.getProperty(key)));
      return map;
    }
    if (value instanceof Map<?,?> map) {
      var result=new LinkedHashMap<String,Object>();
      for (var entry:map.entrySet()) {
        if (!(entry.getKey() instanceof String)) throw new IllegalArgumentException("Non-string Cypher map key");
        result.put((String)entry.getKey(), normalize(entry.getValue()));
      }
      return result;
    }
    if (value instanceof Iterable<?> list) {
      var result=new ArrayList<>();for (var item:list) result.add(normalize(item));return result;
    }
    if (value.getClass().isArray()) {
      var result=new ArrayList<>();for (int i=0;i<java.lang.reflect.Array.getLength(value);i++) result.add(normalize(java.lang.reflect.Array.get(value,i)));return result;
    }
    // Unknown values must never silently become strings and produce a false pass.
    throw new IllegalArgumentException("Unsupported ArcadeDB result type: "+value.getClass().getName());
  }
  static String temporalText(Object value) {
    String text=value.toString();
    // Java emits fractional seconds in groups of three; TCK notation strips trailing zeroes.
    var matcher=java.util.regex.Pattern.compile("\\.([0-9]+)").matcher(text);
    if(matcher.find()) {
      String fraction=matcher.group(1).replaceFirst("0+$","");
      text=text.substring(0,matcher.start())+(fraction.isEmpty()?"":"."+fraction)+text.substring(matcher.end());
    }
    return text;
  }
  static Map<String,Object> properties(Document document) {
    var result=new LinkedHashMap<String,Object>();
    for (var key:document.getPropertyNames()) if (document.get(key)!=null) result.put(key,normalize(document.get(key)));
    return result;
  }
  static void reset() throws Exception {
    for(var name:fixtureProcedures) CypherProcedureRegistry.unregister(name);
    fixtureProcedures.clear();
    if (database!=null) {database.drop();database=null;}
    database=new DatabaseFactory(root.resolve("graph").toString()).create();
  }
  static Object query(JsonNode request) {
    var columns=new ArrayList<String>();var rows=new ArrayList<List<Object>>();
    Map<String,Object> params=JSON.convertValue(request.path("params"),Map.class);
    try {
      database.begin();
      try (var results=database.command("cypher",request.path("query").asText(),params)) {
        while (results.hasNext()) {
          var row=results.next();
          if (columns.isEmpty()) columns.addAll(row.getPropertyNames());
          var values=new ArrayList<Object>();for (var column:columns) values.add(normalize(row.getProperty(column)));rows.add(values);
        }
      }
      database.commit();
      if(rows.isEmpty()) columns.addAll(outputColumns(new Cypher25AntlrParser().parse(request.path("query").asText())));
      return Map.of("columns",columns,"rows",rows);
    } catch (IllegalArgumentException e) {
      if (database.isTransactionActive()) database.rollback();
      if (e.getMessage()!=null && (e.getMessage().startsWith("Unsupported ArcadeDB result") || e.getMessage().startsWith("Non-string Cypher map"))) return Map.of("adapter_error",e.toString());
      return error(request,e);
    } catch (Exception e) {
      if (database.isTransactionActive()) database.rollback();
      return error(request,e);
    }
  }
  static Object error(JsonNode request, Exception error) {
    var diagnostics=new ArrayList<Map<String,String>>();
    for(Throwable cause=error;cause!=null;cause=cause.getCause()) diagnostics.add(Map.of("class",cause.getClass().getName(),"message",Objects.toString(cause.getMessage(),"")));
    String phase="runtime";
    try(var explain=database.command("cypher","EXPLAIN "+request.path("query").asText(),JSON.convertValue(request.path("params"),Map.class))) {
      while(explain.hasNext()) explain.next();
    } catch(Exception compilation) {phase="compile time";}
    if(database.isTransactionActive()) database.rollback();
    return Map.of("error",error.toString(),"exception_class",error.getClass().getName(),"diagnostics",diagnostics,"phase",phase);
  }
  static void declarePath(Set<String> scope, PathPattern path) {
    if(path.getPathVariable()!=null) scope.add(path.getPathVariable());
    for(var node:path.getNodes()) if(node.getVariable()!=null) scope.add(node.getVariable());
    for(var edge:path.getRelationships()) if(edge.getVariable()!=null) scope.add(edge.getVariable());
  }
  static List<String> outputColumns(CypherStatement statement) {
    if(statement instanceof UnionStatement union) return outputColumns(union.getFirstQuery());
    var scope=new LinkedHashSet<String>();
    for(var entry:statement.getClausesInOrder()) {
      switch(entry.getType()) {
        case MATCH -> {MatchClause clause=entry.getTypedClause();for(var path:clause.getPathPatterns()) declarePath(scope,path);}
        case CREATE -> {CreateClause clause=entry.getTypedClause();for(var path:clause.getPathPatterns()) declarePath(scope,path);}
        case MERGE -> {MergeClause clause=entry.getTypedClause();declarePath(scope,clause.getPathPattern());}
        case UNWIND -> {UnwindClause clause=entry.getTypedClause();scope.add(clause.getVariable());}
        case LOAD_CSV -> {LoadCSVClause clause=entry.getTypedClause();scope.add(clause.getVariable());}
        case WITH -> {
          WithClause clause=entry.getTypedClause();var next=new LinkedHashSet<String>();
          for(var item:clause.getItems()) {if(item.isStar()) next.addAll(scope);else next.add(item.getOutputName());}
          scope=next;
        }
        case CALL -> {
          CallClause clause=entry.getTypedClause();
          if(clause.hasYield()) {
            if(clause.isYieldAll()) {var procedure=CypherProcedureRegistry.get(clause.getProcedureName());if(procedure!=null) scope.addAll(procedure.getYieldFields());}
            else for(var item:clause.getYieldItems()) scope.add(item.getOutputName());
          } else {var procedure=CypherProcedureRegistry.get(clause.getProcedureName());if(procedure!=null) scope.addAll(procedure.getYieldFields());}
        }
        case SUBQUERY -> {SubqueryClause clause=entry.getTypedClause();scope.addAll(outputColumns(clause.getInnerStatement()));}
        case RETURN -> {
          ReturnClause clause=entry.getTypedClause();var result=new LinkedHashSet<String>();
          for(var item:clause.getReturnItems()) {if(item.isStar()) result.addAll(scope);else result.add(item.getOutputName());}
          return new ArrayList<>(result);
        }
        default -> {}
      }
    }
    return statement.getCallClauses().isEmpty()?List.of():new ArrayList<>(scope);
  }
  static Object registerProcedure(JsonNode request) {
    String name=request.path("name").asText();var inputs=request.path("inputs");var outputs=request.path("outputs");
    List<List<Object>> rows=JSON.convertValue(request.path("rows"),List.class);
    var fields=new ArrayList<String>();for(var output:outputs) fields.add(output.path("name").asText());
    CypherProcedureRegistry.registerOrReplace(new CypherProcedure() {
      public String getName(){return name;}
      public int getMinArgs(){return inputs.size();}
      public int getMaxArgs(){return inputs.size();}
      public String getDescription(){return "Original TCK GIVEN procedure table";}
      public List<String> getYieldFields(){return fields;}
      public java.util.stream.Stream<Result> execute(Object[] args, Result inputRow, CommandContext context) {
        return rows.stream().filter(row->{
          for(int i=0;i<args.length;i++) if(!Objects.deepEquals(args[i],row.get(i))) {
            if(!(args[i] instanceof Number a && row.get(i) instanceof Number b && new java.math.BigDecimal(a.toString()).compareTo(new java.math.BigDecimal(b.toString()))==0)) return false;
          }
          return true;
        }).map(row->{var result=new ResultInternal();for(int i=0;i<fields.size();i++) result.setProperty(fields.get(i),row.get(inputs.size()+i));return (Result)result;});
      }
    });
    fixtureProcedures.add(name);return Map.of("ok",true);
  }
  public static void main(String[] args) throws Exception {
    // Reserve stdout for protocol, including when the database prints startup information.
    var output=System.out;System.setOut(System.err);
    root=Path.of(args[0]);
    Runtime.getRuntime().addShutdownHook(new Thread(()->{
      try {if(database!=null)database.close();com.arcadedb.utility.FileUtils.deleteRecursively(root.toFile());}catch(Exception ignored){}
    }));
    output.println("{\"ready\":true}");output.flush();
    try (var input=new BufferedReader(new InputStreamReader(System.in))) {
      String line;while((line=input.readLine())!=null) {
        var request=JSON.readTree(line);Object response;
        if(request.path("op").asText().equals("reset")){reset();response=Map.of("ok",true);}
        else if(request.path("op").asText().equals("cypher")) response=query(request);
        else if(request.path("op").asText().equals("register-procedure")) response=registerProcedure(request);
        else response=Map.of("adapter_error","Unknown operation");
        output.println(JSON.writeValueAsString(response));output.flush();
      }
    }
  }
}
