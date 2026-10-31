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
