# Core concepts

Understand the graph model, the mapping contract, and how a graph query reaches a relational engine.

## Nodes and relationships

A node has an identity, labels, and properties. A relationship connects a source node to a destination node and has a type and properties. A label such as `Person` identifies a group of nodes; a type such as `FOLLOWS` identifies a group of relationships.

In a mapped graph, these concepts come from your schema. A node's identity can be a table's primary key. Relationship endpoints can be foreign-key columns. Properties can use graph names different from their physical column names.

## Compiler clients and runtime APIs

The [language clients](client-apis.md) compile mapped read queries to SQL without opening a database. Applications execute the SQL on their own engine and consume Arrow batches. These clients do not use the engine types below.

## Managed and mapped data

| Model | Data ownership | Entry point |
| --- | --- | --- |
| Managed graph | OrchidDB persists graph records in DuckDB. | `GraphEngine` |
| Mapped property graph | Your tables or views provide nodes and relationships. | `MappedGraphEngine` |
| Mapped RDF dataset | Your quad tables provide RDF statements. | `RdfGraphEngine` |

Mapped engines use the schemas registered in their mappings to plan queries, while DuckDB reads the source rows. Registering a schema describes the table to the planner.

## Graph IR

Graph IR (graph intermediate representation) is OrchidDB's shared logical query plan for Cypher, Gremlin, and SPARQL. It describes what a query should do using graph operations such as node scans, relationship expansion, filtering, projection, aggregation, and path traversal. It represents the query, not the stored graph data.

Each language frontend parses and validates a query, then translates it into a tree of these operations. Operators introduce bindings for nodes, relationships, and values; expressions refer to those bindings to read properties, compare values, or compute results. The plan also carries rules for missing values, matching, and paths so that sharing a representation preserves each language's behavior.

For example:

```cypher
MATCH (p:Person)-[:FOLLOWS]->(q:Person)
WHERE p.name = 'Ada'
RETURN q.name
```

Conceptually, this becomes a scan of `Person` nodes, a filter for `p.name`, an outgoing `FOLLOWS` expansion to another `Person`, and a projection of `q.name`. This is a simplified description, not literal explain output. Relational lowering uses the graph mappings to turn scans into table reads, a single-hop expansion into joins on endpoint identities, and property access into column expressions.

Graph IR gives the languages a common planning layer, so relational lowering and SQL generation can be shared instead of implemented separately for every language and SQL dialect. Keeping graph operations and their rules explicit also lets later stages preserve graph identity, duplicate results, and path constraints when translating them into relational operations.

Graph IR is lowered before execution. [Compiler clients](sql-compiler.md) return SQL for supported reads and reject queries that cannot be fully lowered. The optional managed runtime executes the lowered DataFusion plan with eligible SQL regions in DuckDB and remaining native kernels. Graph IR itself is not interpreted at runtime. The [execution guide](execution.md#inspect-graph-ir) shows how to inspect it.

## SQL islands

A SQL island is a region of the lowered relational plan that executes as SQL in DuckDB and returns Arrow data. DataFusion schedules the managed execution DAG and its remaining native kernels. Graph IR is a compiler representation; it is not interpreted at runtime.

```text
Query text
    ↓
Language parser and planner
    ↓
Graph IR
    ↓
Relational lowering
    ↓
DataFusion execution DAG + eligible DuckDB SQL regions
    ↓
Arrow results
```

## Vocabulary and storage

An ontology mapping associates RDF class and predicate IRIs with graph labels, properties, and relationship types. A graph mapping then associates those concepts with physical tables and columns. This lets SPARQL use the same property graph as Cypher and Gremlin.

When your source is already RDF, use an [RDF dataset mapping](rdf.md) to connect quad tables directly.

## Identity and ordering

Choose stable, non-null node IDs. For mapped relationships, source and destination values refer to IDs of the configured endpoint labels. Provide a relationship ID when individual edges need an explicit identity.

Request order in the query whenever the consumer needs it: use Cypher `ORDER BY`, Gremlin `order()`, or SPARQL `ORDER BY`. Explicit order also makes examples and paginated application output reproducible.
