# Java architecture review

Java separates graph compilation from application-owned execution. The typed `Graph` API targets one engine. `FederatedQuery` executes mixed-engine requests through the shared JSON API.

## Boundaries retained

- `SqlCompiler` consumes immutable mappings, declared schemas, functions, query parameters and a dialect. It returns SQL and column names. It owns no connection and can compile offline.
- The Rust JNI library retains its bounded worker runtime and optional statistics catalogs across requests. Java passes versioned JSON; no JDBC or database handles cross JNI. The native compiler disables the core's default DuckDB feature.
- `ExecutionEngine` owns the session abstraction. JDBC is an adapter, not a compiler dependency. Borrowed sessions use the exact caller connection; pooled sessions retain one lease for schema discovery and execution. Neither path changes transaction settings or installs plugins/UDFs.
- `Source` binds tables to named engines. Graph mappings are immutable. The typed `Graph` API rejects mappings spanning engines before acquiring a connection. `FederatedQuery.query` accepts a shared JSON request and an engine registry, runs source SQL islands, and binds their rows into the target SQL. Each transfer is collected in memory; no exchange tables are created.
- `PlanCache` is optional and caches compilation output, not data. Keys include mapped schema types, query values, authorization, functions, mapping, engine, dialect, and statistics catalog identity. JDBC discovers only mapped columns, so an unrelated STRUCT/list column or schema addition does not break or invalidate a graph query. Mapped schema changes still invalidate plans.
- `orchiddb-gremlin` is a separately selected Maven module/artifact. It validates TinkerPop 3.7 bytecode and uses the same compiler/execution path. Consumers of `orchiddb-java` do not receive TinkerPop transitively.

## Permission filtering

`PermissionRelation` describes a caller-maintained relation of effective grants. Applications
choose the authorization system, resolve inherited or group grants, and keep that relation current.
`NodeMapping.protectWith` filters protected node sources before traversal and projection;
`Query.as(Authorization)` supplies the principal. Multiple scopes combine with OR, and duplicate
grants do not duplicate graph rows. Protected mappings require a principal. The permission
relation uses the same engine as the graph. See [permission filtering](../README.md#permission-filtering).

## Release improvements from this review

- One Maven parent/reactor replaces duplicated POMs. `mvn verify` builds the base module; `mvn -Pgremlin verify` adds Gremlin. There is no need to install the base module before a reactor build.
- Maven coordinates use the owned domain namespace `com.orchiddb`. Java package names stay `io.orchiddb` to avoid an unnecessary API break.
- Native compiler JARs are explicit platform classifiers. `NativeSqlCompiler.load()` resolves the installed matching artifact, verifies Java version/compiler revision/checksum and extracts it to a unique temporary directory. It never downloads executable code. `load(Path)` remains available for custom builds and application-managed deployment.
- Release CI pins the Rust core commit, builds or reuses matching native artifacts, verifies their metadata, and assembles signed artifacts for Central staging. Run integration and classpath-loading tests before release; the release workflow skips tests.

## Deliberate limitations

- DuckDB has integration tests. `FederatedQueryTest` exercises Postgres/DuckDB transfers in both directions when `ORCHIDDB_TEST_PG_JDBC` is set; otherwise it skips that test. ClickHouse compilation is unsupported.
- Gremlin supports a documented read-only scalar subset. It buffers a bounded number of rows and returns a completed future on the calling thread. It does not provide asynchronous query execution, cursor streaming, vertex/edge/path objects, arbitrary lambdas, mutations or TinkerPop certification. Full-engine conformance figures do not establish adapter conformance.
- JDBC exceptions remain part of the engine SPI; a future non-JDBC adapter must translate failures or motivate a versioned API change. Cancellation is currently through caller-owned execution or statement timeout, not a portable public cancellation handle.
- Metadata discovery still occurs per operation. This deliberately observes the caller's temporary views and schema changes; metadata caching would need a separate invalidation contract.
- SQL includes specialized parameter literals. Cache entries and diagnostic SQL may contain sensitive values; consumers control caching/logging.
- A borrowed connection is guarded per adapter, not globally across all aliases to the same Connection. Applications must not use it concurrently outside the adapter.
- Native libraries live for the process/classloader lifetime. Sandboxed or no-exec temporary directories may require `load(Path)` from an application-approved path. A runtime checksum detects mismatch/corruption; it is not a substitute for trusting/signature-verifying the artifact source.
- The compiler worker count is bounded, but admission/queue limits are not. Applications accepting untrusted queries should impose their own request limits; hard compile cancellation is not implemented.

## Verification

Tests execute against a caller-owned DuckDB session, compare native Java Gremlin results with TinkerGraph, cover lease/transaction ownership and cache invalidation, and compile with no DuckDB runtime dependency. A separate fresh-JVM check uses packaged API/native JARs only and rejects mismatched version, core revision and checksum metadata. The release workflow verifies packaged artifacts but does not run integration tests. See the [release guide](releases.md).

## Arrow execution boundary

`Session.executeArrow` is the batch extension point, with `Session.execute` as a
row convenience for Arrow adapters. `Graph.queryArrow` couples a result to its
session lease. JDBC adapters explicitly accept an application-supplied exporter
and allocator; no DuckDB classes are imported by the production API. Drivers that
cannot export Arrow must reject batch execution rather than silently convert rows.
Arrow results own query resources and child allocators; connections, pools and
parent allocators retain caller ownership. See [Arrow contracts](arrow.md).

Arrow vectors are now a public API dependency. DuckDB JDBC, Arrow's C Data bridge
and the test allocator remain test dependencies. The existing SQL-only compiler
and JNI transport have no result-data responsibility.


## Optional generated statistics

`Statistics.generate` drives the core coordinator through begin/submit/finish calls using bounded
session reads. Rust owns acquisition policy, summaries, estimation, and catalog caching. Java
retains only an immutable serialized snapshot, coverage report, and native catalog identity.
`Graph.generateStatistics` replaces its catalog atomically. The identity is part of `Compilation`
and therefore the plan cache key. Compilation sends the identity rather than the catalog contents.
`CompiledQuery.diagnosticsJson` retains all native planning diagnostics, including statistics and
representation choices. JNI runs statistics commands on the same bounded runtime as compilation.

JDBC's collection reader applies result limits, a scheduled cancellation deadline, and thread
interruption detection. Applications providing their own `Session` implement `readStatistics` or
receive partial-coverage reports. No automatic full-scan fallback is performed by Java. The
caller owns transaction consistency and decides when to regenerate or clear a snapshot.
