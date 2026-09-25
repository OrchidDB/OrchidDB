package io.orchiddb.gremlin;

import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.structure.io.GraphMigrator;
import org.apache.tinkerpop.gremlin.structure.io.graphml.*;
import org.junit.*;
import java.util.concurrent.atomic.AtomicReference;
import static org.junit.Assert.*;

public class OrchidTransactionOwnershipTest {
    private String executable;
    @Before public void nativeBinary() {
        executable=System.getenv("ORCHIDDB_JVM_STORE");
        Assume.assumeTrue(executable!=null);
    }
    @Test public void helperThreadCannotCompleteAnotherThreadsTransaction() throws Exception {
        try(OrchidGraph graph=OrchidGraph.open(executable)) {
            Vertex vertex=graph.addVertex(T.id,"pending");
            Edge edge=vertex.addEdge("self",vertex);
            AtomicReference<Throwable> failure=new AtomicReference<>();
            Thread helper=new Thread(()->{
                try {
                    assertFalse(graph.tx().isOpen());
                    graph.tx().open();
                    assertNotNull(edge.id());
                    graph.tx().rollback();
                    graph.tx().commit();
                    graph.tx().rollback();
                } catch(Throwable error) { failure.set(error); }
            });
            helper.start(); helper.join();
            if(failure.get()!=null) throw new AssertionError(failure.get());
            assertTrue(graph.tx().isOpen());
            assertTrue(graph.vertices("pending").hasNext());
            graph.tx().rollback();
            assertFalse(graph.vertices("pending").hasNext());
        }
    }
    @Test public void competingWriteTransactionsSerializeAndRollbackIndependently() throws Exception {
        try(OrchidGraph graph=OrchidGraph.open(executable)) {
            java.util.List<Thread> workers=new java.util.ArrayList<>();
            AtomicReference<Throwable> failure=new AtomicReference<>();
            for(int i=0;i<24;i++) {
                final int id=i;
                workers.add(new Thread(()->{
                    try {
                        graph.addVertex(T.id,id,"index",id);
                        if(id%2==0) graph.tx().commit(); else graph.tx().rollback();
                    } catch(Throwable error) { failure.compareAndSet(null,error); }
                }));
            }
            workers.forEach(Thread::start);
            for(Thread worker:workers) { worker.join(10000); assertFalse(worker.isAlive()); }
            if(failure.get()!=null) throw new AssertionError(failure.get());
            assertEquals(Long.valueOf(12),graph.traversal().V().count().next());
            for(int i=0;i<24;i++) assertEquals(i%2==0,graph.vertices(i).hasNext());
        }
    }
    @Test public void graphMigratorWriterCleanupPreservesReaderTransaction() throws Exception {
        try(OrchidGraph source=OrchidGraph.open(executable); OrchidGraph target=OrchidGraph.open(executable)) {
            Vertex a=source.addVertex(T.id,"a","name","alpha");
            a.addEdge("link",source.addVertex(T.id,"b","name","beta"));
            source.tx().commit();
            GraphMigrator.migrateGraph(source,target,GraphMLReader.build().create(),GraphMLWriter.build().create());
            assertEquals(Long.valueOf(2),target.traversal().V().count().next());
            assertEquals(Long.valueOf(1),target.traversal().E().count().next());
        }
    }
    @Test public void waitingWriterCanBeCancelledOrReleasedByAbort() throws Exception {
        try(OrchidGraph graph=OrchidGraph.open(executable)) {
            graph.addVertex(T.id,"owner");
            for(boolean cancel:new boolean[]{true,false}) {
                java.util.concurrent.CountDownLatch started=new java.util.concurrent.CountDownLatch(1);
                AtomicReference<Throwable> outcome=new AtomicReference<>();
                Thread waiter=new Thread(()->{
                    started.countDown();
                    try { graph.addVertex(T.id,"waiter"); }
                    catch(Throwable error) { outcome.set(error); }
                });
                waiter.start(); started.await();
                if(cancel) waiter.interrupt(); else graph.abort();
                waiter.join(2000);
                assertFalse("Waiting writer must stop promptly",waiter.isAlive());
                assertTrue(outcome.get() instanceof java.util.concurrent.CancellationException||outcome.get() instanceof IllegalStateException);
            }
        }
    }
    @Test public void propertyRemovalIsIdempotentAndStillWorksAfterRollback() {
        try(OrchidGraph graph=OrchidGraph.open(executable)) {
            Vertex vertex=graph.addVertex("name","alpha");
            VertexProperty<String> name=vertex.property("name");
            Edge edge=vertex.addEdge("self",vertex,"weight",1);
            Property<Integer> weight=edge.property("weight");
            graph.tx().commit();
            name.remove(); name.remove(); weight.remove(); weight.remove();
            graph.tx().rollback();
            assertEquals("alpha",vertex.value("name"));
            assertEquals(Integer.valueOf(1),edge.value("weight"));
            vertex.property("name").remove(); edge.property("weight").remove();
            assertFalse(vertex.property("name").isPresent());
            assertFalse(edge.property("weight").isPresent());
        }
    }
}
