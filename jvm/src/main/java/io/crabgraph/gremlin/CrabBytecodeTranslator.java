package io.crabgraph.gremlin;
import java.util.*;
import org.apache.tinkerpop.gremlin.process.traversal.Bytecode;
import org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.GraphTraversalSource;
import org.apache.tinkerpop.gremlin.process.traversal.translator.GroovyTranslator;
import org.apache.tinkerpop.gremlin.structure.VertexProperty;

/** Lossless bytecode-to-native request translation; callbacks remain explicit typed bindings. */
public final class CrabBytecodeTranslator extends GroovyTranslator.LanguageTypeTranslator {
  public final Map<String,Object> bindings=new LinkedHashMap<>();
  private final CrabCallbackRegistry callbacks;
  public CrabBytecodeTranslator(CrabCallbackRegistry callbacks){super(false);this.callbacks=callbacks;}
  @Override protected org.apache.tinkerpop.gremlin.process.traversal.Script convertToScript(Object value) {
   if(!(value instanceof Enum<?>)&&(value instanceof java.util.function.Supplier<?>||value instanceof java.util.function.Function<?,?>||value instanceof java.util.function.BiFunction<?,?,?>)) {
    String name="__crabgraph_java_binding_"+bindings.size();
    bindings.put(name,Map.of("type","lambda","script",callbacks.register(value)));return script.append(name);
   }
   return super.convertToScript(value);
  }
  @Override protected org.apache.tinkerpop.gremlin.process.traversal.Script produceScript(VertexProperty<?> value) {
   String name="__crabgraph_java_binding_"+bindings.size();
   bindings.put(name,Map.of("type","vertex_property","id",CrabCodec.encode(value.id()),"owner",Map.of("type","vertex","id",CrabCodec.encode(value.element().id()),"label",value.element().label()),"key",value.key()));return script.append(name);
  }
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
    if(instruction.getOperator().equals("withSack")&&instruction.getArguments().length>0&&instruction.getArguments()[0] instanceof java.util.function.Supplier<?>) {
     Object[] args=instruction.getArguments();
     if(args.length>2||(args.length==2&&!(args[1] instanceof java.util.function.UnaryOperator<?>)))throw new IllegalArgumentException("Java sack configuration requires supplier and optional split operator");
     String name="__crabgraph_java_binding_"+bindings.size();
     Map<String,Object> value=new LinkedHashMap<>();value.put("type","sack_callbacks");value.put("supplier",callbacks.register(args[0]));
     if(args.length==2)value.put("split",callbacks.register(args[1]));
     bindings.put(name,value);script.append(name);
    } else if(instruction.getOperator().equals(GraphTraversalSource.Symbols.tx)&&instruction.getArguments().length>0) {
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
