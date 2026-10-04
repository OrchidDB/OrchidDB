# Search

```cypher
MATCH (seed:Document {id: $id})-[similarity:SIMILAR_TO]->(related)
MATCH (related)-[:WRITTEN_BY]->(author)
RETURN related.title, author.name, similarity.score
ORDER BY similarity.score DESC
```

Find documents similar to a seed, then follow their authors through the graph.
`SIMILAR_TO` is a computed relationship: its edges come from a search rule.
`WRITTEN_BY` is an ordinary stored relationship. Both compose in the same query.

## Define what makes an edge

Once `Document` is [mapped to your data](mapping-reference.md), define similarity
using its graph properties:

```toml
[edge.SIMILAR_TO]
source = "Document"
target = "Document"
predicate = "target.id <> source.id"
order_by = [{ expression = "score", direction = "desc" }]
limit_per_source = 10

[edge.SIMILAR_TO.properties]
score = "vector.cosine_similarity(source.embedding, target.embedding)"
```

This gives each document up to ten outgoing edges to its most similar neighbors,
with a `score` property on each edge. The definition captures the score,
eligibility, and ranking; OrchidDB composes the query that retrieves them.
You can add conditions such as matching tenants or combine scoring expressions.
The same relationships are available through Gremlin.

The ranking belongs to the relationship. Filtering results later in Cypher does
not refill its top ten, and reverse traversal follows those same directed edges.

## Execute where the data lives

OrchidDB translates the relationship into search and traversal operations, then
places them into SQL islands on the engines that own the data. Logical functions
carry both a native implementation and backend mappings, including sort order.
For example, descending cosine similarity becomes ascending cosine distance for
pgvector's index access.

| Backend | Search execution |
| --- | --- |
| PostgreSQL with pgvector | Vector top-k uses distance operators and index-usable ordering inside a correlated SQL island. |
| DuckDB with duckdb-lance | Declared Lance indexes use `lance_vector_search` or `lance_fts`, with source values bound into search statements. |
| Native engine | Scoring expressions execute directly when placed outside SQL islands. |

Index bindings describe the physical table, column, metric, and backend separately
from the relationship. The application supplies the indexes and extensions.
Cross-engine plans transfer source values and search hits between islands;
backend search errors propagate to the caller.

## Full-text search and reranking

The same relationship model supports `text.bm25(source.text, target.body)` for
full-text retrieval and `vector.maxsim(source.tokens, target.tokens)` for
ColBERT-style scoring. A candidate stage can retrieve the best 200 matches using
an indexed vector or BM25 search, then MaxSim can select the best ten from those
candidates. Your application supplies the embeddings.

Indexed BM25 uses Lance's text index. pgvector supplies vector indexes;
PostgreSQL's portable BM25 scoring is a separate, non-indexed expression.
Lance execution requires an extension matching the application's DuckDB version.

See [SQL generation and execution](sql-compiler.md) for the compiler interface.
The [detailed search reference](https://github.com/OrchidDB/OrchidDB/blob/main/docs/computed-relationships-and-search.md)
covers index bindings, candidate declarations, generated SQL, custom functions,
and backend compatibility.
