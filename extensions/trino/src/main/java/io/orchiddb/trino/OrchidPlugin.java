package io.orchiddb.trino;

import com.fasterxml.jackson.annotation.JsonProperty;
import com.fasterxml.jackson.databind.ObjectMapper;
import io.trino.spi.*;
import io.trino.spi.connector.*;
import io.trino.spi.statistics.*;
import io.trino.spi.transaction.IsolationLevel;
import io.trino.spi.type.*;
import org.apache.arrow.memory.RootAllocator;
import org.apache.arrow.vector.VectorSchemaRoot;
import org.apache.arrow.vector.ipc.ArrowStreamReader;
import java.io.*;
import java.net.*;
import java.util.*;
import java.util.concurrent.*;
import java.util.concurrent.atomic.AtomicBoolean;
import static io.trino.spi.StandardErrorCode.*;

/** Connector for query-scoped Orchid Arrow exchanges, not a remote SQL engine. */
public final class OrchidPlugin implements Plugin {
    public record FieldSpec(String name, @JsonProperty("sql_type") String sqlType, boolean nullable) {}
    public record Description(String id, int partitions, @JsonProperty("estimated_rows") Long estimatedRows,
                              boolean replayable, List<FieldSpec> columns) {}
    public record Table(String id, int partitions, Long estimatedRows, boolean replayable, List<FieldSpec> columns) implements ConnectorTableHandle {}
    public record Column(int index) implements ColumnHandle {}
    public record Split(String id, int partition) implements ConnectorSplit {}
    public enum Transaction implements ConnectorTransactionHandle { INSTANCE }
    private static final ObjectMapper JSON = new ObjectMapper();

    @Override public Iterable<ConnectorFactory> getConnectorFactories() {
        return List.of(new ConnectorFactory() {
            @Override public String getName() { return "orchid_arrow"; }
            @Override public Connector create(String catalog, Map<String,String> config, ConnectorContext context) {
                Set<String> known=Set.of("exchange.uri","exchange.bearer-token","exchange.io-threads","exchange.max-batch-bytes");
                if (!known.containsAll(config.keySet())) { throw new IllegalArgumentException("Unknown Orchid connector property"); }
                URI endpoint=URI.create(Objects.requireNonNull(config.get("exchange.uri"),"exchange.uri required"));
                String token=Objects.requireNonNull(config.get("exchange.bearer-token"),"exchange.bearer-token required");
                if (!Set.of("http","https").contains(endpoint.getScheme()) || endpoint.getHost()==null
                        || endpoint.getUserInfo()!=null || endpoint.getQuery()!=null || endpoint.getFragment()!=null || token.length()<32) {
                    throw new IllegalArgumentException("Invalid exchange URI or bearer token");
                }
                int threads=Integer.parseInt(config.getOrDefault("exchange.io-threads","16"));
                long maxBytes=Long.parseLong(config.getOrDefault("exchange.max-batch-bytes","67108864"));
                if (threads<1 || threads>256 || maxBytes<1) { throw new IllegalArgumentException("Invalid exchange limits"); }
                return new OrchidConnector(endpoint,token,threads,maxBytes,context.getTypeManager());
            }
        });
    }
    private static final class OrchidConnector implements Connector {
        private final URI endpoint;
        private final String token;
        private final long maxBytes;
        private final TypeManager types;
        private final ExecutorService io;
        OrchidConnector(URI endpoint,String token,int threads,long maxBytes,TypeManager types) {
            this.endpoint=endpoint;this.token=token;this.maxBytes=maxBytes;this.types=types;
            io=new ThreadPoolExecutor(threads,threads,0,TimeUnit.MILLISECONDS,new ArrayBlockingQueue<>(threads*4),
                    task->{Thread thread=new Thread(task,"orchid-arrow-io");thread.setDaemon(true);return thread;},new ThreadPoolExecutor.AbortPolicy());
        }
        private HttpURLConnection connect(String path) throws IOException {
            HttpURLConnection connection=(HttpURLConnection)endpoint.resolve(path).toURL().openConnection();
            connection.setRequestProperty("Authorization","Bearer "+token);
            connection.setConnectTimeout(5000);connection.setReadTimeout(30000);connection.setInstanceFollowRedirects(false);
            return connection;
        }
        @Override public ConnectorTransactionHandle beginTransaction(IsolationLevel level,boolean readOnly,boolean autoCommit) {return Transaction.INSTANCE;}
        @Override public ConnectorMetadata getMetadata(ConnectorSession session,ConnectorTransactionHandle transaction) {return new Metadata(this);}
        @Override public ConnectorSplitManager getSplitManager() {
            return (transaction,session,handle,columns,constraint)->{
                Table table=(Table)handle;
                List<Split> splits=new ArrayList<>();
                for (int p=0;p<table.partitions();p++) {splits.add(new Split(table.id(),p));}
                return new FixedSplitSource(splits);
            };
        }
        @Override public ConnectorPageSourceProviderFactory getPageSourceProviderFactory() {
            return memory -> new ConnectorPageSourceProvider() {
                @Override public ConnectorPageSource createPageSource(ConnectorTransactionHandle transaction,ConnectorSession session,
                        ConnectorSplit split,ConnectorTableHandle table,Optional<ConnectorTableCredentials> credentials,
                        List<ColumnHandle> columns,DynamicFilter dynamicFilter,MemoryContext sourceMemory) {
                    return new ArrowSource(OrchidConnector.this,(Table)table,(Split)split,columns.stream().map(Column.class::cast).toList(),sourceMemory);
                }
            };
        }
        @Override public void shutdown() {io.shutdownNow();}
    }
    private static final class Metadata implements ConnectorMetadata {
        private final OrchidConnector connector;
        Metadata(OrchidConnector connector) {this.connector=connector;}
        @Override public List<String> listSchemaNames(ConnectorSession session) {return List.of("exchange");}
        @Override public List<SchemaTableName> listTables(ConnectorSession session,Optional<String> schema) {
            // Execution IDs are capabilities; do not enumerate other queries' exchanges.
            return List.of();
        }
        @Override public ConnectorTableHandle getTableHandle(ConnectorSession session,SchemaTableName name,
                Optional<ConnectorTableVersion> start,Optional<ConnectorTableVersion> end) {
            if (start.isPresent() || end.isPresent()) {throw new TrinoException(NOT_SUPPORTED,"Exchange snapshots are execution-scoped");}
            if (!name.getSchemaName().equals("exchange") || !name.getTableName().matches("e[0-9a-f]{32}")) {return null;}
            HttpURLConnection connection=null;
            try {
                connection=connector.connect("/v1/exchanges/"+name.getTableName());
                if (connection.getResponseCode()==404) {return null;}
                if (connection.getResponseCode()!=200) {throw new IOException("exchange metadata HTTP "+connection.getResponseCode());}
                byte[] bytes;
                try(InputStream input=connection.getInputStream()) {bytes=input.readNBytes(1_048_577);}
                if (bytes.length>1_048_576) {throw new IOException("exchange metadata byte limit exceeded");}
                Description description=JSON.readValue(bytes,Description.class);
                if (!description.id().equals(name.getTableName()) || description.partitions()<1 || description.partitions()>65536) {
                    throw new IOException("invalid exchange metadata");
                }
                for (FieldSpec field:description.columns()) {connector.types.fromSqlType(field.sqlType());}
                return new Table(description.id(),description.partitions(),description.estimatedRows(),description.replayable(),List.copyOf(description.columns()));
            } catch(IOException failure) {throw new TrinoException(GENERIC_INTERNAL_ERROR,"Orchid exchange metadata: "+failure.getMessage(),failure);}
            finally {if(connection!=null){connection.disconnect();}}
        }
        @Override public ConnectorTableMetadata getTableMetadata(ConnectorSession session,ConnectorTableHandle handle) {
            Table table=(Table)handle;
            return new ConnectorTableMetadata(new SchemaTableName("exchange",table.id()),table.columns().stream()
                    .map(field->new ColumnMetadata(field.name(),connector.types.fromSqlType(field.sqlType()))).toList());
        }
        @Override public Map<String,ColumnHandle> getColumnHandles(ConnectorSession session,ConnectorTableHandle handle) {
            Table table=(Table)handle;Map<String,ColumnHandle> result=new LinkedHashMap<>();
            for(int i=0;i<table.columns().size();i++){result.put(table.columns().get(i).name(),new Column(i));}return result;
        }
        @Override public ColumnMetadata getColumnMetadata(ConnectorSession session,ConnectorTableHandle handle,ColumnHandle column) {
            FieldSpec field=((Table)handle).columns().get(((Column)column).index());
            return new ColumnMetadata(field.name(),connector.types.fromSqlType(field.sqlType()));
        }
        @Override public Map<SchemaTableName,List<ColumnMetadata>> listTableColumns(ConnectorSession session,SchemaTablePrefix prefix) {return Map.of();}
        @Override public TableStatistics getTableStatistics(ConnectorSession session,ConnectorTableHandle handle) {
            Long rows=((Table)handle).estimatedRows();
            return TableStatistics.builder().setRowCount(rows==null?Estimate.unknown():Estimate.of(rows)).build();
        }
        // No predicate/join/aggregation pushdown: those execute in Trino.
    }
    private static final class ArrowSource implements ConnectorPageSource {
        private final OrchidConnector connector;
        private final Table table;
        private final Split split;
        private final List<Column> columns;
        private final List<Type> types;
        private final MemoryContext memory;
        private final AtomicBoolean closed=new AtomicBoolean();
        private final Object lock=new Object();
        private volatile HttpURLConnection connection;
        private RootAllocator allocator;
        private ArrowStreamReader reader;
        private CompletableFuture<SourcePage> pending;
        private volatile long bytes,positions,readNanos;
        private volatile boolean eof;
        ArrowSource(OrchidConnector connector,Table table,Split split,List<Column> columns,MemoryContext memory) {
            this.connector=connector;this.table=table;this.split=split;this.columns=columns;this.memory=memory;
            types=columns.stream().map(c->connector.types.fromSqlType(table.columns().get(c.index()).sqlType())).toList();
        }
        private void start() {
            if(pending==null && !closed.get() && !eof) {pending=CompletableFuture.supplyAsync(this::readPage,connector.io);}
        }
        private SourcePage readPage() {
            long start=System.nanoTime();
            synchronized(lock) {
                if(closed.get()){return null;}
                try {
                    if(reader==null) {
                        connection=connector.connect("/v1/exchanges/"+split.id()+"/partitions/"+split.partition());
                        int status=connection.getResponseCode();
                        if(status!=200){throw new IOException(status==409?"one-shot exchange was already consumed; provide explicit replay":"exchange stream HTTP "+status);}
                        allocator=new RootAllocator(connector.maxBytes);
                        reader=new ArrowStreamReader(new BufferedInputStream(connection.getInputStream(),65536),allocator);
                        VectorSchemaRoot root=reader.getVectorSchemaRoot();
                        if(!root.getSchema().getFields().stream().map(f->f.getName()).toList().equals(table.columns().stream().map(FieldSpec::name).toList())) {
                            throw new IOException("exchange Arrow schema differs from registered columns");
                        }
                    }
                    if(!reader.loadNextBatch()){eof=true;return null;}
                    VectorSchemaRoot root=reader.getVectorSchemaRoot();
                    PageBuilder page=new PageBuilder(root.getRowCount(),types);
                    for(int row=0;row<root.getRowCount();row++) {
                        page.declarePosition();
                        for(int output=0;output<columns.size();output++) {
                            VectorToPage.write(types.get(output),root.getVector(columns.get(output).index()),row,page.getBlockBuilder(output),reader);
                        }
                    }
                    Page result=page.build();bytes=reader.bytesRead();positions+=result.getPositionCount();
                    memory.setBytes(allocator.getAllocatedMemory()+result.getRetainedSizeInBytes());
                    return SourcePage.create(result);
                } catch(IOException|RuntimeException failure) {
                    throw new TrinoException(GENERIC_INTERNAL_ERROR,"Orchid Arrow stream: "+failure.getMessage(),failure);
                } finally {readNanos+=System.nanoTime()-start;}
            }
        }
        @Override public CompletableFuture<?> isBlocked() {start();return pending==null?NOT_BLOCKED:pending;}
        @Override public SourcePage getNextSourcePage() {
            start();if(pending==null||!pending.isDone()){return null;}
            SourcePage page;
            try {page=pending.join();}catch(CompletionException failure){throw failure.getCause() instanceof RuntimeException r?r:failure;}
            pending=null;
            if(page==null){try{close();}catch(IOException failure){throw new TrinoException(GENERIC_INTERNAL_ERROR,"exchange close failed",failure);}return null;}
            // At most one next batch is prefetched. It cannot grow with row count.
            start();return page;
        }
        @Override public boolean isFinished(){return closed.get()||(eof&&pending==null);}
        @Override public long getCompletedBytes(){return bytes;}
        @Override public OptionalLong getCompletedPositions(){return OptionalLong.of(positions);}
        @Override public long getReadTimeNanos(){return readNanos;}
        @Override public long getMemoryUsage(){synchronized(lock){return allocator==null?0:allocator.getAllocatedMemory();}}
        @Override public void close() throws IOException {
            if(!closed.compareAndSet(false,true)){return;}
            HttpURLConnection current=connection;if(current!=null){current.disconnect();}
            synchronized(lock) {
                try{if(reader!=null){reader.close();}}
                finally{if(allocator!=null){allocator.close();}memory.setBytes(0);}
            }
        }
    }
}
