# Update mapped tables

Use `GraphEngine::mapped` to read and write application tables with Cypher,
Gremlin, and SPARQL. All languages use the same mapping catalog, DuckDB
connection, and transaction owner.

## Create mapped rows

Declare every writable property, including a property alias for keys supplied
through Cypher. For example, add `.property("id", "id")` to the tutorial's
`Person` mapping:

```rust
graph.cypher(
    "CREATE (p:Person {id:4, name:'dave', age:29}) RETURN p.name"
).await?;
```

Mapped keys must be supplied explicitly. Scalar and ordered composite keys are
supported; there is no automatic integer allocation or primary-key default
generation. For Gremlin, `T.id` supplies the mapped key without requiring a
property alias:

```rust
graph.gremlin("g.addV('Person').property(T.id,4).property('name','dave').property('age',29)").await?;
```

These are alternative ways to create the same row; do not execute both against
the same existing key. Every composite-key component must be non-null. Identity
remains qualified by the mapped label or relationship type.

Constant defaults on omitted mapped columns are visible during the statement.
Volatile/function defaults on omitted mapped values are currently rejected.
Unmapped source columns retain their database defaults and constraints. Unknown
properties are not stored in supplemental columns or overflow tables.

## Update and return values

```rust
let result = graph.cypher(
    "MATCH (p:Person) WHERE p.id=4 \
     SET p.age=p.age+1, p.name='David' RETURN p.name,p.age"
).await?;
println!("{} rows", result.returned.batch.num_rows());
```

Use `cypher_with_params` to bind values. `SET p += {...}` assigns properties;
`SET p = {...}` clears omitted writable mapped properties to SQL NULL. Identity
and endpoint columns remain protected. Primary keys are immutable. Scalar and
composite keys use the same typed identity rules.

## Relationships and deletion

Independent edge-row mappings need explicit edge IDs for writes. Their endpoint
columns match node keys in declared order. An FK-backed relationship instead
uses its child row's key as the edge identity: creating a child and its required
relationship becomes one complete row insertion. Deleting that relationship
clears its nullable non-key FK components; it does not delete the child row.
See [FK-backed relationships](mapping-reference.md#writable-one-to-many-relationships).

```cypher
MATCH (p:Person {id:4})
DETACH DELETE p
```

Node deletion checks mapped incident relationships. Database constraints still
apply. Properties must have one write owner: the tutorial's `total` belongs to
`Order`, so update `o.total`, not an unmapped `ORDERED.total` edge property.

```cypher
MATCH (:Person)-[:ORDERED]->(o:Order)
WHERE o.total > 100.0
SET o.total = o.total + 10.0
RETURN o.total
```

## RDF updates

Add writable `RdfMapping` rules with complete physical keys, then call
`sparql_update(update, "default", None)`. RDF inserts and deletes translate to
mapped row transitions; they do not populate a separate triple store. A scalar
property replacement requires deleting the old RDF value and inserting the new
one in the same request. See [RDF updates](rdf.md#updates-and-transactions).

Query-backed mappings, partition-layout logical sources, and collection-backed
node/edge mappings are read-only. Updates must target writable physical rows.
No write is redirected to managed storage when a mapping cannot represent it.

## Transactions

`begin`, `commit`, and `rollback` on `GraphEngine` cover all three languages.
Statements outside an explicit transaction commit atomically; a failed mapped
execution within an explicit transaction requires rollback before further work.
See [mapped transactions](transactions.md#mapped-engine-transactions).

## Compatibility APIs

`MappedGraphEngine::cypher_update` and `cypher_update_with_params` remain for
callers needing an affected-row count. They accept one property assignment over
a mapped target, without RETURN, CREATE, DELETE, MERGE, or map replacement. They
execute through the shared runtime, not a separate direct-SQL update engine.
Use `GraphEngine::cypher` for multiple assignments and returned values.

The compatibility facade also retains `execute_sql` and `executor_mut`.
`GraphEngine` owns its connection: perform relational setup before passing the
connection to `GraphEngine::mapped`, or consume the engine with `into_executor`
to recover it for subsequent SQL work.
