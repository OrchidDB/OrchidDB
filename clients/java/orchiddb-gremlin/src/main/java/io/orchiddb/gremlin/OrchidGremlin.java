package io.orchiddb.gremlin;

import io.orchiddb.OrchidDB;
import io.orchiddb.Query;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.Objects;
import java.util.concurrent.CompletableFuture;
import org.apache.tinkerpop.gremlin.process.remote.RemoteConnection;
import org.apache.tinkerpop.gremlin.process.remote.traversal.AbstractRemoteTraversal;
import org.apache.tinkerpop.gremlin.process.remote.traversal.DefaultRemoteTraverser;
import org.apache.tinkerpop.gremlin.process.remote.traversal.RemoteTraversal;
import org.apache.tinkerpop.gremlin.process.traversal.AnonymousTraversalSource;
import org.apache.tinkerpop.gremlin.process.traversal.Bytecode;
import org.apache.tinkerpop.gremlin.process.traversal.Traverser;
import org.apache.tinkerpop.gremlin.process.traversal.dsl.graph.GraphTraversalSource;

/** Optional, in-process TinkerPop adapter. Does not own the graph's JDBC connection or pool. */
public final class OrchidGremlin {
  private OrchidGremlin() {}

  /** Buffers up to 100,000 scalar results per submitted traversal. */
  public static GraphTraversalSource traversal(OrchidDB.Graph graph) {
    return traversal(graph, 100_000);
  }

  /**
   * Creates a traversal source with a result-count limit. Submission runs on the calling thread.
   * Results are fully read and JDBC resources released before they are handed to TinkerPop, so
   * next(), partial iteration and abandoned traversals cannot leak a database lease.
   */
  public static GraphTraversalSource traversal(OrchidDB.Graph graph, int maxResults) {
    if (maxResults < 1) throw new IllegalArgumentException("maxResults must be positive");
    return AnonymousTraversalSource.traversal()
        .withRemote(new Connection(Objects.requireNonNull(graph), maxResults));
  }

  private static final class Connection implements RemoteConnection {
    private final OrchidDB.Graph graph;
    private final int maxResults;
    private boolean closed;

    Connection(OrchidDB.Graph graph, int maxResults) {
      this.graph = graph;
      this.maxResults = maxResults;
    }

    @Override
    public synchronized <E> CompletableFuture<RemoteTraversal<?, E>> submitAsync(
        Bytecode bytecode) {
      try {
        if (closed) throw new IllegalStateException("OrchidDB traversal source is closed");
        String text = BytecodeQuery.translate(bytecode);
        var values = new ArrayList<E>();
        try (var result = graph.query(Query.gremlin(text))) {
          if (result.columns().size() != 1)
            throw new UnsupportedOperationException("Expected one scalar Gremlin result column");
          while (result.next()) {
            if (values.size() == maxResults)
              throw new IllegalStateException(
                  "Gremlin result limit exceeded ("
                      + maxResults
                      + "); use limit()/range() or raise maxResults");
            Object value = result.get(1);
            if (value != null
                && !(value instanceof String
                    || value instanceof Number
                    || value instanceof Boolean))
              throw new UnsupportedOperationException(
                  "Unsupported Gremlin result type: " + value.getClass().getName());
            @SuppressWarnings("unchecked")
            E typed = (E) value;
            values.add(typed);
          }
        }
        return CompletableFuture.completedFuture(new Results<>(values.iterator()));
      } catch (Exception e) {
        return CompletableFuture.failedFuture(e);
      }
    }

    @Override
    public synchronized void close() {
      closed = true;
    }
  }

  private static final class Results<E> extends AbstractRemoteTraversal<Object, E> {
    private final Iterator<E> values;

    Results(Iterator<E> values) {
      this.values = values;
    }

    @Override
    public boolean hasNext() {
      return values.hasNext();
    }

    @Override
    public E next() {
      return values.next();
    }

    @Override
    public Traverser.Admin<E> nextTraverser() {
      // SQL emits one row per occurrence. Never deduplicate or infer bulk from equal values.
      return new DefaultRemoteTraverser<>(values.next(), 1L);
    }
  }
}
