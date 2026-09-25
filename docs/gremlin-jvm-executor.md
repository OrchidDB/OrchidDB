# JVM executor over native OrchidDB storage

For JVM fragments inside a managed query, see [relational DAG execution](sql-ir-datafusion-execution.md). The standalone profile described below remains available.

`orchiddb-jvm` is an explicit execution profile. TinkerPop 3.7.4 executes traversals
and Groovy callbacks in the JVM; a `OrchidGraph` provider performs every graph read
and mutation through a private `orchiddb-jvm-store` process backed by OrchidDB's
native `PropertyGraph`. It does not execute against a TinkerGraph copy. This
profile does not establish Rust planner or Rust callback conformance.

The production module is `net.orchiddb:orchiddb-jvm:0.1.0`. It depends on pinned
TinkerPop libraries and Jackson, independently of the SQLg conformance adapter.
Java 21, Maven and the repository's Rust toolchain are required. Build and test
locally:

```sh
cargo build --no-default-features --bin orchiddb-jvm-store
export ORCHIDDB_JVM_STORE="$PWD/target/debug/orchiddb-jvm-store"
mvn -f jvm/pom.xml install dependency:build-classpath -Dmdep.outputFile=classpath.txt
```

Use `jvm/orchiddb-jvm --store "$ORCHIDDB_JVM_STORE"` for an in-memory graph or
add `--path /absolute/path/graph.snapshot` for durable native snapshots. The path
is a snapshot file, not a DuckDB database. The bridge uses the existing native
snapshot codec and an exclusive file lock; a second process cannot open the same
snapshot concurrently. Commit writes a temporary file, syncs and atomically
replaces the snapshot. Uncommitted state is discarded on close, process death or
cancellation. Incremental DuckDB persistence remains the separate native engine
storage option.

The command accepts one JSON object per line and emits one response per line:

```json
{"script":"g.addV('person').property('name',name).values('name')","bindings":{"name":{"type":"string","value":"Ada"}},"timeout_ms":30000}
{"script":"g.V().map { t -> t.get().value('name').length() }"}
```

Responses identify `execution_profile: "orchiddb-jvm"` and protocol version 1.
Successful `result` values use GraphSON 3, retaining graph elements, non-string map
keys, paths, sets and numeric widths. Input bindings use the typed native value
protocol described below. Errors produce `ok: false`; callback exceptions are
not converted to empty results. Groovy submissions are trusted application code
with ordinary JVM privileges. This interface is not a sandbox or network server.

Applications can use the same capability directly:

```java
try (OrchidGraph graph = OrchidGraph.open(nativeExecutable, snapshotPath);
     OrchidJvmExecutor executor = new OrchidJvmExecutor(graph)) {
    List<Object> names = executor.submit(
        "g.V().values('name')", Map.of(), Duration.ofSeconds(30)).get();
}
```

`submit(Bytecode, Duration)` uses the pinned upstream JVM bytecode/lambda
evaluator. `remoteConnection()` supplies an embedded asynchronous TinkerPop
`RemoteConnection`: requests execute on the session worker against the native
store. This is a remote-submission API, not a claim of network transport. Native
Edge objects and actual Java Set bindings can be supplied without grammar
stringification. Closures receive upstream traversers, including paths, labels,
loops, bulk, side effects and sacks; arbitrary-precision values remain Java
BigInteger/BigDecimal values.

Each executor owns its graph session and serializes submitted traversals. A
submission fully consumes its results, commits on success and rolls back on
failure. Do not mix direct graph access or manually opened transactions with an
executor that owns that graph. A timeout or cancellation interrupts Groovy code,
terminates the native store and invalidates the executor. Later requests fail;
open a new session to continue. Native I/O has a bounded response deadline.
Blocking application code that ignores JVM interruption can outlive the worker
deadline, but its native store is terminated and it cannot commit graph writes.

Direct `OrchidGraph` users control `tx().open()/commit()/rollback()`. Failed native
operations restore their statement snapshot. Transaction status is thread-local;
write transactions serialize until their owner commits or rolls back. Other
threads read the last committed native state and wait before starting a write.
Committed reads bypass the owning writer's caches and retain committed element
identities, metadata and runtime values while writes are pending. The native
handshake must advertise this capability; older binaries reject such reads
explicitly. A helper thread's
empty transaction cannot complete another thread's writes. Waiting operations
support interruption and stop when the graph closes. GraphComputer borrows its
submitting thread's transaction under an exclusive lease and returns ownership
after execution, preserving caller rollback of published properties.
`atomicMutation(Runnable)` provides
a savepoint without discarding an enclosing caller transaction. Operations within
that block share its checkpoint; a failed operation poisons the block until it
is rolled back. `bulkLoad(Runnable)` additionally defers commits requested by
upstream readers until the import completes, preserving any caller transaction.
`executionLease()` reserves a graph during a multi-request operation; acquire
and release it on the same executing thread. Separate graph instances own
separate native processes. Concurrent traversal access to a single direct Graph
is not an advertised provider feature.

The provider keeps bounded native adjacency and property records between reads.
Every mutation, transaction completion, native error and close invalidates these
caches; atomic mutation blocks bypass them. Set JVM system properties
`orchiddb.native.adjacencyCacheSize=0` or `orchiddb.native.propertyCacheSize=0`
to disable either cache. Defaults are 4,096 and 16,384 requests respectively.

`freshGraph()` creates an independent in-memory native graph. A returned
GraphComputer result graph must be closed by its owner and can outlive the source
graph. Executor close/cancellation explicitly terminates its entire graph family,
including intermediate compute results. Direct graph close affects that graph
only.

`OrchidGraph.open(Configuration)` accepts `orchiddb.native.executable`, optional
`orchiddb.native.path`, `orchiddb.vertex.defaultCardinality` (`single`, `list`
or `set`), and `orchiddb.vertex.idManager`, `orchiddb.edge.idManager`,
`orchiddb.vertexProperty.idManager` (`ANY`, `INTEGER` or `LONG`). `ANY` preserves
supported numeric and string public ID types; it does not advertise support for
arbitrary Java object IDs. Numeric ID lookup accepts string representations,
preferring an exact stored string ID when both forms exist. Reader fixture
configuration can therefore preserve multi-properties without changing the
ordinary single-cardinality default.
Constructor key/value pairs retain list cardinality, including repeated keys,
independently of the default used by subsequent property assignments. Numeric
ID managers generate numeric vertex and edge IDs and skip existing public IDs.
User property names beginning with `__`, including TinkerPop's `__id` strategy
key, are supported through the JVM provider. Internal native columns remain
hidden, and ordinary native property visibility is unchanged.

The JVM provider registers the compatible `tinker.search` and
`tinker.degree.centrality` service names. These scan actual native properties and
adjacency, including metadata, Java regular expressions, selected directions and
traverser bulk; they do not load reference fixture results.

The native protocol uses `{ "op": ..., "version": 1 }` requests and
`{ "ok": true, "value": ... }` or `{ "ok": false, "error": ... }` replies.
Scalars have `{ "type": ..., "value": ... }` records. Supported types include
byte, short, int, long, float, double, bigint, bigdecimal, boolean, string, null,
list, set and map. Maps contain typed key/value pairs. Big numbers use decimal
strings; non-finite floating values retain explicit spellings. Elements carry
typed public identity and opaque native handles scoped to a graph session.
VertexProperty and Property records retain their owner and metadata. Stale and
cross-session native handles are rejected; public references resolve by identity.

GraphComputer may need live JVM `TraverserSet` and `Traverser` property values.
These use the dedicated `jvm_runtime` protocol type, explicit native metadata and
a graph-family registry. Ordinary user maps are never interpreted as handles.
This state is session-only: persistent graph mutations containing such values
are rejected. Closing the final family member releases its registry. Ordinary
persisted properties remain native typed values. The JVM provider enables stored
null values explicitly; the Rust language profile retains its existing default
null-as-removal policy.

The original Gherkin assertions remain in the pinned upstream test library.
`--engine orchiddb-jvm` and `--engine orchiddb-computer` select separate local
conformance profiles; `--engine orchiddb` continues to measure Rust traversal
execution. For the 15 Java-only upstream placeholders, the JVM adapter invokes
the original pinned JUnit counterparts through the Java provider harness. Each
scenario records its Java assertion source, outcome, timing and build provenance;
the original Gherkin placeholder text remains unchanged.
