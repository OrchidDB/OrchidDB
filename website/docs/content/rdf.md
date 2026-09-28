# RDF over application tables

Use `GraphEngine` for Cypher, Gremlin, and SPARQL over the same application tables.
Add RDF vocabulary to `GraphMapping`: each rule describes a subject, predicate,
object, and optional graph. SPARQL patterns resolve to scans and joins of those
sources. OrchidDB does not create or populate a triple store. An ontology or explicit
`rdf:type` root is not required for direct column mappings.

The [complete Rust example](/downloads/rdf_graph.rs) creates a small application
table, queries typed RDF terms, updates through SPARQL, reads the change through
Cypher in the same transaction, and rolls it back. Use the dependencies in
[installation](installation.md#use-the-rust-library).

## Map columns and composite identities

Given `customers(tenant BIGINT, id BIGINT, name VARCHAR)`, register its schema and
map its name column:

```rust
use std::sync::Arc;
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::engine::GraphEngine;
use orchiddb::ir::rel::mapping::{GraphMapping, NodeMapping, schema_only_provider};
use orchiddb::ir::rel::rdf_mapping::{RdfMapping, RdfTermMapping as Term};

let connection = duckdb::Connection::open("application.duckdb")
    .map_err(|e| e.to_string())?;
let mut mapping = GraphMapping::new();
mapping.register_table("customers", schema_only_provider(Arc::new(Schema::new(vec![
    Field::new("tenant", DataType::Int64, false),
    Field::new("id", DataType::Int64, false),
    Field::new("name", DataType::Utf8, true),
]))));
mapping.map_node(NodeMapping::table("Customer", "customers", ["tenant", "id"])
    .property("name", "name"));
mapping.map_rdf(RdfMapping::table(
    "customers", Term::template("urn:customer:", ["tenant", "id"]),
    "https://example.com/name", Term::literal("name"),
).writable(["tenant", "id"]));
let mut graph = GraphEngine::mapped(connection, Arc::new(mapping))?;
let result = graph.sparql_query(
    "SELECT ?name WHERE { ?customer <https://example.com/name> ?name }",
    "default",
).await?;
```

`Term::iri(column)` reads existing IRIs. `Term::template(prefix, columns)` encodes
ordered key components as UTF-8 hexadecimal separated by `/`: tenant `1`, id `7`
become `urn:customer:31/37`. Binary key columns use their original bytes. The encoding is reversible and escapes separators.
A null component emits no statement. Blank-node templates add a declared scope.

Literal rules infer RDF datatypes from registered column types. Use the `Literal`
variant to specify a datatype, constant language, or language column. Language
tags are normalized to lowercase. Native numeric columns expose their stored
value; a text column can preserve an original lexical spelling.

## Map relationships

An orders table can expose a composite foreign key directly:

```rust
mapping.map_rdf(RdfMapping::table(
    "orders", Term::template("urn:customer:", ["tenant", "customer_id"]),
    "https://example.com/hasOrder", Term::template("urn:order:", ["tenant", "id"]),
).writable(["tenant", "id"]));
mapping.map_rdf(RdfMapping::table(
    "orders", Term::template("urn:order:", ["tenant", "id"]),
    "https://example.com/total", Term::literal("total"),
));
```

Register the orders schema and these rules before constructing the engine.
Partial-null foreign keys emit no relationship. Independent edge tables work the
same way: map their source and destination key columns. Registered SQL views can
provide joins or computed read projections.

```sparql
PREFIX ex: <https://example.com/>
SELECT ?name ?total WHERE {
  ?customer ex:name ?name ; ex:hasOrder ?order .
  ?order ex:total ?total .
  FILTER(?total > 100)
}
```

No type assertion is required. Constant predicates prune unrelated mappings.
Compatible composite identity mappings retain native key columns for joins.
Variable predicates enumerate applicable rules. Overlapping statements are
removed at graph boundaries; query solution multiplicity follows SPARQL.

## Default and named graphs

Rules without a graph belong to the default graph. Add
`.graph(Term::constant("urn:team"))` or `.graph(Term::iri("graph_iri"))` for named
graphs. A null mapped graph expression emits no statement. `GRAPH ?g`, `FROM`, and
`FROM NAMED` use the same rules. An explicit named-graph registry preserves empty
graphs; configure it through `RdfDatasetMapping::map_writable_named_graphs` and
attach it with `GraphMapping::with_rdf_mapping` before adding further RDF rules.

## Updates and transactions

Call `graph.sparql_update(update, "default", None).await?`. Writable rules declare
a complete physical row key. Subject and object identities determine key/FK
values; property inserts for a new subject combine into one row insertion.
Deleting a property clears its column, deleting an FK relationship unlinks its
non-key columns, and deleting an all-key edge or class assertion deletes its row.
`DELETE/INSERT` replaces values in a single row transition, including required
columns. Defaults and database constraints still apply.

A scalar column cannot represent two different simultaneous RDF values. To
replace a value, delete the old statement and insert the new one in the same
request. Ambiguous targets, missing key components, and constraint failures abort
the request. No fallback storage is created.

`begin`, `commit`, and `rollback` cover all three query languages. SPARQL writes
are immediately visible to Cypher and Gremlin on that connection. A failed
explicit transaction must be rolled back. Automatic transactions roll back on
errors or cancellation.

## Inspect execution and results

`sparql_query` returns typed `SparqlResults`: SELECT solutions, an ASK boolean, or
CONSTRUCT/DESCRIBE graph triples. `sparql_dataset` returns `QueryResult`, with
Arrow data in `returned.batch` and execution statistics in `stats`.
`sparql_sql` shows generated SQL. `into_executor` returns the connection for
further relational work.

Application-owned quad tables remain a supported source shape. Existing
`RdfDatasetMapping` configurations attach through
`GraphMapping::new().with_rdf_mapping(mapping)`. The old `RdfGraphEngine` API is a
compatibility facade delegating to `GraphEngine`; it owns no separate executor.

## Migration from RDF-specific engine construction

New code constructs `GraphEngine::mapped(connection, Arc::new(mapping))` once.
`GraphEngine` re-exports `SparqlResults` and `RdfTermValue`; callers do not need an
RDF-specific engine to decode terms. Existing facades retain their signatures:

| Existing RDF facade | Shared GraphEngine API |
| --- | --- |
| `query(query)` | `sparql_query(query, dataset)` |
| `sparql(query)` | `sparql_dataset(query, dataset)`; access `returned` for batches |
| `sql(query)` | `sparql_sql(query, dataset)` |
| `update(update, base)` | `sparql_update(update, dataset, base)` |
| `last_query_stats()` | `sparql_dataset(...).await?.stats` |
| `into_executor()` | `into_executor()` |

The dataset argument selects a declared dataset (usually `"default"`); it is
separate from named graphs selected by `GRAPH`. `sparql_query` returns typed terms
without the statistics wrapper. `sparql_dataset` exposes the logical/physical
plans, generated SQL, and physical table choices. The ontology convenience method
`sparql(query, ontology)` continues to return legacy scalar columns.

## Logical sources and collections

RDF rules can reference the same partition-layout logical sources and expanded
collection tables as node and edge mappings. Register physical providers first,
then layout definitions, then collection sources. RDF filters participate in
physical layout selection; collection parent filters may push beneath `Unnest`,
while element filters remain after expansion. Null mapped terms emit no statement;
duplicate statements are removed at RDF graph boundaries.

Use these derived sources for reads. Writable RDF rules should reference the
physical row owner and supply a complete key; declaring a derived mapping
writable does not make its expansion or layout alternatives writable. See
[collection mappings](mapping-reference.md#collection-columns-as-logical-tables)
and [physical layouts](mapping-reference.md#physical-layout-alternatives).

## Execution boundary

SPARQL uses the shared relational DAG and DuckDB transaction owner. It does not
create another connection for RDF queries or updates. Remote `SERVICE`, including
`SERVICE SILENT`, is unsupported. Native maps/JSON are not implicitly RDF sources;
use explicit columns, term rules, or supported collection mappings.

## Equivalent source representations

RDF rules can use a [representation source](representations.md) as their table.
The shared relational optimizer can then choose a collection expansion, a SQL
derivation, or an equivalent materialized table. `sparql_dataset` exposes the
choice in `stats.representation_selections`; RDF term construction and graph
semantics remain above that choice. Representation sources are read-only even
if an RDF rule declares itself writable.
