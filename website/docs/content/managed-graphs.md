# Managed graphs

Create an application-owned graph with persistence, mutations, and transactional query execution.

## Open a graph

```rust
use new_graph::engine::GraphEngine;

let mut graph = GraphEngine::open("social.duckdb")?;
```

Use `GraphEngine::in_memory()` for an ephemeral graph, such as a test fixture or a request-local calculation. Both constructors return `Result<GraphEngine, String>`.

## Create nodes and relationships

```cypher
CREATE (:Person {name:'Alice', age:30})
  -[:KNOWS {since:2020}]->
  (:Person {name:'Bob', age:28})
```

Run the statement with `graph.cypher(query).await?`. A write outside an explicit transaction commits atomically.

Create a relationship between existing nodes by matching their properties:

```cypher
MATCH (a:Person), (b:Person)
WHERE a.name = 'Alice' AND b.name = 'Bob'
CREATE (a)-[:FOLLOWS]->(b)
```

Use [parameters](parameters.md) for values supplied by your application.

## Read and update properties

```cypher
MATCH (p:Person)
WHERE p.name = 'Alice'
SET p.age = 31
RETURN p.name, p.age
```

A subsequent query sees the updated value:

```cypher
MATCH (p:Person)
RETURN p.name, p.age
ORDER BY p.name
```

## Merge and delete

Use `MERGE` when the statement should match or create the specified pattern:

```cypher
MERGE (p:Person {name:'Carol'})
RETURN p.name
```

Use `DETACH DELETE` when removing a node together with its incident relationships:

```cypher
MATCH (p:Person)
WHERE p.name = 'Carol'
DETACH DELETE p
```

## Import an in-memory graph

`replace_graph` accepts an Arrow-backed `PropertyGraph` and installs it as the engine's graph in one transaction. This is useful when an application constructs a graph through the catalog APIs and then wants managed execution and persistence.

```rust
graph.replace_graph(property_graph)?;
```

This operation replaces the stored graph. Use it at an intentional import or reset boundary.

## Inspect execution

Managed queries return `QueryResult`, which includes an Arrow result, an execution backend, and statistics:

```rust
let result = graph.cypher("MATCH (p:Person) RETURN p.name").await?;
println!("backend: {:?}", result.backend);
println!("rows: {}", result.returned.batch.num_rows());
```

Continue to [transactions and storage](transactions.md) for multi-statement work, or [execution and plans](execution.md) for read policies.
