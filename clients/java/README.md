# OrchidDB for Java

Source builds use the shared core in this monorepo. See [build and test instructions](../README.md).

**Start here: [Runnable examples](examples/README.md)**. Arrow batches, Gremlin, existing DuckDB connections, UDFs, SQL-only compilation, and multiple engines.

Compile Cypher, Gremlin, and mapped SPARQL queries to SQL, then execute them on **your existing database connection**. This library contains no DuckDB database, driver, connection pool, SQLg layer, or result cache. Its JNI library contains the OrchidDB compiler, built with `default-features = false`.

The application owns the engine: choose your DuckDB JDBC version, configure extensions and Iceberg credentials, register UDFs, configure caching, and decide when to commit. OrchidDB reads source metadata and runs the generated SELECT on the **same connection**, including temporary tables and functions. It never reopens a JDBC URL or rebuilds your views.

[Architecture review](docs/architecture.md) · [JVM releases and installation](docs/releases.md)

## Installation

For this source checkout, follow [Build from source](#build-from-source-contributors).
Use Java 17+. Packaged JVM distributions, when published on the main repository's
[release page](https://github.com/OrchidDB/OrchidDB/releases), contain the API,
Gremlin client, runtime dependencies, native classifiers, and a Maven repository.
The instructions below describe that distribution format.

Extract the ZIP and put `lib/*` on your application's classpath:

```sh
java -cp "orchiddb-java-0.3.0/lib/*:your-application.jar" your.Main
```

Use `;` instead of `:` on Windows. `NativeSqlCompiler.load()` selects, verifies,
extracts, and loads the compiler for the current JVM. No Rust installation or
compiler build is required.

For Maven, add the extracted `repository/` directory as a file repository and use
matching API and native versions:

```xml
<repositories>
  <repository>
    <id>orchiddb-github</id>
    <url>file:///absolute/path/orchiddb-java-0.3.0/repository</url>
  </repository>
</repositories>
<dependencies>
  <dependency>
    <groupId>com.orchiddb</groupId>
    <artifactId>orchiddb-java</artifactId>
    <version>0.3.0</version>
  </dependency>
  <dependency>
    <groupId>com.orchiddb</groupId>
    <artifactId>orchiddb-java</artifactId>
    <version>0.3.0</version>
    <classifier>macos-aarch64</classifier>
    <scope>runtime</scope>
  </dependency>
</dependencies>
```

Replace the file URL with your extracted directory and choose the classifier that
matches your **JVM's** operating system and architecture:

| Platform | Classifier |
| --- | --- |
| Linux x86-64 | `linux-x86_64` |
| macOS Apple Silicon | `macos-aarch64` |
| macOS Intel | `macos-x86_64` |
| Linux ARM64 | `linux-aarch64` |

For Gremlin, also add `com.orchiddb:orchiddb-gremlin:0.3.0`. Maven resolves ordinary
third-party dependencies through your normal repositories. Add your chosen JDBC
driver separately; DuckDB JDBC is only a test dependency in this project. For Arrow
execution, supply an Arrow memory implementation and any driver-specific export
support as described in the [Arrow guide](docs/arrow.md).

## Bring your own DuckDB

```java
var compiler = NativeSqlCompiler.load();
var people = Source.table("lake", "people");
var friendships = Source.table("lake", "knows");
var mapping = new GraphMapping(
    List.of(NodeMapping.node("Person", people, "id").property("name", "name")),
    List.of(EdgeMapping.edge("KNOWS", friendships, "id", "src", "dst", "Person", "Person")));

// connection was created and configured by your application.
var db = new OrchidDB(compiler, PlanCache.bounded(128),
    JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, connection));
var graph = db.graph(mapping);
var query = Query.cypher(
    "MATCH (a:Person)-[:KNOWS]->(b:Person) WHERE a.name=$name RETURN b.name AS friend",
    Map.of("name", "Ada"));
try (var rows = graph.query(query)) {
    while (rows.next()) System.out.println(rows.get("friend"));
}

// Or take the SQL and do all execution yourself:
CompiledQuery plan = graph.plan(query);
try (var statement = connection.createStatement();
     var rows = statement.executeQuery(plan.sql())) {
    while (rows.next()) System.out.println(rows.getString(1));
}
```

See [BringYourOwnDuckDb.java](orchiddb-java/src/test/java/io/orchiddb/examples/BringYourOwnDuckDb.java) for a complete runnable program with table creation and data. [OfflineSql.java](orchiddb-java/src/test/java/io/orchiddb/examples/OfflineSql.java) compiles against explicit schema metadata without opening a database at all.

`Source.table("lake", "catalog", "schema", "table")` uses distinct literal identifier parts; names are quoted, never interpolated as raw SQL. Use two parts for `schema.table`. Node IDs and edge IDs/endpoints must be signed integer columns, unique within each node label or edge type. Supply a distinct edge ID even for parallel edges. OrchidDB validates metadata but does not scan data to prove uniqueness or referential integrity. NULL or duplicate identities violate the mapping contract.

## Arrow batches

Use `graph.queryArrow(query)` for columnar results. Configure your JDBC adapter
with `withArrow(exporter, allocator, batchSize)` to use your driver's native Arrow
stream. OrchidDB receives vectors directly; it does not reconstruct them from
JDBC rows. The allocator and connection remain caller-owned. See the
[Arrow guide](docs/arrow.md) for runnable setup, dependencies, JVM settings and
buffer lifetimes. The row API becomes a convenience view over Arrow when this
adapter is configured.

## Ownership and execution

- `JdbcEngine.borrowed(...)` uses the exact `Connection`. It never closes, commits, rolls back, or configures it. One active library session is allowed per borrowed adapter. Overlapping use fails immediately; close the result before reusing it. External use of that same connection remains your responsibility; do not create multiple adapters to bypass serialization.
- `JdbcEngine.pooled(...)` borrows a connection from your `DataSource` for each operation. Schema discovery and query execution use one lease. Closing the result returns that lease. Configure extensions, UDFs and session initialization through your pool so every borrowed session has them. OrchidDB never closes the pool.
- Without Arrow configuration, `QueryResult` wraps the driver's row cursor and owns its statement. Always use try-with-resources, including after early iteration. JDBC drivers control actual buffering. `get(int)` is 1-based; `get(String)` rejects ambiguous duplicate column names. Values are driver-native JDBC values, including `java.sql.Array`/`Struct` where applicable.
- Optional `fetchSize` and `timeoutSeconds` overloads configure library-created statements. OrchidDB never changes autocommit to obtain driver-specific streaming behavior. For full control, compile and execute the returned SQL yourself.
- Errors in schema discovery, planning, or execution release acquired resources. Closing a borrowed result leaves its connection and transaction open.

## Functions, plugins and Iceberg

Register actual functions using the engine's API, then declare their compiler signatures. For example, [ClientFunctions.java](orchiddb-java/src/test/java/io/orchiddb/examples/ClientFunctions.java) uses DuckDB's own Java UDF API on the caller's connection:

```java
DuckDBFunctions.scalarFunction().withName("java_next_age")
    .withParameter(long.class).withReturnType(long.class)
    .withLongFunction(age -> age + 1).register(connection);
var signature = FunctionSignature.scalar("nextAge", "java_next_age", "int64", "int64");
var graph = db.graph(mapping, List.of(signature));
// MATCH (p:Person) RETURN nextAge(p.age)
```

Signatures describe functions already installed in the engine. They don't evaluate a function during planning or install an implementation. The compiler checks declared argument types and inserts typed casts; overloads can be declared separately. Plugins and SQL macros use the same declaration mechanism. Graph-language built-ins take precedence.

For Iceberg, configure your own connection using the DuckDB Iceberg extension and your catalog's documented authentication, then expose node and edge tables as views. Map those views just like local tables. OrchidDB does not install extensions, fetch secrets, select a catalog, or implement a second Iceberg cache. Illustrative application-owned setup, after `LOAD iceberg` and catalog configuration:

```sql
CREATE TEMP VIEW people AS
SELECT CAST(person_id AS BIGINT) AS id, full_name AS name
FROM lake.analytics.people;
CREATE TEMP VIEW knows AS
SELECT CAST(edge_id AS BIGINT) AS id,
       CAST(source_id AS BIGINT) AS src, CAST(target_id AS BIGINT) AS dst
FROM lake.analytics.friendships;
```

This catalog example needs your real Iceberg catalog and credentials; it is not part of the offline integration tests. Session-local views work because no second connection is opened. Keep complex extension-specific columns out of mapped source views, or cast them to supported types.

## Permission filtering

Use `PermissionRelation` with effective grants from your chosen authorization system. `protectWith` attaches it to a node mapping; `Authorization` supplies the query principal. The compiler filters the node source before traversal and projection:

```java
var grants = PermissionRelation.flat(
    Source.table("lake", "security", "effective_grants"), "document", "view");
var mapping = new GraphMapping(
    List.of(NodeMapping.node("Document", documents, "id")
        .property("title", "title").protectWith(grants)),
    List.of());
var graph = db.graph(mapping);
var query = Query.cypher("MATCH (d:Document) RETURN d.title AS title")
    .as(new Authorization("user", "alice"));
```

Multiple scopes can protect a row at different resource granularities. For example, keep direct document grants and project grants in one effective-grants relation, then match each against the corresponding node source column:

```java
var grantSource = Source.table("lake", "security", "effective_grants");
var documentGrants = PermissionRelation.flat(grantSource, "document", "view");
var projectGrants = PermissionRelation.flat(grantSource, "project", "view");
var documents = NodeMapping.node("Document", documentTable, "id")
    .property("project_id", "project_id")
    .protectWith(documentGrants)
    .protectWith("project_id", projectGrants);
```

The compiler combines these scopes as an OR of membership predicates, so a row appears once even if multiple scopes authorize it. The scope column must be mapped as a node property so it is available in the source schema. Group or team grants work the same way once the permission relation contains effective grants for the principal and chosen resource type.

The default flat relation columns are `resource_type`, `resource_rel`, `resource_id`, `subject_type`, `subject_rel`, and `subject_id`; use the full `PermissionRelation` constructor or the column-selecting `flat` overload for a different schema. Direct principal membership matches the empty `subject_rel`. For DuckDB and integer graph IDs, the compiler safely casts canonical numeric permission IDs to the graph ID type, keeping the Iceberg-side key typed so the engine can apply dynamic scan filters; nonnumeric and noncanonical IDs do not match. The relation must contain effective grants for the configured resource type, permission, and principal, including any group or nested-set resolution required by the source permission model. Normalize or resolve raw relationship data into that relation in the application or a source view. Duplicate grant rows are safe: the filter preserves each graph row once.

The permission source is included in schema discovery and must be on the same engine as the graph. A query against a graph with protected node mappings fails closed if no principal is supplied. Applications maintain the grants and choose their synchronization and consistency policy. OrchidDB does not require a particular authorization service. Dynamic Iceberg pruning depends on table ordering/layout and the permission key range, so inspect plans and measure on the target data.

## Queries and caching

The library compiles read queries. Mutations, external SERVICE access, opaque extensions and plans requiring engine-managed materialization fail during planning; there is no hidden interpreter/database fallback. Language conformance of the full OrchidDB runtime does **not** imply every construct lowers to standalone SQL. Unsupported relational constructs report `PlanningException`.

Cypher parameters are bound into the parsed AST as typed literals, with SQL escaping handled by the compiler. They are **not JDBC `?` placeholders**. SQL may contain sensitive parameter values: avoid logging it. Supported Java parameter values are null, strings, booleans, signed integral wrappers, finite floats/doubles, lists and maps of these. Some compound operations require runtime values and cannot lower to SQL. Temporal/decimal Java objects need explicit query casts or a source view; they aren't silently serialized to strings. Gremlin and SPARQL bindings are currently rejected; SPARQL takes an explicit `Ontology` mapping for classes, properties, and relationships.

`PlanCache.none()` is the conservative default option; `PlanCache.bounded(n)` is an optional synchronized LRU. Implement `PlanCache` to use your application cache. Keys are immutable `Compilation` snapshots containing the engine ID, dialect, query, parameter **values**, mappings, ontology, function signatures, and schema. Data is never cached. Metadata is rediscovered on each connected call so schema changes generate new keys. Use `NativeSqlCompiler.compile(Compilation)` directly for offline schema snapshots and your own metadata/cache invalidation policy. Schema discovery and execution are not atomic with concurrent external DDL; coordinate schema changes in the application.

Schema types: `boolean`, `int8`, `int16`, `int32`, `int64`, `float32`, `float64`, `string`, `binary`, `date`, `timestamp` (microseconds without timezone), and `decimal:precision:scale` (precision 1–38). Unsigned integers, nanosecond or zoned timestamps, arrays/structs and extension-specific source types require a cast/view in this first JDBC adapter. Unsupported types fail explicitly.

## Multiple engines and federation

[MultipleEngines.java](orchiddb-java/src/test/java/io/orchiddb/examples/MultipleEngines.java) registers two independent engines and routes each graph to its named source. Engine IDs are part of every source mapping and compiled plan. Plans cannot be executed through a mismatched engine/dialect.

`SqlCompiler`, `ExecutionEngine`, and `ExecutionEngine.Session` are separate interfaces. JDBC is one execution adapter; a future ClickHouse HTTP adapter can implement the session interface without exposing a JDBC connection. Dialect identity is separate from transport. The native compiler renders DuckDB and PostgreSQL SQL. Live JDBC integration checks exercise both engines, including mixed execution through `FederatedQuery.query`. Set `ORCHIDDB_TEST_PG_JDBC` to enable the PostgreSQL checks. ClickHouse compilation is explicitly unsupported today.

The typed `Graph` API requires a single engine. For mixed-engine requests, use `FederatedQuery.query(compiler, requestJson, engines)` with the shared JSON mapping format. It executes SQL islands and dependent SQL or HTTP operations, collects each transfer in memory, and binds typed rows into the current plan without creating exchange tables. Close the returned `QueryResult` to release sessions. Applications retain connection and transaction ownership. See [FederatedQueryTest.java](orchiddb-java/src/test/java/io/orchiddb/FederatedQueryTest.java) for a complete request.

Quickwit and Elasticsearch use `RemoteEngine` alongside a JDBC SQL coordinator:

```java
var compiler = NativeSqlCompiler.load();
try (var search = RemoteEngine.quickwit("search", "http://localhost:7280")) {
  var engines = Map.<String, ExecutionEngine>of(
      "lake", JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, duckConnection),
      "search", search);
  try (var rows = FederatedQuery.query(compiler, requestJson, engines)) {
    while (rows.next()) System.out.println(rows.get(1));
  }
}
```

Use `RemoteEngine.elasticsearch("search", "http://localhost:9200")` for Elasticsearch.
The shared request maps an index to the remote engine and describes the `SIMILAR_TO`
relationship, filters, scoring, and per-source limit; see the
[search engine documentation](https://orchiddb.com/docs/remote-engines.html) for complete
mappings and queries. The graph request contains no endpoint or credentials.
`RemoteEngine` uses the same native HTTP transport as Rust, including mapping validation,
batching, pagination, typed JSON exchange, and errors for partial results. A SQL engine
coordinates joins and expressions outside the remote engine's supported operations.

Reuse a `RemoteEngine` across queries, then close it to release its connection pool.
Closing a query releases its borrowed sessions without closing the caller's remote engine
or borrowed JDBC connection. Call `clearMetadataCache()` after changing index mappings.
For authentication and limits, use the options constructor:

```java
var search = new RemoteEngine("search", "elasticsearch", Map.of(
    "endpoint", "https://search.example.com",
    "authentication", Map.of("type", "api_key", "token", apiKey),
    "page_size", 500,
    "max_rows", 100_000));
```

Custom non-SQL engines can implement `ExecutionEngine.Session.executeRequests` for the
bound request batch and declared transfer columns. Return shared-protocol JSON rows;
JSON-domain cells contain JSON text, SQL NULL is `null`, and numeric strings preserve
wide integers. Unsupported request adapters fail explicitly. The
[live integration tests](orchiddb-java/src/test/java/io/orchiddb/RemoteEngineIntegrationTest.java)
exercise both services with DuckDB, repeated queries, and failure cleanup. Set
`ORCHIDDB_REMOTE_FIXTURE` to the JSON fixture generated by the core
[`scripts/clients_remote_fixture.py`](../../scripts/clients_remote_fixture.py)
script to run them.


## Implementation references

The separation of a Java API and native component follows [DuckDB Java](https://github.com/duckdb/duckdb-java). Caller-defined functions use its [official UDF API](https://github.com/duckdb/duckdb-java/blob/main/UDF.MD). The OrchidDB JNI boundary uses versioned JSON requests and responses with no database handles; planning runs on a bounded native worker pool with an independent stack. Rust panics become planning errors at the boundary.

See [LICENSE.md](LICENSE.md) for the project's license terms.

## Optional native Java Gremlin API

The optional [orchiddb-gremlin module](orchiddb-gremlin/README.md) lets you write
`g.V().has("name", "Ada").out("KNOWS").values("name").toList()` against your
existing `OrchidDB.Graph`. It adds TinkerPop only when explicitly selected; the
base library has no TinkerPop dependency. This first adapter supports read-only
scalar results and rejects unsupported traversal semantics explicitly.

Build and run its example with `./scripts/build-gremlin.sh` and
`./scripts/run-gremlin-example.sh`.

## Build from source (contributors)

The JNI crate depends on the core in this checkout. Run from `clients/java/`.

Requirements: Rust 1.93 or newer, a platform C toolchain, Maven 3.9+, and Java 17+. First builds compile DataFusion and can take several minutes. No DuckDB C++ build is needed. On macOS, select Java 17+ if your shell still defaults to Java 8:

```sh
export JAVA_HOME=$(/usr/libexec/java_home -v 21)
cd clients/java
./scripts/build.sh
./scripts/run-example.sh ArrowBatches
./scripts/run-example.sh BringYourOwnDuckDb
./scripts/run-example.sh ClientFunctions
./scripts/run-example.sh OfflineSql
./scripts/run-example.sh MultipleEngines
./scripts/check-dependencies.sh
```

`build.sh` builds the native compiler and runs integration tests. `check-dependencies.sh` verifies the driver-free dependency graph. The resulting JAR is `orchiddb-java/target/orchiddb-java-0.3.0.jar`. Native output is `../../target/debug/liborchiddb_java.dylib` on macOS or `liborchiddb_java.so` on Linux. A release native build uses `cargo build --manifest-path native/Cargo.toml --locked --release`; load the library from `../../target/release/` instead. Native binaries must match your JVM's OS and architecture. On Windows, build with Cargo, then run Maven with `-Dorchiddb.native.path=C:\absolute\path\orchiddb_java.dll`.

`Query.sparql(text, rdfRules, dataset)` accepts relational RDF rules as immutable
maps. Each rule names a registered table and subject/predicate/object term
mappings, with an optional graph. The existing ontology overload remains
available. Relational RDF rules require the corresponding shared-mapping native
compiler build and query the same caller-owned connection.

### Generate and reuse statistics

Statistics are optional. Generate them explicitly through the same graph/session you use for
queries; the shared Rust coordinator chooses bounded reads and computes the catalog. There
are no collection tiers or tuning profiles. Generation may make several SQL calls.

```java
try (var graph = orchid.graph(mapping)) {
  String report = graph.generateStatistics();
  graph.saveStatistics(Path.of("statistics.json"));
  var plan = graph.plan(Query.cypher("MATCH (p:Person) WHERE p.age = 36 RETURN p.name"));
  System.out.println(plan.diagnosticsJson());
  graph.clearStatistics();
  graph.loadStatistics(Path.of("statistics.json"));
}
```

Subsequent Cypher, Gremlin, and SPARQL compilations use the retained native catalog automatically.
Plan-cache keys include the catalog identity. Regeneration installs a replacement only after
successful completion; interruption leaves the previous snapshot active. Clear or close the graph
to release its catalog. No connection is closed by graph close, and no background refresh runs.
Call `generateStatistics(metadataQuery)` to include a SPARQL query's RDF rules and dataset metadata.

The report records partial coverage when reads cannot be supported or complete within the shared
work budget. JDBC collection applies row limits and deadline/thread-interruption cancellation;
the JDBC driver must honor `Statement.cancel()`. Adapters without bounded collection support
report missing coverage instead of falling back to unrestricted scans. Persisted statistics are
estimates, not constraints: regenerate explicitly after substantial data changes, and retain
snapshots only where their sampled values can be stored safely.

Applications owning their sessions can use `Statistics.generate(compiler, compilation, session)`
and retain the returned `AutoCloseable` catalog. Pass its `catalogId()` as the final `Compilation`
argument when compiling directly. `SqlCompiler.statisticsCommand(String)` exposes the same JSON
protocol for custom transports; `ExecutionEngine.Session.readStatistics(StatisticsRead)` is the
only additional execution seam. Clients do not implement sampling policy or estimators.
