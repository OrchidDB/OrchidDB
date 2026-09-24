package io.crabgraph.gremlin;

import org.apache.tinkerpop.gremlin.structure.*;
import org.junit.*;
import java.math.*;
import java.nio.file.*;
import java.util.*;
import static org.junit.Assert.*;

public class CrabGraphTest {
    private String executable;
    @Before public void nativeBinary() {
        executable=System.getenv("CRABGRAPH_JVM_STORE");
        Assume.assumeTrue("CRABGRAPH_JVM_STORE is required for native integration",executable!=null);
    }
    @Test public void configuredDefaultCardinalityAppliesToTraversalCreationAndFreshGraphs() {
        org.apache.commons.configuration2.BaseConfiguration config=new org.apache.commons.configuration2.BaseConfiguration();
        config.setProperty("crabgraph.native.executable",executable);
        config.setProperty(CrabGraph.DEFAULT_CARDINALITY,"list");
        try(CrabGraph graph=CrabGraph.open(config); CrabGraph fresh=graph.freshGraph()) {
            assertEquals(VertexProperty.Cardinality.list,graph.features().vertex().getCardinality("x"));
            Vertex v=graph.traversal().addV().property("x",1).property("x",2).next();
            assertEquals(Long.valueOf(2),graph.traversal().V(v).properties("x").count().next());
            Vertex constructed=graph.addVertex("x",1,"x",2);
            assertEquals(Long.valueOf(2),graph.traversal().V(constructed).properties("x").count().next());
            assertEquals(VertexProperty.Cardinality.list,fresh.features().vertex().getCardinality("x"));
            Vertex child=fresh.addVertex("x",1,"x",2);
            assertEquals(Long.valueOf(2),fresh.traversal().V(child).properties("x").count().next());
        }
        try(CrabGraph graph=CrabGraph.open(executable)) {
            Vertex v=graph.traversal().addV().property("x",1).property("x",2).next();
            assertEquals(Long.valueOf(1),graph.traversal().V(v).properties("x").count().next());
            assertEquals(Integer.valueOf(2),v.value("x"));
        }
    }
    @Test public void configuredIdManagersCoerceSuppliedIdsAndPreserveTypedDefaults() {
        org.apache.commons.configuration2.BaseConfiguration config=new org.apache.commons.configuration2.BaseConfiguration();
        config.setProperty("crabgraph.native.executable",executable);
        config.setProperty(CrabGraph.VERTEX_ID_MANAGER,"LONG");
        config.setProperty(CrabGraph.EDGE_ID_MANAGER,"INTEGER");
        config.setProperty(CrabGraph.VERTEX_PROPERTY_ID_MANAGER,"INTEGER");
        try(CrabGraph graph=CrabGraph.open(config); CrabGraph fresh=graph.freshGraph()) {
            Vertex v=graph.addVertex(T.id,"42");
            assertEquals(Long.valueOf(42),v.id());
            assertEquals(v,graph.vertices(42.0d).next());
            assertEquals(v,graph.vertices("42").next());
            Edge edge=v.addEdge("self",v,T.id,"31");
            assertEquals(Integer.valueOf(31),edge.id());
            assertEquals(edge,graph.edges(31L).next());
            VertexProperty<Integer> property=v.property(VertexProperty.Cardinality.single,"x",1,T.id,"81");
            assertEquals(Integer.valueOf(81),property.id());
            assertFalse(graph.features().vertex().supportsStringIds());
            assertTrue(graph.features().vertex().supportsNumericIds());
            assertTrue(graph.features().vertex().willAllowId("42"));
            assertFalse(graph.features().vertex().willAllowId("arbitrary"));
            assertEquals(Long.valueOf(43),fresh.addVertex(T.id,"43").id());
        }
        try(CrabGraph graph=CrabGraph.open(executable)) {
            Vertex number=graph.addVertex(T.id,1), text=graph.addVertex(T.id,"1");
            assertEquals(Integer.valueOf(1),number.id()); assertEquals("1",text.id());
            assertEquals(number,graph.vertices(1).next()); assertEquals(text,graph.vertices("1").next());
            assertTrue(graph.features().vertex().supportsStringIds());
        }
    }
    @Test public void numericStringLookupPrefersExactTypedStringIdentity() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            Vertex numeric=graph.addVertex(T.id,7);
            assertEquals(numeric,graph.vertices("7").next());
            Vertex text=graph.addVertex(T.id,"7");
            assertEquals(text,graph.vertices("7").next());
            assertEquals(numeric,graph.vertices(7).next());
            Edge numericEdge=numeric.addEdge("self",numeric,T.id,9);
            assertEquals(numericEdge,graph.edges("9").next());
            Edge textEdge=numeric.addEdge("self",numeric,T.id,"9");
            assertEquals(textEdge,graph.edges("9").next());
            assertEquals(numericEdge,graph.edges(9).next());
            assertFalse(graph.vertices("not-a-number").hasNext());
        }
    }
    @Test public void bulkImportDefersReaderCommitsAndPreservesCallerTransaction() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            graph.bulkLoad(()->{graph.addVertex(T.id,"committed"); graph.tx().commit(); graph.addVertex(T.id,"also-committed");});
            assertFalse(graph.tx().isOpen());
            graph.tx().rollback();
            assertTrue(graph.vertices("committed").hasNext());
            graph.addVertex(T.id,"callers-pending");
            graph.bulkLoad(()->{graph.addVertex(T.id,"loaded-pending");graph.tx().commit();});
            graph.tx().rollback();
            assertFalse(graph.vertices("callers-pending").hasNext());
            assertFalse(graph.vertices("loaded-pending").hasNext());
            assertTrue(graph.vertices("also-committed").hasNext());
        }
    }
    @Test public void cancelledAtomicMutationRollsBackWithInterruptFlagPreserved() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            graph.addVertex(T.id,"before");
            try {
                graph.atomicMutation(()->{
                    graph.addVertex(T.id,"during"); Thread.currentThread().interrupt();
                    throw new java.util.concurrent.CancellationException("cancelled");
                }); fail();
            } catch(java.util.concurrent.CancellationException expected) {
                assertTrue(Thread.interrupted());
            } finally { Thread.interrupted(); }
            assertTrue(graph.vertices("before").hasNext()); assertFalse(graph.vertices("during").hasNext());
        }
    }
    @Test public void cachedAndUncachedTraversalPreserveTypedIdsOrderAndMultiplicity() {
        String key="crabgraph.native.adjacencyCacheSize",previous=System.getProperty(key);
        List<Object> uncached=null;
        try {
            for(int limit:new int[]{0,4096}) {
                System.setProperty(key,Integer.toString(limit));
                try(CrabGraph graph=CrabGraph.open(executable)) {
                    Vertex a=graph.addVertex(T.id,"a"),b=graph.addVertex(T.id,7L),c=graph.addVertex(T.id,9);
                    a.addEdge("link",b); a.addEdge("loop",a); a.addEdge("link",b);
                    b.addEdge("link",c); c.addEdge("link",a);
                    List<Object> result=graph.traversal().V("a").repeat(org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.__.out()).times(4).id().toList();
                    if(limit==0) uncached=result; else assertEquals(uncached,result);
                    assertTrue(result.size()>5); assertTrue(result.contains(7L)); assertTrue(result.contains(9));
                }
            }
        } finally { if(previous==null) System.clearProperty(key); else System.setProperty(key,previous); }
    }
    @Test public void cachedAdjacencyDoesNotHideNativeProcessDeath() throws Exception {
        CrabGraph graph=CrabGraph.open(executable);
        try {
            Vertex a=graph.addVertex(),b=graph.addVertex(); a.addEdge("link",b);
            assertTrue(a.edges(Direction.OUT).hasNext());
            java.lang.reflect.Field sessionField=CrabGraph.class.getDeclaredField("session"); sessionField.setAccessible(true);
            java.lang.reflect.Field processField=CrabSession.class.getDeclaredField("process"); processField.setAccessible(true);
            Process process=(Process)processField.get(sessionField.get(graph));
            process.destroyForcibly().waitFor();
            try { a.edges(Direction.OUT); fail("Dead native session must not serve cached topology"); }
            catch(IllegalStateException expected) { }
        } finally { graph.abort(); }
    }
    @Test public void repeatedAdjacencyReadsTrackMutationsSelfLoopsAndRollback() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            Vertex a=graph.addVertex(T.id,"a"),b=graph.addVertex(T.id,"b"),c=graph.addVertex(T.id,"c");
            Edge original=a.addEdge("base",b); graph.tx().commit();
            for(int i=0;i<3;i++) {
                assertEquals(Long.valueOf(1),graph.traversal().V(a).outE().count().next());
                assertEquals(Long.valueOf(0),graph.traversal().V(a).inE().count().next());
                assertEquals(Long.valueOf(0),graph.traversal().V(a).outE("new").count().next());
            }
            a.addEdge("loop",a);
            assertEquals(Long.valueOf(3),graph.traversal().V(a).bothE().count().next());
            a.addEdge("new",c);
            assertEquals(Long.valueOf(1),graph.traversal().V(a).outE("new").count().next());
            original.remove();
            assertEquals(Long.valueOf(2),graph.traversal().V(a).outE().count().next());
            c.remove();
            assertEquals(Long.valueOf(1),graph.traversal().V(a).outE().count().next());
            graph.tx().rollback();
            assertEquals(Long.valueOf(1),graph.traversal().V(a).outE("base").count().next());
            assertEquals(Long.valueOf(0),graph.traversal().V(a).outE("loop","new").count().next());
            assertEquals(Long.valueOf(0),graph.traversal().V(a).inE().count().next());
            try {
                graph.atomicMutation(()->{
                    a.addEdge("temporary",b);
                    assertEquals(Long.valueOf(2),graph.traversal().V(a).outE().count().next());
                    throw new IllegalStateException("undo");
                }); fail();
            } catch(IllegalStateException expected) { assertEquals("undo",expected.getMessage()); }
            assertEquals(Long.valueOf(1),graph.traversal().V(a).outE().count().next());
        }
    }
    @Test public void adjacencyCacheNeverHidesPoisonedAtomicBlockErrors() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            Vertex a=graph.addVertex(T.id,"a"),b=graph.addVertex(T.id,"b"); a.addEdge("link",b);
            assertTrue(a.edges(Direction.OUT).hasNext());
            try {
                graph.atomicMutation(()->{
                    assertTrue(a.edges(Direction.OUT).hasNext());
                    try { graph.addVertex(T.id,"a"); fail(); } catch(IllegalArgumentException expected) { }
                    a.edges(Direction.OUT);
                    fail("Read in a poisoned native atomic block must fail");
                }); fail();
            } catch(IllegalArgumentException expected) { }
            assertTrue(a.edges(Direction.OUT).hasNext());
        }
    }
    @Test public void nullPropertyKeySelectsNothingAndMixedKeysIgnoreNull() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            Vertex v=graph.addVertex("x",1);
            VertexProperty<Object> vp=v.property("x"); vp.property("meta",2);
            Edge edge=v.addEdge("self",v,"weight",3);
            for(Element element:Arrays.asList(v,vp,edge)) {
                assertFalse(element.properties((String)null).hasNext());
                assertFalse(element.properties(null,null).hasNext());
                assertTrue(element.properties((String[])null).hasNext());
            }
            assertEquals("x",v.properties(null,"x").next().key());
            assertEquals("meta",vp.properties(null,"meta").next().key());
            assertEquals("weight",edge.properties(null,"weight").next().key());
            assertEquals(Long.valueOf(0),graph.traversal().V().has((String)null).count().next());
        }
    }
    @Test public void traversesAndMutatesNativeElementsWithTypedProperties() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            Vertex a=graph.addVertex(T.id,"a",T.label,"person","name","Ada","age",31);
            Vertex b=graph.addVertex(T.id,9007199254740993L,T.label,"person","name","Grace");
            Edge edge=a.addEdge("knows",b,T.id,"e","weight",0.5f);
            assertEquals(Collections.singletonList("Grace"),graph.traversal().V("a").out("knows").values("name").toList());
            assertEquals(Float.class,edge.value("weight").getClass());
            a.property("infinity",Double.POSITIVE_INFINITY);
            a.property("negativeInfinity",Float.NEGATIVE_INFINITY);
            assertEquals(Double.POSITIVE_INFINITY,(Double)a.value("infinity"),0.0);
            assertEquals(Float.NEGATIVE_INFINITY,(Float)a.value("negativeInfinity"),0.0f);
            assertEquals(Long.valueOf(9007199254740993L),b.id());
            VertexProperty<Object> property=a.property(VertexProperty.Cardinality.list,"precise",new BigDecimal("12345678901234567890.0000000000001"),"source","input");
            assertEquals(new BigDecimal("12345678901234567890.0000000000001"),property.value());
            assertEquals("input",property.value("source"));
            property.property("source","updated");
            assertEquals("updated",a.property("precise").value("source"));
            graph.tx().commit();
            graph.traversal().V("a").property("age",32).iterate();
            edge.remove();
            graph.tx().rollback();
            assertEquals(Integer.valueOf(31),graph.traversal().V("a").values("age").next());
            assertTrue(graph.edges("e").hasNext());
        }
    }
    @Test public void preservesPresentNullCardinalityAndMetaProperties() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            Vertex v=graph.addVertex(T.id,"v");
            v.property(VertexProperty.Cardinality.list,"x",null,"meta",null);
            assertTrue(v.property("x").isPresent()); assertNull(v.property("x").value());
            assertTrue(v.property("x").property("meta").isPresent());
            assertNull(v.property("x").property("meta").value());
            v.property(VertexProperty.Cardinality.list,"x",2);
            assertEquals(Long.valueOf(2),graph.traversal().V().properties("x").count().next());
            v.property(VertexProperty.Cardinality.single,"x",3);
            assertEquals(Long.valueOf(1),graph.traversal().V().properties("x").count().next());
            v.property(VertexProperty.Cardinality.set,"x",3);
            assertEquals(Long.valueOf(1),graph.traversal().V().properties("x").count().next());
        }
    }
    @Test public void savepointFailurePreservesCallersUncommittedState() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            graph.addVertex(T.id,"before");
            try { graph.atomicMutation(()->{graph.addVertex(T.id,"during");throw new IllegalStateException("stop");}); fail(); }
            catch(IllegalStateException expected) { assertEquals("stop",expected.getMessage()); }
            assertTrue(graph.vertices("before").hasNext());
            assertFalse(graph.vertices("during").hasNext());
            graph.tx().rollback(); assertFalse(graph.vertices().hasNext());
        }
    }
    @Test public void durableCommitAndCloseRollback() throws Exception {
        Path directory=Files.createTempDirectory("crabgraph-provider-"); Path snapshot=directory.resolve("graph.ngsp");
        try {
            try(CrabGraph graph=CrabGraph.open(executable,snapshot)) {
                graph.addVertex(T.id,"kept","amount",new BigInteger("9999999999999999999999999999")); graph.tx().commit();
                graph.addVertex(T.id,"discarded");
            }
            try(CrabGraph graph=CrabGraph.open(executable,snapshot)) {
                assertTrue(graph.vertices("kept").hasNext()); assertFalse(graph.vertices("discarded").hasNext());
                assertEquals(new BigInteger("9999999999999999999999999999"),graph.vertices("kept").next().value("amount"));
            }
        } finally {
            try(java.util.stream.Stream<Path> files=Files.walk(directory)) { files.sorted(Comparator.reverseOrder()).forEach(path->{try{Files.deleteIfExists(path);}catch(Exception ignored){}}); }
        }
    }
    @Test public void runtimeObjectsHaveDedicatedTypedSessionReferences() {
        try(CrabGraph graph=CrabGraph.open(executable)) {
            org.apache.tinkerpop.gremlin.process.traversal.traverser.util.TraverserSet<Object> runtime=new org.apache.tinkerpop.gremlin.process.traversal.traverser.util.TraverserSet<>();
            Object encoded=graph.encodeValue(runtime);
            assertEquals("jvm_runtime",((Map<?,?>)encoded).get("type"));
            assertSame(runtime,graph.decodeValue(encoded));
            try(CrabGraph child=graph.freshGraph()) { assertSame(runtime,child.decodeValue(encoded)); }
            Vertex v=graph.addVertex(); v.property("runtime",runtime);
            assertSame(runtime,v.property("runtime").value());
            Map<String,String> ordinary=Collections.singletonMap("crabgraph:runtime:fake","123");
            assertEquals(ordinary,graph.decodeValue(graph.encodeValue(ordinary)));
        }
    }
    @Test public void resolvesExplicitCrossGraphElementReferencesByPublicIdentity() {
        try(CrabGraph source=CrabGraph.open(executable); CrabGraph target=source.freshGraph()) {
            Vertex sourceA=source.addVertex(T.id,"a"), sourceB=source.addVertex(T.id,"b");
            Edge sourceEdge=sourceA.addEdge("link",sourceB,T.id,"edge");
            Vertex targetA=target.addVertex(T.id,"a"), targetB=target.addVertex(T.id,"b");
            targetA.addEdge("link",targetB,T.id,"edge");
            targetA.property("vertexReference",sourceB);
            Vertex reference=targetA.value("vertexReference");
            assertSame(target,reference.graph()); assertEquals("b",reference.id());
            targetA.property("edgeReference",org.apache.tinkerpop.gremlin.structure.util.detached.DetachedFactory.detach(sourceEdge,true));
            Edge edgeReference=targetA.value("edgeReference");
            assertSame(target,edgeReference.graph()); assertEquals("edge",edgeReference.id());
            Vertex missing=source.addVertex(T.id,"missing");
            try { targetA.property("missing",missing); fail(); } catch(IllegalArgumentException expected) { }
        }
    }
    @Test public void resultGraphAndRuntimeOutliveOriginalGraph() {
        CrabGraph source=CrabGraph.open(executable); CrabGraph result=source.freshGraph();
        try {
            org.apache.tinkerpop.gremlin.process.traversal.traverser.util.TraverserSet<Object> runtime=new org.apache.tinkerpop.gremlin.process.traversal.traverser.util.TraverserSet<>();
            Vertex vertex=result.addVertex(); vertex.property("runtime",runtime);
            source.close(); assertSame(runtime,result.vertices(vertex.id()).next().value("runtime"));
        } finally { source.close(); result.close(); }
    }
    @Test public void queuedExecutionLeaseCanBeCancelledWithoutAbortingGraph() throws Exception {
        try(CrabGraph graph=CrabGraph.open(executable); AutoCloseable lease=graph.executionLease()) {
            java.util.concurrent.CountDownLatch started=new java.util.concurrent.CountDownLatch(1);
            java.util.concurrent.atomic.AtomicBoolean interrupted=new java.util.concurrent.atomic.AtomicBoolean();
            Thread queued=new Thread(()->{
                started.countDown();
                try(AutoCloseable ignored=graph.executionLease()) { }
                catch(Exception expected) { interrupted.set(Thread.currentThread().isInterrupted()); }
            });
            queued.start(); started.await(); queued.interrupt(); queued.join(2000);
            assertFalse("Cancelled lease must stop while another thread holds it",queued.isAlive());
            assertTrue(interrupted.get());
            graph.addVertex(T.id,"still-open"); assertTrue(graph.vertices("still-open").hasNext());
        }
    }
    @Test public void closeAndAbortInvalidateNativeSession() {
        CrabGraph graph=CrabGraph.open(executable); graph.addVertex(); graph.abort();
        try { graph.vertices(); fail(); } catch(IllegalStateException expected) { assertTrue(expected.getMessage().contains("closed")); }
        graph.close();
    }
}
