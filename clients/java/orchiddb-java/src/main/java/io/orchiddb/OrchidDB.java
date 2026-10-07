package io.orchiddb;

import java.sql.SQLException;
import java.util.*;
import org.apache.arrow.vector.VectorSchemaRoot;
import org.apache.arrow.vector.dictionary.DictionaryProvider;
import org.apache.arrow.vector.types.pojo.Schema;

/** Immutable engine registry. Owns no engines, pools or connections. */
public final class OrchidDB {
  private final SqlCompiler compiler;
  private final PlanCache cache;
  private final Map<String, ExecutionEngine> engines;

  public OrchidDB(SqlCompiler compiler, PlanCache cache, ExecutionEngine... engines) {
    this.compiler = Objects.requireNonNull(compiler);
    this.cache = Objects.requireNonNull(cache);
    var map = new HashMap<String, ExecutionEngine>();
    for (var engine : engines)
      if (map.putIfAbsent(engine.id(), engine) != null)
        throw new IllegalArgumentException("Duplicate engine " + engine.id());
    this.engines = Map.copyOf(map);
  }

  public Graph graph(GraphMapping mapping) {
    return graph(mapping, List.of());
  }

  public Graph graph(GraphMapping mapping, List<FunctionSignature> functions) {
    String id = mapping.singleEngine();
    var engine = engines.get(id);
    if (engine == null) throw new PlanningException("Unknown engine " + id);
    return new Graph(engine, mapping, List.copyOf(functions));
  }

  public final class Graph implements AutoCloseable {
    private final ExecutionEngine engine;
    private final GraphMapping mapping;
    private final List<FunctionSignature> functions;
    private Statistics statistics;

    private Graph(ExecutionEngine e, GraphMapping m, List<FunctionSignature> f) {
      engine = e;
      mapping = m;
      functions = f;
    }

    private synchronized CompiledQuery compile(ExecutionEngine.Session session, Query query)
        throws SQLException {
      var request =
          new Compilation(
              engine.id(),
              engine.dialect(),
              mapping,
              session.schemas(mapping),
              functions,
              query,
              statistics == null ? null : statistics.catalogId());
      var plan = cache.get(request);
      if (plan == null) {
        plan = compiler.compile(request);
        validate(plan);
        cache.put(request, plan);
      }
      validate(plan);
      return plan;
    }

    private void validate(CompiledQuery plan) {
      if (!plan.engine().equals(engine.id()) || !plan.dialect().equals(engine.dialect()))
        throw new PlanningException("Compiler/cache returned a plan for another engine");
    }

    /** Generate once; successful regeneration atomically replaces the previous snapshot. */
    public synchronized String generateStatistics() throws SQLException {
      return generateStatistics(Query.cypher("RETURN 1"));
    }

    /** Include RDF dataset/rule metadata when generating statistics for a SPARQL mapping. */
    public synchronized String generateStatistics(Query metadataQuery) throws SQLException {
      try (var session = engine.openSession()) {
        var metadata =
            new Compilation(
                engine.id(),
                engine.dialect(),
                mapping,
                session.schemas(mapping),
                functions,
                metadataQuery);
        var generated = Statistics.generate(compiler, metadata, session);
        replaceStatistics(generated);
        return generated.reportJson();
      }
    }

    public synchronized void saveStatistics(java.nio.file.Path path) throws java.io.IOException {
      if (statistics == null) throw new IllegalStateException("No statistics generated");
      statistics.save(path);
    }

    public synchronized void loadStatistics(java.nio.file.Path path) throws java.io.IOException {
      replaceStatistics(Statistics.load(compiler, path));
    }

    public synchronized String statisticsSnapshotJson() {
      return statistics == null ? null : statistics.snapshotJson();
    }

    public synchronized void clearStatistics() {
      replaceStatistics(null);
    }

    private void replaceStatistics(Statistics replacement) {
      var previous = statistics;
      statistics = replacement;
      if (previous != null) previous.close();
    }

    /** Release this graph's statistics handle; caller-owned engines remain open. */
    public void close() {
      clearStatistics();
    }

    public CompiledQuery plan(Query query) throws SQLException {
      try (var session = engine.openSession()) {
        return compile(session, query);
      }
    }

    /** Primary bulk API. Close the result before closing its caller-owned allocator. */
    public ArrowResult queryArrow(Query query) throws SQLException {
      var session = engine.openSession();
      try {
        var result = session.executeArrow(compile(session, query));
        return new ArrowResult() {
          private boolean closed;

          private void checkOpen() throws SQLException {
            if (closed) throw new SQLException("Arrow result is closed");
          }

          public Schema schema() throws SQLException {
            checkOpen();
            return result.schema();
          }

          public DictionaryProvider dictionaries() throws SQLException {
            checkOpen();
            return result.dictionaries();
          }

          public VectorSchemaRoot batch() throws SQLException {
            checkOpen();
            return result.batch();
          }

          public boolean nextBatch() throws SQLException {
            checkOpen();
            try {
              return result.nextBatch();
            } catch (SQLException | RuntimeException | Error e) {
              try {
                close();
              } catch (Throwable cleanup) {
                e.addSuppressed(cleanup);
              }
              throw e;
            }
          }

          public void close() throws SQLException {
            if (closed) return;
            closed = true;
            ResultResources.close(result, session);
          }
        };
      } catch (SQLException | RuntimeException | Error e) {
        try {
          session.close();
        } catch (Throwable cleanup) {
          e.addSuppressed(cleanup);
        }
        throw e;
      }
    }

    public QueryResult query(Query query) throws SQLException {
      var session = engine.openSession();
      try {
        var result = session.execute(compile(session, query));
        return new QueryResult() {
          private boolean closed;

          public List<String> columns() {
            return result.columns();
          }

          public boolean next() throws SQLException {
            try {
              return result.next();
            } catch (SQLException | RuntimeException | Error e) {
              try {
                close();
              } catch (Throwable cleanup) {
                e.addSuppressed(cleanup);
              }
              throw e;
            }
          }

          public Object get(int column) throws SQLException {
            return result.get(column);
          }

          public void close() throws SQLException {
            if (closed) return;
            closed = true;
            try {
              result.close();
            } catch (SQLException | RuntimeException | Error e) {
              try {
                session.close();
              } catch (Exception x) {
                e.addSuppressed(x);
              }
              throw e;
            }
            session.close();
          }
        };
      } catch (SQLException | RuntimeException | Error e) {
        try {
          session.close();
        } catch (Exception x) {
          e.addSuppressed(x);
        }
        throw e;
      }
    }
  }
}
