import com.fasterxml.jackson.databind.JsonNode;
import java.io.PrintStream;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.*;
import org.apache.commons.configuration2.Configuration;
import org.apache.tinkerpop.gremlin.*;
import org.apache.tinkerpop.gremlin.process.traversal.TraversalEngine;
import org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.GraphTraversalSource;
import org.apache.tinkerpop.gremlin.structure.Graph;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.TinkerGraph;
import org.junit.AssumptionViolatedException;
import org.junit.runner.*;
import org.junit.runner.notification.*;

/** Original Java assertions for upstream Gherkin placeholders, using the suite's existing engine. */
final class NativeJUnitAssertions {
    static JsonNode mapping(String id) throws Exception {
        var mappings=UpstreamGremlin.json.readTree(Path.of("conformance/adapters/jvm-provider-tests/placeholders.json").toFile());
        for(var row:mappings.get("cases")) if(id.endsWith("/"+row.get("gherkin").asText())) return row;
        return null;
    }
    static void verify(Path path, String expected) throws Exception {
        String actual=HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(path)));
        if(!actual.equals(expected))throw new IllegalStateException("Pinned upstream source changed: "+path);
    }
    static void run(JsonNode mapping, UpstreamGremlin.Context context) throws Exception {
        Path upstream=Path.of(Objects.requireNonNull(System.getenv("CONFORMANCE_TINKERPOP_SOURCE"),"CONFORMANCE_TINKERPOP_SOURCE"));
        verify(upstream.resolve(mapping.get("source").asText()),mapping.get("source_sha256").asText());
        String feature=mapping.get("gherkin").asText().split(":")[0];
        verify(upstream.resolve("gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/"+feature),mapping.get("feature_source_sha256").asText());
        System.setProperty("build.dir",Path.of("target","conformance-junit").toAbsolutePath().toString());
        GraphManager.setTraversalEngineType(TraversalEngine.Type.STANDARD);
        GraphManager.setGraphProvider(new FixtureProvider(context));
        JUnitCore junit=new JUnitCore();
        List<Failure> assumptions=new ArrayList<>();
        junit.addListener(new RunListener(){@Override public void testAssumptionFailure(Failure failure){assumptions.add(failure);}});
        PrintStream protocol=System.out;
        Result result;
        try {
            System.setOut(System.err);
            result=junit.run(Request.method(Class.forName(mapping.get("java_class").asText()),mapping.get("java_method").asText()));
        } finally {System.setOut(protocol);}
        context.javaAssertionEvidence=Map.of(
            "class",mapping.get("java_class").asText(),"method",mapping.get("java_method").asText(),
            "source",mapping.get("source").asText(),"source_sha256",mapping.get("source_sha256").asText(),
            "feature_source_sha256",mapping.get("feature_source_sha256").asText(),
            "run_count",result.getRunCount(),"failure_count",result.getFailureCount(),
            "ignored_count",result.getIgnoreCount(),"assumption_count",assumptions.size());
        if(!result.wasSuccessful())throw new AssertionError(result.getFailures().stream().map(Failure::getTrace).reduce("",(a,b)->a+b));
        if(!assumptions.isEmpty())throw new AssumptionViolatedException(assumptions.toString());
        if(result.getRunCount()!=1||result.getIgnoreCount()!=0)throw new IllegalStateException("Expected exactly one original JUnit test: "+result.getRunCount());
        if(UpstreamGremlin.backend.equals("orchiddb")&&context.queryTransports.isEmpty())throw new IllegalStateException("Original assertion made no requests to OrchidDB");
    }
    /** TinkerGraph loads input fixtures only. Every traversal is submitted to the same native process. */
    static final class FixtureProvider extends AbstractGraphProvider {
        final UpstreamGremlin.Context context;
        LoadGraphWith.GraphData data;
        FixtureProvider(UpstreamGremlin.Context context){this.context=context;}
        @Override public Map<String,Object> getBaseConfiguration(String name,Class<?> test,String method,LoadGraphWith.GraphData data){
            this.data=data;
            return Map.of(Graph.GRAPH,TinkerGraph.class.getName(),
                TinkerGraph.GREMLIN_TINKERGRAPH_VERTEX_ID_MANAGER,"INTEGER",
                TinkerGraph.GREMLIN_TINKERGRAPH_EDGE_ID_MANAGER,"INTEGER",
                TinkerGraph.GREMLIN_TINKERGRAPH_VERTEX_PROPERTY_ID_MANAGER,"LONG");
        }
        @Override public Graph openTestGraph(Configuration config) {
            if(UpstreamGremlin.backend.equals("janusgraph")){context.getGraphTraversalSource(data);return context.graph;}
            return super.openTestGraph(config);
        }
        @Override public GraphTraversalSource traversal(Graph graph){return UpstreamGremlin.backend.equals("orchiddb")?context.remoteSource():graph.traversal();}
        @Override public void loadGraphData(Graph graph,LoadGraphWith data,Class test,String method){
            if(UpstreamGremlin.backend.equals("janusgraph"))return;
            if(data!=null)super.loadGraphData(graph,data,test,method);
            List<Object> nodes=new ArrayList<>(),edges=new ArrayList<>();
            graph.vertices().forEachRemaining(v->nodes.add(UpstreamGremlin.fixtureNode(v,"orchiddb")));
            graph.edges().forEachRemaining(e->edges.add(UpstreamGremlin.fixtureEdge(e)));
            try {
                var response=UpstreamGremlin.bridge.send(Map.of("op","fixture","name",data==null?"empty":data.value().name().toLowerCase(),"nodes",nodes,"edges",edges));
                if(response.has("error"))throw new IllegalStateException(response.get("error").asText());
            }catch(Exception error){throw new IllegalStateException("Original Java fixture load failed",error);}
        }
        @Override public void clear(Graph graph,Configuration config)throws Exception{if(graph!=null){graph.close();if(context.graph==graph){context.graph=null;context.source=null;}}}
        @Override public Set<Class> getImplementations(){return Set.of(TinkerGraph.class);}
    }
}
