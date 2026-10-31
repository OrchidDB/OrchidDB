# OrchidDB

OrchidDB is a Rust library for executing Cypher over SQL sources and Arrow data.
Applications register a schema, supply database sessions, and submit Cypher queries
with parameters. A Go binding uses the same core through an internal native ABI.

The built-in SQL sources are PostgreSQL, DuckDB, and Trino. DuckDB is the sole
completion engine. Fully pushed-down queries avoid initializing or executing
DuckDB; remaining relational work runs in DuckDB over Arrow inputs.

The compiler should push work into source SQL and avoid redundant label checks,
casts, row numbering, and value wrapping where the schema proves them unnecessary.
Arrow record batches carry results between the library and database sessions.

Engine extensions remain usable through function signatures and rewrite hooks.
For example, a PostgreSQL mapping can target pgvector functions; DuckDB mappings
can target installed extension functions; Trino mappings can target connector
functions. The application supplies the corresponding database capabilities.

The optional DuckDB extension is a separate UDF module under
`extension/functions`. It supplies Cypher semantic helpers that cannot be
expressed adequately in ordinary SQL. It registers scalar functions only.
Schema registration and Cypher query execution belong to the library.

The supported query language is Cypher. Gremlin, SPARQL, and the Python,
JavaScript, Java, C++, and Elixir clients have been retired. Weaviate,
Elasticsearch, Quickwit, Prometheus, Loki, and StarRocks are no longer built-in
source adapters. Historical reports and build caches may still mention them.

## Rust

Add a local dependency on `clients/rust`:

```toml
[dependencies]
orchiddb-client = { path = "path/to/orchiddb/clients/rust" }
```

The application supplies a `SqlSession` implementation for its driver:

```rust,ignore
use orchiddb_client::{Connection, Query, Schema};

let schema = Schema::from_json(&std::fs::read_to_string("schema.json")?)?;
let mut graph = Connection::new(session, schema);
let rows = graph.query(
    Query::cypher("MATCH (p:Person) WHERE p.name=$name RETURN p.name")
        .parameter("name", "Ada")
).await?;
for batch in rows {
    let batch = batch?; // Arrow RecordBatch
}
```

The complete borrowed DuckDB session example is in
[clients/rust/examples/borrowed_duckdb.rs](clients/rust/examples/borrowed_duckdb.rs).
Schema configuration and query text are separate API inputs.

## Declared table functions

Declare a row-producing SQL function with
`source_metadata.format = "table_function"`, its function name, arguments, and
returned columns. The application supplies the function in its database session.
The declaration and predicate binding are shared; SQL rendering currently supports
DuckDB and PostgreSQL.

For example, suppose the application provides `find_items(category)`, returning
`id`, `name`, and `price`. Register it as a graph source:

```json
{
  "tables": [{"name": "items", "columns": [
    {"name": "id", "data_type": "string"},
    {"name": "name", "data_type": "string"},
    {"name": "price", "data_type": "float64"},
    {"name": "category", "data_type": "string"}
  ]}],
  "nodes": [{"label": "Item", "table": "items", "id": "id", "properties": {
    "name": "name", "price": "price", "category": "category"
  }}],
  "source_metadata": [{"table": "items", "format": "table_function", "options": {
    "function": ["find_items"],
    "arguments": [{"column": "category"}]
  }}]
}
```

Query it through the normal Rust or Go connection with a bound parameter:

```cypher
MATCH (i:Item)
WHERE i.category = $category AND i.price < 100
RETURN i.name, i.price
```

The `category` equality supplies the required function argument; the price
predicate filters returned rows. Only columns explicitly declared in `arguments`
can supply inputs. These virtual input columns project the supplied value.

`function` is an array of identifier components, such as `["schema", "function"]`.
Arguments are ordered; omitting `name` makes an argument positional. Named
arguments follow positional arguments. Each argument references a declared
column whose type controls the SQL cast. A scalar `value` fixes the argument;
a conflicting query predicate still filters the result and never overrides it.
Without `value`, a required argument must have a direct equality to a non-null
literal or bound query parameter. An optional named argument (`"optional": true`)
is omitted when unbound, and its virtual input column is null. OR predicates,
ranges, and correlated expressions do not supply required arguments.

## Optional OPA authorization

Enable the Rust client's `opa` feature and attach a policy to a connection. The
application authenticates the user and supplies their groups. Policy configuration
is connection-owned and cannot be supplied or disabled through query/schema JSON.
The same `with_opa` method is available on `Connection`, `FederatedConnection`, and
`SourceConnection`.

```toml
[dependencies]
orchiddb-client = { path = "path/to/orchiddb/clients/rust", features = ["opa"] }
```

```rust,ignore
use std::sync::Arc;
use orchiddb_client::{Connection, Query, opa};

let mut config = opa::Config::new("https://opa.example/v1/data/orchid/allow");
config.batched_uri = Some("https://opa.example/v1/data/orchid/batch".into());
config.row_filters_uri = Some("https://opa.example/v1/data/orchid/rowFilters".into());
config.batch_column_masking_uri = Some("https://opa.example/v1/data/orchid/batchMasks".into());
let client = Arc::new(opa::Client::new(config)?); // share across connections
let mut identity = opa::Identity::new(authenticated_user);
identity.groups = authenticated_groups;
let policy = opa::Policy::new(client, identity)?;
let mut graph = Connection::new(session, schema).with_opa(policy);
let rows = graph.query(Query::cypher("MATCH (p:Person) RETURN p.name")).await?;
```

Go's packaged native runtime includes OPA support. Set `Config.OPA` (or
`SourceConfig.OPA`) with trusted identity and endpoints; leaving it nil disables
OPA. Configuration is copied at construction, and each connection pools HTTP
requests. Call `Close` to release its policy resources.

```go
graph, err := orchiddb.New(session, schema, orchiddb.Config{
    Dialect: "postgres",
    OPA: &orchiddb.OPAConfig{
        PolicyURI: "https://opa.example/v1/data/orchid/allow",
        RowFiltersURI: "https://opa.example/v1/data/orchid/rowFilters",
        BatchColumnMaskingURI: "https://opa.example/v1/data/orchid/batchMasks",
        User: authenticatedUser,
        Groups: authenticatedGroups,
    },
})
if err != nil { return err }
defer graph.Close()
```

The protocol follows [Trino's OPA integration](https://trino.io/docs/current/security/opa-access-control.html):
HTTP POST bodies contain `input.context` and `input.action`; OPA responses wrap
policy results in `result`. Context includes `identity.user`, `identity.groups`, a
query ID, the OrchidDB version, and configured `custom` context. Credentials,
query text, and parameter values are not forwarded.

| Operation | Resource | Required result |
| --- | --- | --- |
| `ExecuteQuery` | none | boolean |
| `SelectFromColumns` | `resource.table`, with referenced `columns` | boolean |
| Batched `SelectFromColumns` | `filterResources`, one entry per table | array of allowed indices; every requested table must be allowed |
| `GetRowFilters` | `resource.table` | array of `{ "expression": "SQL predicate" }` |
| `GetColumnMask` | `resource.column` | `{ "expression": "SQL expression" }` or null |
| Batched `GetColumnMask` | `filterResources`, one entry per column | array of `{ "index": 0, "viewExpression": { "expression": "SQL expression" } }` |

Table resources use `catalogName`, `schemaName`, and `tableName`; column resources
also have `columnName` and `columnType`. Unqualified names use configured defaults
(`orchiddb.default`). Names with catalog/schema qualification retain it. Configure
either `column_masking_uri` or `batch_column_masking_uri`, never both. A batch
policy must handle `SelectFromColumns`, rather than only Trino's metadata-listing
operations. An example adaptation of a boolean policy is:

```rego
package orchid
import rego.v1

default allow := false
allow if {
    input.context.identity.user == "alice"
    input.action.operation == "ExecuteQuery"
}
allow if {
    input.context.identity.user == "alice"
    input.action.operation == "SelectFromColumns"
    input.action.resource.table.catalogName == "orchiddb"
    input.action.resource.table.schemaName == "default"
    input.action.resource.table.tableName == "people"
}
batch contains i if {
    some i, resource in input.action.filterResources
    allow with input.action.resource as resource
}
rowFilters := []
batchMasks := []
```

Row filters are combined with AND over raw rows. Masks are then applied before
user predicates, joins, aggregates, and limits. Expressions are typed scalar SQL;
subqueries, statements, aggregates, windows, volatile functions, and alternate
expression identities are rejected. Existing permission scopes still apply.

`ExecuteQuery` is checked before frontend planning; resource checks and policy
expressions precede relational optimization. There are no per-row HTTP calls.
Repeated scans share one
resource decision within a query. Batch endpoints avoid per-column round trips;
unbatched requests run concurrently. One HTTP client pools connections and limits
in-flight requests across its connections (default 16). Batches are capped at 256
resources, responses at 1 MiB, and each request—including concurrency queueing—at
10 seconds. All limits are configurable. Decisions are fetched anew for every
query; there is no cross-user or stale-policy decision cache. Missing/malformed
results, invalid batch indices, network errors, and timeouts deny execution.

This layer currently covers read queries through the Rust connection APIs.
Managed stores, registered host functions/procedures, alternate physical source
layouts, and policy-expression impersonation are rejected on protected
connections. Statistics collection requires a separate administrative connection.
OPA is disabled by default; without a policy, no authorization HTTP calls or
policy plan transformations occur. Applications using low-level APIs must retain
the host-owned policy binding and execute only plans compiled with it.

Validation uses `cargo test --features opa --test opa` (requires the DuckDB CLI
and localhost HTTP access). For the real OPA test, start an OPA server with
`tools/diagnostics/opa-fixture.rego` and set `ORCHIDDB_TEST_OPA_URL` to its HTTP base
URL. The 64-column fixture checks that batched masks require three policy requests
instead of 66, and reports elapsed time with simulated 5 ms service latency.
This measures OrchidDB's batching benefit, not comparative Trino throughput.

## Go

The Go module is `github.com/OrchidDB/OrchidDB/clients/go`. Build its native
bridge locally and set `ORCHIDDB_NATIVE_LIBRARY` to the path printed by:

```sh
make native
```

Use a local Go module replacement when developing from this checkout. See
[clients/go/examples](clients/go/examples) for caller-owned session examples.
The native ABI is an implementation detail of the Go binding.

## Local builds

```sh
make build
make clients-check
make go-test
cargo test --test source_scope --test trino_pushdown --test source_completion
```

The default library build does not require a DuckDB extension or JVM. Enable the
`duckdb` Cargo feature when using the embedded DuckDB adapter and provide a
compatible DuckDB library, or use `duckdb-download` to obtain it.

Build and test the optional DuckDB scalar extension separately:

```sh
make extension
make extension-test
```

Extension binaries are tied to their DuckDB version and platform. The build
script uses pinned source and preserves downloaded dependencies and caches.
All builds and release validation run locally.
