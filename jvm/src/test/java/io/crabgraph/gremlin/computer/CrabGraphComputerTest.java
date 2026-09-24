package io.crabgraph.gremlin.computer;

import io.crabgraph.gremlin.CrabGraph;
import org.apache.tinkerpop.gremlin.process.computer.*;
import org.apache.tinkerpop.gremlin.process.computer.traversal.step.map.ConnectedComponent;
import org.apache.tinkerpop.gremlin.process.computer.traversal.step.map.PageRank;
import org.apache.tinkerpop.gremlin.process.computer.traversal.step.map.PeerPressure;
import org.apache.tinkerpop.gremlin.process.computer.traversal.step.map.ShortestPath;
import org.apache.tinkerpop.gremlin.process.traversal.Operator;
import org.apache.tinkerpop.gremlin.process.traversal.Path;
import org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.GraphTraversalSource;
import org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.__;
import org.apache.tinkerpop.gremlin.structure.*;
import org.junit.After;
import org.junit.Before;
import org.junit.Test;

import java.util.*;
import java.util.concurrent.*;
import java.util.stream.Collectors;

import static org.junit.Assert.*;

/** Supplemental native-provider regressions, separate from unchanged upstream assertions. */
public class CrabGraphComputerTest {
    private CrabGraph graph;

    @Before public void open() { graph = CrabGraph.open(); }
    @After public void close() throws Exception { if (graph != null) graph.closeFamily(); }
    private GraphComputer computer() { return new CrabGraphComputer(graph, graph::freshGraph); }
    private Vertex vertex(String id) { return graph.addVertex(T.id, id, "name", "renamed-" + id); }
    private GraphTraversalSource g() { return graph.traversal().withComputer(); }
    private static Set<List<Object>> ids(List<Path> paths) {
        return paths.stream().map(p -> p.objects().stream().map(o -> ((Element) o).id())
                .collect(Collectors.toList())).collect(Collectors.toSet());
    }

    @Test public void componentsRespectSelectedEdgesAndDisconnectedRenamedVertices() {
        Vertex c = vertex("c-90"), a = vertex("a-42"), z = vertex("z-3"), b = vertex("b-11");
        c.addEdge("join", a); a.addEdge("join", b); b.addEdge("ignored", z);
        List<Map<String,Object>> rows = g().V().connectedComponent()
                .with(ConnectedComponent.edges, __.bothE("join"))
                .with(ConnectedComponent.propertyName, "partition")
                .project("id", "partition").by(T.id).by("partition").toList();
        Map<Object,Object> partitions = new HashMap<>();
        rows.forEach(row -> partitions.put(row.get("id"), row.get("partition")));
        assertEquals(4, partitions.size());
        assertEquals(partitions.get(a.id()), partitions.get(b.id()));
        assertEquals(partitions.get(a.id()), partitions.get(c.id()));
        assertNotEquals(partitions.get(a.id()), partitions.get(z.id()));
        assertFalse(a.property("partition").isPresent());
    }

    @Test public void weightedShortestPathRetainsTiesAndIncludesRealEdges() {
        Vertex d = vertex("d-4"), c = vertex("c-3"), a = vertex("a-1"), b = vertex("b-2");
        vertex("isolated-0");
        a.addEdge("route", b, T.id, "ab", "cost", 1.25);
        b.addEdge("route", d, T.id, "bd", "cost", 2.75);
        a.addEdge("route", c, T.id, "ac", "cost", 2.0);
        c.addEdge("route", d, T.id, "cd", "cost", 2.0);
        a.addEdge("route", d, T.id, "ad", "cost", 9.0);
        List<Path> paths = g().V(a.id()).shortestPath().with(ShortestPath.target, __.hasId(d.id()))
                .with(ShortestPath.edges, __.outE("route")).with(ShortestPath.distance, "cost")
                .with(ShortestPath.includeEdges).toList();
        assertEquals(2, paths.size());
        assertEquals(Set.of(List.of(a.id(), "ab", b.id(), "bd", d.id()),
                List.of(a.id(), "ac", c.id(), "cd", d.id())), ids(paths));
    }

    @Test public void shortestPathHonorsDirectionAndMaximumWeightedDistance() {
        Vertex a = vertex("a-100"), b = vertex("b-7"), c = vertex("c-20");
        a.addEdge("route", b, "cost", 1.0); b.addEdge("route", c, "cost", 2.0);
        assertFalse(g().V(c.id()).shortestPath().with(ShortestPath.edges, Direction.OUT)
                .with(ShortestPath.target, __.hasId(a.id())).hasNext());
        assertEquals(Set.of(List.of(c.id(), b.id(), a.id())), ids(g().V(c.id()).shortestPath()
                .with(ShortestPath.edges, Direction.IN).with(ShortestPath.target, __.hasId(a.id())).toList()));
        assertFalse(g().V(a.id()).shortestPath().with(ShortestPath.edges, __.outE())
                .with(ShortestPath.target, __.hasId(c.id())).with(ShortestPath.distance, "cost")
                .with(ShortestPath.maxDistance, 2.99).hasNext());
        assertEquals(1, g().V(a.id()).shortestPath().with(ShortestPath.edges, __.outE())
                .with(ShortestPath.target, __.hasId(c.id())).with(ShortestPath.distance, "cost")
                .with(ShortestPath.maxDistance, 3.0).toList().size());
    }

    @Test public void pageRankZeroIterationsPreservesTraversalSeedMultiplicity() {
        Vertex target = vertex("target-55"), a = vertex("source-81"), b = vertex("source-9");
        a.addEdge("seed", target); b.addEdge("seed", target);
        List<Number> ranks = g().V().out("seed").pageRank()
                .with(PageRank.propertyName, "seedRank").with(PageRank.times, 0)
                .<Number>values("seedRank").toList();
        assertEquals(2, ranks.size());
        ranks.forEach(rank -> assertEquals(2.0, rank.doubleValue(), 0.000001));
    }

    @Test public void peerPressureThenPageRankUsesComputedStateOnArbitraryCycle() {
        Vertex a = vertex("z-88"), b = vertex("m-4"), c = vertex("p-70");
        a.addEdge("cycle", b); b.addEdge("cycle", c); c.addEdge("cycle", a);
        List<Map<String,Object>> rows = g().V().peerPressure().with(PeerPressure.propertyName, "cluster")
                .with(PeerPressure.edges, __.bothE("cycle")).with(PeerPressure.times, 5)
                .pageRank(1.0).with(PageRank.propertyName, "rank").with(PageRank.edges, __.outE("cycle"))
                .with(PageRank.times, 2).project("cluster", "rank").by("cluster").by("rank").toList();
        assertEquals(3, rows.size());
        for (Map<String,Object> row : rows) {
            assertNotNull(row.get("cluster"));
            // A regular directed cycle has uniform stationary rank, normalized to one.
            assertEquals(1.0 / 3.0, ((Number) row.get("rank")).doubleValue(), 0.000001);
        }
    }

    @Test public void newResultOwnsNativeStateAndTransientKeysDoNotEscape() throws Exception {
        Vertex a = vertex("native-a"), b = vertex("native-b"); a.addEdge("link", b, "weight", 2);
        ComputerResult result = computer().program(new CountingProgram()).result(GraphComputer.ResultGraph.NEW)
                .persist(GraphComputer.Persist.EDGES).submit().get(20, TimeUnit.SECONDS);
        Graph output = result.graph();
        try {
            assertNotSame(graph, output);
            assertEquals(2L, output.traversal().V().count().next().longValue());
            assertEquals(1L, output.traversal().E().count().next().longValue());
            assertEquals(2L, result.memory().<Long>get("seen").longValue());
            assertEquals(1, result.memory().getIteration());
            assertFalse(result.memory().exists("scratch"));
            assertTrue(output.traversal().V().has("computed", 2L).count().next() == 2L);
            assertFalse(output.traversal().V().has("temporary").hasNext());
            assertFalse(a.property("computed").isPresent());
            output.vertices(a.id()).next().property("name", "only-result");
            assertEquals("renamed-native-a", a.value("name"));
            assertThrows(IllegalStateException.class, () -> result.memory().set("seen", 0L));
            output.tx().commit();
            graph.close(); graph = null;
            assertEquals(2L, output.traversal().V().count().next().longValue());
        } finally { output.close(); }
    }

    @Test public void originalPersistenceCanBeRolledBackByNativeTransaction() throws Exception {
        Vertex v = vertex("rollback"); v.property("computed", 99L); graph.tx().commit(); graph.tx().open();
        ComputerResult result = computer().program(new CountingProgram()).result(GraphComputer.ResultGraph.ORIGINAL)
                .persist(GraphComputer.Persist.VERTEX_PROPERTIES).submit().get(20, TimeUnit.SECONDS);
        assertSame(graph, result.graph());
        assertEquals(1L, v.<Long>value("computed").longValue());
        assertFalse(v.property("temporary").isPresent());
        graph.tx().rollback();
        assertEquals(99L, graph.vertices("rollback").next().<Long>value("computed").longValue());
    }

    @Test public void persistNothingLeavesOriginalPropertiesUntouched() throws Exception {
        Vertex v = vertex("untouched"); v.property("computed", 99L);
        ComputerResult result = computer().program(new CountingProgram()).result(GraphComputer.ResultGraph.ORIGINAL)
                .persist(GraphComputer.Persist.NOTHING).submit().get(20, TimeUnit.SECONDS);
        assertSame(graph, result.graph());
        assertEquals(99L, v.<Long>value("computed").longValue());
        assertFalse(v.property("temporary").isPresent());
        assertEquals(1L, result.memory().<Long>get("seen").longValue());
    }

    @Test public void newPersistNothingReturnsEmptyNativeGraphWithComputedMemory() throws Exception {
        vertex("a"); vertex("b");
        ComputerResult result = computer().program(new CountingProgram()).result(GraphComputer.ResultGraph.NEW)
                .persist(GraphComputer.Persist.NOTHING).submit().get(20, TimeUnit.SECONDS);
        try (Graph output = result.graph()) {
            assertFalse(output.vertices().hasNext());
            assertFalse(output.edges().hasNext());
            assertEquals(2L, result.memory().<Long>get("seen").longValue());
            output.addVertex(T.id, "new-native-vertex");
            assertEquals(1L, output.traversal().V().count().next().longValue());
            assertEquals(2L, graph.traversal().V().count().next().longValue());
        }
    }

    @Test public void newVertexPropertiesResultPreservesMultiAndMetaPropertiesWithoutEdges() throws Exception {
        Vertex a = vertex("a"), b = vertex("b"); a.addEdge("link", b);
        a.property(VertexProperty.Cardinality.list, "tag", "red", T.id, "red-tag", "source", "one");
        a.property(VertexProperty.Cardinality.list, "tag", "blue", T.id, "blue-tag", "source", "two");
        ComputerResult result = computer().program(new CountingProgram()).result(GraphComputer.ResultGraph.NEW)
                .persist(GraphComputer.Persist.VERTEX_PROPERTIES).submit().get(20, TimeUnit.SECONDS);
        try (Graph output = result.graph()) {
            assertEquals(2L, output.traversal().V().count().next().longValue());
            assertFalse(output.edges().hasNext());
            Map<Object,List<Object>> tags = new HashMap<>();
            output.vertices(a.id()).next().properties("tag").forEachRemaining(p ->
                    tags.put(p.id(), List.of(p.value(), p.value("source"))));
            assertEquals(Map.of("red-tag", List.of("red", "one"), "blue-tag", List.of("blue", "two")), tags);
            assertEquals(2L, output.vertices(a.id()).next().<Long>value("computed").longValue());
        }
    }

    @Test public void graphFilterRestrictsExecutionAndNewResultGraph() throws Exception {
        Vertex a = vertex("a"), b = vertex("b"), c = vertex("c");
        a.property("keep", true); b.property("keep", true); c.property("keep", false);
        a.addEdge("selected", b); a.addEdge("ignored", b); b.addEdge("selected", c);
        ComputerResult result = computer().vertices(__.has("keep", true)).edges(__.outE("selected"))
                .program(new CountingProgram()).result(GraphComputer.ResultGraph.NEW)
                .persist(GraphComputer.Persist.EDGES).submit().get(20, TimeUnit.SECONDS);
        try (Graph output = result.graph()) {
            assertEquals(Set.of(a.id(), b.id()), new HashSet<>(output.traversal().V().id().toList()));
            assertEquals(List.of("selected"), output.traversal().E().label().toList());
            assertEquals(2L, result.memory().<Long>get("seen").longValue());
        }
    }

    @Test public void localMessagesRespectDirectionWeightsSelfLoopsAndSuperstepBoundary() throws Exception {
        Vertex a = vertex("a"), b = vertex("b"), c = vertex("c");
        a.addEdge("route", b, "weight", 2); a.addEdge("route", c, "weight", 3);
        b.addEdge("route", c, "weight", 5); c.addEdge("route", a, "weight", 7);
        a.addEdge("route", a, "weight", 11); a.addEdge("ignored", b, "weight", 1000);
        MessageScope.Local<Long> scope = MessageScope.Local.of(() -> __.outE("route"),
                (message, edge) -> message * edge.<Integer>value("weight"));
        ComputerResult result = computer().program(new CountingProgram() {
            @Override public void setup(Memory memory) { }
            @Override public Set<MemoryComputeKey> getMemoryComputeKeys() { return Collections.emptySet(); }
            @Override public Set<MessageScope> getMessageScopes(Memory memory) { return Set.of(scope); }
            @Override public void execute(Vertex vertex, Messenger<Long> messenger, Memory memory) {
                if (memory.isInitialIteration()) {
                    assertFalse(messenger.receiveMessages().hasNext());
                    messenger.sendMessage(scope, 1L);
                } else {
                    long received = 0L;
                    Iterator<Long> messages = messenger.receiveMessages();
                    while (messages.hasNext()) received += messages.next();
                    vertex.property(VertexProperty.Cardinality.single, "computed", received);
                }
            }
        }).result(GraphComputer.ResultGraph.NEW).persist(GraphComputer.Persist.VERTEX_PROPERTIES)
                .submit().get(20, TimeUnit.SECONDS);
        try (Graph output = result.graph()) {
            assertEquals(18L, output.vertices(a.id()).next().<Long>value("computed").longValue());
            assertEquals(2L, output.vertices(b.id()).next().<Long>value("computed").longValue());
            assertEquals(8L, output.vertices(c.id()).next().<Long>value("computed").longValue());
        }
    }

    @Test public void failedProgramDoesNotPublishPartialComputeProperties() throws Exception {
        Vertex v = vertex("failure"); v.property("computed", 77L);
        ExecutionException error = assertThrows(ExecutionException.class, () -> computer().program(new CountingProgram() {
            @Override public void execute(Vertex vertex, Messenger<Long> messenger, Memory memory) {
                vertex.property(VertexProperty.Cardinality.single, "computed", -1L);
                throw new IllegalStateException("intentional vertex program failure");
            }
        }).result(GraphComputer.ResultGraph.ORIGINAL).persist(GraphComputer.Persist.VERTEX_PROPERTIES)
                .submit().get(20, TimeUnit.SECONDS));
        assertEquals("intentional vertex program failure", error.getCause().getMessage());
        assertEquals(77L, v.<Long>value("computed").longValue());
    }

    @Test public void cancellationInterruptsWorkerAndDoesNotPublishProperties() throws Exception {
        Vertex v = vertex("cancel"); v.property("computed", 88L);
        CountDownLatch started = new CountDownLatch(1), stopped = new CountDownLatch(1);
        Future<ComputerResult> future = computer().program(new CountingProgram() {
            @Override public void execute(Vertex vertex, Messenger<Long> messenger, Memory memory) {
                vertex.property(VertexProperty.Cardinality.single, "computed", -1L);
                started.countDown();
                try { new CountDownLatch(1).await(); }
                catch (InterruptedException e) { Thread.currentThread().interrupt(); throw new IllegalStateException(e); }
                finally { stopped.countDown(); }
            }
        }).result(GraphComputer.ResultGraph.ORIGINAL).persist(GraphComputer.Persist.VERTEX_PROPERTIES).submit();
        assertTrue(started.await(10, TimeUnit.SECONDS));
        assertTrue(future.cancel(true));
        assertTrue(stopped.await(10, TimeUnit.SECONDS));
        assertTrue(future.isCancelled());
        assertThrows(CancellationException.class, future::get);
        assertEquals(88L, v.<Long>value("computed").longValue());
    }

    @Test public void submittedComputerCannotBeReused() throws Exception {
        vertex("once");
        GraphComputer computer = computer().program(new CountingProgram()).result(GraphComputer.ResultGraph.ORIGINAL)
                .persist(GraphComputer.Persist.NOTHING);
        computer.submit().get(20, TimeUnit.SECONDS);
        assertThrows(IllegalStateException.class, computer::submit);
    }

    /** Checks that reducers are invisible within an iteration and broadcast next iteration. */
    private static class CountingProgram implements VertexProgram<Long> {
        @Override public void setup(Memory memory) { memory.set("seen", 0L); memory.set("scratch", 0L); }
        @Override public void execute(Vertex vertex, Messenger<Long> messenger, Memory memory) {
            long previous = memory.get("seen");
            if (memory.isInitialIteration()) {
                assertEquals(0L, previous);
                memory.add("seen", 1L);
                memory.add("scratch", 1L);
            }
            vertex.property(VertexProperty.Cardinality.single, "computed", previous);
            vertex.property(VertexProperty.Cardinality.single, "temporary", "transient-value");
        }
        @Override public boolean terminate(Memory memory) { return memory.getIteration() == 1; }
        @Override public Set<VertexComputeKey> getVertexComputeKeys() {
            return new HashSet<>(Arrays.asList(VertexComputeKey.of("computed", false), VertexComputeKey.of("temporary", true)));
        }
        @Override public Set<MemoryComputeKey> getMemoryComputeKeys() {
            // The pinned validator probes contains(null), which Set.of intentionally rejects.
            return new HashSet<>(Arrays.asList(MemoryComputeKey.of("seen", Operator.sum, true, false),
                    MemoryComputeKey.of("scratch", Operator.sum, false, true)));
        }
        @Override public Set<MessageScope> getMessageScopes(Memory memory) { return Collections.emptySet(); }
        @Override public CountingProgram clone() { return this; }
        @Override public GraphComputer.ResultGraph getPreferredResultGraph() { return GraphComputer.ResultGraph.ORIGINAL; }
        @Override public GraphComputer.Persist getPreferredPersist() { return GraphComputer.Persist.NOTHING; }
    }
}
