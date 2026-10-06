# PostgreSQL and mixed SQL engines

This is a retained internal library reference. The current product is the
[DuckDB extension](../extension/README.md), which uses only its host DuckDB executor.
The legacy adapters documented below are not the extension execution path.

The shared JSON compiler accepts `duckdb` and `postgres`. Cypher, Gremlin and
SPARQL use the same lowering and dialect adapters. Connections, credentials and
transactions belong to the application.

A single-engine request continues to use `dialect`, `tables`, `nodes` and `edges`.
For multiple connections, add an engine registry, an execution engine and table
ownership:

```json
{
  "version": 1,
  "dialect": "duckdb",
  "execution_engine": "analytics",
  "engines": {
    "analytics": {"dialect": "duckdb"},
    "operational": {"dialect": "postgres"}
  },
  "language": "cypher",
  "query": "MATCH (p:Person) WHERE p.age > 25 RETURN count(p) AS count",
  "tables": [{
    "name": "people",
    "engine": "operational",
    "columns": [
      {"name": "id", "data_type": "int64", "nullable": false},
      {"name": "age", "data_type": "int64"}
    ]
  }],
  "nodes": [{
    "label": "Person", "table": "people", "id": "id",
    "properties": {"age": "age"}
  }]
}
```

Engine IDs identify connections, not dialects. Two PostgreSQL connections are
still different engines. Tables without `engine` belong to `execution_engine`.
Its dialect must match the request. Unknown engines are rejected before execution.

## SQL islands

The planner pushes filters and projections down, then attempts to compile the
largest closed relational subtree on the engine that owns its sources. This
includes same-engine joins, grouping, aggregates, ordering and limits. A subtree
that spans engines stays on the execution engine; its independent source
subtrees become SQL islands. Correlated subtrees are not detached from their
outer bindings. Declared native UDFs stay on their configured execution engine.

The compiled result contains `sql`, `field_types`, `execution_engine` and
`transfers`. Each
transfer supplies `source_engine`, `source_dialect`, source SQL, a
`target_relation` name and typed columns. PostgreSQL can execute the filter and
count in the example and transfer one aggregate result. Unused tables produce
no transfers.

The coordinator binds each island's result as a typed CTE in the final read
statement. **Execution never creates tables, temporary tables, or views.**
Source relations belong to the caller. Queries requiring managed runtime
materialization are rejected by this SQL-only API. Query results are closed even
when binding or the consuming callback fails.

Bindings use the shared compiler's `bind` operation with Arrow IPC, Arrow C
streams, or typed JSON rows. Values are encoded by type; SQL supplied as data
remains data. The coordinator buffers island results to construct the final
statement, so result size affects memory use and SQL size. Pushdown reduces that
volume. Independent sessions retain their own snapshots and transaction policy.

## Connectors

- **Python:** `PostgresEngine(psycopg_connection)` and `DuckDBEngine(connection)`;
  `query_federated(compiler, request, {id: engine})` returns a context manager for
  an Arrow reader. Install the `postgres` extra for psycopg and PyArrow.
- **Java:** `NativeSqlCompiler.compileJson(requestJson)` and
  `FederatedQuery.query(compiler, requestJson, engines)` use named `ExecutionEngine`
  sessions and bind JDBC result rows through the shared compiler.
- **Rust:** `federation::Session` is the application adapter and
  `federation::execute` executes a compiled plan using a registry of sessions.
  This convenience API returns materialized Arrow batches.
- **TypeScript:** `queryFederated(compiler, request, engines, consume)` uses
  existing `ExecutionEngine` adapters and consumes the result inside the callback.
- **C++:** `query_federated(compiler, request, engines, consume)` uses
  existing read adapters and Arrow C streams. The native binder consumes each
  source stream without copying it into a database object.
- **Elixir:** `OrchidDB.query_federated/4` accepts engines with `:dialect`, `:query`
  callbacks. Sources can return ADBC streams directly or row arrays;
  `:exchange_data` can adapt other drivers to typed rows or Arrow IPC.

The compiler/native library remains driver-free. Generic adapters use the
application's PostgreSQL driver or Arrow transport. Existing single-engine
execution helpers reject plans requiring transfers rather than executing an
incomplete plan.

The SQL compiler remains a read interface. Managed graph writes and native graph
values use the existing managed executor; this protocol does not add distributed
writes. PostgreSQL nested lists use one-dimensional `JSONB[]` values whose cells
contain child lists. This preserves ragged, empty and null children. Existing
PostgreSQL multidimensional source arrays are normalized into that representation.
Adapters receive logical `field_types` so they can decode nested results without
losing binary, decimal, date or time values.

## Validation

Run the dynamic engine matrix against an application-provided local PostgreSQL
connection (fixtures use session-local temporary tables):

```sh
export ORCHIDDB_NATIVE_LIBRARY=/path/to/liborchiddb_compiler.dylib
export ORCHIDDB_TEST_PG_URL='dbname=postgres'
python -m pytest ../orchiddb-python/tests/test_postgres_federation.py
```

It covers all three query languages, both single-engine layouts, mixed execution
in both directions, same-engine remote islands, native function declarations,
operator semantics, filter/aggregate pushdown, resource cleanup, and execution
that issues only read queries.

## Full language conformance

The common execution DAG supports caller-owned PostgreSQL sessions through
`GraphEngine::set_sql_region_session(Box::new(PostgresRegionSession::new(client)))`.
Eligible regions execute on the selected SQL engine; DataFusion executes the
existing residual graph and language kernels. Unsupported operators split the
plan into smaller SQL islands during planning. SQL execution errors propagate;
they never trigger a retry on another engine. Managed fixture storage is separate
from the selected SQL execution engine.

Run the **complete original language suites independently** on both engines:

```sh
# Configure the native/JVM binaries using conformance/README.md first.
export ORCHIDDB_TEST_PG_URL='host=localhost dbname=postgres'
python conformance/upstream/engine_matrix.py
```

The matrix invokes the existing upstream harness for all 3,897 openCypher,
1,511 Gremlin and 1,125 SPARQL catalog entries per engine. It preserves original
assertions, source hashes, skips and applicability decisions. It fails if any
applicable case fails, times out, or encounters an adapter error, if catalog
coverage is incomplete, or if measured SQL regions use the other engine.
Each backend has its own report; pass counts are never merged across engines.
The PostgreSQL runner disables JIT and sets an 8-second statement timeout on its
own conformance connection. Applications retain control of their session settings.

`conformance/upstream/sql_engines.py` is a separate, limited diagnostic for
empty-graph SQL-only Cypher reads. It is not the three-language conformance
suite. Its optional baseline comparison is diagnostic; shared failures still
produce an unsuccessful exit status.

### Validated results, 2026-10-01

The final optimized run completed all 6,533 catalog entries independently on
each engine. There were **zero failures, timeouts, or adapter errors**.

| Language | DuckDB | PostgreSQL |
| --- | ---: | ---: |
| openCypher | 3,897 / 3,897 passed | 3,897 / 3,897 passed |
| Gremlin | 1,511 / 1,511 passed | 1,511 / 1,511 passed |
| SPARQL, applicable cases | 974 / 974 passed | 974 / 974 passed |

Each SPARQL report also records 77 existing skips (70 require reasoning datasets,
7 require SERVICE fixture endpoints) and 74 cases outside the embedded API
(HTTP protocol and wire serialization). These are not counted as passes.
PostgreSQL reports contain zero DuckDB SQL regions; DuckDB reports contain zero
PostgreSQL SQL regions. Fixture storage and native residual operators retain
their existing implementation.

See [the recorded summary](../conformance/sql-engine-results.json) for counts,
source revisions, region counts and full-report hashes. Full per-case reports
are under `target/conformance/sql-engines/`.

Connector checks passed with real local database connections where supported:
Python 142, JavaScript 17, Java 60 (including Gremlin integration), Rust 19,
Elixir 8, and the C++ integration executable. Core validation passed 213 unit
checks and 28 compiler, execution, federation and mapped-source checks. The
native ABI smoke check passed. The C++ adapter protocol was checked with DuckDB;
applications supply their PostgreSQL Arrow adapter.

### Execution constraints

- The standalone SQL compiler supports its existing read-query subset. Queries
  requiring native graph kernels use the common execution DAG described above.
- SQL islands exchange scalar columns and homogeneous lists, including nested
  lists. Structs, graph runtime values and unsupported Arrow types are rejected
  at that boundary.
- Island results are buffered into typed SQL literals. Large transfers increase
  memory use and statement size; no streaming exchange or distributed snapshot
  is provided. Transactions remain caller-owned.

### Source publication

The connector repositories pin the supporting core and native commits. The Rust
binding uses the published core Git revision without a local Cargo patch.
Build native artifacts from the clients' pinned revisions so their embedded
compiler identity matches the client metadata.
