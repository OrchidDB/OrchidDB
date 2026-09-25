# Introduction

Bring embedded graph queries to your data lake. Learn to map relational tables, query with three graph languages, and use OrchidDB from Rust, Java, Python, JavaScript/TypeScript, Elixir, or C++.

## Start with your data

OrchidDB is a graph query layer built in Rust, with DataFusion planning and Apache Arrow results. Compile SQL and execute through your own engine, or use the standalone CLI with bundled DuckDB and default Iceberg support. Describe nodes, relationships, and properties over your existing tables, then ask graph questions in Cypher, Gremlin, or SPARQL.

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

The language frontends produce a shared Graph IR. Compiler-only clients lower supported reads to SQL and return the plan to your application. The optional managed runtime can also execute remaining stages itself. Read [SQL compilation](sql-compiler.md) for the client boundary and [core concepts](concepts.md) for the runtime model.

## Follow a learning path

Begin with [installation](installation.md), run the [quickstart](quickstart.md), then choose a [client example](client-apis.md). The separate [mapped graph tutorial](mapped-graphs.md) covers core runtime APIs and a customer/order dataset used in the [query recipes](recipes.md).

For application integration, continue with [typed parameters](parameters.md), [Arrow results](results.md), and [transactions](transactions.md). The [CLI reference](cli.md), [mapping reference](mapping-reference.md), and [Rust API](rust-api.md) provide the details to keep nearby while coding.

## Project links

[Website](https://orchiddb.com/) · [GitHub](https://github.com/OrchidDB/OrchidDB) · [Community](https://orchiddb.com/community.html)
