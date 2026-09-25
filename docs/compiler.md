# SQL-only compiler for client bindings

`orchiddb::compiler` is the database-free entry point for clients that own their
SQL engine. It parses Cypher, Gremlin or mapped SPARQL, lowers through Graph IR
and relational IR, and returns SQL plus output field names. It never opens a
connection, registers functions, mutates a schema, or executes a SQL statement.

```toml
[dependencies]
orchiddb = { path = "../orchiddb", default-features = false }
```

The Java binding lives in the sibling development directory `orchiddb-java`.
Its JNI library uses this configuration: DuckDB and `libduckdb-sys` are absent
from the native dependency graph. Existing managed engine/CLI builds retain
their optional `duckdb` feature and default behavior.

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

Regression tests: `cargo test --no-default-features --test sql_compiler`.
