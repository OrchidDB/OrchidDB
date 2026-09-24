package io.crabgraph.conformance;

import java.util.*;
import java.nio.file.*;
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
        Map<String,Object> settings=new HashMap<>();
        settings.put(Graph.GRAPH,"io.crabgraph.gremlin.CrabGraph");
        settings.put("crabgraph.native.path",Path.of(makeTestDirectory(name,test,method),"graph.snapshot").toAbsolutePath().toString());
        // Same fixture/schema requirement as the pinned TinkerGraph provider:
        // GryoReader delegates property cardinality to the target graph.
        if (data == LoadGraphWith.GraphData.CREW
            || (test.getSimpleName().equals("StarGraphTest") && method.equals("shouldAttachWithCreateMethod"))
            || (test.getSimpleName().equals("DetachedGraphTest") && method.equals("testAttachableCreateMethod")))
            settings.put("crabgraph.vertex.defaultCardinality","list");
        String idManager=idManager(test,method,data);
        for(String kind:List.of("vertex","edge","vertexProperty"))
            settings.put("crabgraph."+kind+".idManager",idManager);
        return settings;
    }
    // These are provider configuration requirements from pinned TinkerGraphProvider,
    // not modified queries or expected results. Source SHA-256: dcb3eef57e2a6076c0e4f4cff0fa8eb160509831ad4114e0866a75702139ab3e
    private static String idManager(Class<?> test, String method, LoadGraphWith.GraphData data) {
        if (data != null) return "INTEGER";
        if (test.getSimpleName().equals("GraphTest") && Set.of(
            "shouldIterateVerticesWithNumericIdSupportUsingDoubleRepresentation",
            "shouldIterateVerticesWithNumericIdSupportUsingDoubleRepresentations",
            "shouldIterateVerticesWithNumericIdSupportUsingIntegerRepresentation",
            "shouldIterateVerticesWithNumericIdSupportUsingIntegerRepresentations",
            "shouldIterateVerticesWithNumericIdSupportUsingFloatRepresentation",
            "shouldIterateVerticesWithNumericIdSupportUsingFloatRepresentations",
            "shouldIterateVerticesWithNumericIdSupportUsingStringRepresentation",
            "shouldIterateVerticesWithNumericIdSupportUsingStringRepresentations",
            "shouldIterateEdgesWithNumericIdSupportUsingDoubleRepresentation",
            "shouldIterateEdgesWithNumericIdSupportUsingDoubleRepresentations",
            "shouldIterateEdgesWithNumericIdSupportUsingIntegerRepresentation",
            "shouldIterateEdgesWithNumericIdSupportUsingIntegerRepresentations",
            "shouldIterateEdgesWithNumericIdSupportUsingFloatRepresentation",
            "shouldIterateEdgesWithNumericIdSupportUsingFloatRepresentations",
            "shouldIterateEdgesWithNumericIdSupportUsingStringRepresentation",
            "shouldIterateEdgesWithNumericIdSupportUsingStringRepresentations").contains(method)) return "LONG";
        if (test.getSimpleName().equals("IoEdgeTest") && Set.of(
            "shouldReadWriteEdge[graphson-v1]",
            "shouldReadWriteDetachedEdgeAsReference[graphson-v1]",
            "shouldReadWriteDetachedEdge[graphson-v1]",
            "shouldReadWriteEdge[graphson-v2]",
            "shouldReadWriteDetachedEdgeAsReference[graphson-v2]",
            "shouldReadWriteDetachedEdge[graphson-v2]").contains(method)) return "LONG";
        if (test.getSimpleName().equals("IoVertexTest") && Set.of(
            "shouldReadWriteVertexWithBOTHEdges[graphson-v1]",
            "shouldReadWriteVertexWithINEdges[graphson-v1]",
            "shouldReadWriteVertexWithOUTEdges[graphson-v1]",
            "shouldReadWriteVertexNoEdges[graphson-v1]",
            "shouldReadWriteDetachedVertexNoEdges[graphson-v1]",
            "shouldReadWriteDetachedVertexAsReferenceNoEdges[graphson-v1]",
            "shouldReadWriteVertexMultiPropsNoEdges[graphson-v1]",
            "shouldReadWriteVertexWithBOTHEdges[graphson-v2]",
            "shouldReadWriteVertexWithINEdges[graphson-v2]",
            "shouldReadWriteVertexWithOUTEdges[graphson-v2]",
            "shouldReadWriteVertexNoEdges[graphson-v2]",
            "shouldReadWriteDetachedVertexNoEdges[graphson-v2]",
            "shouldReadWriteDetachedVertexAsReferenceNoEdges[graphson-v2]",
            "shouldReadWriteVertexMultiPropsNoEdges[graphson-v2]").contains(method)) return "LONG";
        return "ANY";
    }
    @Override public Graph openTestGraph(Configuration configuration) {
        try {
            if (configuration == null)
                return (Graph) Class.forName("io.crabgraph.gremlin.CrabGraph").getMethod("open").invoke(null);
            Path path=Path.of(configuration.getString("crabgraph.native.path"));
            Files.createDirectories(path.getParent());
            return (Graph) Class.forName("io.crabgraph.gremlin.CrabGraph").getMethod("open",Configuration.class).invoke(null,configuration);
        } catch (ReflectiveOperationException | java.io.IOException e) { throw new IllegalStateException("Cannot open production CrabGraph", e); }
    }
    @Override public void loadGraphData(Graph graph, LoadGraphWith data, Class test, String method) {
        if (data == null) return;
        Runnable load=()->super.loadGraphData(graph,data,test,method);
        try { graph.getClass().getMethod("bulkLoad",Runnable.class).invoke(graph,load); }
        catch (java.lang.reflect.InvocationTargetException e) {
            if(e.getCause() instanceof RuntimeException failure) throw failure;
            if(e.getCause() instanceof Error failure) throw failure;
            throw new IllegalStateException("Native fixture load failed",e.getCause());
        } catch (ReflectiveOperationException e) { throw new IllegalStateException("Production bulkLoad API is required",e); }
    }
    @Override public GraphTraversalSource traversal(Graph graph) {
        return computer ? graph.traversal().withComputer() : graph.traversal();
    }
    @Override public void clear(Graph graph, Configuration configuration) throws Exception {
        if (graph != null) {
            try { graph.getClass().getMethod("closeFamily").invoke(graph); }
            catch (NoSuchMethodException absent) { graph.close(); }
        }
        if (configuration != null && configuration.containsKey("crabgraph.native.path"))
            Files.deleteIfExists(Path.of(configuration.getString("crabgraph.native.path")));
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
