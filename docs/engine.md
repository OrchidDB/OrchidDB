# DuckDB graph engine

The crate provides two integration APIs, both behind the default `duckdb`
feature. Language coverage remains partial; unsupported SQL shapes return an
error in SQL-only execution. The corpus counts in `HANDOFF.md` are historical
measurements, not a claim of complete language conformance.

## Managed graph

`engine::GraphEngine` owns a graph stored in a DuckDB database file. Use
`GraphEngine::open(path)` for durability or `GraphEngine::in_memory()` for an
ephemeral graph. `cypher`, `cypher_with_params`, `gremlin`, and `sparql` return
Arrow batches plus execution metadata. SPARQL takes an explicit ontology.

```rust,no_run
use new_graph::engine::GraphEngine;

# async fn example() -> Result<(), String> {
let mut graph = GraphEngine::open("social.duckdb")?;
graph.begin()?;
graph.cypher("CREATE (:Person {name:'Alice'})-[:KNOWS]->(:Person {name:'Bob'})").await?;
graph.commit()?;
let result = graph.cypher(
    "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name"
).await?;
println!("{:?}", result.backend);
# Ok(()) }
```

Each write outside an explicit transaction commits atomically. Inside a
transaction, later queries see earlier writes. A failed graph statement restores
its prior state. `rollback` and dropping an active engine discard uncommitted
writes. Separate engines use DuckDB snapshot isolation; conflicting updates
fail and require rollback. Outside a transaction, each query refreshes the
committed snapshot so an engine sees other committed writers.

Parameters are bound into the parsed AST as typed values, without interpolating
query text. Missing parameters fail before execution. `replace_graph` imports
an Arrow-backed `PropertyGraph` in one transaction.

The default `ReadMode::Hybrid` uses DuckDB SQL islands and the Graph IR runtime
for residual operators. `ReadMode::SqlOnly` returns an error when a read cannot
be fully executed in DuckDB. `QueryResult.backend` and `stats` expose the
execution path. `set_sql_timeout` bounds individual DuckDB queries and setup;
it does not bound residual runtime work or snapshot serialization.
The runtime rejects repeats exceeding 10,000 iterations with an execution-limit
error. `NEW_GRAPH_INTERPRETER_MAX_STEPS` can impose a lower total operation budget.
These limits fail explicitly; they never return a successful truncated result.

Managed mutation semantics currently execute in the Graph IR runtime. Persistence
uses DuckDB transactions to upsert only touched node, edge, and label-metadata
records; ordinary writes do not rewrite the graph checkpoint. The revision row
serializes writers, preserving allocation and `MERGE` consistency. Records have
versioned checksums covering their identity and payload, and preserve exact value
types, relationship order, deletion tombstones, and allocation counters.

`checkpoint()` compacts the records into a graph checkpoint and clears the record
table atomically. It joins an explicit transaction when one is active, so rollback
also undoes compaction. `replace_graph` installs a new checkpoint and clears the
old records. Opening a format-1 database upgrades its schema to format 2 in a
transaction while preserving its checkpoint bytes. Older engine versions cannot
read format 2.

A revision check avoids reloading an unchanged graph. When committed data changes,
the engine still reconstructs the whole graph in memory from its checkpoint and
records. Mutation execution still clones the in-memory graph for statement
rollback, and SQL reads materialize their inputs into a query session. Incremental
persistence removes whole-graph serialization on ordinary writes; it does not yet
provide disk-backed traversal or direct SQL compilation of managed mutations.

## Existing DuckDB tables

`mapped_engine::MappedGraphEngine` combines a `DuckDbExecutor` with a
`GraphMapping`. Register schemas and map labels, relationships, identifiers,
and properties using `ir::rel::mapping`. The engine compiles queries against
those existing tables or views; it does not load their contents into the graph
runtime. See `tests/mapped_engine.rs` and `tests/byos_smoke.rs` for concrete
schema mappings.

The `cypher`, `gremlin`, and `sparql` methods are SQL-only read APIs and reject
graph mutations. `explain_cypher` returns generated read SQL.

`cypher_update` and `cypher_update_with_params` are explicit write APIs. They
compile one `MATCH ... SET binding.property = expression` assignment to native
DuckDB `UPDATE` and return the affected-row count. The match and assigned value
are evaluated in SQL, without loading source tables into the graph runtime.
For example, with a `Person` mapping exposing `age`:

```rust,no_run
# async fn update(mapped: &mut new_graph::mapped_engine::MappedGraphEngine) -> Result<(), String> {
let changed = mapped.cypher_update(
    "MATCH (p:Person) WHERE p.age >= 18 SET p.age = p.age + 1"
).await?;
println!("updated {changed} rows");
# Ok(()) }
```

Updates require a statically known label/type and a table-backed mapping.
Relationship updates require an explicit edge ID column. Matching IDs must be
non-null and unique; duplicate matches are rejected rather than picking an
arbitrary update value. Changes to identity or endpoint columns, query-backed
mappings, multiple assignments, whole-map replacement, and `RETURN` are rejected.
`CREATE`, `MERGE`, and `DELETE` are not supported by this API. Source column types
and constraints remain enforced by DuckDB.

Updates commit atomically outside explicit transactions, or participate in an
executor transaction started through `executor_mut().begin()`. Roll back explicit
transactions after a DuckDB transaction error. `execute_sql` remains available
for raw relational statements. Query support is bounded by relational lowering
and DuckDB result conversion. File-backed `DuckDbExecutor::open` and
`GraphEngine::open` sessions share a database instance, so they see committed
changes and detect write conflicts. For externally supplied connections, use
DuckDB `try_clone` when sessions must share the same live database instance.

Relational deduplication uses the requested keys and retains the first row's
other bindings. Element keys include both label and local ID. Ordered SPARQL
`SELECT DISTINCT` preserves ordering even when its sort field is not projected.
Repeated Gremlin deduplication keeps its cross-iteration state in the interpreter;
the relational backend explicitly declines that form.

Current completion work targets read execution and query conformance; new
`CREATE` and `DELETE` support is outside that scope.

## Existing RDF quad tables

`rdf_engine::RdfGraphEngine` runs SPARQL against user-owned DuckDB tables.
Register their schemas with `RdfDatasetMapping`, map one or more
sources to a dataset name, and give the engine a `DuckDbExecutor` using
the same DuckDB tables. `RdfGraphEngine::sparql` parses, lowers, and executes
the query without copying source rows or requiring an ontology mapping.

Sources can use IRI-only columns or typed term columns for IRIs, literals,
and blank nodes. A null graph value denotes the default graph. Queries support
variable predicates, joins across triple patterns, `GRAPH`, `FROM`, and
`FROM NAMED`; selected `FROM` graphs are merged with duplicate triples removed.
Property paths, aggregates, and expressions retain their SPARQL semantics
through Graph IR and SQL IR lowering. See `tests/sparql_typed_algebra.rs` and
`tests/sparql_property_paths.rs` for examples.

### Mapped SPARQL updates

Call `engine.update(request, base_iri).await` to execute a SPARQL update.
Writable sources are declared with `IriQuadSource::writable()`. Use
`.predicate(iri)` to partition predicates across specific tables; the same
predicate restriction applies to reads and writes. Named graph lifecycle uses
an existing registry declared with `map_writable_named_graphs`.

Updates write the exact tables and columns in those mappings. Each triple must
resolve to one writable target. WHERE clauses use the query planner and SQL
execution path, and their results are captured before DELETE/INSERT effects.
All deletes precede inserts for an operation; later operations see earlier
writes. The complete request commits atomically, rolling back every affected
table if mapping validation or a database constraint fails. Graph creation
registers a graph name in the mapped registry; it does not create SQL tables.

`tests/sparql_mapped_updates.rs` demonstrates predicate partitions across two
tables, updates derived from existing values, duplicate prevention, blank-node
identity, graph lifecycle, and rollback.

## Release work still required
- Full Cypher, Gremlin, and mapped SPARQL execution conformance. Current
  Gremlin gaps include implicit traversal order, partition strategies, path
  values, and placeholder graph algorithm properties in the interpreter.
- Consistent cancellation and resource limits across SQL and residual operators.
- Streaming results and disk-backed execution without full graph reconstruction.
- Stable, versioned public errors and a long-term storage migration policy.

The engine's integration tests exercise persistence, mutation visibility,
transactions, failed statements, parameters, and mapped SQL execution. Passing
those tests does not imply that the full imported language corpora pass.
