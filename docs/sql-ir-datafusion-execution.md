# Relational DAG execution

The managed `GraphEngine` lowers each query before execution:

```text
language parser and planner
        |
     Graph IR                 frontend only
        |
DataFusion relational LogicalPlan DAG
        |
physical planning
        +-- DuckDbExec        eligible SQL regions
        +-- DataFusion        relational operators and native kernels
                +-- JvmExec   explicit native JVM fragment
```

There is no Graph IR interpreter fallback in the managed engine. The historical
interpreter remains a reference implementation for tests. Scalar evaluation,
aggregation, graph access, and result-formatting kernels are shared with it;
DataFusion owns physical execution and input scheduling.

## Lowering and placement

`ir::rel::runtime` compiles frontend operators into relational plans. SQL-compatible
regions use the existing relational compiler. Residual operators become
DataFusion logical extensions with compiled scalar metadata and relational
inputs. They retain no executable Graph IR subtree.

`ir::rel::dag` partitions the resulting relational plan. DuckDB regions are
explicit sources in the physical DAG; their inputs are registered Arrow tables.
DataFusion executes the remaining joins, expressions, and extension kernels.
Extension nodes, volatile or unknown scalar UDFs, and empty relations are excluded
from SQL regions. Explicitly bound DuckDB-native functions retain their native
placement, including native volatility and null semantics. SQL execution errors propagate; execution is never retried in
another engine.

Correlated branches and repeat bodies are compiled before execution. Their
frontiers are query-local relations, and their physical plans execute through
DataFusion. Physical subplans are cached within a query, with fresh frontier and
state for every invocation; neither rows nor effects are cached. A loop preserves shared traversal state, bulk, named loop counters,
barrier scope, and cancellation across iterations. Group reducers split their
insertion prefix and finalizer during compilation, so reading a group does not
repeat its writes. Bounded lazy side-effect pipelines retain bounded consumption.

Inputs with observable effects run in dependency order. A statement containing
mutating branches uses live native scans so a later branch sees earlier writes.
Correlated subplans use live native scans for the same reason. Ordinary read
regions and a pure prefix before a JVM operation remain eligible for DuckDB.

## Arrow boundary

Relational scans and SQL regions use ordinary Arrow columns. The boundary adapter
reconstructs graph identities and declared scalar types. Native extension kernels
exchange a fixed Arrow schema:

| Column | Type | Meaning |
| --- | --- | --- |
| `bindings` | non-null Binary | Tagged native value map |
| `bulk` | non-null UInt64 | Traverser multiplicity |

The tagged encoding preserves integer widths, arbitrary-precision numbers, graph
and property identities, paths, maps, sets, nulls, and hidden traverser bindings.
It does not infer a schema from the first JVM result. This representation trades
columnar scalar performance for exact language semantics at native boundaries;
ordinary relational regions keep their typed columns.

## JVM fragments

The frontend emits `GraphJvm`, which is compiled to a relational `Jvm` extension
and a DataFusion physical operator. The JVM receives only the fragment and typed
arguments. Native graph callbacks operate on the statement's current overlay.
The outer query owns commit and rollback.

```groovy
g.V().call('crabgraph.jvm', [
  'script': 'current.value("name").toUpperCase()',
  'mode': 'map'
])
```

Supported fragment modes are `map`, `flatMap`, and Boolean `filter`. Configure
`CRABGRAPH_JVM_CLASSPATH` with the production JVM artifact and dependencies;
`CRABGRAPH_JAVA` selects Java (Java 21 is used for local verification). Fragment
text is trusted application code; binding values are transported separately.

GraphComputer NEW results use private native graph stores. Project their elements
to values before returning across the fragment boundary. The source graph remains
unchanged by a NEW result graph.

## Atomicity and cancellation

Execution uses a private statement overlay, including uncommitted caller writes.
Only successful execution and successful result conversion publish that overlay.
A native, SQL, JVM, or result-conversion error leaves the caller's graph unchanged.
Managed transactions retain their existing durable commit/rollback behavior.

JVM execution shares a cancellation token and deadline across fragments and
nested subplans. Worker I/O is bounded, and failed or cancelled workers are killed
and reaped. A JVM fragment cannot commit, roll back, or close the caller's native
transaction. DuckDB retains the engine's configured SQL timeout.

## Local verification

With Java 21 selected through `JAVA_HOME`, run:

```sh
bash conformance/run-relational-gremlin.sh
```

This builds the native runner, JVM provider, codec module, and upstream adapter;
runs the focused regression suites; then records the complete pinned Gremlin
suite under `target/conformance-datafusion/`. An optional first argument selects
the output JSON path. It refuses to execute in GitHub Actions.


`tests/jvm_ir.rs` verifies physical placement, typed transport, bulk and hidden
bindings, correlated JVM repeat, GraphComputer result ownership, cancellation,
and transaction rollback. Its JVM tests require the production classpath and
must be run with `--include-ignored`.

The pinned Apache TinkerPop scenarios run through `GraphEngine` using
`conformance/upstream/run.py --engine crabgraph --suite tinkerpop`. That exercises
the production relational executor, not the reference interpreter. GitHub Actions
publishes committed static results only; it does not execute these tests.

## Typed Gremlin callbacks

`GraphEngine::gremlin_with_bindings` accepts `GremlinBinding::Value`,
`GremlinBinding::Predicate`, and trusted `GremlinBinding::Lambda` arguments.
The frontend lowers callbacks and vertex programs into operators in the SQL IR
DAG. The conformance adapter submits every traversal to this same API; it does
not choose a different executor for individual scenarios.

JVM workers are shared within a query, including correlated and repeated
operators. Each operator borrows the statement's native graph overlay. Failure
invalidates the worker and rolls back the statement. Comparator operators reorder
original rows, retaining their labels, paths, and bulk. Vertex-program properties
are visible to later operators in the query without being committed to the graph.

Enable JVM operators by setting `CRABGRAPH_JVM_CLASSPATH` to the built production
JVM classes and dependencies, and optionally `CRABGRAPH_JAVA` to the Java executable.

### Write storage

Managed mutations, including JVM callback writes, currently modify the native
`PropertyGraph` overlay. `GraphEngine` persists incremental records in
`__crabgraph_records` and checkpoints in `__crabgraph_state`. This is separate
from the external table mappings used by relational reads. Mapped write-through
requires mutation lowering against those same table/column mappings and a shared
transaction; it is not implemented by this execution change.
