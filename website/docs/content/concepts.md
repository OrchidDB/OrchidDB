# Core concepts

Understand the graph model, the mapping contract, and how a graph query reaches a relational engine.

## Nodes and relationships

A node has an identity, labels, and properties. A relationship connects a source node to a destination node and has a type and properties. A label such as `Person` identifies a group of nodes; a type such as `FOLLOWS` identifies a group of relationships.

In a mapped graph, these concepts come from your schema. A node's identity can be a table's primary key. Relationship endpoints can be foreign-key columns. Properties can use graph names different from their physical column names.

## Managed and mapped data

| Model | Data ownership | Entry point |
| --- | --- | --- |
| Managed graph | OrchidDB persists graph records in DuckDB. | `GraphEngine` |
| Mapped property graph | Your tables or views provide nodes and relationships. | `MappedGraphEngine` |
| Mapped RDF dataset | Your quad tables provide RDF statements. | `RdfGraphEngine` |

Mapped engines use the schemas registered in their mappings to plan queries, while DuckDB reads the source rows. Registering a schema describes the table to the planner.

## Graph IR

Graph IR is the common representation produced by the language frontends. It carries graph operations such as node scans, expansion, filtering, projection, aggregation, and path traversal.

Keeping these operations explicit gives the planner the information needed to preserve language semantics while choosing relational execution. The [execution guide](execution.md) shows how to inspect this representation.

## SQL islands

A SQL island is a region of a graph plan that can execute as relational SQL. DuckDB performs that work, and the result is represented as Arrow data. Hybrid managed execution combines SQL islands with graph runtime operators.

```text
Query text
    ↓
Language parser and planner
    ↓
Graph IR
    ↓
Relational lowering
    ↓
DuckDB SQL execution + graph runtime where selected
    ↓
Arrow results
```

## Vocabulary and storage

An ontology mapping associates RDF class and predicate IRIs with graph labels, properties, and relationship types. A graph mapping then associates those concepts with physical tables and columns. This lets SPARQL use the same property graph as Cypher and Gremlin.

When your source is already RDF, use an [RDF dataset mapping](rdf.md) to connect quad tables directly.

## Identity and ordering

Choose stable, non-null node IDs. For mapped relationships, source and destination values refer to IDs of the configured endpoint labels. Provide a relationship ID when individual edges need an explicit identity.

Request order in the query whenever the consumer needs it: use Cypher `ORDER BY`, Gremlin `order()`, or SPARQL `ORDER BY`. Explicit order also makes examples and paginated application output reproducible.
