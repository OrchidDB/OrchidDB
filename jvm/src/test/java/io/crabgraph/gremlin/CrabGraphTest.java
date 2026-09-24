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
