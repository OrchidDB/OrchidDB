# Optional relational engine and source-first execution

Follow-up scope: [Three execution suites and a smaller Orchid release](execution-testing-and-scope-plan.md)
contains the complete testing and feature-removal plan. Its native-first release
boundary supersedes the multi-client distribution scope below. The supported
native execution ABI and proposed removals remain follow-up work.

Status: implemented through explicit source-first client APIs. Existing single-session
APIs remain available; legacy Trino completion-engine configurations fail with a
migration message. The README was not changed.

Implementation entry points:

- `src/execution/plan.rs`: typed source/UDF/completion plan, source pushdown,
  local function placement, validation, bounded requests, and lazy completion.
- `src/compiler.rs`: internal version-2 plan output without mandatory final SQL;
  local scalar declarations use `local: true`.
- Rust, Java, JavaScript, C++, and Go expose source-first connection APIs;
  Elixir exposes `OrchidDB.SourceConnection`. Python `Connection` accepts `none`
  or `duckdb` with source/function bindings and an optional lazy completion adapter.
- Default core and native library builds exclude DuckDB. Trino has a source-only
  constructor and rejects intermediate relation inputs before network access.

Local functions currently preserve input row cardinality. They can return nested
values, but arbitrary local relational operators and local aggregate functions are
not introduced. Unsupported plans fail explicitly; short-circuit/conditional
function hoisting remains conservative. Row-oriented clients reject literals they
cannot represent; Arrow clients retain exact literal types through IPC.

Validation completed locally: core plan/execution/federation tests; mocked Trino
paging, cancellation, and intermediate-input rejection; source/UDF client tests;
end-to-end native Python, Java, C++, and Elixir tests; DuckDB-enabled and engine-free
build checks. Native dependency/link inspection found no DuckDB or PostgreSQL driver.
The optional live Trino test requires a running deployment and was not exercised.
Petabyte datasets and million-QPS workloads have not been benchmarked.

## Objective

Make Orchid's relational engine optional. Supported engine selections are `none`
and `duckdb`. Trino is a SQL-capable source, alongside application-provided SQL
sources and service adapters. Push supported work into sources and execute local
UDFs in the host runtime. Use DuckDB only for remaining relational operations that
the compiled plan explicitly assigns to it.

An engine-enabled connection must not manufacture a DuckDB query, initialize a
DuckDB session, or register source results when the plan does not need DuckDB.
The engine-free distribution must not compile, link, load, or initialize DuckDB.

## Required contracts

1. **Engine and source are separate concepts.** The engine completes relational
   work over source results. A source owns SQL or service-request execution and
   advertises its supported operations. Source SQL dialect does not select an engine.
2. **Engine selection grants capability.** `duckdb` permits DuckDB operations;
   `none` forbids them. Neither requires a final engine query.
3. **Applications own SQL drivers.** Clients accept a caller-provided SQL execution
   interface. Orchid submits generated SQL and bound parameters and consumes typed
   results. Never add or bundle a PostgreSQL driver in production Orchid packages.
4. **Local UDFs are engine-independent.** Their implementations run locally with
   explicit dependencies and typed inputs/outputs; they need no DuckDB registration.
   Functions available only through a database remain source- or engine-owned.
5. **Trino is a pushdown destination, not a completion engine.** It can execute an
   entire supported query, including joins across its own connectors. Results flow
   back to Orchid for local UDF work or DuckDB completion when necessary. Do not
   upload intermediate relations back into Trino to finish an Orchid plan.
6. **Validate before execution.** Reject unsupported operations, missing function
   bindings, missing source bindings, and required-but-disabled engine operations
   before opening source cursors or invoking functions. Never silently enable an engine.
7. **Keep public queries execution-oriented.** Schema registration remains separate
   from query text, parameters, and authorization. Generated SQL and plan objects
   stay internal; do not introduce a public compile-only API.

## Execution examples

| Plan | Engine `none` | Engine `duckdb` |
| --- | --- | --- |
| Entire query lowers to Trino | Return Trino results directly | Same; no DuckDB activity |
| Entire query lowers to an application-owned PostgreSQL session | Return source results directly | Same; no DuckDB activity |
| Source query followed by local reranking UDF | Execute source and UDF | Same; no DuckDB activity |
| Local embedding UDF followed by vector search | Execute UDF and source request | Same; no DuckDB activity |
| Join fully supported within one SQL source | Push the join into that source | Same; no DuckDB activity |
| Join between independent sources that cannot be pushed down | Reject before execution | Consume source batches and join in DuckDB |
| Local UDF followed by a relational aggregate that cannot be pushed down | Reject before execution | Run the UDF, then aggregate in DuckDB |

A RAG pipeline can be `input → local embedding → vector search → parameterized
SQL fetch → local reranking → result`. Dependent source requests use explicit
binding and batching contracts. They must not implicitly implement arbitrary
cross-source joins or intermediate-table uploads. Correlating batched responses
must preserve input identity, multiplicity, and declared ordering semantics.

## Historical implementation constraints

The following records the pre-change assumptions used to scope implementation;
it is not a description of the current implemented state:

| Location | Current assumption to remove or separate |
| --- | --- |
| `src/execution.rs` | `ExecutableEngine` contains DuckDB and Trino; SQL execution validates a dialect against that engine enum |
| `src/compiler.rs` | `CompiledSql` requires top-level `dialect` and `sql`, with transfers targeting an execution engine |
| `src/federation.rs` | Validation, request routing, and stream execution require a SQL execution target; SQL-source support is coupled to engine recognition |
| `src/session.rs` | Schema validation injects a DuckDB dialect; host bindings expose the current engine/source split |
| `src/execution/trino.rs` | Trino SQL execution is coupled to an executable-engine session and exchange bindings |
| `src/ir/functions/host_execution.rs` | Some host functions execute SQL through a DuckDB-oriented scope; these must not become the implementation of local UDFs |
| `src/ir/rel/runtime/` and `src/ir/rel/host/` | Audit reusable scheduling, local function, and transport behavior for implicit engine dependencies |
| `Cargo.toml` and client/build manifests | Root default features currently enable DuckDB; audit feature unification and packaging across clients |
| `extensions/trino/` and Trino exchange tests | Identify and retire the intermediate-relation exchange path once no supported execution path needs it |

Retain DataFusion's existing logical planning components. Do not restore its
physical executor or build another general relational executor for `none`.

## Implementation phases

### 1. Separate configuration, source capabilities, and bindings

- Introduce an explicit engine selection with `none` and `duckdb`; keep source
  identifiers independent of that selection. Exact SDK spelling can follow each
  language's conventions, but semantics must match.
- Model SQL sources by dialect, capabilities, and a caller-owned execution binding.
  Model request sources by their service adapter and request binding.
- Move Trino into SQL source classification. Preserve its connector mappings,
  SQL lowering, authentication options, and supported function signatures.
- Define source capabilities conservatively: expression/function support, joins,
  aggregation, ordering, limits, parameter binding, and dependent request support.
  Same dialect does not mean same source, connection, or transaction.
- Define local UDF signatures and runtime bindings independently of SQL function
  registrations. Record determinism, effects, batching, cardinality, and any
  ordering requirements needed for safe placement and reuse.
- Keep credentials and executable callbacks in host bindings, not published ontology
  metadata. Remove implicit DuckDB assumptions from schema-only validation.

Deliverable: configuration and capability validation with explicit migration errors.

### 2. Introduce an engine-independent execution plan

- Replace the mandatory final-SQL shape with an internal typed dependency graph.
  Proposed node roles: `SourceSql`, `SourceRequest`, `LocalUdf`, `DuckDbRelational`,
  and a result reference. Names are illustrative, not a new public API.
- Store SQL dialect on each SQL operation, not on the entire plan. Each node
  declares its inputs, output schema, source/function binding, and dependencies.
- Make the result reference point directly to the terminal source, UDF, or engine
  output. Preserve result-form decoding without a `SELECT *` wrapper query.
- Support constant/parameter inputs and trivial output assembly without an engine;
  do not disguise joins, sorts, or aggregates as result assembly.
- Preserve diagnostics and optimizer evidence. Explain output must show actual
  execution locations and whether DuckDB is required.
- Version internal serialized/native protocols as needed; reject mismatched
  client/runtime versions rather than interpreting old fields ambiguously.

Deliverable: represent and validate source-only, UDF-only, mixed, and DuckDB plans.

### 3. Partition and lower by execution location

- Reuse language frontends, Graph IR, logical optimization, and existing source
  lowering. Push the largest semantically valid regions to their owning sources.
- Respect local UDF boundaries. Do not move non-deterministic or effectful calls,
  duplicate their execution, or push functions into sources without an equivalent
  declared implementation.
- Preserve null behavior, types, duplicates, ordering, limits, and graph semantics
  when splitting plans. Unsupported source semantics are a real planning boundary.
- If no residual relational work remains, emit no DuckDB operation or DuckDB SQL.
- If residual relational work remains and the selected engine is DuckDB, lower
  only those regions to DuckDB over typed inputs.
- If the engine is `none`, report the unsupported operation and its location.
- Permit explicit parameterized dependent source calls. Do not reintroduce Trino
  exchange uploads through an optimization or fallback path.

Deliverable: plan-shape tests demonstrating maximal safe pushdown and no final SQL requirement.

### 4. Execute sources and local functions without an engine

- Reuse existing batch transport and request batching where possible. Validate
  the complete plan before any externally visible execution begins.
- Define the SQL execution contract around SQL, typed bound parameters, typed
  results, cancellation, and errors. Preserve caller-owned connection and
  transaction lifetimes. This is an adapter interface, not a required HTTP service.
- Stream terminal source results directly. Use Arrow batches where composition
  requires them, preserving existing native result paths where compatible.
- Execute local UDFs through a host runtime binding with no DuckDB host scope.
  Keep local and database-owned functions distinguishable in planning and errors.
- Add bounded concurrency, backpressure, cancellation propagation, and deterministic
  cleanup. Respect session reentrancy limits for dependent operations.
- Initialize DuckDB through a lazy provider only when a `DuckDbRelational` node
  runs. Register only that node's needed inputs and release them after consumption.
- Preserve a supplied DuckDB host session for extension users without creating an
  additional session. The no-initialization guarantee concerns Orchid-created work.

Deliverable: direct execution and local UDF pipelines work with an engine provider
that fails immediately if called.

### 5. Remove engine dependencies from core builds and clients

- Make default library compilation DuckDB-free. Gate DuckDB adapters, bindings,
  initialization, checkpoint implementations, and extension hosting explicitly.
- Audit native, JVM, Rust, and other client build paths for transitive feature
  activation. A no-engine artifact must have no DuckDB dependency in its selected
  production build graph and no linked or packaged DuckDB library.
- Separate durable `GraphEngine` features that require DuckDB checkpoint storage;
  do not construct that runtime for stateless source/UDF execution. Engine-free
  durable storage is outside this change unless an existing independent backend fits.
- Build the optional DuckDB distribution/extension explicitly. A runtime `none`
  setting in a DuckDB-enabled artifact is not proof of the DuckDB-free distribution.
- Update all client execution bindings and protocol adapters together. Applications
  supply PostgreSQL drivers; production dependency trees must contain none added
  by Orchid. Test applications may own external drivers for integration coverage.

Deliverable: independently built and exercised engine-free and DuckDB-enabled artifacts.

### 6. Migrate Trino and existing configuration

- Replace Trino engine sessions with source execution bindings. Reuse appropriate
  SQL transport code without requiring exchange catalogs or connector installation.
- For legacy `engine: trino` configurations, give a concrete migration to a Trino
  source plus `engine: none` or `engine: duckdb`. Do not infer a new authorization
  boundary or silently enable DuckDB.
- Inventory consumers of internal compiled plans and old engine registries before
  removing compatibility code. Preserve existing explicit DuckDB extension use.
- Retire Orchid-to-Trino intermediate relation uploads and their connector only
  after verifying no retained feature depends on them. Preserve unrelated source
  streaming and authenticated transport.
- Do not edit the README as part of this plan. Any later documentation changes
  must respect the user's explicit instruction about that file.

Deliverable: clear migration behavior with no remaining Trino engine selection.

## Authorization and operational boundaries

Trino/OPA governs the source queries executed through Trino. Application-owned SQL
sessions and direct services retain their own authentication and authorization.
Direct lake access uses storage-scoped credentials. Catalog grants do not substitute
for permission to read source data, and local completion does not extend OPA coverage
to other sources. Preserve authorized identities through dependent calls; do not
retry authorization failures through a different source or engine.

Any plan cache must account for schema revision, source capabilities, function
bindings, and engine selection. Any result cache must additionally respect identity,
policy and freshness boundaries. This change does not require introducing a cache.

## Acceptance tests

| Case | Required evidence |
| --- | --- |
| Full Trino pushdown under both engine selections | One source execution; no DuckDB SQL, initialization, registration, or exchange upload |
| Full PostgreSQL pushdown | Application-provided executor receives correct SQL and bound parameters; no bundled PostgreSQL driver |
| Local-only UDF and constant results | Correct results without source calls or engine creation |
| RAG source/UDF pipeline | Correct dependency order, batched correlation, types, duplicates, and cancellation without DuckDB |
| Source query followed by local UDF with DuckDB available | An engine-provider tripwire is never invoked |
| Residual cross-source join with DuckDB | Correct results; engine created lazily; only residual regions execute there |
| Same residual join with engine `none` | Planning error before source execution or UDF effects |
| Database-only function in a local position | Explicit capability error; no implicit DuckDB fallback |
| Trino plus local completion | No intermediate relations uploaded to Trino; policy-filtered source output remains the input |
| Empty, null, duplicate, and ordered results | Semantics agree with reference execution across split boundaries |
| Partial failure and cancellation | Source cursors and temporary inputs close; caller-owned sessions remain open |
| Engine-free release artifacts | Production dependency, linking, package-content, and runtime checks show no DuckDB |
| Legacy configuration/protocol | Clear migration/version errors; no silent reinterpretation |
| Optional DuckDB host | Existing caller-owned extension/session behavior continues to pass |

Run focused tests and required client checks locally; do not dispatch GitHub Actions.
Use a dedicated feature/build matrix so workspace feature unification cannot hide
DuckDB dependencies in the engine-free checks. Inspect production dependencies
separately from test fixtures. Preserve existing artifacts and caches.

## Performance validation and completion criteria

Benchmark source-only dispatch, local UDF batching, and DuckDB completion separately.
Report planning time, per-request orchestration overhead, allocations, throughput,
and tail latency against direct application-owned source calls. Source-only plans
must show zero DuckDB activity, not merely lower elapsed time. Do not claim 1M QPS
from this architectural change; backend and workload capacity need separate evidence.

The change is complete when all supported clients can execute source/UDF plans
without a final engine query, `none` artifacts exclude DuckDB, DuckDB completion is
lazy and explicit, Trino operates solely as a source, and the acceptance matrix
passes with authorization and result semantics preserved.
