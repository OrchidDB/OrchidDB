package io.crabgraph.gremlin;

import java.util.*;
import java.util.regex.Pattern;
import java.util.stream.LongStream;
import org.apache.tinkerpop.gremlin.process.traversal.Traverser;
import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.structure.service.Service;
import org.apache.tinkerpop.gremlin.structure.service.ServiceRegistry;
import org.apache.tinkerpop.gremlin.structure.util.CloseableIterator;

/** Compatible service names with general implementations reading the native provider graph. */
final class CrabServices {
    private CrabServices() { }
    static ServiceRegistry create(Graph graph) {
        ServiceRegistry registry=new ServiceRegistry();
        registry.registerService(new Search(graph));
        registry.registerService(new Degree());
        return registry;
    }
    private abstract static class Factory<I,R> implements Service.ServiceFactory<I,R>, Service<I,R> {
        @Override public Set<Type> getSupportedTypes() { return Collections.singleton(getType()); }
        @Override public Service<I,R> createService(boolean start,Map params) {
            if (start!=(getType()==Type.Start)) throw new UnsupportedOperationException(
                    start?Service.Exceptions.cannotStartTraversal:Service.Exceptions.cannotUseMidTraversal);
            return this;
        }
        @Override public void close() { }
    }
    private static final class Search extends Factory<Object,Property<?>> {
        private final Graph graph;
        Search(Graph graph) { this.graph=graph; }
        @Override public String getName() { return "tinker.search"; }
        @Override public Type getType() { return Type.Start; }
        @Override public Map describeParams() {
            Map<String,String> descriptions=new LinkedHashMap<>();
            descriptions.put("search","Specify a search term - will be converted to regex via .*(search).*");
            descriptions.put("regex","Directly specify the regex");
            descriptions.put("type","Specify the type of Element to search for, one of Vertex/Edge/VertexProperty (optional)");
            return descriptions;
        }
        @Override public CloseableIterator<Property<?>> execute(ServiceCallContext context,Map parameters) {
            String expression;
            if(parameters.containsKey("regex")) expression=(String)parameters.get("regex");
            else if(parameters.containsKey("search")) expression=".*("+parameters.get("search")+").*";
            else throw new IllegalStateException("Missing search/regex parameter");
            String type=(String)parameters.get("type");
            if(type!=null&&!Set.of("Vertex","Edge","VertexProperty").contains(type))
                throw new IllegalArgumentException("Type must be one of Vertex/Edge/VertexProperty: "+type);
            Pattern pattern=Pattern.compile(expression);
            List<Property<?>> matches=new ArrayList<>();
            if(type==null||type.equals("Vertex")||type.equals("VertexProperty")) {
                graph.vertices().forEachRemaining(vertex -> vertex.properties().forEachRemaining(property -> {
                    if(type==null||type.equals("Vertex")) match(matches,pattern,property);
                    if(type==null||type.equals("VertexProperty"))
                        property.properties().forEachRemaining(meta -> match(matches,pattern,meta));
                }));
            }
            if(type==null||type.equals("Edge")) graph.edges().forEachRemaining(edge ->
                    edge.properties().forEachRemaining(property -> match(matches,pattern,property)));
            return CloseableIterator.of(matches.iterator());
        }
        private static void match(List<Property<?>> results,Pattern pattern,Property<?> property) {
            Object value=property.value();
            if(value!=null&&pattern.matcher(value.toString()).matches()) results.add(property);
        }
    }
    private static final class Degree extends Factory<Vertex,Long> {
        @Override public String getName() { return "tinker.degree.centrality"; }
        @Override public Type getType() { return Type.Streaming; }
        @Override public Map describeParams() {
            return Collections.singletonMap("direction","Specify the edge direction (optional), default is Direction.IN");
        }
        @Override public CloseableIterator<Long> execute(ServiceCallContext context,Traverser.Admin<Vertex> traverser,Map parameters) {
            Direction direction=(Direction)parameters.getOrDefault("direction",Direction.IN);
            long count=0;
            try(CloseableIterator<Edge> edges=CloseableIterator.of(traverser.get().edges(direction))) {
                while(edges.hasNext()) { edges.next(); count++; }
            }
            final long degree=count;
            return CloseableIterator.of(LongStream.range(0,traverser.bulk()).map(i->degree).iterator());
        }
    }
}
