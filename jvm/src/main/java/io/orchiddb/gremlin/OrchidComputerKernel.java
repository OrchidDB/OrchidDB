package io.orchiddb.gremlin;

import java.util.*;
import javax.script.SimpleBindings;
import org.apache.tinkerpop.gremlin.groovy.jsr223.GremlinGroovyScriptEngine;
import org.apache.tinkerpop.gremlin.process.traversal.Path;
import org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.GraphTraversal;
import org.apache.tinkerpop.gremlin.process.traversal.step.util.MutablePath;
import org.apache.tinkerpop.gremlin.structure.*;

/** A single vertex-program operator over the caller's staged native graph. */
final class OrchidComputerKernel {
    private OrchidComputerKernel() { }
    @SuppressWarnings({"unchecked","rawtypes"})
    static List<Object> execute(Map<String,Object> request, OrchidGraph graph, GremlinGroovyScriptEngine engine) throws Exception {
        List<Map<String,Object>> rows=(List<Map<String,Object>>)request.get("rows");
        List<Map<String,Object>> frames=(List<Map<String,Object>>)request.get("traversers");
        List<Vertex> input=new ArrayList<>();Map<Object,Long> counts=new LinkedHashMap<>();
        for(int i=0;i<rows.size();i++) {
            Object current=OrchidCodec.decode(rows.get(i).get("current"),graph);
            if(!(current instanceof Vertex vertex))throw new IllegalArgumentException("GraphComputer operator requires vertex traversers");
            input.add(vertex);counts.merge(vertex.id(),((Number)frames.get(i).get("bulk")).longValue(),Math::addExact);
        }
        GraphTraversal<Vertex,Vertex> seed=graph.traversal().withComputer(org.apache.tinkerpop.gremlin.process.computer.Computer.compute()
            .persist(org.apache.tinkerpop.gremlin.process.computer.GraphComputer.Persist.EDGES)
            .result(org.apache.tinkerpop.gremlin.process.computer.GraphComputer.ResultGraph.NEW)).V();
        if(Boolean.TRUE.equals(OrchidCodec.decode(rows.get(0).get("__orchiddb_traverser_priors"),graph)))
            seed=seed.filter(t->counts.containsKey(t.get().id()))
                .sideEffect(t->t.asAdmin().setBulk(counts.get(t.get().id())));
        SimpleBindings bindings=new SimpleBindings();
        if(!rows.isEmpty())rows.get(0).forEach((key,value)->bindings.put(key,OrchidCodec.decode(value,graph)));
        bindings.put("traversal",seed);
        Object configured=engine.eval((String)request.get("script"),bindings);
        if(!(configured instanceof GraphTraversal<?,?> traversal))throw new IllegalArgumentException("GraphComputer configuration did not return an operator");
        Map<Object,LinkedHashSet<Object>> results=new LinkedHashMap<>();Graph computed=null;
        var admin=traversal.asAdmin();
        admin.applyStrategies();
        if(!(admin.getEndStep() instanceof org.apache.tinkerpop.gremlin.process.computer.traversal.step.map.ComputerResultStep))
            throw new IllegalStateException("GraphComputer operator must end at a ComputerResult boundary");
        // Consume the public ComputerResult directly. The convenience result
        // step detaches elements and discards the graph view needed by the next
        // relational operator; it is not part of the vertex program itself.
        var computerResults=admin.getEndStep().getPreviousStep();
        try(traversal) {
            while(computerResults.hasNext()) {
                var result=(org.apache.tinkerpop.gremlin.process.computer.ComputerResult)computerResults.next().get();
                computed=result.graph();
                String halted=org.apache.tinkerpop.gremlin.process.computer.traversal.TraversalVertexProgram.HALTED_TRAVERSERS;
                if(!result.memory().exists(halted))continue;
                Iterable<org.apache.tinkerpop.gremlin.process.traversal.Traverser.Admin<?>> traversers=result.memory().get(halted);
                for(var traverser:traversers) {
                    Object value=traverser.get();Vertex source;
                    if(value instanceof Vertex v) source=v;
                    else if(value instanceof Path p && p.size()>0 && p.get(0) instanceof Vertex v) source=v;
                    else throw new IllegalArgumentException("Unexpected GraphComputer operator result: "+value);
                    results.computeIfAbsent(source.id(),key->new LinkedHashSet<>()).add(rebind(value,graph));
                }
            }
        }
        // Expose computed properties to subsequent relational operators on the
        // query-local overlay. The Rust executor never publishes this view.
        if(computed!=null) {
            Iterator<Vertex> vertices=computed.vertices();
            while(vertices.hasNext()) {
                Vertex vertex=vertices.next();Vertex target=graph.vertices(vertex.id()).next();
                for(String key:vertex.keys()) {
                    if(Graph.Hidden.isHidden(key))continue;
                    List<Object> after=values(vertex,key), before=values(target,key);
                    if(!after.equals(before)) {
                        List<VertexProperty<Object>> old=new ArrayList<>();target.properties(key).forEachRemaining(old::add);old.forEach(VertexProperty::remove);
                        for(Object value:after)target.property(VertexProperty.Cardinality.list,key,value);
                    }
                }
            }
        }
        List<Object> output=new ArrayList<>();
        for(Vertex vertex:input) {
            List<Object> group=new ArrayList<>();
            for(Object value:results.getOrDefault(vertex.id(),new LinkedHashSet<>()))group.add(OrchidCodec.encode(value,graph));
            output.add(group);
        }
        return output;
    }
    private static List<Object> values(Vertex vertex,String key) {
        List<Object> values=new ArrayList<>();vertex.properties(key).forEachRemaining(p->values.add(p.value()));return values;
    }
    private static Object rebind(Object value,OrchidGraph graph) {
        if(value instanceof Vertex v)return graph.vertices(v.id()).next();
        if(value instanceof Edge e)return graph.edges(e.id()).next();
        if(value instanceof Path p) {
            Path result=MutablePath.make();
            for(int i=0;i<p.size();i++)result.extend(rebind(p.get(i),graph),p.labels().get(i));
            return result;
        }
        throw new IllegalArgumentException("GraphComputer result must preserve native element identity");
    }
}
