# SQL compiler and caller-owned engines

`orchiddb::compiler` is the database-free entry point for clients that own their
SQL engine. It parses Cypher, Gremlin or mapped SPARQL, lowers through Graph IR
and relational IR, and returns SQL plus output field names. It never opens a
connection, registers functions, mutates a schema, or executes a SQL statement.

```toml
[dependencies]
orchiddb = { path = "../orchiddb" }
```

All [language clients](client-apis.md) use the same compiler. The [Java binding](https://github.com/OrchidDB/OrchidDB-java) exposes it through JNI.
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
// Response contains version, dialect, sql, fields, logical_plan,
// constraint_proofs, and layout_selections.
// The client decides whether, where and how to execute that SQL.
```

Table names use SQL identifier syntax, including quoted qualified identifiers.
Bindings must quote individual identifier parts. Column/property names are
literal strings. Schemas contain metadata only; no source data is passed into
the compiler. Mapped identities may use a non-null scalar or an ordered tuple of
non-null scalars: booleans, integers, floats, strings, binary, decimals, dates,
times, timestamps, durations and intervals. Floating-point keys follow the target
engine's equality semantics. Each label retains its native component types.
Heterogeneous scans and joins use typed struct variants to preserve types and
component boundaries; label equality accompanies identity comparisons.
Node `id` and edge `id`, `source`, and `target` accept either a column-name string
or an ordered array of column names, for example `"id": ["tenant", "id"]`.
Endpoint columns correspond positionally to the referenced node key. Arrays must
be nonempty with distinct column names; a one-column array is a scalar key.
Bindings must ensure
ID uniqueness/non-nullability and referential integrity in their actual data.
Mapped identities do not require a physical primary-key index; supported
index types depend on the target database.

Optional `edges` describe label, table, id, source/target columns,
source_label/target_label, and property-to-column mappings. An FK-backed edge also
sets `foreign_key` to `"src"` or `"dst"` (the endpoint whose row owns the FK), uses
the child table, and supplies the child key as its `id`. Partially NULL FKs do not
produce graph edges. These declarations describe reads; the standalone compiler
is not a mutation engine. Gremlin tuple-ID projection may require the unified
runtime's native operator rather than standalone SQL compilation.

Optional `functions`
describe language-facing name, SQL target, parameter type list, return type
(`returns`), and `aggregate` flag. The caller installs real implementations in
its engine. SPARQL's optional `ontology` maps classes, properties and directed
relationships; see the public request structs for the complete contract.

Supported schema types are boolean, int8/int16/int32/int64,
uint8/uint16/uint32/uint64, float32/float64, string, binary, date, time and
duration (microseconds), interval (month/day/nanosecond),
timestamp (microseconds, no timezone), and
`decimal:precision:scale` with precision up to 38. Native list and struct schema
declarations use `list:<element-type>` and `struct:<JSON field-to-type object>`,
for example `list:string` or `list:struct:{"sku":"string","quantity":"int64"}`.
These type strings recurse; collection mappings expose one list level and scalar
struct fields. Unsupported source types
require a caller-owned cast/view. Unsupported SQL dialects fail explicitly;
DuckDB and PostgreSQL rendering are currently implemented.

## Collection-backed logical tables

The optional `collection_sources` array defines named relations by expanding a
native list column. Each definition supplies `name`, parent `table`, list
`column`, optional `parent_columns` (output alias to parent column), and either
`element` (a scalar element's output name) or `fields` (output alias to struct
field). Node, edge, and RDF mappings reference the resulting relation by name.

```json
{
  "collection_sources": [{
    "name": "person_tags",
    "table": "people",
    "column": "tags",
    "parent_columns": {"person_id": "id"},
    "element": "tag"
  }]
}
```

This is a request fragment; declare `people.id` and `people.tags` in `tables`,
using `list:string` for the tags column. No database view is created or required.
For a full struct-list mapping, identity rules, and limitations, see
[collection columns as logical tables](mapping-reference.md#collection-columns-as-logical-tables).

The compiler registers physical schemas, then optional `logical_sources`
(physical-layout alternatives), then collection sources. Parent filters can guide
layout selection below `UNNEST`; element filters apply after expansion.
`logical_plan` exposes the expansion and `layout_selections` reports physical
choices and parent-scan byte/file estimates. Those estimates do not describe
expanded row counts. Include collection and layout definitions and their metadata
in plan cache keys. These definitions do not enable writes.

## Boundaries

The API specializes SQL to typed Cypher parameter values. It does not return
JDBC placeholders. Bindings must treat generated SQL as potentially sensitive
and include parameters, schema, ontology, function declarations, mappings, and
dialect in their plan cache keys. Gremlin/SPARQL parameter bindings are not yet
implemented. Do not treat the full runtime's language conformance as coverage
of the standalone SQL subset.

Writes, opaque extensions, and unlowerable operations fail. SPARQL `SERVICE`,
including `SERVICE SILENT`, is unsupported in both compiler and managed runtime;
OrchidDB does not make remote SPARQL HTTP requests. Internal table materializations
are rejected by inspecting the logical plan before unparsing; they are never collected or executed. Constant seed
rows in mapped plans are emitted as SQL expressions instead of private tables.

Source-to-engine routing belongs to the client. Today's Java binding requires
one engine per graph, while keeping engine IDs on every mapped source and plan.
A future federated coordinator can split Graph IR into source-specific fragments
and use the compiler for each engine without changing connection ownership.

Regression tests: `cargo test --test sql_compiler --test execution`.

## Caller-owned data flow

You can execute `CompiledSql.sql` directly. For a common adapter boundary,
implement `execution::SqlSession`: declare a dialect, a driver error type, and a
result type implementing Arrow `RecordBatchReader` that can borrow the session. Its async `query` method executes the
SQL; `execution::execute` first rejects protocol/dialect mismatches. It performs
no schema discovery, setup statements, data copying, buffering or commits.
Futures may stay on the calling thread; `Send` and background scheduling are not
required. Your adapter controls streaming errors, cancellation and drop cleanup.

The [DuckDB application example](https://github.com/OrchidDB/OrchidDB/tree/main/examples/duckdb-client) declares its own
driver dependency and implements the interface with a borrowed connection and
native Arrow reader. Retained Rust batches own their buffers after advancing or dropping the reader. It registers a SQL function, queries uncommitted caller data,
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

## RDF rules in compiler requests

SPARQL requests accept `rdf` rules and an optional `dataset` (default `default`).
The same `tables`, `nodes`, and `edges` catalog can serve all three languages.
For example, a rule over registered customer columns is:

```json
{"table":"customers",
 "subject":{"kind":"template","prefix":"urn:customer:","columns":["tenant","id"]},
 "predicate":{"kind":"constant","value":"https://example.com/name"},
 "object":{"kind":"literal","column":"name"}}
```

Term kinds are `iri` (column), `template` (prefix and ordered columns), `blank`
(scope and ordered columns), `literal` (column with optional datatype, language,
or language_column), and `constant` (value with optional datatype or language).
Rules may specify `graph` and `dataset`. Runtime writes additionally require
`writable: true` and a complete `key` array. The compiler remains read-only.
Existing `ontology` declarations translate into the same relational rules.

Requests using `rdf` retain typed RDF output: each variable's lexical column is
accompanied by `__rdf:term:kind:?variable`, `__rdf:term:datatype:?variable`, and
`__rdf:term:language:?variable`. A null kind means unbound. `fields` lists the
visible variable names. Legacy ontology requests retain their scalar result
columns and native numeric types.
