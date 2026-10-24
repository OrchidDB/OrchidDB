package demo;

import io.trino.spi.Plugin;
import io.trino.spi.Page;
import io.trino.spi.PageBuilder;
import io.trino.spi.TrinoException;
import io.trino.spi.connector.*;
import io.trino.spi.statistics.Estimate;
import io.trino.spi.statistics.TableStatistics;
import io.trino.spi.transaction.IsolationLevel;
import io.trino.spi.type.Type;
import org.apache.arrow.memory.RootAllocator;
import org.apache.arrow.vector.BigIntVector;
import org.apache.arrow.vector.Float8Vector;
import org.apache.arrow.vector.VectorSchemaRoot;
import org.apache.arrow.vector.ipc.ArrowStreamReader;

import java.io.IOException;
import java.net.HttpURLConnection;
import java.net.URI;
import java.util.*;

import static io.trino.spi.StandardErrorCode.GENERIC_INTERNAL_ERROR;
import static io.trino.spi.StandardErrorCode.NOT_SUPPORTED;
import static io.trino.spi.type.BigintType.BIGINT;
import static io.trino.spi.type.DoubleType.DOUBLE;

/** Minimal read-only connector. All relational operations stay in Trino. */
public final class ArrowPlugin implements Plugin {
    private static final SchemaTableName MATCHES = new SchemaTableName("default", "matches");
    private static final List<String> NAMES = List.of("source_id", "document_id", "score");
    private static final List<Type> TYPES = List.of(BIGINT, BIGINT, DOUBLE);
    private static final List<ColumnMetadata> COLUMNS = List.of(
        new ColumnMetadata("source_id", BIGINT), new ColumnMetadata("document_id", BIGINT),
        new ColumnMetadata("score", DOUBLE));

    // Records are serializable handles transported from coordinator to workers.
    // Benchmark table names encode rows, Arrow batch rows, partitions, latency ms.
    private static long[] benchmark(String name) {
        if (!name.startsWith("bench_")) { return null; }
        try {
            String[] fields = name.substring(6).split("_");
            if (fields.length != 4) { return null; }
            long[] a = Arrays.stream(fields).mapToLong(Long::parseLong).toArray();
            if (a[0] < 1 || a[0] > 1_000_000_000 || a[1] < 1 || a[1] > 65536 || a[2] < 1 || a[2] > 16 || a[3] < 0 || a[3] > 1000) { return null; }
            return a;
        } catch (NumberFormatException e) { return null; }
    }
    public record Table(String name) implements ConnectorTableHandle {}
    public record Column(int index) implements ColumnHandle {}
    public record Split(int partition, String table) implements ConnectorSplit {}
    public enum Transaction implements ConnectorTransactionHandle { INSTANCE }

    @Override public Iterable<ConnectorFactory> getConnectorFactories() {
        return List.of(new ConnectorFactory() {
            @Override public String getName() { return "arrow_demo"; }
            @Override public Connector create(String catalog, Map<String, String> config, ConnectorContext context) {
                if (!config.keySet().equals(Set.of("arrow.endpoint"))) {
                    throw new IllegalArgumentException("Only arrow.endpoint is supported");
                }
                URI endpoint = URI.create(config.get("arrow.endpoint"));
                if (!Set.of("http", "https").contains(endpoint.getScheme()) || endpoint.getHost() == null) {
                    throw new IllegalArgumentException("arrow.endpoint must be an HTTP(S) URL");
                }
                return new ArrowConnector(endpoint);
            }
        });
    }

    private static final class ArrowConnector implements Connector {
        private final URI endpoint;
        ArrowConnector(URI endpoint) { this.endpoint = endpoint; }
        @Override public ConnectorTransactionHandle beginTransaction(IsolationLevel isolation, boolean readOnly, boolean autoCommit) {
            return Transaction.INSTANCE;
        }
        @Override public ConnectorMetadata getMetadata(ConnectorSession session, ConnectorTransactionHandle transaction) {
            return new Metadata();
        }
        @Override public ConnectorSplitManager getSplitManager() {
            return (transaction, session, table, columns, constraint) -> {
                String name = ((Table)table).name();
                long[] config = benchmark(name);
                int parts = config == null ? 2 : (int)config[2];
                List<Split> splits = new ArrayList<>();
                for (int p = 0; p < parts; p++) { splits.add(new Split(p, name)); }
                return new FixedSplitSource(splits);
            };
        }
        @Override public ConnectorPageSourceProvider getPageSourceProvider() {
            return new ConnectorPageSourceProvider() {
                @Override public ConnectorPageSource createPageSource(
                        ConnectorTransactionHandle transaction, ConnectorSession session, ConnectorSplit split,
                        ConnectorTableHandle table, Optional<ConnectorTableCredentials> credentials,
                        List<ColumnHandle> columns, DynamicFilter dynamicFilter) {
                    Split partition = (Split) split;
                    long[] config = benchmark(partition.table());
                    String path = config == null ? "/" + partition.table() + "/" + partition.partition()
                        : "/bench/" + config[0] + "/" + config[1] + "/" + config[2] + "/" + partition.partition() + "/" + config[3];
                    URI uri = endpoint.resolve(path);
                    return new ArrowSource(uri, columns.stream().map(Column.class::cast).toList());
                }
            };
        }
        @Override public void shutdown() {}
    }

    private static final class Metadata implements ConnectorMetadata {
        @Override public List<String> listSchemaNames(ConnectorSession session) { return List.of("default"); }
        @Override public List<SchemaTableName> listTables(ConnectorSession session, Optional<String> schema) {
            if (schema.isPresent() && !schema.get().equals("default")) { return List.of(); }
            return List.of(MATCHES, new SchemaTableName("default", "broken"));
        }
        @Override public ConnectorTableHandle getTableHandle(ConnectorSession session, SchemaTableName name,
                Optional<ConnectorTableVersion> start, Optional<ConnectorTableVersion> end) {
            if (start.isPresent() || end.isPresent()) { throw new TrinoException(NOT_SUPPORTED, "No time travel in fixture"); }
            boolean bench = name.getSchemaName().equals("default") && benchmark(name.getTableName()) != null;
            return bench || listTables(session, Optional.of(name.getSchemaName())).contains(name) ? new Table(name.getTableName()) : null;
        }
        @Override public ConnectorTableMetadata getTableMetadata(ConnectorSession session, ConnectorTableHandle table) {
            return new ConnectorTableMetadata(new SchemaTableName("default", ((Table)table).name()), COLUMNS);
        }
        @Override public Map<String, ColumnHandle> getColumnHandles(ConnectorSession session, ConnectorTableHandle table) {
            Map<String, ColumnHandle> result = new LinkedHashMap<>();
            for (int i = 0; i < NAMES.size(); i++) { result.put(NAMES.get(i), new Column(i)); }
            return result;
        }
        @Override public ColumnMetadata getColumnMetadata(ConnectorSession session, ConnectorTableHandle table, ColumnHandle column) {
            return COLUMNS.get(((Column)column).index());
        }
        @Override public Map<SchemaTableName, List<ColumnMetadata>> listTableColumns(ConnectorSession session, SchemaTablePrefix prefix) {
            Map<SchemaTableName, List<ColumnMetadata>> result = new LinkedHashMap<>();
            for (SchemaTableName table : listTables(session, prefix.getSchema())) {
                if (prefix.matches(table)) { result.put(table, COLUMNS); }
            }
            return result;
        }
        @Override public TableStatistics getTableStatistics(ConnectorSession session, ConnectorTableHandle table) {
            // Actual fixture cardinality, rather than pretending the stream has one row.
            long[] config = benchmark(((Table)table).name());
            return TableStatistics.builder().setRowCount(Estimate.of(config == null ? 12 : config[0])).build();
        }
        // No applyFilter/applyJoin overrides: Trino evaluates those operations itself.
    }

    /** One Arrow batch becomes one Trino page. Never collects the entire stream. */
    private static final class ArrowSource implements ConnectorPageSource {
        private final URI uri;
        private final List<Column> columns;
        private RootAllocator allocator;
        private ArrowStreamReader reader;
        private HttpURLConnection connection;
        private boolean finished;
        private long bytes;
        private long positions;
        private long readNanos;

        ArrowSource(URI uri, List<Column> columns) { this.uri = uri; this.columns = columns; }

        private void open() throws IOException {
            // Lazy: metadata/planning never contact the remote service.
            connection = (HttpURLConnection) uri.toURL().openConnection();
            connection.setConnectTimeout(5000);
            connection.setReadTimeout(10000);
            connection.setInstanceFollowRedirects(false);
            if (connection.getResponseCode() != 200) {
                throw new IOException("Arrow endpoint returned HTTP " + connection.getResponseCode());
            }
            allocator = new RootAllocator(16 * 1024 * 1024);
            reader = new ArrowStreamReader(connection.getInputStream(), allocator);
            VectorSchemaRoot root = reader.getVectorSchemaRoot();
            if (!root.getSchema().getFields().stream().map(f -> f.getName()).toList().equals(NAMES)
                    || !(root.getVector(0) instanceof BigIntVector)
                    || !(root.getVector(1) instanceof BigIntVector)
                    || !(root.getVector(2) instanceof Float8Vector)) {
                throw new IOException("Unexpected Arrow schema");
            }
        }

        @Override public SourcePage getNextSourcePage() {
            if (finished) { return null; }
            long start = System.nanoTime();
            try {
                if (reader == null) { open(); }
                if (!reader.loadNextBatch()) { close(); return null; }
                VectorSchemaRoot root = reader.getVectorSchemaRoot();
                PageBuilder builder = new PageBuilder(columns.stream().map(c -> TYPES.get(c.index())).toList());
                for (int row = 0; row < root.getRowCount(); row++) {
                    builder.declarePosition();
                    // Respect projected/reordered columns, including zero-column count(*).
                    for (int output = 0; output < columns.size(); output++) {
                        int source = columns.get(output).index();
                        if (root.getVector(source).isNull(row)) { builder.getBlockBuilder(output).appendNull(); }
                        else if (source == 2) {
                            DOUBLE.writeDouble(builder.getBlockBuilder(output), ((Float8Vector)root.getVector(source)).get(row));
                        } else {
                            BIGINT.writeLong(builder.getBlockBuilder(output), ((BigIntVector)root.getVector(source)).get(row));
                        }
                    }
                }
                Page page = builder.build();
                positions += page.getPositionCount();
                bytes = reader.bytesRead();
                return SourcePage.create(page);
            } catch (IOException | RuntimeException failure) {
                try { close(); } catch (IOException cleanup) { failure.addSuppressed(cleanup); }
                throw new TrinoException(GENERIC_INTERNAL_ERROR, "Remote Arrow scan failed: " + failure.getMessage(), failure);
            } finally { readNanos += System.nanoTime() - start; }
        }
        @Override public boolean isFinished() { return finished; }
        @Override public long getCompletedBytes() { return bytes; }
        @Override public OptionalLong getCompletedPositions() { return OptionalLong.of(positions); }
        @Override public long getReadTimeNanos() { return readNanos; }
        @Override public long getMemoryUsage() { return allocator == null ? 0 : allocator.getAllocatedMemory(); }
        @Override public void close() throws IOException {
            if (finished) { return; }
            finished = true;
            try { if (reader != null) { reader.close(); } }
            finally {
                try { if (allocator != null) { allocator.close(); } }
                finally { if (connection != null) { connection.disconnect(); } }
            }
        }
    }
}
