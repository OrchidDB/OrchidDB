# Introduction

Bring embedded graph queries to your data lake. Learn to map relational tables, query with three graph languages, and embed OrchidDB in a Rust application.

## Start with your data

OrchidDB is a graph query layer built in Rust, with DuckDB execution, DataFusion planning, and Apache Arrow results. Describe nodes, relationships, and properties over your existing tables, then ask graph questions in Cypher, Gremlin, or SPARQL.

A graph mapping connects your application vocabulary to physical columns. A customer can become a `Person`, an order row can define an `ORDERED` relationship, and a SQL view can expose a selected group of accounts. Your database remains the source of those rows.

## Choose a starting point

| Your goal | Start here |
| --- | --- |
| Run your first graph query | [Quickstart](quickstart.md) |
| Query tables you already own | [Map existing tables](mapped-graphs.md) |
| Create and persist a graph | [Managed graphs](managed-graphs.md) |
| Query with an RDF vocabulary | [SPARQL](sparql.md) |
| Connect existing quad tables | [RDF datasets](rdf.md) |
| Embed the engine in Rust | [Rust API](rust-api.md) |
| Explore language integrations | [Client APIs](client-apis.md) |

## Three languages, one graph

Cypher describes patterns between nodes:

```cypher
MATCH (p:Person)-[:FOLLOWS]->(friend:Person)
WHERE p.name = 'alice'
RETURN friend.name
ORDER BY friend.name
```

Gremlin describes a traversal through the same relationships:

```gremlin
g.V().hasLabel('Person').has('name', 'alice')
  .out('FOLLOWS').values('name').order()
```

SPARQL uses an ontology mapping to connect a vocabulary to the graph:

```sparql
PREFIX ex: <https://example.com/>
SELECT ?name WHERE {
  ?person a ex:Person ; ex:name "alice" ; ex:follows ?friend .
  ?friend ex:name ?name .
}
ORDER BY ?name
```

The language frontends produce a shared Graph IR. Relational regions become SQL islands, which execute in DuckDB. Read [core concepts](concepts.md) for the execution model.

## Follow a learning path

Begin with [installation](installation.md), run the [quickstart](quickstart.md), and then build the [mapped graph tutorial](mapped-graphs.md). It uses a small customer and order dataset that also appears in the [query recipes](recipes.md).

For application integration, continue with [typed parameters](parameters.md), [Arrow results](results.md), and [transactions](transactions.md). The [CLI reference](cli.md), [mapping reference](mapping-reference.md), and [Rust API](rust-api.md) provide the details to keep nearby while coding.

## Project links

[Website](https://orchiddb.com/) · [GitHub](https://github.com/OrchidDB/OrchidDB) · [Community](https://orchiddb.com/community.html)
