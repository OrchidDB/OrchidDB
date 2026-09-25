package io.orchiddb.gremlin;

import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.process.traversal.traverser.util.TraverserSet;
import org.apache.tinkerpop.gremlin.process.traversal.util.TraversalInterruptedException;
import org.junit.*;
import java.util.*;
import java.util.concurrent.*;
import static org.junit.Assert.*;

public class OrchidCommittedReadTest {
    private String executable;
    @Before public void nativeBinary() {
        executable=System.getenv("ORCHIDDB_JVM_STORE"); Assume.assumeNotNull(executable);
    }
    private <T> T readInThread(OrchidGraph graph,Callable<T> body) throws Exception {
        FutureTask<T> task=new FutureTask<>(()->{try{return body.call();}finally{graph.tx().rollback();}});
        Thread reader=new Thread(task,"committed-reader"); reader.start();
        try { return task.get(3,TimeUnit.SECONDS); }
        finally { task.cancel(true); reader.join(1000); assertFalse("Reader must finish before the writer commits",reader.isAlive()); }
    }
    @Test public void foreignReaderSeesCommittedCountsBeforeWriterCommitAndRollback() throws Exception {
        try(OrchidGraph graph=OrchidGraph.open(executable)) {
            Vertex a=graph.addVertex(T.id,1),b=graph.addVertex(T.id,"b"); a.addEdge("link",b,T.id,3L);
            assertEquals(Arrays.asList(0L,0L),readInThread(graph,()->Arrays.asList(graph.traversal().V().count().next(),graph.traversal().E().count().next())));
            assertEquals(Long.valueOf(2),graph.traversal().V().count().next());
            graph.tx().commit();
            assertEquals(Arrays.asList(2L,1L),readInThread(graph,()->Arrays.asList(graph.traversal().V().count().next(),graph.traversal().E().count().next())));
            b.remove();
            assertEquals(Arrays.asList(2L,1L),readInThread(graph,()->Arrays.asList(graph.traversal().V().count().next(),graph.traversal().E().count().next())));
            graph.tx().rollback();
            assertEquals(Long.valueOf(2),graph.traversal().V().count().next());
            b=graph.vertices("b").next(); b.remove(); graph.tx().commit();
            assertEquals(Arrays.asList(1L,0L),readInThread(graph,()->Arrays.asList(graph.traversal().V().count().next(),graph.traversal().E().count().next())));
        }
    }
    @Test public void committedReadsIgnoreOwnerCachesAndPreservePropertiesHandlesAndRuntime() throws Exception {
        try(OrchidGraph graph=OrchidGraph.open(executable)) {
            Vertex a=graph.addVertex(T.id,1),b=graph.addVertex(T.id,"b");
            a.property(VertexProperty.Cardinality.single,"name","committed","source","base");
            a.property("nullable",null);
            TraverserSet<Object> committedRuntime=new TraverserSet<>();
            a.property("runtime",committedRuntime);
            Edge edge=a.addEdge("link",b,T.id,3L,"weight",0.5f);
            graph.tx().commit();
            a.property(VertexProperty.Cardinality.single,"name","pending","source","dirty");
            a.property("runtime",new TraverserSet<>());
            edge.remove(); b.remove(); a.addEdge("pending",graph.addVertex(T.id,"new"));
            // Populate the owning writer's caches with uncommitted topology and values.
            assertEquals("pending",a.value("name")); assertTrue(a.edges(Direction.OUT,"pending").hasNext());
            assertFalse(a.edges(Direction.OUT,"link").hasNext());
            readInThread(graph,()->{
                Vertex committed=graph.vertices(1L).next();
                assertEquals(Integer.valueOf(1),committed.id());
                assertEquals("committed",committed.value("name"));
                assertEquals("base",committed.property("name").value("source"));
                assertTrue(committed.property("nullable").isPresent()); assertNull(committed.property("nullable").value());
                assertSame(committedRuntime,committed.value("runtime"));
                Edge old=committed.edges(Direction.OUT,"link").next();
                assertEquals(Long.valueOf(3),old.id()); assertEquals(Float.valueOf(0.5f),old.value("weight"));
                assertEquals("b",old.inVertex().id()); assertTrue(graph.vertices("b").hasNext());
                assertFalse(graph.vertices("new").hasNext());
                assertEquals("committed",a.value("name")); // existing live handle is resolved in the committed view
                return null;
            });
            assertEquals("pending",a.value("name")); assertFalse(a.edges(Direction.OUT,"link").hasNext());
            graph.tx().rollback();
            assertEquals("committed",a.value("name")); assertTrue(a.edges(Direction.OUT,"link").hasNext());
            assertSame(committedRuntime,a.value("runtime"));
        }
    }
    @Test public void interruptedPublicReadUsesTinkerPopExceptionAndPreservesSession() {
        try(OrchidGraph graph=OrchidGraph.open(executable)) {
            Vertex vertex=graph.addVertex(); vertex.addEdge("self",vertex); graph.tx().commit();
            for(Runnable read:Arrays.<Runnable>asList(()->graph.vertices(),()->vertex.edges(Direction.OUT),()->vertex.properties("name"))) {
                Thread.currentThread().interrupt();
                try { assertThrows(TraversalInterruptedException.class,read::run); assertTrue(Thread.currentThread().isInterrupted()); }
                finally { Thread.interrupted(); }
            }
            assertEquals(Long.valueOf(1),graph.traversal().V().out().count().next());
        }
    }
}
