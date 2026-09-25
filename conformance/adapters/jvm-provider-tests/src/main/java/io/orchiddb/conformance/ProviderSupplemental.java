package io.orchiddb.conformance;

import com.fasterxml.jackson.databind.ObjectMapper;
import java.io.*;
import java.math.BigInteger;
import java.nio.file.*;
import java.util.*;
import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.structure.io.*;
import org.apache.tinkerpop.gremlin.structure.io.graphson.*;
import org.apache.tinkerpop.gremlin.structure.io.gryo.*;
import org.apache.tinkerpop.gremlin.structure.io.graphml.*;
import org.apache.tinkerpop.gremlin.tinkergraph.structure.TinkerGraph;
import org.junit.*;
import org.junit.runner.*;
import static org.junit.Assert.*;
import org.apache.tinkerpop.gremlin.process.traversal.IO;

/** Supplemental contracts, separately labelled from every original upstream test. */
public class ProviderSupplemental {
    private Graph graph;
    @Before public void setup() { graph = new NativeGraphProvider().openTestGraph(null); }
    @After public void close() throws Exception { if (graph != null) graph.close(); }
    @Test public void presentNullsAndRollbackPreserveIdentity() {
        Vertex v = graph.addVertex(T.id,"account", "name", "Ada");
        VertexProperty<Object> p = v.property(VertexProperty.Cardinality.list,"nullable",null,T.id,"null-prop");
        p.property("meta",null);
        Edge e = v.addEdge("self",v,T.id,"edge", "nullable",null);
        assertTrue(p.isPresent()); assertNull(p.value());
        assertTrue(p.property("meta").isPresent()); assertNull(p.property("meta").value());
        assertTrue(e.property("nullable").isPresent()); assertFalse(e.property("missing").isPresent());
        graph.tx().commit();
        v.property("name","Changed"); p.remove(); e.property("nullable").remove();
        graph.tx().rollback();
        Vertex fresh = graph.vertices("account").next();
        assertEquals("Ada",fresh.value("name"));
        assertEquals("null-prop",fresh.property("nullable").id());
        assertTrue(fresh.property("nullable").property("meta").isPresent());
        assertTrue(graph.edges("edge").next().property("nullable").isPresent());
    }
    @Test public void nestedTypedKeysAndExactIntegersSurviveNativeStorage() {
        BigInteger huge=BigInteger.TEN.pow(1000);
        Map<Object,Object> nested = new LinkedHashMap<>();
        nested.put(1L,Arrays.asList(huge,null));
        nested.put("1",new LinkedHashSet<>(List.of(1,2)));
        graph.addVertex(T.id,"typed","payload",nested);
        graph.tx().commit();
        Object value=graph.vertices("typed").next().value("payload");
        assertEquals(nested,value);
        Map<?,?> result=(Map<?,?>)value;
        assertTrue(result.containsKey(1L)); assertFalse(result.containsKey(1));
        assertTrue(result.get("1") instanceof Set);
        assertEquals(huge,((List<?>)result.get(1L)).get(0));
    }
    @Test public void sackSplitsCloneMutableValues() {
        Vertex root=graph.addVertex(T.id,"root");
        root.addEdge("branch",graph.addVertex(T.id,"left","name","L"));
        root.addEdge("branch",graph.addVertex(T.id,"right","name","R"));
        List<Map<String,String>> sacks = graph.traversal().withSack(
            (java.util.function.Supplier<Map<String,String>>)HashMap<String,String>::new, (Map<String,String> m)->new HashMap<>(m))
            .V("root").out().sideEffect(t->{
                Map<String,String> sack=t.sack(); sack.put("name",t.get().value("name"));
            }).<Map<String,String>>sack().toList();
        assertEquals(2,sacks.size()); assertNotSame(sacks.get(0),sacks.get(1));
        assertEquals(Set.of("L","R"),Set.of(sacks.get(0).get("name"),sacks.get(1).get("name")));
    }
    @Test public void dynamicCardinalityValuesUseNativePropertyRecords() {
        graph.addVertex(T.id,"target", "tag", "old");
        var values = org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.__.constant(
            Map.of("tag",VertexProperty.Cardinality.set("new")));
        graph.traversal().mergeV(Map.of(T.id,"target"))
            .option(org.apache.tinkerpop.gremlin.process.traversal.Merge.onMatch,values).iterate();
        graph.traversal().mergeV(Map.of(T.id,"target"))
            .option(org.apache.tinkerpop.gremlin.process.traversal.Merge.onMatch,
                org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.__.constant(
                    Map.of("tag",VertexProperty.Cardinality.set("new")))).iterate();
        assertEquals(Set.of("old","new"),graph.traversal().V("target").values("tag").toSet());
        graph.traversal().mergeV(Map.of(T.id,"target"))
            .option(org.apache.tinkerpop.gremlin.process.traversal.Merge.onMatch,
                org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.__.constant(
                    Map.of("tag",VertexProperty.Cardinality.single("only")))).iterate();
        assertEquals(List.of("only"),graph.traversal().V("target").values("tag").toList());
    }
    @Test public void jvmRegexSupportsLookaroundAndBackreferencesOnNativeProperties() {
        graph.addVertex("text","aabb"); graph.addVertex("text","ab");
        assertEquals(List.of("aabb"),graph.traversal().V().values("text")
            .is(org.apache.tinkerpop.gremlin.process.traversal.TextP.regex("(a)\\1(?=bb)")).toList());
    }
    @Test public void jvmGroupedSamplingHandlesTreeifiedHashCollisions() {
        List<String> keys=new ArrayList<>(List.of(""));
        for(int i=0;i<5;i++) {
            List<String> next=new ArrayList<>();
            for(String key:keys) { next.add(key+"Aa"); next.add(key+"BB"); }
            keys=next;
        }
        assertEquals(1L,keys.stream().map(String::hashCode).distinct().count());
        Collections.reverse(keys);
        for(String key:keys) for(int n=0;n<4;n++) graph.addVertex("group",key,"n",n);
        var source=graph.traversal().withStrategies(
            org.apache.tinkerpop.gremlin.process.traversal.strategy.decoration.SeedStrategy.build().seed(71).create());
        Map<Object,Object> first=source.V().group().by("group").by(
            org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.__.values("n").sample(1).fold()).next();
        Map<Object,Object> second=source.V().group().by("group").by(
            org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.__.values("n").sample(1).fold()).next();
        assertEquals(32,first.size()); assertEquals(first,second);
        for(Object value:first.values()) {
            List<?> selected=(List<?>)value;
            assertEquals(1,selected.size()); assertTrue(Set.of(0,1,2,3).contains(selected.get(0)));
        }
    }
    private void roundtrip(String format, boolean explicit) throws Exception {
        Vertex a=graph.addVertex(T.id,"a","name","Ada","age",41L);
        Vertex b=graph.addVertex(T.id,"b","name","Bea");
        a.addEdge("knows",b,T.id,"ab","weight",0.5d);
        if (!format.equals("graphml")) {
            a.property(VertexProperty.Cardinality.list,"tag","one",T.id,"p1","year",2021);
            a.property(VertexProperty.Cardinality.list,"tag","two",T.id,"p2");
        }
        String extension=switch(format) { case "graphson" -> ".json"; case "gryo" -> ".kryo"; default -> ".xml"; };
        assertEquals(format.equals("graphml")?0L:2L,(long)graph.traversal().V("a").properties("tag").count().next());
        Path path=Files.createTempFile("orchiddb-provider-roundtrip-", extension);
        try {
            var traversal=graph.traversal().io(path.toString());
            if (explicit) traversal.with(IO.writer,format);
            traversal.write().iterate();
            var configuration=new org.apache.commons.configuration2.BaseConfiguration();
            configuration.setProperty("gremlin.tinkergraph.defaultVertexPropertyCardinality","list");
            try (Graph read=TinkerGraph.open(configuration); InputStream input=Files.newInputStream(path)) {
                GraphReader reader=switch(format) {
                    case "graphson" -> GraphSONReader.build().mapper(GraphSONMapper.build().version(GraphSONVersion.V3_0).create()).create();
                    case "gryo" -> GryoReader.build().create();
                    case "graphml" -> GraphMLReader.build().create();
                    default -> throw new IllegalArgumentException(format);
                };
                reader.readGraph(input,read);
                assertEquals(2L,(long)read.traversal().V().count().next());
                assertEquals(1L,(long)read.traversal().E().count().next());
                Vertex restored=read.vertices("a").next();
                assertEquals("Ada",restored.value("name"));
                assertEquals(Long.valueOf(41),restored.value("age"));
                Edge edge=read.edges("ab").next();
                assertEquals("a",edge.outVertex().id()); assertEquals("b",edge.inVertex().id());
                assertEquals(0.5d,(Double)edge.value("weight"),0d);
                if (!format.equals("graphml")) {
                    assertEquals(2L,(long)read.traversal().V("a").properties("tag").count().next());
                    VertexProperty<?> prop=(VertexProperty<?>)read.traversal().V("a").properties("tag").has(T.id,"p1").next();
                    assertEquals(Integer.valueOf(2021),prop.value("year"));
                }
            }
        } finally { Files.deleteIfExists(path); }
    }
    @Test public void graphsonDefaultRoundtrip() throws Exception { roundtrip("graphson",false); }
    @Test public void graphsonExplicitRoundtrip() throws Exception { roundtrip("graphson",true); }
    @Test public void gryoDefaultRoundtrip() throws Exception { roundtrip("gryo",false); }
    @Test public void gryoExplicitRoundtrip() throws Exception { roundtrip("gryo",true); }
    @Test public void graphmlDefaultRoundtrip() throws Exception { roundtrip("graphml",false); }
    @Test public void graphmlExplicitRoundtrip() throws Exception { roundtrip("graphml",true); }
    @Test public void writerFailuresPropagate() throws Exception {
        graph.addVertex(T.id,"source");
        Path directory=Files.createTempDirectory("orchiddb-writer-error-");
        try {
            try { graph.traversal().io(directory.resolve("missing/graph.json").toString()).write().iterate(); fail("Missing parent accepted"); }
            catch (RuntimeException expected) { assertNotNull(expected.getMessage()); }
            assertEquals(1L,(long)graph.traversal().V().count().next());
        } finally { Files.delete(directory); }
    }
    public static void main(String[] args) throws Exception {
        Result result=JUnitCore.runClasses(ProviderSupplemental.class);
        var report=new LinkedHashMap<String,Object>();
        report.put("suite","supplemental-provider-contracts");
        report.put("profile","jvm-provider");
        report.put("original_upstream_tests",false);
        report.put("run",result.getRunCount()); report.put("failed",result.getFailureCount());
        report.put("ignored",result.getIgnoreCount());
        report.put("failures",result.getFailures().stream().map(f->Map.of("test",f.getDescription().toString(),"trace",f.getTrace())).toList());
        new ObjectMapper().writerWithDefaultPrettyPrinter().writeValue(Path.of(args[0]).toFile(),report);
        System.err.println(report);
        if (!result.wasSuccessful()) System.exit(1);
    }
}
