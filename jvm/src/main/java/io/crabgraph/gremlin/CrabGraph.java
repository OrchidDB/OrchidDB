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
    public static final String DEFAULT_CARDINALITY = "crabgraph.vertex.defaultCardinality";
    private final VertexProperty.Cardinality defaultCardinality;
    public static final String VERTEX_ID_MANAGER = "crabgraph.vertex.idManager";
    public static final String EDGE_ID_MANAGER = "crabgraph.edge.idManager";
    public static final String VERTEX_PROPERTY_ID_MANAGER = "crabgraph.vertexProperty.idManager";
    private final IdManager vertexIdManager,edgeIdManager,vertexPropertyIdManager;
    private final String executable;
    private final CrabSession session;
    private final boolean persistent;
    private final boolean committedReadsSupported;
    private final boolean nullPropertyValues;
    private RuntimeValues runtime = new RuntimeValues();
    
    private Configuration configuration = new BaseConfiguration();
    private final NativeTransaction transaction = new NativeTransaction();
    private final org.apache.tinkerpop.gremlin.structure.service.ServiceRegistry services=CrabServices.create(this);
    private int bulkLoadDepth;
    private int atomicMutationDepth;
    private long nextNumericId;
    private boolean numericIdsExhausted;
    private final int adjacencyCacheSize=Math.max(0,Integer.getInteger("crabgraph.native.adjacencyCacheSize",4096));
    private final Map<Object,Object> adjacencyCache=Collections.synchronizedMap(
        new LinkedHashMap<Object,Object>(128,0.75f,true) {
            @Override protected boolean removeEldestEntry(Map.Entry<Object,Object> eldest) { return size()>adjacencyCacheSize; }
        });
    private final int propertyCacheSize=Math.max(0,Integer.getInteger("crabgraph.native.propertyCacheSize",16384));
    private final Map<Object,Object> propertyCache=Collections.synchronizedMap(
        new LinkedHashMap<Object,Object>(128,0.75f,true) {
            @Override protected boolean removeEldestEntry(Map.Entry<Object,Object> eldest) { return size()>propertyCacheSize; }
        });
    private volatile boolean closed;
    private final java.util.concurrent.locks.ReentrantLock lease=new java.util.concurrent.locks.ReentrantLock();
    private final java.util.concurrent.locks.Condition writerFinished=lease.newCondition();

    private CrabGraph(String executable,Path path) { this(executable,path,null,VertexProperty.Cardinality.single); }
    private CrabGraph(String executable,Path path,RuntimeValues family,VertexProperty.Cardinality cardinality) {
        this(executable,path,family,cardinality,IdManager.ANY,IdManager.ANY,IdManager.ANY);
    }
    private CrabGraph(String executable,Path path,RuntimeValues family,VertexProperty.Cardinality cardinality,
                      IdManager vertexIds,IdManager edgeIds,IdManager propertyIds) {
        this(executable,path,family,cardinality,vertexIds,edgeIds,propertyIds,null);
    }
    static CrabGraph forIr(java.io.BufferedReader source, java.io.OutputStream sink) {
        return new CrabGraph("IR-owned",null,null,VertexProperty.Cardinality.single,
                IdManager.ANY,IdManager.ANY,IdManager.ANY,new CrabSession(source,sink));
    }
    private CrabGraph(String executable,Path path,RuntimeValues family,VertexProperty.Cardinality cardinality,
                      IdManager vertexIds,IdManager edgeIds,IdManager propertyIds,CrabSession borrowed) {
        vertexIdManager=vertexIds; edgeIdManager=edgeIds; vertexPropertyIdManager=propertyIds;
        defaultCardinality=Objects.requireNonNull(cardinality,"default vertex-property cardinality");
        this.executable=Objects.requireNonNull(executable,"native executable");
        persistent=path!=null;
        if(path!=null) {
            try { java.nio.file.Files.createDirectories(path.toAbsolutePath().getParent()); }
            catch(java.io.IOException e) { throw new IllegalStateException("Cannot create native graph directory",e); }
        }
        session=borrowed==null?new CrabSession(executable,path):borrowed;
        if(family!=null) runtime=family;
        synchronized(runtime) {
            if(runtime.closed) { session.abort(); throw new IllegalStateException("Graph family is closed"); }
            runtime.members.add(this);
        }
        try {
            Object hello=session.call(fields("op","hello","version",1));
            committedReadsSupported=hello instanceof Map&&Boolean.TRUE.equals(((Map<?,?>)hello).get("committedReads"));
            nullPropertyValues=!(hello instanceof Map)||!Boolean.FALSE.equals(((Map<?,?>)hello).get("nullPropertyValues"));
        } catch(RuntimeException failure) { session.close(); releaseRuntime(); throw failure; }
        configuration.setProperty(Graph.GRAPH,CrabGraph.class.getName());
        configuration.setProperty(DEFAULT_CARDINALITY,defaultCardinality.name());
        configuration.setProperty(VERTEX_ID_MANAGER,vertexIdManager.name());
        configuration.setProperty(EDGE_ID_MANAGER,edgeIdManager.name());
        configuration.setProperty(VERTEX_PROPERTY_ID_MANAGER,vertexPropertyIdManager.name());
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
        CrabGraph graph=new CrabGraph(executable,path==null?null:Path.of(path),null,
            VertexProperty.Cardinality.valueOf(config.getString(DEFAULT_CARDINALITY,"single")),
            IdManager.valueOf(config.getString(VERTEX_ID_MANAGER,"ANY")),
            IdManager.valueOf(config.getString(EDGE_ID_MANAGER,"ANY")),
            IdManager.valueOf(config.getString(VERTEX_PROPERTY_ID_MANAGER,"ANY")));
        graph.configuration=config;
        return graph;
    }
    public CrabGraph freshGraph() {
        if(closed || runtime.closed) throw new IllegalStateException("Graph family is closed");
        return new CrabGraph(executable,null,runtime,defaultCardinality,vertexIdManager,edgeIdManager,vertexPropertyIdManager,session.isBorrowed()?session.fork():null);
    }
    public Object encodeValue(Object value) { return CrabCodec.encode(value,this); }
    public Object decodeValue(Object value) { return CrabCodec.decode(value,this); }
    Object runtimeEncode(Object value) {
        if (!(value instanceof org.apache.tinkerpop.gremlin.process.traversal.Traverser) &&
            !(value instanceof org.apache.tinkerpop.gremlin.process.traversal.traverser.util.TraverserSet)) return null;
        if(persistent) throw new IllegalArgumentException("Traversal runtime objects cannot be persisted");
        synchronized(runtime) {
            if(closed || runtime.closed) throw new IllegalStateException("Graph family is closed");
            long id=runtime.nextId.getAndIncrement(); runtime.values.put(id,value);
            return fields("type","jvm_runtime","session",runtime.namespace,"id",id);
        }
    }
    Object runtimeDecode(Map<String,Object> record) {
        if(!runtime.namespace.equals(record.get("session"))) throw new IllegalArgumentException("Runtime value belongs to another graph family");
        Object result=runtime.values.get(((Number)record.get("id")).longValue());
        if(result==null) throw new IllegalStateException("Expired traversal runtime reference");
        return result;
    }
    private static final class RuntimeValues {
        volatile boolean closed;
        final String namespace=UUID.randomUUID().toString();
        final java.util.concurrent.atomic.AtomicLong nextId=new java.util.concurrent.atomic.AtomicLong();
        final Map<Long,Object> values=new java.util.concurrent.ConcurrentHashMap<>();
        final Set<CrabGraph> members=java.util.concurrent.ConcurrentHashMap.newKeySet();
    }
    /** Acquire on the executing thread and close on that same thread. */
    public AutoCloseable executionLease() {
        try { lease.lockInterruptibly(); }
        catch(InterruptedException e) { Thread.currentThread().interrupt(); throw new IllegalStateException("Graph execution lease interrupted",e); }
        if(closed) { lease.unlock(); throw new IllegalStateException("Graph is closed"); }
        return ()->lease.unlock();
    }
    /** Borrow the submitting thread's transaction for an exclusively leased computation. */
    public AutoCloseable executionLease(Thread submittingThread,boolean callerTransactionOpen) {
        AutoCloseable lock=executionLease();
        Thread previous=transaction.writeOwner;
        if(previous!=null&&previous!=submittingThread&&previous!=Thread.currentThread()) {
            lease.unlock();
            throw new IllegalStateException("Computation cannot borrow another thread's transaction");
        }
        if(previous!=null) transaction.writeOwner=Thread.currentThread();
        boolean previousOpen=transaction.open.get();
        if(callerTransactionOpen) transaction.open.set(true);
        return ()->{
            try {
                if(transaction.writeOwner==Thread.currentThread())
                    transaction.writeOwner=previous==null?submittingThread:previous;
            } finally { transaction.open.set(previousOpen); lock.close(); }
        };
    }
    /** Import atomically, deferring reader commits and preserving any existing caller transaction. */
    public void bulkLoad(Runnable action) {
        lockForMutation();
        boolean hadTransaction=transaction.isOpen();
        boolean outer=bulkLoadDepth++==0;
        try {
            try { atomicMutation(action); }
            finally { bulkLoadDepth--; }
            if(outer&&!hadTransaction) transaction.commit();
        } catch(RuntimeException|Error failure) {
            if(outer&&!hadTransaction) {
                boolean interrupted=Thread.interrupted();
                try { transaction.rollback(); } catch(RuntimeException rollback) { failure.addSuppressed(rollback); }
                finally { if(interrupted) Thread.currentThread().interrupt(); }
            }
            throw failure;
        } finally { lease.unlock(); }
    }
    /** Native savepoint preserves the surrounding caller transaction on failure. */
    public void atomicMutation(Runnable action) {
        lockForMutation();
        atomicMutationDepth++;
        try {
            transaction.readWrite(); Object id=call(fields("op","savepoint"));
            try { action.run(); call(fields("op","release","id",id)); }
            catch(RuntimeException|Error failure) {
                boolean interrupted=Thread.interrupted();
                try { call(fields("op","rollbackTo","id",id)); } catch(RuntimeException rollback) { failure.addSuppressed(rollback); }
                finally { if(interrupted) Thread.currentThread().interrupt(); }
                throw failure;
            }
        } finally { atomicMutationDepth--; lease.unlock(); }
    }
    private void lockForMutation() {
        lease.lock();
        try { transaction.awaitOwner(); }
        catch(RuntimeException|Error failure) { lease.unlock(); throw failure; }
    }
    private Object call(Map<String,Object> request) {
        lease.lock();
        try {
            if(closed) throw new IllegalStateException("Graph is closed");
            if(Thread.currentThread().isInterrupted()) throw new java.util.concurrent.CancellationException("Native request cancelled before submission");
            if(!session.isAlive()) throw new IllegalStateException("Native graph session is closed or disconnected");
            String op=(String)request.get("op");
            boolean committedRead=isTraversalRead(op)&&transaction.writeOwner!=null&&transaction.writeOwner!=Thread.currentThread();
            if(committedRead) {
                if(!committedReadsSupported) throw new UnsupportedOperationException("Native store lacks committed snapshot reads; use a compatible crabgraph-jvm-store binary");
                request.put("committed",true);
            }
            if(op.equals("addVertex")||op.equals("addEdge")||op.equals("setVertexProperty")||op.equals("setProperty")||op.equals("remove")||op.equals("savepoint"))
                transaction.claimWrite();
            boolean read=op.equals("vertices")||op.equals("edges")||op.equals("adjacent")||op.equals("properties")||op.equals("begin")||op.equals("hello");
            if(!read) clearReadCaches();
            if((op.equals("addVertex")||op.equals("addEdge"))&&!request.containsKey("id")) {
                IdManager manager=op.equals("addVertex")?vertexIdManager:edgeIdManager;
                if(manager!=IdManager.ANY) request.put("id",nextNumericId(manager));
            }
            Map<Object,Object> cache=atomicMutationDepth!=0||committedRead?null:
                op.equals("adjacent")&&adjacencyCacheSize>0?adjacencyCache:
                op.equals("properties")&&propertyCacheSize>0?propertyCache:null;
            if(cache!=null) {
                Object hit=cache.get(request);
                if(hit!=null) return hit;
            }
            Object value=session.call(request);
            if(cache!=null) {
                value=immutableRecord(value);
                Object key=immutableRecord(request);
                synchronized(cache) { if(!closed) cache.put(key,value); }
            }
            return value;
        } catch(RuntimeException|Error failure) { clearReadCaches(); throw failure; }
        finally { lease.unlock(); }
    }
    private void clearReadCaches() { adjacencyCache.clear(); propertyCache.clear(); }
    // Invoked under the session lease. The counter never rewinds on rollback; after
    // reopen, exact native identity lookups skip every surviving supplied/generated ID.
    private Object nextNumericId(IdManager manager) {
        while(true) {
            if(numericIdsExhausted || (manager==IdManager.INTEGER&&nextNumericId>Integer.MAX_VALUE))
                throw new IllegalStateException("Automatic "+manager+" identifier space is exhausted");
            long candidate=nextNumericId;
            if(candidate==Long.MAX_VALUE) numericIdsExhausted=true; else nextNumericId++;
            Object id;
            if(manager==IdManager.INTEGER) id=Integer.valueOf((int)candidate); else id=Long.valueOf(candidate);
            Object encoded=CrabCodec.encode(id);
            List<Object> ids=Collections.singletonList(encoded);
            if(((List<?>)call(fields("op","vertices","ids",ids))).isEmpty() &&
               ((List<?>)call(fields("op","edges","ids",ids))).isEmpty()) return encoded;
        }
    }
    private static Object immutableRecord(Object value) {
        if(value instanceof Map) {
            Map<Object,Object> copy=new LinkedHashMap<>();
            ((Map<?,?>)value).forEach((key,item)->copy.put(key,immutableRecord(item)));
            return Collections.unmodifiableMap(copy);
        }
        if(value instanceof List) {
            List<Object> copy=new ArrayList<>();
            for(Object item:(List<?>)value) copy.add(immutableRecord(item));
            return Collections.unmodifiableList(copy);
        }
        return value;
    }
    private static boolean isTraversalRead(String op) {
        return op.equals("vertices")||op.equals("edges")||op.equals("adjacent")||op.equals("properties");
    }
    Object request(String op,Object... values) {
        if(closed) throw new IllegalStateException("Graph is closed");
        try {
            transaction.readWrite();
            Map<String,Object> request=fields(values); request.put("op",op);
            return call(request);
        } catch(java.util.concurrent.CancellationException cancelled) {
            if(isTraversalRead(op)&&Thread.currentThread().isInterrupted()) {
                org.apache.tinkerpop.gremlin.process.traversal.util.TraversalInterruptedException interrupted=
                    new org.apache.tinkerpop.gremlin.process.traversal.util.TraversalInterruptedException();
                interrupted.initCause(cancelled);
                throw interrupted;
            }
            throw cancelled;
        }
    }
    @SuppressWarnings("unchecked")
    <E> E decode(Object value) { return (E)CrabCodec.decode(value,this); }
    <E> Iterator<E> records(Object values) {
        List<E> result=new ArrayList<>(); for(Object value:(List<?>)values) result.add(decode(value));
        return result.iterator();
    }
    private enum IdManager {
        ANY, LONG, INTEGER;
        Object convert(Object id) {
            if(id==null) return null;
            if(this==ANY) {
                if(id instanceof String || id instanceof Byte || id instanceof Short || id instanceof Integer ||
                   id instanceof Long || id instanceof Float || id instanceof Double || id instanceof java.math.BigInteger ||
                   id instanceof java.math.BigDecimal) return id;
                throw new IllegalArgumentException("Unsupported native identifier type: "+id.getClass().getName());
            }
            if(id instanceof Number) {
                if(this==LONG) return ((Number)id).longValue();
                return ((Number)id).intValue();
            }
            if(id instanceof String) {
                if(this==LONG) return Long.valueOf((String)id);
                return Integer.valueOf((String)id);
            }
            throw new IllegalArgumentException("Cannot convert identifier to "+name()+": "+id);
        }
        boolean allows(Object id) { try { return convert(id)!=null; } catch(IllegalArgumentException e) { return false; } }
    }
    private IdManager idManager(String type) {
        return type.equals("vertex")?vertexIdManager:type.equals("edge")?edgeIdManager:vertexPropertyIdManager;
    }
    Object encodeId(Object id,String type) {
        IdManager manager=idManager(type);
        if(id!=null&&!manager.allows(id)) {
            if(type.equals("vertex")) throw Vertex.Exceptions.userSuppliedIdsOfThisTypeNotSupported();
            if(type.equals("edge")) throw Edge.Exceptions.userSuppliedIdsOfThisTypeNotSupported();
            throw VertexProperty.Exceptions.userSuppliedIdsOfThisTypeNotSupported();
        }
        return CrabCodec.encode(manager.convert(id));
    }
    Object elementId(Map<String,Object> record) { return idManager((String)record.get("type")).convert(decode(record.get("id"))); }
    private List<Object> ids(Object[] ids,String type) {
        List<Object> result=new ArrayList<>();
        for(Object id:ids) result.add(encodeId(id instanceof Element?((Element)id).id():id,type));
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
        ElementHelper.getIdValue(keyValues).ifPresent(id->request.put("id",encodeId(id,"vertex")));
        Set<Object> keys=new HashSet<>();
        boolean duplicateKey=false;
        for(Object property:props) if(!keys.add(((List<?>)property).get(0))) duplicateKey=true;
        if(!duplicateKey) {
            transaction.readWrite(); return decode(call(request));
        }
        // Constructor key/value pairs have list cardinality independently of the
        // default used by subsequent Vertex.property calls (TinkerPop semantics).
        request.put("properties",Collections.emptyList());
        Vertex[] created=new Vertex[1];
        atomicMutation(()->{
            created[0]=decode(call(request));
            for(int i=0;i<keyValues.length;i+=2) if(keyValues[i] instanceof String)
                created[0].property(VertexProperty.Cardinality.list,(String)keyValues[i],keyValues[i+1]);
        });
        return created[0];
    }
    @Override public Iterator<Vertex> vertices(Object... ids) { return records(request("vertices","ids",ids(ids,"vertex"))); }
    @Override public Iterator<Edge> edges(Object... ids) { return records(request("edges","ids",ids(ids,"edge"))); }
    @Override public Transaction tx() { return transaction; }
    @Override public Configuration configuration() { return configuration; }
    @Override public org.apache.tinkerpop.gremlin.structure.service.ServiceRegistry getServiceRegistry() { return services; }
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
    public void abort() { closed=true; clearReadCaches(); session.abort(); services.close(); releaseRuntime(); }
    public void abortFamily() {
        List<CrabGraph> members;
        synchronized(runtime) { runtime.closed=true; members=new ArrayList<>(runtime.members); }
        for(CrabGraph member:members) member.abort();
    }
    public void closeFamily() {
        List<CrabGraph> members;
        synchronized(runtime) { runtime.closed=true; members=new ArrayList<>(runtime.members); }
        RuntimeException failure=null;
        for(CrabGraph member:members) try { member.close(); } catch(RuntimeException e) { if(failure==null) failure=e; else failure.addSuppressed(e); }
        if(failure!=null) throw failure;
    }
    private void releaseRuntime() { synchronized(runtime) { runtime.members.remove(this); if(runtime.members.isEmpty()) runtime.values.clear(); } }
    @Override public void close() {
        if(closed) return;
        try { if(transaction.isOpen()) transaction.rollback(); }
        finally { closed=true; clearReadCaches(); session.close(); services.close(); releaseRuntime(); }
    }
    private final class NativeTransaction extends AbstractThreadLocalTransaction {
        private final ThreadLocal<Boolean> open=ThreadLocal.withInitial(()->false);
        private Thread writeOwner;
        NativeTransaction() { super(CrabGraph.this); }
        @Override public boolean isOpen() { return open.get()&&!closed; }
        @Override public void commit() { if(writeOwner!=Thread.currentThread()||bulkLoadDepth==0) super.commit(); }
        // The provider supports one writer. Read-only transactions in helper threads
        // must never commit or roll back another thread's pending native mutations.
        void claimWrite() {
            awaitOwner();
            writeOwner=Thread.currentThread();
        }
        void awaitOwner() {
            while(writeOwner!=null&&writeOwner!=Thread.currentThread()&&!closed) {
                try { writerFinished.awaitNanos(java.util.concurrent.TimeUnit.MILLISECONDS.toNanos(100)); }
                catch(InterruptedException interrupted) {
                    Thread.currentThread().interrupt();
                    throw new java.util.concurrent.CancellationException("Native transaction wait interrupted");
                }
            }
            if(closed) throw new IllegalStateException("Graph is closed");
        }
        @Override protected void doOpen() {
            if(closed) throw new IllegalStateException("Graph is closed");
            call(fields("op","begin")); open.set(true);
        }
        private void finish(String operation) {
            lease.lock();
            try {
                if(writeOwner==Thread.currentThread()) {
                    call(fields("op",operation));
                    writeOwner=null;
                    writerFinished.signalAll();
                }
                open.remove();
            } finally { lease.unlock(); }
        }
        @Override protected void doCommit() { finish("commit"); }
        @Override protected void doRollback() { finish("rollback"); }
    }
    @Override public String toString() { return org.apache.tinkerpop.gremlin.structure.util.StringFactory.graphString(this,"native"); }
    @Override public Features features() { return FEATURES; }
    private final Features FEATURES=new NativeFeatures();
    public final class NativeFeatures implements Features {
        private final GraphFeatures graph=new NativeGraphFeatures();
        private final VertexFeatures vertex=new NativeVertexFeatures();
        private final EdgeFeatures edge=new NativeEdgeFeatures();
        @Override public GraphFeatures graph() { return graph; }
        @Override public VertexFeatures vertex() { return vertex; }
        @Override public EdgeFeatures edge() { return edge; }
        @Override public String toString() { return org.apache.tinkerpop.gremlin.structure.util.StringFactory.featureString(this); }
    }
    public static final class NativeGraphFeatures implements Features.GraphFeatures {
        private final Features.VariableFeatures variables=new NativeVariableFeatures();
        @Override public boolean supportsServiceCall() { return true; }
        @Override public boolean supportsComputer() { try { Class.forName("io.crabgraph.gremlin.computer.CrabGraphComputer"); return true; } catch(ClassNotFoundException e) { return false; } }
        @Override public boolean supportsPersistence() { return true; }
        @Override public boolean supportsConcurrentAccess() { return false; }
        @Override public boolean supportsThreadedTransactions() { return false; }
        @Override public Features.VariableFeatures variables() { return variables; }
    }
    public final class NativeVertexFeatures implements Features.VertexFeatures {
        private final Features.VertexPropertyFeatures properties=new NativeVertexPropertyFeatures();
        @Override public boolean supportsStringIds() { return vertexIdManager==IdManager.ANY; }
        @Override public boolean willAllowId(Object id) { return vertexIdManager.allows(id); }
        @Override public VertexProperty.Cardinality getCardinality(String key) { return defaultCardinality; }
        @Override public boolean supportsNullPropertyValues() { return nullPropertyValues; }
        @Override public boolean supportsUuidIds() { return false; }
        @Override public boolean supportsCustomIds() { return false; }
        @Override public boolean supportsAnyIds() { return false; }
        @Override public Features.VertexPropertyFeatures properties() { return properties; }
    }
    public final class NativeEdgeFeatures implements Features.EdgeFeatures {
        private final Features.EdgePropertyFeatures properties=new NativeEdgePropertyFeatures();
        @Override public boolean supportsStringIds() { return edgeIdManager==IdManager.ANY; }
        @Override public boolean willAllowId(Object id) { return edgeIdManager.allows(id); }
        @Override public boolean supportsNullPropertyValues() { return nullPropertyValues; }
        @Override public boolean supportsUuidIds() { return false; }
        @Override public boolean supportsCustomIds() { return false; }
        @Override public boolean supportsAnyIds() { return false; }
        @Override public Features.EdgePropertyFeatures properties() { return properties; }
    }
    public static final class NativeVariableFeatures implements Features.VariableFeatures,NativeDataFeatures {
        @Override public boolean supportsVariables() { return false; }
        @Override public boolean supportsBooleanValues() { return false; }
        @Override public boolean supportsByteValues() { return false; }
        @Override public boolean supportsDoubleValues() { return false; }
        @Override public boolean supportsFloatValues() { return false; }
        @Override public boolean supportsIntegerValues() { return false; }
        @Override public boolean supportsLongValues() { return false; }
        @Override public boolean supportsMapValues() { return false; }
        @Override public boolean supportsMixedListValues() { return false; }
        @Override public boolean supportsStringValues() { return false; }
        @Override public boolean supportsUniformListValues() { return false; }
    }
    public interface NativeDataFeatures extends Features.DataTypeFeatures {
        @Override default boolean supportsBooleanArrayValues() { return false; }
        @Override default boolean supportsByteArrayValues() { return false; }
        @Override default boolean supportsDoubleArrayValues() { return false; }
        @Override default boolean supportsFloatArrayValues() { return false; }
        @Override default boolean supportsIntegerArrayValues() { return false; }
        @Override default boolean supportsStringArrayValues() { return false; }
        @Override default boolean supportsLongArrayValues() { return false; }
        @Override default boolean supportsSerializableValues() { return false; }
    }
    public final class NativeVertexPropertyFeatures implements Features.VertexPropertyFeatures,NativeDataFeatures {
        @Override public boolean supportsStringIds() { return vertexPropertyIdManager==IdManager.ANY; }
        @Override public boolean willAllowId(Object id) { return vertexPropertyIdManager.allows(id); }
        @Override public boolean supportsNullPropertyValues() { return nullPropertyValues; }
        @Override public boolean supportsUuidIds() { return false; }
        @Override public boolean supportsCustomIds() { return false; }
        @Override public boolean supportsAnyIds() { return false; }
    }
    public static final class NativeEdgePropertyFeatures implements Features.EdgePropertyFeatures,NativeDataFeatures {}
}
