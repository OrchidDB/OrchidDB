# SQL compiler and caller-owned engines

`orchiddb::compiler` is the database-free entry point for clients that own their
SQL engine. It parses Cypher, Gremlin or mapped SPARQL, lowers through Graph IR
and relational IR, and returns SQL plus output field names. It never opens a
connection, registers functions, mutates a schema, or executes a SQL statement.

```toml
[dependencies]
orchiddb = { path = "../orchiddb" }
```

The [Java binding](https://github.com/OrchidDB/OrchidDB-java) uses the same compiler.
Its JNI library uses this configuration: DuckDB and `libduckdb-sys` are absent
from the native dependency graph. Existing managed engine/CLI builds retain
their optional `duckdb` feature; it must now be enabled explicitly.

## Versioned request

`compile(CompileRequest)` is the typed Rust API. `compile_json(&str)` provides
the same API for language bindings. Both return errors before client execution.
The JSON protocol is version 1; unknown fields and unsupported versions fail.

```json
{
  "version": 1,
  "dialect": "duckdb",
  "language": "cypher",
  "query": "MATCH (p:Person) WHERE p.name=$name RETURN p.name AS name",
  "parameters": {"name": "Ada"},
  "tables": [{
    "name": "\"people\"",
    "columns": [
      {"name": "id", "data_type": "int64", "nullable": false},
      {"name": "name", "data_type": "string", "nullable": true}
    ]
  }],
  "nodes": [{
    "label": "Person", "table": "\"people\"", "id": "id",
    "properties": {"name": "name"}
  }]
}
```

```rust,ignore
let response = orchiddb::compiler::compile_json(request_json).await?;
// Response contains version, dialect, sql, fields.
// The client decides whether, where and how to execute that SQL.
```

Table names use SQL identifier syntax, including quoted qualified identifiers.
Bindings must quote individual identifier parts. Column/property names are
literal strings. Schemas contain metadata only; no source data is passed into
the compiler. Mapped identities must be signed integers. Bindings must ensure
ID uniqueness/non-nullability and referential integrity in their actual data.

Optional `edges` describe label, table, id, source/target columns,
source_label/target_label, and property-to-column mappings. Optional `functions`
describe language-facing name, SQL target, parameter type list, return type
(`returns`), and `aggregate` flag. The caller installs real implementations in
its engine. SPARQL's optional `ontology` maps classes, properties and directed
relationships; see the public request structs for the complete contract.

Supported schema types are boolean, int8/int16/int32/int64, float32/float64,
string, binary, date, timestamp (microseconds, no timezone), and
`decimal:precision:scale` with precision up to 38. Unsupported source types
require a caller-owned cast/view. Unsupported SQL dialects fail explicitly;
DuckDB and PostgreSQL rendering are currently implemented.

## Boundaries

The API specializes SQL to typed Cypher parameter values. It does not return
JDBC placeholders. Bindings must treat generated SQL as potentially sensitive
and include parameters, schema, ontology, function declarations, mappings, and
dialect in their plan cache keys. Gremlin/SPARQL parameter bindings are not yet
implemented. Do not treat the full runtime's language conformance as coverage
of the standalone SQL subset.

Writes, remote SERVICE calls, opaque extensions, and unlowerable operations
fail. Internal table materializations are rejected by inspecting the logical
plan before unparsing; they are never collected or executed. Constant seed
rows in mapped plans are emitted as SQL expressions instead of private tables.

Source-to-engine routing belongs to the client. Today's Java binding requires
one engine per graph, while keeping engine IDs on every mapped source and plan.
A future federated coordinator can split Graph IR into source-specific fragments
and use the compiler for each engine without changing connection ownership.

Regression tests: `cargo test --test sql_compiler --test execution`.

## Caller-owned data flow

You can execute `CompiledSql.sql` directly. For a common adapter boundary,
implement `execution::SqlSession`: declare a dialect, a driver error type, and a
result type that can borrow the session. Its async `query` method executes the
SQL; `execution::execute` first rejects protocol/dialect mismatches. It performs
no schema discovery, setup statements, data copying, buffering or commits.
Futures may stay on the calling thread; `Send` and background scheduling are not
required. Your adapter controls streaming errors, cancellation and drop cleanup.

The [DuckDB application example](https://github.com/OrchidDB/OrchidDB/tree/main/examples/duckdb-client) declares its own
driver dependency and implements the interface with a borrowed connection and
streaming cursor. It registers a SQL function, queries uncommitted caller data,
and verifies caller rollback afterward:

```sh
cargo run --manifest-path examples/duckdb-client/Cargo.toml
```

For SQL output alone, with no DuckDB build:

```sh
cargo run --example compile_sql
```

Resolve metadata and compile against the same session/schema version used for
execution. The dialect check cannot distinguish two databases using the same
SQL dialect; engine identity and routing belong to your application.
