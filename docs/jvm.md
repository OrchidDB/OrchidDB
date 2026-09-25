# Native JVM execution

These production modules are independent of the SQL-only Java client in [OrchidDB-java](https://github.com/OrchidDB/OrchidDB-java).

## JVM executor over native OrchidDB storage

For JVM fragments inside a managed query, see [relational DAG execution](runtime.md). The standalone profile described below remains available.

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


## Native-provider JVM GraphComputer

OrchidDB's single-process GraphComputer executes the pinned TinkerPop 3.7.4 vertex programs over `OrchidGraph`, whose vertices, edges, properties, identity, and mutations belong to the native store. It is a separate execution profile from the native Rust traversal planner. It does not construct a TinkerGraph or substitute reference results.

The public entry points are `graph.compute()` and `graph.traversal().withComputer()`. See [JVM executor setup](jvm.md) for the production module and native-store executable. The conformance adapter identifies this profile as `orchiddb-computer`; native Rust evidence remains separately reported as `orchiddb`.

### Execution and state

One worker runs the upstream vertex program through supersteps, with iteration-delayed local/global messages, optional message combination, broadcast memory, memory reducers, worker lifecycle, termination, and map/reduce output sorting. `ComputerGraph` enforces the upstream star-graph access rules. Graph filters constrain loaded vertices, edges, and vertex properties.

A provider-backed view isolates compute properties from ordinary graph properties. It supports arbitrary JVM computation-local values, compute-key cardinality and metadata, and transient-key cleanup. Rich halted-traverser values that cross chained jobs use the native provider's explicit session-family runtime references; these are runtime state, not durable serialized graph properties.

A computation holds the provider's execution lease. Native source vertex, edge and meta-property records are cached only for that immutable view, and computed-key reads resolve directly from the overlay. `Persist.NOTHING` leaves the original graph unchanged. Publishing to `ResultGraph.ORIGINAL` uses a native savepoint; failures restore prior properties and preserve unrelated caller transaction writes. Caller-owned transactions remain open. A transaction opened by the computation is committed only after successful original-graph publication, or rolled back when it held only reads.

`ResultGraph.NEW` creates another native-backed graph through the provider factory. `VERTEX_PROPERTIES` copies filtered vertices with their existing/explicit property IDs, cardinality and metadata; `EDGES` also copies filtered edges and their properties. Copying reserves all existing and explicit property IDs before allocating IDs for generated compute properties, so configured native ID types and future source IDs are respected. Original-graph publication also reserves explicit IDs before allocating generated IDs. It uses an atomic native batch. Failure closes the incomplete new graph. A successfully returned new graph survives closing the original; callers close result graphs, or use `closeFamily()` when disposing an entire traversal session and its intermediate graphs.

Cancellation interrupts the worker. The provider drains a submitted native response before propagating cancellation, so the protocol remains synchronized. Cleanup temporarily clears the interrupt flag while rolling back owned read transactions or savepoints, then restores it. Explicit executor abort remains a separate operation that terminates its session family.

### Supported algorithms and boundaries

ConnectedComponent, PageRank, PeerPressure and ShortestPath use the pinned upstream vertex-program implementations, including their traversal modulators, edge selection, property names, iteration settings and chaining. The native Rust grammar path explicitly reports that these steps require the JVM GraphComputer profile. The previous fixture-name-based virtual algorithm properties have been removed; ordinary reads return only stored property values.

The initial computer supports one worker. It advertises no vertex/edge addition or removal, no edge-property mutation, and no vertex-property removal capability. Compute-key replacement and lifecycle cleanup remain supported. This is an in-process execution capability; distributed execution is not claimed.

### Verification profiles

All execution is local. Original assertions and pinned source hashes remain unchanged. The [algorithm Java selection](../conformance/adapters/jvm-provider-tests/computer-algorithms.json) identifies the 31 original methods. The [provider harness](../conformance/adapters/jvm-provider-tests/README.md) records original assumptions and the official `is.testing` and `assertNonDeterministic` flags separately from outcomes.

The upstream ProcessComputerSuite declares 868 test occurrences and 852 unique method IDs: its 16 ProfileTest methods appear twice. The [local computer suite runner](../conformance/adapters/jvm-provider-tests/run_computer.py) retains both occurrences, runs each class in an isolated JVM, and records frozen native/runtime identities, original source hashes, assumptions and timeouts. Its `--exclude-algorithms` option leaves the 31 algorithm methods to their separate selection and executes the remaining 837 occurrences. Run `python3 conformance/adapters/jvm-provider-tests/run_computer.py --help` for the required pinned-source and frozen-runtime arguments. Supplemental tests are reported separately from the upstream suites.

The native algorithm-boundary change preserves the baseline passing set: 1,427 passes, 83 skips, one unsupported case, and zero failures/timeouts/adapter errors in the unchanged 1,511-case catalog. Twenty-one supplemental native-provider computer tests cover arbitrary topology and renamed IDs, weighted tied paths, direction and distance limits, zero iterations and algorithm chaining, result/persist modes, filters, multi/meta-properties, messages/memory/transients and mutable broadcast reducer isolation, result ownership, publication rollback and cancellation. These checks are recorded in the [supplemental report](../conformance/upstream-results/orchiddb-computer-supplemental.json).

The frozen JVM `cbd8fa3` and native store `2ae5e75` pass all [31 original algorithm Gherkin scenarios](../conformance/upstream-results/orchiddb-computer-algorithms.json), with unchanged assertions and deadlines. The grateful weighted shortest-path case completes in about 30 seconds within its 90-second deadline. The [original ProcessComputerSuite report](../conformance/upstream-results/orchiddb-process-computer.json) records 700 passes and 168 upstream skips across all 868 declared occurrences, with zero failures or timeouts. It combines 31 original algorithm Java methods executed in a fresh data directory with 837 remaining occurrences executed in isolated class JVMs. All 37 GraphComputer contract tests and all 10 interruption tests pass.

The 168 exclusions remain visible: 137 upstream JUnit ignores, 16 TinkerGraph-specific assertion guards, 14 COMPUTER profile guards, and one negative null-property feature requirement because this provider supports stored nulls. These are not counted as passes. Native Rust traversal evidence, JVM algorithm scenarios, original Java suite occurrences, and supplemental regressions remain distinct profiles.
