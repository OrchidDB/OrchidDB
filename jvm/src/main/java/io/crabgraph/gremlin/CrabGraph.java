package io.crabgraph.gremlin;

import org.apache.commons.configuration2.BaseConfiguration;
import org.apache.commons.configuration2.Configuration;
import org.apache.tinkerpop.gremlin.process.computer.GraphComputer;
import org.apache.tinkerpop.gremlin.structure.*;
import org.apache.tinkerpop.gremlin.structure.util.AbstractThreadLocalTransaction;
import org.apache.tinkerpop.gremlin.structure.util.ElementHelper;
import java.nio.file.Path;
import java.util.*;
import java.util.function.Supplier;
import static io.crabgraph.gremlin.CrabCodec.fields;

/**
 * TinkerPop graph backed by a private Crabgraph native process. A graph owns a single
 * transaction/session and is not safe for concurrent traversal use. Separate graph
 * instances provide independent sessions. Closing rolls back uncommitted changes.
 */
public final class CrabGraph implements Graph {
    private final String executable;
    private final CrabSession session;
    private final boolean persistent;
    private RuntimeValues runtime = new RuntimeValues();
    
    private final Configuration configuration = new BaseConfiguration();
    private final NativeTransaction transaction = new NativeTransaction();
    private volatile boolean closed;
    private final java.util.concurrent.locks.ReentrantLock lease=new java.util.concurrent.locks.ReentrantLock();

    private CrabGraph(String executable,Path path) {
        this.executable=Objects.requireNonNull(executable,"native executable");
        persistent=path!=null;
        session=new CrabSession(executable,path);
        runtime.members.add(this);
        try { session.call(fields("op","hello","version",1)); } catch(RuntimeException failure) { session.close(); releaseRuntime(); throw failure; }
        configuration.setProperty(Graph.GRAPH,CrabGraph.class.getName());
        configuration.setProperty("crabgraph.native.executable",executable);
        if(path!=null) configuration.setProperty("crabgraph.native.path",path.toString());
    }
    public static CrabGraph open() {
        String executable=System.getenv("CRABGRAPH_JVM_STORE");
        if(executable==null || executable.isEmpty()) throw new IllegalStateException("Set CRABGRAPH_JVM_STORE to the crabgraph-jvm-store executable");
        return open(executable);
    }
    public static CrabGraph open(String executable) { return new CrabGraph(executable,null); }
    public static CrabGraph open(String executable,Path path) { return new CrabGraph(executable,path); }
    public static CrabGraph open(Configuration config) {
        String executable=config.getString("crabgraph.native.executable",System.getenv("CRABGRAPH_JVM_STORE"));
        String path=config.getString("crabgraph.native.path",null);
        return new CrabGraph(executable,path==null?null:Path.of(path));
    }
    public CrabGraph freshGraph() {
        if(closed) throw new IllegalStateException("Graph is closed");
        CrabGraph child=open(executable); child.runtime.members.remove(child); child.runtime=runtime; runtime.members.add(child); return child;
    }
    public Object encodeValue(Object value) { return CrabCodec.encode(value,this); }
    public Object decodeValue(Object value) { return CrabCodec.decode(value,this); }
    Object runtimeEncode(Object value) {
        if (!(value instanceof org.apache.tinkerpop.gremlin.process.traversal.Traverser) &&
            !(value instanceof org.apache.tinkerpop.gremlin.process.traversal.traverser.util.TraverserSet)) return null;
        if(persistent) throw new IllegalArgumentException("Traversal runtime objects cannot be persisted");
        long id=runtime.nextId.getAndIncrement(); runtime.values.put(id,value);
        return fields("type","jvm_runtime","session",runtime.namespace,"id",id);
    }
    Object runtimeDecode(Map<String,Object> record) {
        if(!runtime.namespace.equals(record.get("session"))) throw new IllegalArgumentException("Runtime value belongs to another graph family");
        Object result=runtime.values.get(((Number)record.get("id")).longValue());
        if(result==null) throw new IllegalStateException("Expired traversal runtime reference");
        return result;
    }
    private static final class RuntimeValues {
        final String namespace=UUID.randomUUID().toString();
        final java.util.concurrent.atomic.AtomicLong nextId=new java.util.concurrent.atomic.AtomicLong();
        final Map<Long,Object> values=new java.util.concurrent.ConcurrentHashMap<>();
        final Set<CrabGraph> members=java.util.concurrent.ConcurrentHashMap.newKeySet();
    }
    /** Acquire on the executing thread and close on that same thread. */
    public AutoCloseable executionLease() {
        lease.lock();
        if(closed) { lease.unlock(); throw new IllegalStateException("Graph is closed"); }
        return ()->lease.unlock();
    }
    /** Native savepoint preserves the surrounding caller transaction on failure. */
    public void atomicMutation(Runnable action) {
        lease.lock();
        try {
            transaction.readWrite(); Object id=call(fields("op","savepoint"));
            try { action.run(); call(fields("op","release","id",id)); }
            catch(RuntimeException|Error failure) {
                try { call(fields("op","rollbackTo","id",id)); } catch(RuntimeException rollback) { failure.addSuppressed(rollback); }
                throw failure;
            }
        } finally { lease.unlock(); }
    }
    private Object call(Map<String,Object> request) {
        lease.lock(); try { return session.call(request); } finally { lease.unlock(); }
    }
    Object request(String op,Object... values) {
        if(closed) throw new IllegalStateException("Graph is closed");
        transaction.readWrite();
        Map<String,Object> request=fields(values); request.put("op",op);
        return call(request);
    }
    @SuppressWarnings("unchecked")
    <E> E decode(Object value) { return (E)CrabCodec.decode(value,this); }
    <E> Iterator<E> records(Object values) {
        List<E> result=new ArrayList<>(); for(Object value:(List<?>)values) result.add(decode(value));
        return result.iterator();
    }
    static List<Object> ids(Object[] ids) {
        List<Object> result=new ArrayList<>();
        for(Object id:ids) result.add(CrabCodec.encode(id instanceof Element?((Element)id).id():id));
        return result;
    }
    List<Object> properties(Object... keyValues) {
        ElementHelper.legalPropertyKeyValueArray(keyValues);
        List<Object> result=new ArrayList<>();
        for(int i=0;i<keyValues.length;i+=2) if(keyValues[i] instanceof String) {
            ElementHelper.validateProperty((String)keyValues[i],keyValues[i+1]);
            result.add(Arrays.asList(keyValues[i],encodeValue(keyValues[i+1])));
        }
        return result;
    }
    @Override public Vertex addVertex(Object... keyValues) {
        List<Object> props=properties(keyValues);
        String label=ElementHelper.getLabelValue(keyValues).orElse(Vertex.DEFAULT_LABEL);
        ElementHelper.validateLabel(label);
        Map<String,Object> request=fields("op","addVertex","label",label,"properties",props);
        ElementHelper.getIdValue(keyValues).ifPresent(id->request.put("id",CrabCodec.encode(id)));
        transaction.readWrite(); return decode(call(request));
    }
    @Override public Iterator<Vertex> vertices(Object... ids) { return records(request("vertices","ids",ids(ids))); }
    @Override public Iterator<Edge> edges(Object... ids) { return records(request("edges","ids",ids(ids))); }
    @Override public Transaction tx() { return transaction; }
    @Override public Configuration configuration() { return configuration; }
    @Override public Variables variables() { throw Graph.Exceptions.variablesNotSupported(); }
    @Override public GraphComputer compute() {
        try {
            return (GraphComputer)Class.forName("io.crabgraph.gremlin.computer.CrabGraphComputer")
                .getConstructor(Graph.class,Supplier.class).newInstance(this,(Supplier<Graph>)this::freshGraph);
        } catch(ReflectiveOperationException e) { throw new UnsupportedOperationException("CrabGraphComputer module is unavailable",e); }
    }
    @Override public <C extends GraphComputer> C compute(Class<C> type) {
        GraphComputer computer=compute();
        if(!type.isInstance(computer)) throw Graph.Exceptions.graphDoesNotSupportProvidedGraphComputer(type);
        return type.cast(computer);
    }
    /** Cancel pending I/O and discard this session. Safe to call from another thread. */
    public void abort() { closed=true; session.abort(); releaseRuntime(); }
    public void abortFamily() { for(CrabGraph member:new ArrayList<>(runtime.members)) member.abort(); }
    public void closeFamily() { for(CrabGraph member:new ArrayList<>(runtime.members)) member.close(); }
    private void releaseRuntime() { runtime.members.remove(this); if(runtime.members.isEmpty()) runtime.values.clear(); }
    @Override public void close() {
        if(closed) return;
        try { if(transaction.isOpen()) transaction.rollback(); }
        finally { closed=true; session.close(); releaseRuntime(); }
    }
    private final class NativeTransaction extends AbstractThreadLocalTransaction {
        private boolean open;
        NativeTransaction() { super(CrabGraph.this); }
        @Override public boolean isOpen() { return open&&!closed; }
        @Override protected void doOpen() {
            if(closed) throw new IllegalStateException("Graph is closed");
            call(fields("op","begin")); open=true;
        }
        @Override protected void doCommit() { call(fields("op","commit")); open=false; }
        @Override protected void doRollback() { call(fields("op","rollback")); open=false; }
    }
    @Override public Features features() { return FEATURES; }
    private static final Features FEATURES=new Features() {
        private final GraphFeatures graph=new GraphFeatures() {
            @Override public boolean supportsComputer() { try { Class.forName("io.crabgraph.gremlin.computer.CrabGraphComputer"); return true; } catch(ClassNotFoundException e) { return false; } }
            @Override public boolean supportsPersistence() { return true; }
            @Override public boolean supportsConcurrentAccess() { return false; }
            @Override public boolean supportsThreadedTransactions() { return false; }
            @Override public VariableFeatures variables() { return new VariableFeatures() { @Override public boolean supportsVariables() { return false; } }; }
        };
        private final VertexPropertyFeatures vp=new NativeVertexPropertyFeatures();
        private final VertexFeatures vertex=new VertexFeatures() {
            @Override public VertexProperty.Cardinality getCardinality(String key) { return VertexProperty.Cardinality.single; }
            @Override public boolean supportsNullPropertyValues() { return true; }
            @Override public boolean supportsUuidIds() { return false; }
            @Override public boolean supportsCustomIds() { return false; }
            @Override public boolean supportsAnyIds() { return false; }
            @Override public VertexPropertyFeatures properties() { return vp; }
        };
        private final EdgeFeatures edge=new EdgeFeatures() {
            @Override public boolean supportsNullPropertyValues() { return true; }
            @Override public boolean supportsUuidIds() { return false; }
            @Override public boolean supportsCustomIds() { return false; }
            @Override public boolean supportsAnyIds() { return false; }
            @Override public EdgePropertyFeatures properties() { return new NativeEdgePropertyFeatures(); }
        };
        @Override public GraphFeatures graph() { return graph; }
        @Override public VertexFeatures vertex() { return vertex; }
        @Override public EdgeFeatures edge() { return edge; }
    };
    private interface NativeDataFeatures extends Features.DataTypeFeatures {
        @Override default boolean supportsBooleanArrayValues() { return false; }
        @Override default boolean supportsByteArrayValues() { return false; }
        @Override default boolean supportsDoubleArrayValues() { return false; }
        @Override default boolean supportsFloatArrayValues() { return false; }
        @Override default boolean supportsIntegerArrayValues() { return false; }
        @Override default boolean supportsStringArrayValues() { return false; }
        @Override default boolean supportsLongArrayValues() { return false; }
        @Override default boolean supportsSerializableValues() { return false; }
    }
    private static class NativeVertexPropertyFeatures implements Features.VertexPropertyFeatures,NativeDataFeatures {
        @Override public boolean supportsNullPropertyValues() { return true; }
        @Override public boolean supportsUuidIds() { return false; }
        @Override public boolean supportsCustomIds() { return false; }
        @Override public boolean supportsAnyIds() { return false; }
    }
    private static class NativeEdgePropertyFeatures implements Features.EdgePropertyFeatures,NativeDataFeatures {}
}
