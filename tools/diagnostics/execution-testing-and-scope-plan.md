# Three execution suites and a smaller Orchid release

Status: complete planning document; implementation and deletions proposed here are
not yet performed. This supersedes the multi-language distribution/testing scope of
`optional-relational-engine-plan.md`, while retaining its source-first execution
contract. No README changes are included.

## 1. Product boundary

Release two deliverables:

1. **Native Orchid:** the shared compiler and execution runtime, a supported native
   embedding API, typed results, application-owned SQL/request callbacks, local
   functions, and table access hooks. Default builds contain no DuckDB and no
   PostgreSQL driver. Schema registration is separate from query and parameters;
   callers receive results, not a plan they must implement themselves.
2. **Orchid DuckDB extension/function bundle:** the same language semantics,
   lowering, kernels, codecs, and required SQL helper functions hosted by the
   caller's DuckDB connection. Keep schema registration and query execution in this
   artifact; do not reduce it to scalar helpers if doing so strands graph queries.
   DuckDB extensions supply access to external systems.

Interpret “native thing” as an executable embedding library, not a server or a
compile-only tool. Interpret “bundle of functions for the extension” as the existing
DuckDB host with shared function implementations. A new Trino function plugin is
not implied. Trino connector functions remain source-owned; unsupported functions
run locally or in optional DuckDB completion.

Only two completion-engine values exist: `none` and `duckdb`. There are exactly
three top-level integration suites, named for the requested execution paths:

| Suite | What executes work | Authorization boundary |
| --- | --- | --- |
| DuckDB lowering and extensions | Caller-owned DuckDB, its extensions, and explicit residual completion | Caller session credentials; S3/object permissions for lake data |
| Trino lowering and connectors/functions | Trino source; local UDFs or DuckDB only for unsupported residual work | Trino with externally configured OPA |
| Native/none | Caller SQL/request endpoints and native local UDF execution | Application-owned authentication and table/source hooks |

These are execution paths, not three engines or three separate graph products.
Lake and realtime workloads use the appropriate path. Neither 4 TB/petabyte data
nor one million QPS is a correctness threshold or an established performance claim.

## 2. Inventory and proposed disposition

The review covers the core module/feature declarations, language and SQL lowering,
execution adapters, extension entry points, clients, test families, and release
scripts. This is a product-surface review, not proof every internal function is
unused. Before deleting shared internals, trace consumers and run retained suites.

| Feature family / evidence | Disposition | Required change or constraint |
| --- | --- | --- |
| Cypher, Gremlin, SPARQL frontends; `src/language`, `src/grammar`, `src/spargebra` | Keep | One shared semantic test corpus; do not multiply release products by language |
| Graph IR, logical planner, relational lowering; `src/ir`, `src/planner`, `compiler/datafusion` | Keep | DataFusion logical components are not a physical engine; do not remove them because DataFusion execution was removed |
| Kernels, scalar/collection/RDF codecs, exact value transport | Keep | Shared between native and extension; preserve null, numeric, temporal, nested and graph identity semantics |
| Schema mapping, relationship declarations, foreign keys, temporal source mappings | Keep | Ontology functionality independent of storage ownership |
| Computed/RAG edges, retrieval, vector/search lowering, local UDFs | Keep | Explicit capabilities, types, dependencies and effects; no DuckDB registration for native UDFs |
| `src/execution/plan.rs` and typed Arrow streaming | Keep/consolidate | One runtime executes source-first plans; language wrappers must not become separate runtimes |
| SQL dialects including PostgreSQL and Trino | Keep | Dialect/function lowering does not imply a bundled driver; StarRocks and other less-used mappings remain internal until covered and claimed |
| DuckDB host and extension helper functions; `extension/src`, `extension/compiler` | Keep | Caller-owned connection/catalog; lazy optional completion in native integrations |
| Trino SQL source and codec; `src/execution/trino.rs` | Keep as optional source adapter | Full pushdown where possible; no intermediate-result uploads or Trino completion-engine option |
| Trino Arrow exchange plugin; `extensions/trino` | Remove from supported release, then delete obsolete consumers | Inspect plugin function consumers first; retain any independently required lowering function elsewhere; generic streaming is not automatically obsolete |
| Generic SQL/request execution interfaces and source capability declarations | Keep | Application owns endpoint/driver/pool/auth; Orchid passes SQL and typed parameters and consumes typed streams |
| Built-in SpiceDB integration; `extension/compiler/src/authorization.rs`, `extension/src/authorization.hpp` | Remove | Remove HTTP checks, provider configuration, SQL helper registrations, policy syntax, session auth state and provider-specific caching |
| Compiler permission machinery; `Authorization`, `PermissionScope`, `PermissionRelation` in `src/compiler.rs` | Remove built-in policy model | Trace relation rewriting, schema fields and streaming checks; replace application integration with generic table hooks, not another permission engine |
| Table/source hooks | Keep/design native contract | Caller policy may deny or resolve access to a scoped relation; fail closed and cover every access path |
| Rust, Python, JS, Java, Go, C++, Elixir SDKs under `clients/` | Remove as supported products | Move native runtime, shared useful code and unique regression tests first; remove duplicated plan interpreters, JNI/NIF and package publishing |
| `clients/native` C ABI | Retain and promote, not delete with clients | Currently planning/statistics oriented; add native execution/session/callback/stream lifecycle before retiring SDK execution |
| Remote adapters: Weaviate, Elasticsearch, Quickwit, Prometheus, Loki | Keep lowering contracts; make built-in transports optional | No default transport bundle; callers can bind request execution; keep real protocol regression tests for adapters still shipped |
| DuckDB PromQL/LogQL and lake/vector extension mappings | Keep when supported by pinned integrations | Verify actual extension functions, versions and result schemas; missing integration must fail explicitly |
| Kafka/Flink streaming materialization, subscriptions and offsets; `src/streaming` | Remove from these release deliverables / defer as separate scope | Remove Kafka default feature/packaging and query-level permission checks; preserve reusable bounded result-streaming primitives |
| Managed graph storage, incremental engine, checkpointing; `src/engine.rs`, `src/storage.rs`, managed/RDF engine modules | Defer storage-owning product surface | Remove managed store lifecycle/background processing from slim release; retain codecs/kernels used by queries and shared host helpers |
| Mapped writes, RDF updates, effectful graph operations | Narrow scope | Preserve source-owned semantics already required by retained execution; reject unsupported routes before side effects; no new distributed transactions or implicit local database |
| JVM store process and bridge; `src/bin/orchiddb-jvm-store.rs`, `src/jvm_bridge.rs` | Remove standalone distribution; retain required internals pending dependency trace | Removing Java SDK does not justify deleting Gremlin semantics or shared JVM-related IR blindly |
| Local schema/catalog structures and function registry | Keep | Required for planning, source ownership, capabilities and bindings |
| Remote Orchid Catalog client; `src/catalog/remote.rs`, `src/catalog/auth.rs`, `orchid-catalog` feature | Exclude from default artifacts; defer integration | Keep schema injection usable without network; do not conflate catalog authentication with SpiceDB query authorization |
| Sibling `../orchid-catalog` service | Outside this deletion change | Separate repository/product; recommend separating its release and ownership, not deleting it as collateral |
| Statistics, optimizer cost logic, diagnostics/observability | Keep essentials; trim auxiliary product APIs | Preserve planning correctness, source attribution and execution counters; do not make sampling or telemetry mandatory network traffic |
| Syntax feature flags (`sep-0002`, `sep-0006`, `sparql-12`, Unicode escaping) | Keep compile-time/internal | No extra release variant per parser flag; test supported syntax in shared corpus |
| Existing broad language/conformance regression tests | Keep relevant semantics | Organize under shared checks supporting three suites; do not erase hard-won correctness coverage to reduce matrix size |
| SDK release matrices, package metadata generators and client smoke orchestration | Remove/replace | Two artifact families, one native harness, extension host harness; update Makefile and release verification |
| Demonstrations, benchmarks, generated comparison reports | Diagnostics only | Exclude from product packaging and release gates unless they validate a supported contract |

“Remove clients” includes the newly added source-first SDK wrappers. Do not spend
more effort maintaining seven copies of the executor. A small C test embedding
program or Python extension test driver is test infrastructure, not a released SDK.
Keep the Rust core crate as implementation/native integration, not a separate
client product with another runtime.

## 3. Native API and table hooks: prerequisite to removing clients

The current `clients/native/src/lib.rs` exports `orchiddb_execution_command`,
Arrow binding, statistics, and optional remote commands. Its main command dispatches
to compilation; it is not yet a complete execution API. The Rust core already has
an execution-plan runner, but foreign callers need a supported bridge to it.

Implement and test:

- Opaque session/schema handles; schema registration and explicit binding updates.
- `execute(schema_handle, query, parameters, request_context)` returning a typed
  Arrow C stream plus structured errors; no exposed plan interpreter requirement.
- SQL callback registration with source identity, dialect/capabilities, SQL,
  bound parameters, request context, deadline and cancellation.
- Request and local-UDF callbacks with explicit input/output schemas, ownership,
  error propagation and threading/reentrancy contract.
- A table hook invoked for all referenced tables before execution. It receives
  canonical source/table identity, operation and opaque caller context; it returns
  allow/deny or an application-supplied scoped binding. Policy decisions and any
  row/column restrictions are application-owned, not a new Orchid policy language.
- Scope-safe planning/caching: authorized binding identity/version must participate
  in cache identity, or authorization-dependent plans must not be shared. Never
  re-use one principal's scoped relation for another principal.
- Cover aliases, joins, nested queries, computed edges, dependent requests and any
  retained writes. Raw service access must also pass its source authorization
  callback; a table hook alone cannot authorize arbitrary service operations.
- Hook errors/denials prevent execution and are distinguishable from empty results.
  Cancellation, early stream close, late source errors and callback panics must
  release resources exactly once. Document who owns and frees every FFI object.

Hooks live in the native embedding API and extension host integration as needed,
not in a retained family of language clients. They complement Trino/S3 enforcement;
they do not replace those systems or promise row-level isolation from bucket ACLs.

## 4. Shared fixtures and assertions

Use one fixture manifest describing schemas, rows, queries, parameters, expected
typed results, supported routes and required functions/extensions. Include two
tenants, users/documents/edges, duplicate IDs scoped by source, nullable fields,
large integers/decimals, timestamps/timezones, nested collections, empty tables,
and deterministic embeddings/search responses.

Every integration case records: result/schema, generated operation destinations,
source call count, DuckDB initialization/query count, local function call count,
authorization outcome, and stream/resource closure. SQL assertions check placement
and semantics, not only unstable full-string snapshots. Compare ordered results
only when ordering is requested; otherwise compare multisets with exact types.

Keep parser, IR, optimizer and scalar unit tests as supporting checks, not a fourth
product suite. Do not cross-product every language, source, extension, platform and
query: run the shared language corpus once per applicable route and targeted cases
for each extension's distinct contract.

## 5. Suite A: DuckDB lowering and extensions

Build the actual Orchid extension for a pinned DuckDB version and load it in a
caller-owned session. Use the native runtime with a lazy completion binding for
cases exercising source-first residual execution.

| Case | Integration / query shape | Required evidence |
| --- | --- | --- |
| A1 | Local DuckDB tables; Cypher/Gremlin/SPARQL supported corpus | Real results, required Orchid helper registration, exact value semantics |
| A2 | Parquet/object-store access through DuckDB; Iceberg mapping when supported | Predicate/projection placement, partitioned fixture results, storage credentials and denied object access |
| A3 | DuckDB PostgreSQL extension against fixture PostgreSQL | Real extension lowering and typed results; no PostgreSQL driver in Orchid artifacts |
| A4 | Lance/vector/search mapping supported by chosen extension build | Supported search/filter/order semantics and missing-function diagnostics |
| A5 | Prometheus/PromQL and Loki/LogQL extension mappings | Time range, labels, parameters, result schema; malformed/service-error propagation |
| A6 | Two independent sources requiring a join/aggregate | Exactly the residual operators run in DuckDB; source pushdown retained; typed input registration and cleanup |
| A7 | Source → local UDF → residual relational work | UDF runs locally once per declared input semantics; no unregistered local function emitted into SQL |
| A8 | Source-only plan while completion capability is `duckdb` | DuckDB factory/query tripwire never fires; no manufactured final SELECT |
| A9 | Nulls, empty batches, large/nested values, cancellation and late errors | Same semantics as shared fixtures; bounded streaming and deterministic cleanup |

A4/A5 must pin the real available extension builds, not pretend an emitted SQL
string proves execution. If an extension cannot be supplied, mark that capability
unverified and exclude it from the release's supported matrix. Existing
`extension/tests/test_lake_queries.py` uses local tables: it is useful query-shape
coverage, not evidence of real S3/Iceberg access.

## 6. Suite B: Trino lowering with connectors/functions

Run a real pinned Trino deployment with PostgreSQL and Iceberg/object-store
connectors, a function inventory, and external OPA policy fixtures. Add configured
search/metrics connector/function integrations only when they are in the supported
matrix. Distinguish Trino connector capabilities from DuckDB extensions and Orchid
HTTP request adapters.

| Case | Integration / query shape | Required evidence |
| --- | --- | --- |
| B1 | Full query over one Trino catalog, `engine=none` and `engine=duckdb` | One source query, correct results, zero DuckDB initialization/queries |
| B2 | Join across catalogs accessible through the same Trino source | Entire supported join lowers to Trino; no per-table Orchid fan-out |
| B3 | Iceberg table/snapshot/history mappings | Real fixture snapshot identity and expected rows; unsupported change/history capabilities rejected |
| B4 | Source-owned functions/connector extensions | Correct qualified function calls, types and predicates; missing capabilities diagnosed before execution where known |
| B5 | Trino → native local UDF → result | No DuckDB activity; no requirement to install local UDF in Trino |
| B6 | Trino → local work → required relational completion | `duckdb` completes residual; `none` rejects before source/UDF side effects |
| B7 | Intermediate Arrow input supplied to Trino | Explicit rejection before network; no exchange plugin/upload path |
| B8 | OPA allow, deny, row/column restrictions when configured, two principals | Real authorized results and denied operations; no credential/context bleed across reused sessions |
| B9 | Pagination, null/nested/decimal/time values, cancellation, server/late-page errors | Native stream receives correct types/errors; remote query cancellation and closure verified |

B8 uses policy features enabled in the pinned Trino/OPA setup. Do not advertise
masking/filtering merely because a synthetic allow/deny test passed. Tests exercise
Orchid's identity propagation and enforcement boundary, not implement OPA in Orchid.

## 7. Suite C: Native/none

Build the native artifact with explicit minimal features and exercise it from one
small C ABI harness plus Rust core tests. SQL endpoints and request adapters are
caller supplied. For live PostgreSQL coverage, a fixture-side endpoint/driver can
exist outside the distributed Orchid artifact.

| Case | Integration / query shape | Required evidence |
| --- | --- | --- |
| C1 | Clean default/minimal package build, load and execute | Dependency tree, dynamic links and packaged contents contain no DuckDB/libduckdb or PostgreSQL driver; no runtime engine loading |
| C2 | SQL callback with PostgreSQL dialect; projection/filter/join/aggregate supported by one source | Whole supported query reaches caller endpoint with typed parameters; correct streamed results |
| C3 | Constants/parameters → local UDF; source → UDF | No SQL or DuckDB generated solely to evaluate local functions; exact type/null behavior |
| C4 | Embedding → vector request → SQL fetch → reranking | Correct dependency order, batching and row correlation; no accidental fan-out duplication |
| C5 | Unsupported cross-source join, sort/aggregate residual, missing source/UDF | Preflight failure before callbacks execute; no implicit engine fallback |
| C6 | Table/source hooks across nested, aliased and dependent access | All required hooks invoked, deny/error fail closed, tenant separation under cache/session reuse |
| C7 | Callback errors, invalid UDF type/cardinality, empty results, cancellation/reentrancy | Structured errors, bounded buffering, early-close/late-error handling, no leaked streams |
| C8 | ABI version mismatch, null arguments, ownership/release and concurrent requests | Safe failures and lifecycle contract; no result/context leakage between concurrent callers |
| C9 | Artifact consumption from a clean environment | Install/load/execute using only documented native headers/libraries; no SDK interpreter dependency |

Current local UDF execution preserves row count; test and document that boundary.
Do not imply arbitrary local joins/aggregates or cardinality-changing UDFs exist.

## 8. Test organization, commands and release gates

Proposed local targets (not existing commands yet):

- `make test-native`: shared semantic/runtime checks and Suite C.
- `make test-duckdb`: Suite A and retained extension conformance.
- `make test-trino`: Suite B against provisioned pinned services.
- `make test-release`: all three suites plus artifact validation; no SDK matrix.

Each runner reads the same manifest, writes machine-readable results under
`target/`, and distinguishes PASS, FAIL and SKIP. Missing services may skip developer
runs; a release advertising an integration must fail if its required live tests
were skipped. A test that returns early because an environment variable is absent
must never be counted as live integration success.

Native and extension packages require independent dependency/feature inspection:
root `default=[]` does not neutralize defaults in `clients/native/Cargo.toml` or
`extension/compiler/Cargo.toml`. Verify actual release feature resolution, remove
SpiceDB default activation, and remove native default Kafka/catalog/service
transports unless explicitly included in a separately declared optional build.

Preserve the current locally built platform policy: macOS ARM64, Linux ARM64 and
Linux AMD64; no Windows artifacts. Run full integration suites on a provisioned
reference platform, plus load/ABI/function/execution smoke checks on every shipped
platform and every supported DuckDB ABI version. Record build revision, dependency
versions, feature set, checksums, and the exact verified connector/function matrix.
Never publish untested bytes or replace existing published artifacts. Build and test
locally; publication is separate and not authorized by this document.

Performance measurements belong within these suites: native callback/UDF overhead
and concurrent requests; Trino pushdown call counts and scanned data; DuckDB residual
memory/spill and partition pruning. Record dataset size, hardware, source limits,
concurrency, throughput, p50/p95/p99, errors and memory. Establish baselines before
setting regression thresholds. Test millions of QPS or petabyte datasets separately
before making those claims; neither follows from correct routing.

## 9. Implementation order and completion criteria

1. **Freeze the boundary and inventory supported integrations.** Record exact
   extension/connector/function names and pinned versions; identify whether each
   current helper belongs in native, DuckDB, or source SQL. No README edits.
2. **Complete native execution.** Bridge the existing core plan runner to the
   supported native API and implement SQL/request/UDF/table hooks and Arrow stream
   ownership. Move native packaging out of SDK orchestration while keeping its
   source location consistent with repository binding conventions.
3. **Implement shared fixtures and Suite C first.** Port unique tests from SDKs,
   including duplicate inputs, nulls, dependent request correlation and cleanup.
   Prove a native caller executes without any language plan interpreter.
4. **Remove built-in authorization.** Delete SpiceDB transport/state/functions and
   compiler permission models only after hook coverage exists. Reject removed
   authorization fields/syntax with actionable migration errors rather than silently
   ignoring them or weakening access restrictions. Update non-README configuration
   reference/test fixtures; log README follow-up for a separately authorized change.
5. **Complete Suites A and B.** Retain applicable existing tests, replace optional
   live-test early returns with explicit skips/failing release gates, add real
   OPA/S3/connector fixtures, prove full-pushdown bypass and residual routing.
6. **Remove SDK products and obsolete exchange path.** Retire client package builds,
   wrappers, JNI/NIF, duplicated executors and Trino upload plugin after consumer
   tracing. Update workspace membership, lockfiles, Makefile, release scripts and
   conformance harness imports. Preserve native bindings and shared core internals.
7. **Trim defaults and deferred product surfaces.** Exclude Kafka/materialization,
   remote catalog and managed-store/JVM service distributions. Prune unused
   dependencies only after retained-feature builds and tests show they are unused.
8. **Validate the two artifact families.** Run all release gates, inspect package
   contents and dependencies, test clean installation, and produce a migration
   report for removed APIs/configuration. Preserve existing published releases.

Done means: three passing integration suites for every advertised integration;
minimal native execution with no DuckDB or PostgreSQL driver; no client-specific
executor needed; no SpiceDB or implicit permission graph machinery; table hooks
cover access; Trino is source-only; source-only plans never invoke DuckDB; the
extension's function and schema/query APIs work in caller sessions; only two
artifact families appear in the new release manifest.

## 10. Existing evidence and remaining work

Follow-up: the [local execution integration harness](execution-integration/TESTING.md)
now exercises live DuckDB extensions and Trino/OPA, including required residual
completion. Its generated report records the tested working tree and exact scope.
This adds evidence for Suites A/B; it does not complete the proposed native ABI,
product removals, all connector variants, S3 permissions, or production validation.

The preceding optional-engine implementation has local coverage for source-only
pushdown, local UDF placement, unsupported residual rejection, dependent requests,
mocked Trino streaming, and several SDK/native smoke paths. Dependency/link checks
on the tested native build found no DuckDB or PostgreSQL driver. Those checks do
not establish that the proposed supported native execution ABI already exists.

The complete three-suite plan above has not been executed. Real Trino connector/OPA,
S3 permission and broad extension integration gates remain to be built/run. The
proposed SDK, authorization and product-surface removals have not been performed.
This document completes the testing and feature-review plan; it does not report
those implementation steps as finished.
