# One graph engine, mapping-driven DuckDB storage

## Contract

OrchidDB has one language planner and one execution engine. DuckDB is the only
writable database backend in scope. A graph uses either the existing managed
layout or an explicit `GraphMapping` onto caller-owned tables. Storage location
must not select a different query implementation or a smaller language subset.

`GraphEngine` is the public engine. `MappedGraphEngine` is retained as a compatibility
facade, not an independent query executor. Both entry points share frontend
preparation, parameter validation, callable handling, and DAG execution helpers. The standalone SQL compiler remains a
compiler API; its representable SQL subset is not the runtime's language contract.

No supplemental tables, automatic column creation, implicit property mappings,
or overflow property storage are part of this change. Unknown mappings and writes
to read-only/query-backed sources fail explicitly. Source constraints remain
binding. A failed mapped write is never redirected into managed storage.

## Execution and identity

Both layouts execute through the existing DataFusion relational DAG and native
kernels, with SQL islands as the primary execution path. Preserve the managed implementation
and its observable language behavior during migration.

Node and edge identities are typed scalar or ordered composite source keys throughout the runtime,
including adjacency, paths, equality, serialization, SQL joins, mutations, and
result conversion. Do not remap source keys to synthetic integer identities.
Arrow row positions may locate data, but are not graph identities. Identical keys
in different label mappings do not identify the same element.

Mapped tables remain SQL sources. Construction and statement setup may read
schema metadata, but must not materialize source data. Filters, projections,
joins and supported traversals lower into SQL islands against those tables.
Residual kernels fetch referenced records in typed-key batches and adjacency
for their current traversal frontier, using the same transaction connection.
A query requesting the entire graph can naturally read it; a selective query
must not incur unrelated full-table scans or load unrelated mappings.

Execution statistics expose generated island SQL and residual source queries
and row counts. Regression tests must verify this execution contract in addition
to query answers; language conformance alone does not establish pushdown.

## Mutations and transactions

All source reads and writes for a statement use the same DuckDB transaction.
Native operators must see earlier writes in that statement, including database
defaults where observable. Property assignments update mapped columns; deletions
respect incident edges, mapping boundaries, and database constraints. Missing
keys require a supplied key or a declared database generation policy, regardless
of component types. Never use `MAX(id) + 1` or translate source keys to integer handles.
Do not invent string keys or silently replace existing rows.

Default and explicit transactions must preserve rollback behavior. Validate
mapping capabilities before publishing changes. Queries that cannot be represented
by the configured storage may fail with a storage-capability error; they must not
succeed and discard labels, properties, cardinality, or metadata.

## API

Construct the unified engine with `GraphEngine::mapped(connection, mapping)`.
The connection remains the source of truth; `mapping` is an `Arc<GraphMapping>`.
Existing `MappedGraphEngine` callers remain supported, including their active
executor transactions, but query execution delegates to the shared runtime.

For a node table `people(key VARCHAR PRIMARY KEY, name VARCHAR)`, a mapping can
declare `NodeMapping::table("Person", "people", "key").property("name", "name")`.
`g.addV('Person').property(T.id, 'alice').property('name', 'Alice')` then writes
that actual string key into `people.key`. No hidden integer key or extra table is
created. Cypher creation can supply the key through an explicitly declared key
property alias.

`Value::Node` and `Value::Edge` IDs and endpoints now use `ElementId` instead of
`i64`; callers constructing graph values must migrate accordingly. Scalar Arrow
values are preserved by `Value::Scalar`. Use typed scalar parameters when a key
needs a type or range that a language literal cannot express. Composite IDs use flat ordered lists at the language boundary and typed Arrow
structs internally. Every component must be non-null and scalar; nested collections
are rejected. Key order follows the mapping, not property iteration order. Existing managed snapshots and integer
records are read and migrated; new typed encodings require the new reader.

## Mapped execution

Both public entry points now use the shared DAG
runtime and mapped writes use the source-table adapter. The standalone mapped
mutation interpreter has been removed. Native element identities, adjacency,
paths, snapshots, incremental records, parameters, and JVM handles carry scalar
keys. Legacy managed integer keys remain actual integer values, not translations
of mapped keys.

The eager mapped snapshot adapter has been replaced by metadata binding, mapped
SQL islands and demand-driven native storage access. Mapped lateral subplans use
the same island planner and connection; JVM callbacks initialize lazily. The
[executed-plan review](mapped-execution-plan-review.md) and
[full SQL/DAG dump](mapped-execution-plans.md) document nine asserted examples. Inserts
require an explicit primary key, supplied through Gremlin `T.id` or a property
already mapped to the key column. `T.id` needs no property alias. There is no
implicit integer generation or extra-property storage. Constant defaults on
mapped columns are visible during the statement; omitted mapped values with
volatile/function defaults are rejected until their evaluation can be preserved
by the shared runtime. Unmapped source columns retain their SQL defaults and
constraints. Primary-key default generation is not implemented.

### Foreign-key-backed relationships

`EdgeMapping::foreign_key(ForeignKeyEndpoint::{Source,Destination})` explicitly
selects the endpoint whose row owns the FK. Its primary key also identifies the
edge. The runtime groups mutations by physical table and key, so child creation
and its relationship become a single complete insert. Existing child links are
updates; edge deletion clears the FK rather than deleting the row. Required
constraints remain binding. NULL FKs are excluded from SQL and native edge reads.

Write dependencies order parent insertion and child removal, preserving batching
within each dependency wave. Reparenting is an edge deletion followed by creation
in the same statement, persisted as one FK assignment. Column ownership must be
unique: FK columns are relationship-owned, and node and edge properties may not
alias the same non-key column. Cyclic writes requiring deferred constraints fail
explicitly. See the website mapping reference for Rust and TOML examples.


## Composite key mappings

`KeyColumns` accepts a string or an ordered array/vector, preserving existing
scalar constructor calls. Node keys, explicit edge keys and both endpoint keys
support multiple columns. TOML and compiler JSON use arrays for composite keys.
`ElementId::from_components` and `ElementId::cast_to` preserve and normalize each
component separately. Arrow IPC serialization carries the full typed tuple;
heterogeneous relational scans use sparse typed structs, never concatenated keys.
Traversal history uses length-delimited typed tokens solely for membership.

Writes group by table and the complete ordered key, merge child properties with
FK components, and match every key column in UPDATE/DELETE predicates. Dependency
ordering uses full referenced keys. Shared child-PK/FK components are immutable
and must agree with the selected parent. Any NULL FK component means no edge
(SQL MATCH SIMPLE); unlink clears nullable non-key components only. Fully
identifying child keys require their explicit relationship in the creating
statement. DuckDB's immediate constraints, including its restrictions on updating
indexed columns of referenced rows, remain binding.

See `examples/composite_keys.rs` for Cypher and Gremlin usage.
`tests/sql_compiler_composite_keys.rs` executes SQL-generated joins against DuckDB
with repeated IDs across tenants and partially NULL FKs. Scalar and heterogeneous
identity regressions remain covered by `tests/sql_compiler_identities.rs`.

## RDF integration

`GraphMapping::map_rdf` adds term rules over application columns to the same catalog
as node and edge mappings. `with_rdf_mapping` also accepts existing quad-source
catalogs. `GraphEngine::sparql_dataset` returns `QueryResult`, `sparql_query`
returns decoded `SparqlResults`, and `sparql_update` applies writable mapped row
transitions. All three languages share connection and transaction ownership.
`RdfGraphEngine` remains a compatibility facade; `RdfSession` is a borrowed
execution context, not an independent engine. Ontology builders over mapped
storage translate into these same rules.

See the [RDF guide](../website/docs/content/rdf.md) for term templates, datasets,
null and duplicate handling, update limitations, and migration signatures.
