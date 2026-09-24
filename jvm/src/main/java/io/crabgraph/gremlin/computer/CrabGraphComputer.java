package io.crabgraph.gremlin.computer;

import io.crabgraph.gremlin.CrabGraph;
import org.apache.tinkerpop.gremlin.process.computer.*;
import org.apache.tinkerpop.gremlin.process.computer.util.ComputerGraph;
import org.apache.tinkerpop.gremlin.process.computer.util.DefaultComputerResult;
import org.apache.tinkerpop.gremlin.process.computer.util.GraphComputerHelper;
import org.apache.tinkerpop.gremlin.process.traversal.Traversal;
import org.apache.tinkerpop.gremlin.process.traversal.util.TraversalInterruptedException;
import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.structure.util.StringFactory;

import java.util.*;
import java.util.concurrent.*;
import java.util.function.Supplier;

/**
 * Single-worker bulk-synchronous executor over a Crabgraph provider graph.
 * Vertex programs and traversal strategies are the pinned TinkerPop implementation;
 * graph reads and persisted results belong to the native provider. Computation-local
 * state is isolated in ComputerView until successful publication.
 */
@SuppressWarnings({"rawtypes", "unchecked"})
public final class CrabGraphComputer implements GraphComputer {
    private final Graph graph;
    private final Supplier<Graph> freshGraph;
    private final GraphFilter filter = new GraphFilter();
    private final Set<MapReduce> reducers = new LinkedHashSet<>();
    private VertexProgram program;
    private ResultGraph resultGraph;
    private Persist persist;
    private int workers = 1;
    private boolean submitted;

    public CrabGraphComputer(Graph graph, Supplier<Graph> freshGraph) {
        this.graph = Objects.requireNonNull(graph);
        this.freshGraph = Objects.requireNonNull(freshGraph);
    }
    @Override public GraphComputer result(ResultGraph result) { resultGraph = result; return this; }
    @Override public GraphComputer persist(Persist value) { persist = value; return this; }
    @Override public GraphComputer program(VertexProgram value) { program = value; return this; }
    @Override public GraphComputer mapReduce(MapReduce value) { reducers.add(value); return this; }
    @Override public GraphComputer workers(int value) {
        if (value < 1) throw new IllegalArgumentException("workers must be positive");
        workers = value; return this;
    }
    @Override public GraphComputer vertices(Traversal<Vertex, Vertex> value) { filter.setVertexFilter(value); return this; }
    @Override public GraphComputer edges(Traversal<Vertex, Edge> value) { filter.setEdgeFilter(value); return this; }
    @Override public GraphComputer vertexProperties(Traversal<Vertex, ? extends Property<?>> value) {
        filter.setVertexPropertyFilter(value); return this;
    }

    @Override public synchronized Future<ComputerResult> submit() {
        if (submitted) throw Exceptions.computerHasAlreadyBeenSubmittedAVertexProgram();
        submitted = true;
        if (program == null && reducers.isEmpty()) throw Exceptions.computerHasNoVertexProgramNorMapReducers();
        if (program != null) {
            GraphComputerHelper.validateProgramOnComputer(this, program);
            reducers.addAll(program.getMapReducers());
        }
        resultGraph = GraphComputerHelper.getResultGraphState(Optional.ofNullable(program), Optional.ofNullable(resultGraph));
        persist = GraphComputerHelper.getPersistState(Optional.ofNullable(program), Optional.ofNullable(persist));
        if (!features().supportsResultGraphPersistCombination(resultGraph, persist))
            throw Exceptions.resultGraphPersistCombinationNotSupported(resultGraph, persist);
        if (workers > features().getMaxWorkers())
            throw Exceptions.computerRequiresMoreWorkersThanSupported(workers, features().getMaxWorkers());
        // No executor is allocated on validation failure; the one-use executor is closed
        // immediately after scheduling, including when callers cancel the returned Future.
        ExecutorService service = Executors.newSingleThreadExecutor(r -> {
            Thread thread = new Thread(r, "crabgraph-computer");
            thread.setDaemon(true);
            return thread;
        });
        try { return service.submit(this::execute); }
        finally { service.shutdown(); }
    }

    private ComputerResult execute() throws Exception {
        try (AutoCloseable lease = graph instanceof CrabGraph ? ((CrabGraph) graph).executionLease() : () -> {}) {
            interrupted();
            return executeLeased();
        }
    }

    private ComputerResult executeLeased() throws Exception {
        long start = System.nanoTime();
        CrabMemory memory = new CrabMemory(program, reducers);
        try (ComputerView view = new ComputerView(graph, filter,
                program == null ? Collections.emptySet() : program.getVertexComputeKeys())) {
        CrabMessageBoard messages = new CrabMessageBoard();
        if (program != null) {
            program.setup(memory);
            // A worker clone has its own iteration lifecycle just as a remote worker does.
            VertexProgram worker = program.clone();
            CrabWorkerMemory workerMemory = new CrabWorkerMemory(memory);
            while (true) {
                interrupted();
                memory.completeSubRound();
                worker.workerIterationStart(workerMemory.asImmutable());
                Iterator<Vertex> vertices = view.vertices();
                while (vertices.hasNext()) {
                    interrupted();
                    Vertex vertex = vertices.next();
                    worker.execute(ComputerGraph.vertexProgram(vertex, worker),
                            new CrabMessenger(vertex, messages, worker.getMessageCombiner()), workerMemory);
                }
                worker.workerIterationEnd(workerMemory.asImmutable());
                workerMemory.complete();
                messages.completeIteration();
                memory.completeSubRound();
                boolean done = program.terminate(memory);
                memory.incrIteration();
                if (done) break;
            }
            view.complete();
        }
        for (MapReduce reducer : reducers) {
            interrupted();
            MapReduce worker = reducer.clone();
            CrabMapEmitter emitter = new CrabMapEmitter(reducer.doStage(MapReduce.Stage.REDUCE));
            if (worker.doStage(MapReduce.Stage.MAP)) {
                worker.workerStart(MapReduce.Stage.MAP);
                Iterator<Vertex> vertices = view.vertices();
                while (vertices.hasNext()) {
                    interrupted();
                    worker.map(ComputerGraph.mapReduce(vertices.next()), emitter);
                }
                worker.workerEnd(MapReduce.Stage.MAP);
            }
            emitter.complete(reducer);
            if (reducer.doStage(MapReduce.Stage.REDUCE)) {
                CrabReduceEmitter reduced = new CrabReduceEmitter();
                worker.workerStart(MapReduce.Stage.REDUCE);
                for (Object raw : emitter.reduceMap.entrySet()) {
                    interrupted();
                    Map.Entry entry = (Map.Entry) raw;
                    worker.reduce(entry.getKey(), ((Queue) entry.getValue()).iterator(), reduced);
                }
                worker.workerEnd(MapReduce.Stage.REDUCE);
                reduced.complete(reducer);
                reducer.addResultToMemory(memory, reduced.reduceQueue.iterator());
            } else {
                reducer.addResultToMemory(memory, emitter.mapQueue.iterator());
            }
        }
        interrupted();
        memory.setRuntime(TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - start));
        memory.complete();
        if (program == null) memory.setIteration(0);
        Graph result = view.finish(resultGraph, persist, freshGraph);
        return new DefaultComputerResult(result, memory.asImmutable());
        }
    }

    private static void interrupted() { if (Thread.currentThread().isInterrupted()) throw new TraversalInterruptedException(); }
    @Override public String toString() { return StringFactory.graphComputerString(this); }
    @Override public Features features() {
        return new Features() {
            @Override public int getMaxWorkers() { return 1; }
            @Override public boolean supportsVertexAddition() { return false; }
            @Override public boolean supportsVertexRemoval() { return false; }
            @Override public boolean supportsVertexPropertyRemoval() { return false; }
            @Override public boolean supportsEdgeAddition() { return false; }
            @Override public boolean supportsEdgeRemoval() { return false; }
            @Override public boolean supportsEdgePropertyAddition() { return false; }
            @Override public boolean supportsEdgePropertyRemoval() { return false; }
        };
    }
}
