package io.crabgraph.gremlin;

import java.time.Duration;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;
import javax.script.Bindings;
import javax.script.SimpleBindings;
import org.apache.tinkerpop.gremlin.groovy.jsr223.GremlinGroovyScriptEngine;
import org.apache.tinkerpop.gremlin.groovy.jsr223.GroovyCompilerGremlinPlugin;
import org.apache.tinkerpop.gremlin.process.remote.RemoteConnection;
import org.apache.tinkerpop.gremlin.process.remote.traversal.AbstractRemoteTraversal;
import org.apache.tinkerpop.gremlin.process.remote.traversal.DefaultRemoteTraverser;
import org.apache.tinkerpop.gremlin.process.remote.traversal.RemoteTraversal;
import org.apache.tinkerpop.gremlin.process.traversal.Bytecode;
import org.apache.tinkerpop.gremlin.process.traversal.Traversal;
import org.apache.tinkerpop.gremlin.process.traversal.Traverser;

/**
 * Sequential JVM traversal session over a native Crabgraph store. Scripts are trusted application
 * code. Each submission is a native transaction: fully consume, commit, then deliver results.
 * Exceptions roll back. Cancellation invalidates the session and terminates its native process.
 * The executor owns the supplied graph; do not access it concurrently through another API.
 */
public final class CrabJvmExecutor implements AutoCloseable {
    private final CrabGraph graph;
    private final GremlinGroovyScriptEngine engine;
    private final ExecutorService worker = Executors.newSingleThreadExecutor(r -> {
        Thread t = new Thread(r, "crabgraph-jvm-session"); t.setDaemon(true); return t;
    });
    private final ScheduledExecutorService deadlines = Executors.newSingleThreadScheduledExecutor(r -> {
        Thread t = new Thread(r, "crabgraph-jvm-deadline"); t.setDaemon(true); return t;
    });
    private final AtomicBoolean closed = new AtomicBoolean();
    private final Set<CompletableFuture<?>> pending = ConcurrentHashMap.newKeySet();

    public CrabJvmExecutor(CrabGraph graph) {
        this.graph = Objects.requireNonNull(graph);
        engine = new GremlinGroovyScriptEngine(GroovyCompilerGremlinPlugin.build()
                .enableThreadInterrupt(true).create().getCustomizers("gremlin-groovy").orElseThrow());
    }

    public CompletableFuture<List<Object>> submit(String script, Map<String, Object> parameters, Duration timeout) {
        Objects.requireNonNull(script);
        Map<String,Object> copy = new HashMap<>(parameters);
        if (copy.containsKey("g") || copy.containsKey("graph"))
            throw new IllegalArgumentException("g and graph are reserved session bindings");
        return submitWork(() -> {
            Bindings bindings = new SimpleBindings(copy);
            bindings.put("g", graph.traversal());
            bindings.put("graph", graph);
            return consume(engine.eval(script, bindings));
        }, timeout);
    }

    public CompletableFuture<List<Object>> submit(Bytecode bytecode, Duration timeout) {
        Objects.requireNonNull(bytecode);
        // Clone before enqueuing so callers cannot append instructions while the worker executes.
        Bytecode request = bytecode.clone();
        return submitWork(() -> {
            Bindings bindings = new SimpleBindings();
            bindings.put("g", graph.traversal());
            return consume(engine.eval(request, bindings, "g"));
        }, timeout);
    }

    private static List<Object> consume(Object result) throws Exception {
        if (result instanceof Traversal<?,?> traversal) {
            try (traversal) {
                List<Object> values = new ArrayList<>();
                while (traversal.hasNext()) {
                    if (Thread.currentThread().isInterrupted()) throw new InterruptedException("Traversal cancelled");
                    values.add(traversal.next());
                }
                return values;
            }
        }
        return Collections.singletonList(result);
    }

    private CompletableFuture<List<Object>> submitWork(Callable<List<Object>> operation, Duration timeout) {
        if (timeout.isNegative() || timeout.isZero()) throw new IllegalArgumentException("Positive timeout required");
        if (closed.get()) return CompletableFuture.failedFuture(new IllegalStateException("Session is closed"));
        CompletableFuture<List<Object>> result = new CompletableFuture<>();
        pending.add(result);
        final Future<?> task;
        try {
            task = worker.submit(() -> {
                try {
                    if (closed.get()) throw new CancellationException("Session closed");
                    if (!graph.tx().isOpen()) graph.tx().open();
                    List<Object> values = operation.call();
                    if (closed.get() || Thread.currentThread().isInterrupted()) throw new InterruptedException("Submission cancelled");
                    graph.tx().commit();
                    result.complete(values);
                } catch (Throwable failure) {
                    if (!closed.get()) {
                        try { if (graph.tx().isOpen()) graph.tx().rollback(); }
                        catch (Throwable rollback) { failure.addSuppressed(rollback); abort(); }
                    }
                    // Session termination owns completion of pending requests. In particular,
                    // a deadline must become visible only after the session is invalidated.
                    if (!closed.get()) result.completeExceptionally(failure);
                }
            });
        } catch (RejectedExecutionException failure) {
            pending.remove(result); result.completeExceptionally(failure); return result;
        }
        ScheduledFuture<?> deadline = deadlines.schedule(() -> {
            if (!result.isDone()) {
                abort(result, new TimeoutException("JVM submission exceeded " + timeout));
            }
        }, timeout.toNanos(), TimeUnit.NANOSECONDS);
        result.whenComplete((value, failure) -> {
            deadline.cancel(false); pending.remove(result);
            if (result.isCancelled()) { task.cancel(true); abort(); }
        });
        return result;
    }

    /** Embedded asynchronous remote submission: upstream bytecode/lambdas execute on the session worker. */
    public RemoteConnection remoteConnection() {
        return new RemoteConnection() {
            @Override public <E> CompletableFuture<RemoteTraversal<?,E>> submitAsync(Bytecode bytecode) {
                return submit(bytecode, Duration.ofSeconds(30)).thenApply(values -> new AbstractRemoteTraversal<Object,E>() {
                    final Iterator<Object> rows = values.iterator();
                    @Override public boolean hasNext() { return rows.hasNext(); }
                    @SuppressWarnings("unchecked") @Override public E next() { return (E) rows.next(); }
                    @Override public Traverser.Admin<E> nextTraverser() { return new DefaultRemoteTraverser<>(next(), 1L); }
                    @Override public void close() { }
                });
            }
            @Override public void close() { CrabJvmExecutor.this.close(); }
        };
    }

    private void abort() {
        abort(null, null);
    }

    private void abort(CompletableFuture<?> expired, Throwable deadline) {
        if (closed.compareAndSet(false, true)) {
            graph.abortFamily();
            worker.shutdownNow(); deadlines.shutdownNow();
            for (CompletableFuture<?> item : pending)
                item.completeExceptionally(item == expired ? deadline : new CancellationException("Session terminated"));
        }
    }

    @Override public void close() {
        abort();
        try { worker.awaitTermination(5, TimeUnit.SECONDS); }
        catch (InterruptedException interrupted) { Thread.currentThread().interrupt(); }
        if (worker.isTerminated()) engine.reset();
    }
}
