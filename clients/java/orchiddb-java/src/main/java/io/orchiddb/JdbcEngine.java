package io.orchiddb;

import java.sql.*;
import java.util.*;
import java.util.concurrent.atomic.AtomicBoolean;
import javax.sql.DataSource;
import org.apache.arrow.memory.BufferAllocator;
import org.apache.arrow.vector.VectorSchemaRoot;
import org.apache.arrow.vector.dictionary.DictionaryProvider;
import org.apache.arrow.vector.ipc.ArrowReader;
import org.apache.arrow.vector.types.pojo.Schema;

/** JDBC execution adapter. Borrowing preserves the exact session supplied by the application. */
public final class JdbcEngine implements ExecutionEngine {
  private static final java.util.concurrent.ScheduledExecutorService STATISTICS_TIMER =
      java.util.concurrent.Executors.newSingleThreadScheduledExecutor(
          r -> {
            var thread = new Thread(r, "orchiddb-statistics-deadlines");
            thread.setDaemon(true);
            return thread;
          });
  private final String id;
  private final SqlDialect dialect;
  private final Connection borrowed;
  private final DataSource pool;
  private final AtomicBoolean inUse;
  private final ArrowExporter arrowExporter;
  private final BufferAllocator arrowAllocator;
  private final int arrowBatchSize;
  private final int fetchSize;
  private final int timeoutSeconds;

  private JdbcEngine(
      String id,
      SqlDialect dialect,
      Connection borrowed,
      DataSource pool,
      int fetchSize,
      int timeoutSeconds) {
    this.inUse = new AtomicBoolean();
    this.arrowExporter = null;
    this.arrowAllocator = null;
    this.arrowBatchSize = 0;
    this.id = Checks.name(id);
    this.dialect = Objects.requireNonNull(dialect);
    this.borrowed = borrowed;
    this.pool = pool;
    if (fetchSize < 0 || timeoutSeconds < 0)
      throw new IllegalArgumentException("Negative execution setting");
    this.fetchSize = fetchSize;
    this.timeoutSeconds = timeoutSeconds;
  }

  private JdbcEngine(
      JdbcEngine base, ArrowExporter exporter, BufferAllocator allocator, int batchSize) {
    if (batchSize <= 0) throw new IllegalArgumentException("Arrow batch size must be positive");
    id = base.id;
    dialect = base.dialect;
    borrowed = base.borrowed;
    pool = base.pool;
    fetchSize = base.fetchSize;
    timeoutSeconds = base.timeoutSeconds;
    inUse = base.inUse;
    arrowExporter = Objects.requireNonNull(exporter);
    arrowAllocator = Objects.requireNonNull(allocator);
    arrowBatchSize = batchSize;
  }

  /** Configure native Arrow export. The caller owns the parent allocator and driver. */
  public JdbcEngine withArrow(ArrowExporter exporter, BufferAllocator allocator, int batchSize) {
    return new JdbcEngine(this, exporter, allocator, batchSize);
  }

  public static JdbcEngine borrowed(String id, SqlDialect dialect, Connection connection) {
    return new JdbcEngine(id, dialect, Objects.requireNonNull(connection), null, 0, 0);
  }

  public static JdbcEngine pooled(String id, SqlDialect dialect, DataSource pool) {
    return new JdbcEngine(id, dialect, null, Objects.requireNonNull(pool), 0, 0);
  }

  public static JdbcEngine borrowed(
      String id, SqlDialect dialect, Connection connection, int fetchSize, int timeoutSeconds) {
    return new JdbcEngine(
        id, dialect, Objects.requireNonNull(connection), null, fetchSize, timeoutSeconds);
  }

  public static JdbcEngine pooled(
      String id, SqlDialect dialect, DataSource pool, int fetchSize, int timeoutSeconds) {
    return new JdbcEngine(
        id, dialect, null, Objects.requireNonNull(pool), fetchSize, timeoutSeconds);
  }

  public String id() {
    return id;
  }

  public SqlDialect dialect() {
    return dialect;
  }

  public Session openSession() throws SQLException {
    if (borrowed != null && !inUse.compareAndSet(false, true))
      throw new SQLException(
          "Engine "
              + id
              + " already has an active session; close its result before reusing this borrowed connection");
    try {
      return new JdbcSession(borrowed != null ? borrowed : pool.getConnection());
    } catch (SQLException | RuntimeException | Error e) {
      if (borrowed != null) inUse.set(false);
      throw e;
    }
  }

  private final class JdbcSession implements Session {
    private final Connection connection;
    private final Set<Cursor> cursors = Collections.newSetFromMap(new IdentityHashMap<>());
    private final Set<ArrowResult> arrowResults =
        Collections.newSetFromMap(new IdentityHashMap<>());
    private boolean closed;

    JdbcSession(Connection connection) {
      this.connection = Objects.requireNonNull(connection);
    }

    private void checkOpen() throws SQLException {
      if (closed) throw new SQLException("Session is closed");
    }

    public Map<Source, List<Column>> schemas(Set<Source> sources) throws SQLException {
      return schemas(sources, Map.of());
    }

    public Map<Source, List<Column>> schemas(GraphMapping mapping) throws SQLException {
      var selected = new LinkedHashMap<Source, Set<String>>();
      for (var node : mapping.nodes()) {
        var columns = selected.computeIfAbsent(node.source(), ignored -> new LinkedHashSet<>());
        columns.add(node.id());
        columns.addAll(node.properties().values());
      }
      for (var edge : mapping.edges()) {
        var columns = selected.computeIfAbsent(edge.source(), ignored -> new LinkedHashSet<>());
        columns.add(edge.id());
        columns.add(edge.sourceColumn());
        columns.add(edge.targetColumn());
        columns.addAll(edge.properties().values());
      }
      return schemas(mapping.sources(), selected);
    }

    private Map<Source, List<Column>> schemas(
        Set<Source> sources, Map<Source, Set<String>> selected) throws SQLException {
      checkOpen();
      var result = new LinkedHashMap<Source, List<Column>>();
      for (var source : sources) {
        if (!source.engine().equals(id))
          throw new SQLException("Source is bound to a different engine");
        // Metadata is discovered in this session. No views, tables or transactions are changed.
        try (var statement = connection.createStatement()) {
          if (timeoutSeconds > 0) statement.setQueryTimeout(timeoutSeconds);
          String projection =
              selected.containsKey(source)
                  ? selected.get(source).stream()
                      .map(dialect::quote)
                      .collect(java.util.stream.Collectors.joining(", "))
                  : "*";
          try (var rows =
              statement.executeQuery(
                  "SELECT " + projection + " FROM " + source.sql(dialect) + " WHERE 1 = 0")) {
            var meta = rows.getMetaData();
            var columns = new ArrayList<Column>();
            for (int i = 1; i <= meta.getColumnCount(); i++)
              columns.add(
                  new Column(
                      meta.getColumnName(i),
                      type(meta, i),
                      meta.isNullable(i) != ResultSetMetaData.columnNoNulls));
            result.put(source, List.copyOf(columns));
          }
        }
      }
      return Collections.unmodifiableMap(result);
    }

    public ArrowResult executeArrow(CompiledQuery query) throws SQLException {
      checkOpen();
      FederatedQuery.requireSingleEngine(query);
      if (!query.engine().equals(id) || !query.dialect().equals(dialect))
        throw new SQLException("Plan is bound to a different engine or dialect");
      if (arrowExporter == null)
        throw new SQLFeatureNotSupportedException(
            "Configure JdbcEngine.withArrow with your driver's native exporter");
      Statement statement = null;
      ResultSet rows = null;
      BufferAllocator allocator = null;
      ArrowReader reader = null;
      try {
        allocator =
            arrowAllocator.newChildAllocator("orchiddb-result", 0, arrowAllocator.getLimit());
        statement = connection.createStatement();
        if (fetchSize > 0) statement.setFetchSize(fetchSize);
        if (timeoutSeconds > 0) statement.setQueryTimeout(timeoutSeconds);
        rows = statement.executeQuery(query.sql());
        reader = Objects.requireNonNull(arrowExporter.export(rows, allocator, arrowBatchSize));
        var result = new NativeArrowResult(reader, allocator, rows, statement);
        arrowResults.add(result);
        return result;
      } catch (Exception | Error e) {
        try {
          ResultResources.close(reader, rows, statement, allocator);
        } catch (Throwable cleanup) {
          e.addSuppressed(cleanup);
        }
        if (e instanceof SQLException sql) throw sql;
        if (e instanceof RuntimeException runtime) throw runtime;
        if (e instanceof Error error) throw error;
        throw new SQLException("Could not export Arrow results", e);
      }
    }

    public QueryResult execute(CompiledQuery query) throws SQLException {
      FederatedQuery.requireSingleEngine(query);
      if (arrowExporter != null) return executeArrow(query).rows();
      checkOpen();
      if (!query.engine().equals(id) || !query.dialect().equals(dialect))
        throw new SQLException("Plan is bound to a different engine or dialect");
      var statement = connection.createStatement();
      try {
        if (fetchSize > 0) statement.setFetchSize(fetchSize);
        if (timeoutSeconds > 0) statement.setQueryTimeout(timeoutSeconds);
        var rows = statement.executeQuery(query.sql());
        var cursor = new Cursor(statement, rows);
        cursors.add(cursor);
        return cursor;
      } catch (SQLException | RuntimeException | Error e) {
        try {
          statement.close();
        } catch (SQLException x) {
          e.addSuppressed(x);
        }
        throw e;
      }
    }

    public QueryResult readStatistics(StatisticsRead request) throws SQLException {
      checkOpen();
      var statement = connection.createStatement();
      var owner = Thread.currentThread();
      var expired = new AtomicBoolean();
      long deadline = System.nanoTime() + request.timeoutMillis() * 1_000_000L;
      var cancellation =
          STATISTICS_TIMER.scheduleAtFixedRate(
              () -> {
                if (owner.isInterrupted() || System.nanoTime() >= deadline) {
                  expired.set(true);
                  try {
                    statement.cancel();
                  } catch (SQLException ignored) {
                  }
                }
              },
              0,
              20,
              java.util.concurrent.TimeUnit.MILLISECONDS);
      try {
        statement.setMaxRows((int) Math.min(Integer.MAX_VALUE, request.maxRows()));
        statement.setFetchSize((int) Math.min(256, request.maxRows()));
        try {
          statement.setQueryTimeout((int) Math.max(1, (request.timeoutMillis() + 999) / 1000));
        } catch (SQLFeatureNotSupportedException ignored) {
          /* Cancellation timer enforces deadline. */
        }
        var rows = statement.executeQuery(request.sql());
        var cursor = new Cursor(statement, rows, () -> cancellation.cancel(false));
        cursors.add(cursor);
        return new QueryResult() {
          private long count;

          public List<String> columns() {
            return cursor.columns();
          }

          public boolean next() throws SQLException {
            if (expired.get() || owner.isInterrupted())
              throw new SQLException("Statistics read cancelled or timed out");
            if (count >= request.maxRows()) return false;
            boolean more = cursor.next();
            if (more) count++;
            return more;
          }

          public Object get(int column) throws SQLException {
            return cursor.get(column);
          }

          public void close() throws SQLException {
            cancellation.cancel(false);
            cursor.close();
          }
        };
      } catch (SQLException | RuntimeException | Error failure) {
        cancellation.cancel(false);
        try {
          statement.close();
        } catch (SQLException cleanup) {
          failure.addSuppressed(cleanup);
        }
        throw failure;
      }
    }

    public void close() throws SQLException {
      if (closed) return;
      closed = true;
      var resources = new ArrayList<AutoCloseable>();
      resources.addAll(arrowResults);
      resources.addAll(cursors);
      if (borrowed == null) resources.add(connection);
      try {
        ResultResources.close(resources.toArray(AutoCloseable[]::new));
      } finally {
        if (borrowed != null) inUse.set(false);
      }
    }

    private final class NativeArrowResult implements ArrowResult {
      private final ArrowReader reader;
      private final BufferAllocator allocator;
      private final ResultSet rows;
      private final Statement statement;
      private final VectorSchemaRoot root;
      private boolean closed;
      private boolean current;
      private boolean finished;

      NativeArrowResult(
          ArrowReader reader, BufferAllocator allocator, ResultSet rows, Statement statement)
          throws java.io.IOException {
        this.reader = reader;
        this.allocator = allocator;
        this.rows = rows;
        this.statement = statement;
        root = reader.getVectorSchemaRoot();
      }

      private void checkOpen() throws SQLException {
        if (closed) throw new SQLException("Arrow result is closed");
      }

      public Schema schema() throws SQLException {
        checkOpen();
        return root.getSchema();
      }

      public DictionaryProvider dictionaries() throws SQLException {
        checkOpen();
        return reader;
      }

      public VectorSchemaRoot batch() throws SQLException {
        checkOpen();
        if (!current) throw new SQLException("No current Arrow batch; call nextBatch first");
        return root;
      }

      public boolean nextBatch() throws SQLException {
        checkOpen();
        current = false;
        if (finished) return false;
        try {
          current = reader.loadNextBatch();
          finished = !current;
          return current;
        } catch (Exception | Error e) {
          try {
            close();
          } catch (Throwable cleanup) {
            e.addSuppressed(cleanup);
          }
          if (e instanceof RuntimeException runtime) throw runtime;
          if (e instanceof Error error) throw error;
          throw new SQLException("Could not read Arrow batch", e);
        }
      }

      public void close() throws SQLException {
        if (closed) return;
        closed = true;
        current = false;
        arrowResults.remove(this);
        ResultResources.close(reader, rows, statement, allocator);
      }
    }

    private final class Cursor implements QueryResult {
      private final Statement statement;
      private final ResultSet rows;
      private final List<String> columns;
      private boolean closed;
      private final Runnable onClose;

      Cursor(Statement statement, ResultSet rows) throws SQLException {
        this(statement, rows, () -> {});
      }

      Cursor(Statement statement, ResultSet rows, Runnable onClose) throws SQLException {
        this.onClose = onClose;
        this.statement = statement;
        this.rows = rows;
        var labels = new ArrayList<String>();
        var metadata = rows.getMetaData();
        for (int i = 1; i <= metadata.getColumnCount(); i++) labels.add(metadata.getColumnLabel(i));
        columns = List.copyOf(labels);
      }

      public List<String> columns() {
        return columns;
      }

      public boolean next() throws SQLException {
        if (closed) throw new SQLException("Result is closed");
        return rows.next();
      }

      public Object get(int column) throws SQLException {
        if (closed) throw new SQLException("Result is closed");
        return rows.getObject(column);
      }

      public void close() throws SQLException {
        if (closed) return;
        closed = true;
        onClose.run();
        cursors.remove(this);
        try {
          rows.close();
        } catch (SQLException e) {
          try {
            statement.close();
          } catch (SQLException x) {
            e.addSuppressed(x);
          }
          throw e;
        }
        statement.close();
      }
    }
  }

  private static String type(ResultSetMetaData m, int i) throws SQLException {
    // JDBC's integer code alone cannot distinguish DuckDB unsigned values.
    String nativeType = m.getColumnTypeName(i).toUpperCase(Locale.ROOT);
    if (nativeType.startsWith("U")
        && Set.of("UTINYINT", "USMALLINT", "UINTEGER", "UBIGINT", "UHUGEINT").contains(nativeType))
      throw new SQLException(
          "Unsupported unsigned column " + m.getColumnName(i) + "; cast it in a view");
    if (m.getColumnType(i) == Types.TIMESTAMP
        && (nativeType.equals("TIMESTAMP_NS") || m.getScale(i) > 6)) {
      throw new SQLException(
          "Timestamp column "
              + m.getColumnName(i)
              + " exceeds microsecond precision; cast it explicitly in a view");
    }
    return switch (m.getColumnType(i)) {
      case Types.BOOLEAN, Types.BIT -> "boolean";
      case Types.TINYINT -> "int8";
      case Types.SMALLINT -> "int16";
      case Types.INTEGER -> "int32";
      case Types.BIGINT -> "int64";
      case Types.REAL -> "float32";
      case Types.FLOAT, Types.DOUBLE -> "float64";
      case Types.CHAR,
              Types.VARCHAR,
              Types.LONGVARCHAR,
              Types.NCHAR,
              Types.NVARCHAR,
              Types.LONGNVARCHAR ->
          "string";
      case Types.BINARY, Types.VARBINARY, Types.LONGVARBINARY -> "binary";
      case Types.DATE -> "date";
      case Types.TIMESTAMP -> "timestamp";
      case Types.NUMERIC, Types.DECIMAL -> "decimal:" + m.getPrecision(i) + ":" + m.getScale(i);
      default ->
          throw new SQLException(
              "Unsupported JDBC column "
                  + m.getColumnName(i)
                  + " ("
                  + nativeType
                  + "); cast it in a source view");
    };
  }
}
