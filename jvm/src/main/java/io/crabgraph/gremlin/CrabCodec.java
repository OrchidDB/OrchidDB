package io.crabgraph.gremlin;

import java.math.BigDecimal;
import java.math.BigInteger;
import java.util.*;

/** Lossless value protocol. Element records carry native handles, never detached graph copies. */
public final class CrabCodec {
    private CrabCodec() {}
    public static Map<String,Object> fields(Object... values) {
        Map<String,Object> result = new LinkedHashMap<>();
        for (int i=0;i<values.length;i+=2) result.put((String)values[i],values[i+1]);
        return result;
    }
    public static Object encode(Object value) { return encode(value,null); }
    public static Object encode(Object value,CrabGraph graph) {
        if(graph!=null) { Object runtime=graph.runtimeEncode(value); if(runtime!=null) return runtime; }
        if (value == null) return fields("type","null");
        if (value instanceof CrabElement && (graph==null || ((CrabElement)value).graph==graph)) return ((CrabElement)value).record;
        if(value instanceof org.apache.tinkerpop.gremlin.structure.Vertex) return fields("type","vertex_ref","id",encode(((org.apache.tinkerpop.gremlin.structure.Vertex)value).id()));
        if(value instanceof org.apache.tinkerpop.gremlin.structure.Edge) return fields("type","edge_ref","id",encode(((org.apache.tinkerpop.gremlin.structure.Edge)value).id()));
        if(value instanceof org.apache.tinkerpop.gremlin.structure.VertexProperty) {
            org.apache.tinkerpop.gremlin.structure.VertexProperty<?> property=(org.apache.tinkerpop.gremlin.structure.VertexProperty<?>)value;
            return fields("type","vertex_property_ref","owner",fields("type","vertex_ref","id",encode(property.element().id())),"id",encode(property.id()));
        }
        if (value instanceof CrabProperty) return ((CrabProperty<?>)value).record;
        String type;
        if (value instanceof String) type="string";
        else if (value instanceof Boolean) type="boolean";
        else if (value instanceof Byte) type="byte";
        else if (value instanceof Short) type="short";
        else if (value instanceof Integer) type="int";
        else if (value instanceof Long) type="long";
        else if (value instanceof Float) type="float";
        else if (value instanceof Double) type="double";
        else if (value instanceof BigInteger) { type="bigint"; value=value.toString(); }
        else if (value instanceof BigDecimal) { type="bigdecimal"; value=value.toString(); }
        else if (value instanceof Map) {
            List<Object> entries=new ArrayList<>();
            ((Map<?,?>)value).forEach((k,v)->entries.add(Arrays.asList(encode(k,graph),encode(v,graph))));
            return fields("type","map","value",entries);
        } else if (value instanceof Collection) {
            List<Object> entries=new ArrayList<>();
            for (Object item:(Collection<?>)value) entries.add(encode(item,graph));
            return fields("type",value instanceof Set?"set":"list","value",entries);
        } else throw new IllegalArgumentException("Unsupported native property type: "+value.getClass().getName());
        if ((value instanceof Double && !Double.isFinite((Double)value)) || (value instanceof Float && !Float.isFinite((Float)value))) value=value.toString();
        return fields("type",type,"value",value);
    }
    @SuppressWarnings("unchecked")
    public static Object decode(Object raw, CrabGraph graph) {
        if (!(raw instanceof Map)) throw new IllegalArgumentException("Expected typed native value: "+raw);
        Map<String,Object> record=(Map<String,Object>)raw;
        String type=(String)record.get("type"); Object value=record.get("value");
        switch(type) {
            case "null": return null;
            case "boolean": case "string": return value;
            case "byte": return Byte.valueOf(value.toString());
            case "short": return Short.valueOf(value.toString());
            case "int": return Integer.valueOf(value.toString());
            case "long": return Long.valueOf(value.toString());
            case "float": return Float.valueOf(value.toString());
            case "double": return Double.valueOf(value.toString());
            case "bigint": return new BigInteger(value.toString());
            case "bigdecimal": return new BigDecimal(value.toString());
            case "jvm_runtime": if(graph==null) throw new IllegalArgumentException("Runtime value requires a graph session"); return graph.runtimeDecode(record);
            case "vertex": return new CrabVertex(graph,record);
            case "edge": return new CrabEdge(graph,record);
            case "vertex_property": return new CrabVertexProperty<>(graph,record);
            case "property": return new CrabProperty<>(graph,record);
            case "list": case "set": {
                Collection<Object> result=type.equals("set")?new LinkedHashSet<>():new ArrayList<>();
                for(Object item:(List<?>)value) result.add(decode(item,graph));
                return result;
            }
            case "map": {
                Map<Object,Object> result=new LinkedHashMap<>();
                for(Object item:(List<?>)value) { List<?> pair=(List<?>)item; result.put(decode(pair.get(0),graph),decode(pair.get(1),graph)); }
                return result;
            }
            default: throw new IllegalArgumentException("Unknown native type: "+type);
        }
    }
}
