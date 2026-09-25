package io.orchiddb.gremlin;

import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.structure.util.ElementHelper;
import org.apache.tinkerpop.gremlin.structure.util.StringFactory;
import java.util.*;

abstract class OrchidElement implements Element {
    final OrchidGraph graph;
    final Map<String,Object> record;
    OrchidElement(OrchidGraph graph,Map<String,Object> record) { this.graph=graph; this.record=record; }
    Object handle() { return record.get("handle"); }
    @Override public Object id() { return graph.elementId(record); }
    @Override public String label() { return (String)record.get("label"); }
    @Override public Graph graph() { return graph; }
    @Override public void remove() { graph.request("remove","owner",handle()); }
    @Override public <V> Property<V> property(String key,V value) {
        ElementHelper.validateProperty(key,value);
        return graph.decode(graph.request("setProperty","owner",handle(),"key",key,"value",graph.encodeValue(value)));
    }
    @Override public <V> Iterator<? extends Property<V>> properties(String... keys) {
        return propertyRecords(keys);
    }
    <P> Iterator<P> propertyRecords(String... keys) {
        List<String> selected=new ArrayList<>();
        if(keys!=null) for(String key:keys) if(key!=null) selected.add(key);
        if(keys!=null && keys.length>0 && selected.isEmpty()) return Collections.emptyIterator();
        return graph.records(graph.request("properties","owner",handle(),"keys",selected));
    }
    @Override public boolean equals(Object other) { return ElementHelper.areEqual(this,other); }
    @Override public int hashCode() { return ElementHelper.hashCode(this); }
}

final class OrchidVertex extends OrchidElement implements Vertex {
    OrchidVertex(OrchidGraph graph,Map<String,Object> record) { super(graph,record); }
    @Override public Edge addEdge(String label,Vertex inVertex,Object... keyValues) {
        ElementHelper.validateLabel(label);
        if(!(inVertex instanceof OrchidVertex)||inVertex.graph()!=graph) throw new IllegalArgumentException("Edge endpoint belongs to a different native session");
        Map<String,Object> args=OrchidCodec.fields("out",handle(),"in",((OrchidVertex)inVertex).handle(),"label",label,"properties",graph.properties(keyValues));
        ElementHelper.getIdValue(keyValues).ifPresent(id->args.put("id",graph.encodeId(id,"edge")));
        List<Object> pairs=new ArrayList<>(); args.forEach((k,v)->{pairs.add(k);pairs.add(v);});
        return graph.decode(graph.request("addEdge",pairs.toArray()));
    }
    @Override public <V> VertexProperty<V> property(String key) {
        Iterator<VertexProperty<V>> iterator=properties(key);
        if(!iterator.hasNext()) return VertexProperty.empty();
        VertexProperty<V> first=iterator.next();
        if(iterator.hasNext()) throw Vertex.Exceptions.multiplePropertiesExistForProvidedKey(key);
        return first;
    }
    @Override public <V> VertexProperty<V> property(String key,V value) { return property(graph.features().vertex().getCardinality(key),key,value); }
    @Override public <V> VertexProperty<V> property(VertexProperty.Cardinality cardinality,String key,V value,Object... keyValues) {
        ElementHelper.validateProperty(key,value);
        Map<String,Object> args=OrchidCodec.fields("owner",handle(),"key",key,"value",graph.encodeValue(value),"cardinality",cardinality.name(),"meta",graph.properties(keyValues));
        ElementHelper.getIdValue(keyValues).ifPresent(id->args.put("id",graph.encodeId(id,"vertex_property")));
        List<Object> pairs=new ArrayList<>(); args.forEach((k,v)->{pairs.add(k);pairs.add(v);});
        return graph.decode(graph.request("setVertexProperty",pairs.toArray()));
    }
    @Override public <V> Iterator<VertexProperty<V>> properties(String... keys) {
        return propertyRecords(keys);
    }
    @Override public Iterator<Edge> edges(Direction direction,String... labels) {
        return graph.records(graph.request("adjacent","vertex",handle(),"direction",direction.name(),"labels",Arrays.asList(labels)));
    }
    @Override public Iterator<Vertex> vertices(Direction direction,String... labels) {
        List<Vertex> result=new ArrayList<>();
        // Separate directions preserve two occurrences of a self-loop in BOTH.
        if(direction==Direction.OUT||direction==Direction.BOTH) edges(Direction.OUT,labels).forEachRemaining(e->result.add(e.inVertex()));
        if(direction==Direction.IN||direction==Direction.BOTH) edges(Direction.IN,labels).forEachRemaining(e->result.add(e.outVertex()));
        return result.iterator();
    }
    @Override public String toString() { return StringFactory.vertexString(this); }
}

final class OrchidEdge extends OrchidElement implements Edge {
    OrchidEdge(OrchidGraph graph,Map<String,Object> record) { super(graph,record); }
    @Override public Iterator<Vertex> vertices(Direction direction) {
        List<Vertex> result=new ArrayList<>();
        if(direction==Direction.OUT||direction==Direction.BOTH) result.add(graph.decode(record.get("out")));
        if(direction==Direction.IN||direction==Direction.BOTH) result.add(graph.decode(record.get("in")));
        return result.iterator();
    }
    @Override public <V> Iterator<Property<V>> properties(String... keys) {
        return propertyRecords(keys);
    }
    @Override public String toString() { return StringFactory.edgeString(this); }
}

final class OrchidVertexProperty<V> extends OrchidElement implements VertexProperty<V> {
    OrchidVertexProperty(OrchidGraph graph,Map<String,Object> record) { super(graph,record); }
    @Override public String key() { return (String)record.get("key"); }
    @Override public String label() { return key(); }
    @Override public V value() { return graph.decode(record.get("value")); }
    @Override public boolean isPresent() { return true; }
    @Override public Vertex element() { return graph.decode(record.get("owner")); }
    @Override public void remove() {
        Iterator<VertexProperty<Object>> current=element().properties(key());
        while(current.hasNext()) {
            VertexProperty<?> property=current.next();
            if(property instanceof OrchidElement&&Objects.equals(handle(),((OrchidElement)property).handle())) {
                super.remove(); return;
            }
        }
    }
    @Override public <U> Iterator<Property<U>> properties(String... keys) {
        return propertyRecords(keys);
    }
    @Override public String toString() { return StringFactory.propertyString(this); }
}

final class OrchidProperty<V> implements Property<V> {
    final OrchidGraph graph;
    final Map<String,Object> record;
    OrchidProperty(OrchidGraph graph,Map<String,Object> record) { this.graph=graph;this.record=record; }
    @Override public String key() { return (String)record.get("key"); }
    @Override public V value() { return graph.decode(record.get("value")); }
    @Override public boolean isPresent() { return true; }
    @Override public Element element() { return graph.decode(record.get("owner")); }
    @Override public void remove() {
        Iterator<? extends Property<Object>> current=element().properties(key());
        while(current.hasNext()) {
            Property<?> property=current.next();
            if(property instanceof OrchidProperty&&Objects.equals(record.get("handle"),((OrchidProperty<?>)property).record.get("handle"))) {
                graph.request("remove","owner",record.get("handle")); return;
            }
        }
    }
    @Override public boolean equals(Object other) { return ElementHelper.areEqual(this,other); }
    @Override public int hashCode() { return ElementHelper.hashCode(this); }
    @Override public String toString() { return StringFactory.propertyString(this); }
}
