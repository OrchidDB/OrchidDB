# Introduction

Bring embedded graph queries to your data lake. Learn to map relational tables, query with three graph languages, and use OrchidDB from Rust, Java, Python, JavaScript/TypeScript, Elixir, or C++.

## Start with your data

OrchidDB is a graph query layer built in Rust, with DataFusion planning and Apache Arrow results. Compile SQL and execute through your own engine, or use the standalone CLI with bundled DuckDB and default Iceberg support. Describe nodes, relationships, and properties over your existing tables, then ask graph questions in Cypher, Gremlin, or SPARQL.

A graph mapping connects your application vocabulary to physical columns. A customer can become a `Person`, an order row can define an `ORDERED` relationship, and a SQL view can expose a selected group of accounts. Your database remains the source of those rows.

## Choose a starting point

Version 0.1.0 is available as a [CLI installer and published language packages](installation.md#published-packages). On macOS ARM64, install the CLI and run the [quickstart](quickstart.md) without a Rust toolchain or source checkout:

```sh
curl -fsSL https://install.orchiddb.com | bash
export PATH="$HOME/.local/bin:$PATH"
orchiddb --version
```

| Your goal | Start here |
| --- | --- |
| Run your first graph query | [Quickstart](quickstart.md) |
| Query tables you already own | [Map existing tables](mapped-graphs.md) |
| Query documents and turn JSON arrays into edges | [JSON documents](json.md) |
| Combine vector or full-text search with graph traversal | [Search](search.md) |
| Search remote indexes and join results into the graph | [Quickwit and Elasticsearch](remote-engines.md) |
| Create and persist a graph | [Managed graphs](managed-graphs.md) |
| Query with an RDF vocabulary | [SPARQL](sparql.md) |
| Use scalar functions across DuckDB and PostgreSQL | [Portable scalar functions](portable-functions.md) and [function catalog](portable-function-catalog.md) |
| Map application columns or quad tables to RDF | [RDF datasets](rdf.md) |
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

SPARQL uses RDF rules over application columns or an ontology mapping over graph labels:

```sparql
PREFIX ex: <https://example.com/>
SELECT ?name WHERE {
  ?person a ex:Person ; ex:name "alice" ; ex:follows ?friend .
  ?friend ex:name ?name .
}
ORDER BY ?name
```

The language frontends produce a shared Graph IR and relational plan. Execution uses SQL regions and native operators as supported by the selected API. Read [SQL compilation](sql-compiler.md) to inspect or execute generated SQL, and [core concepts](concepts.md) for the shared execution model.

## Follow a learning path

Begin with [installation](installation.md), run the [quickstart](quickstart.md), then choose a [client example](client-apis.md). The [mapped graph tutorial](mapped-graphs.md) demonstrates `GraphEngine` using a customer/order dataset.

For application integration, continue with [typed parameters](parameters.md), [Arrow results](results.md), and [transactions](transactions.md). The [CLI reference](cli.md), [mapping reference](mapping-reference.md), and [Rust API](rust-api.md) provide the details to keep nearby while coding.

## Project links

[Website](https://orchiddb.com/) · [GitHub](https://github.com/OrchidDB/OrchidDB) · [Community](https://orchiddb.com/community.html)
