package io.crabgraph.conformance;

import java.util.*;
import org.apache.commons.configuration2.Configuration;
import org.apache.tinkerpop.gremlin.AbstractGraphProvider;
import org.apache.tinkerpop.gremlin.LoadGraphWith;
import org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.GraphTraversalSource;
import org.apache.tinkerpop.gremlin.structure.Graph;

/** All fixture loading and tests operate on the production native-backed Graph. */
public class NativeGraphProvider extends AbstractGraphProvider {
    private final boolean computer;
    public NativeGraphProvider() { this(false); }
    public NativeGraphProvider(boolean computer) { this.computer = computer; }
    @Override public Map<String,Object> getBaseConfiguration(String name, Class<?> test, String method, LoadGraphWith.GraphData data) {
        return Map.of(Graph.GRAPH, "io.crabgraph.gremlin.CrabGraph");
    }
    @Override public Graph openTestGraph(Configuration configuration) {
        try {
            return (Graph) Class.forName("io.crabgraph.gremlin.CrabGraph").getMethod("open").invoke(null);
        } catch (ReflectiveOperationException e) { throw new IllegalStateException("Cannot open production CrabGraph", e); }
    }
    @Override public GraphTraversalSource traversal(Graph graph) {
        return computer ? graph.traversal().withComputer() : graph.traversal();
    }
    @Override public void clear(Graph graph, Configuration configuration) throws Exception {
        if (graph != null) graph.close();
    }
    @Override public Set<Class> getImplementations() {
        Set<Class> result = new HashSet<>(CORE_IMPLEMENTATIONS);
        try {
            Class<?> graph = Class.forName("io.crabgraph.gremlin.CrabGraph");
            result.add(graph);
            result.addAll(Arrays.asList(graph.getDeclaredClasses()));
        } catch (ClassNotFoundException e) { throw new IllegalStateException(e); }
        return result;
    }
}
