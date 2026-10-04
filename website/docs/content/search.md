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

You define the relationship once, then bind its search to pgvector, duckdb-lance,
or native scoring. The examples below assume documents have IDs, titles, body
text, and embeddings; map `WRITTEN_BY` separately as an ordinary
[stored relationship](mapping-reference.md#relationship-mappings).

## Define the relationship

Map your columns to graph properties and describe how to select neighbors:

```toml
[node.Document]
table = "documents"
id = "id"

[node.Document.properties]
id = "id"
title = "title"
body = "body"
embedding = "embedding"

[edge.SIMILAR_TO]
source = "Document"
target = "Document"
predicate = "target.id <> source.id"
order_by = [{ expression = "score", direction = "desc", nulls = "last" }]
limit_per_source = 10
retrieval = "approximate_allowed"

[edge.SIMILAR_TO.properties]
score = "vector.cosine_similarity(source.embedding, target.embedding)"
```

Each document has up to ten outgoing edges, with a `score` property on each edge.
Expressions use mapped properties and support arithmetic, comparisons, Boolean
logic, CASE, and registered scalar functions. Add eligibility conditions to
`predicate`, such as `source.tenant_id = target.tenant_id` after mapping that
property. OrchidDB constructs the search and traversal plan from this rule.

Load the TOML with `GraphMapping::from_toml`. Register matching physical schemas
for SQL compilation, or table providers for native execution, as described in the
[mapping reference](mapping-reference.md#register-source-schemas).
All `[[search_indexes]]` blocks below belong at the top level of the same TOML
file. Choose one backend binding for each table/column/metric combination.

The relationship also works in Gremlin:

```gremlin
g.V().has('Document', 'id', 7).
  outE('SIMILAR_TO').as('similarity').
  inV().as('document').select('document', 'similarity')
```

Ranking belongs to the relationship: filtering its destinations later does not
refill its top ten, and reverse traversal follows the same directed edges.
Computed edges are read-only. `approximate_allowed` is the default; `exact`
requests exhaustive vector ranking.

## PostgreSQL with pgvector

Use a PostgreSQL `documents` table with a native `vector(N)` embedding column,
where `N` matches your embedding dimension. Enable the installed extension and
create a cosine index on your existing table:

```sql
CREATE EXTENSION IF NOT EXISTS vector;
CREATE INDEX documents_cosine ON documents
  USING hnsw (embedding vector_cosine_ops);
```

Bind the column in the mapping:

```toml
[[search_indexes]]
table = "documents"
column = "embedding"
metric = "cosine"
backend = { kind = "pgvector" }
```

Assign `documents` to a PostgreSQL engine in the
[compiler request](sql-compiler.md). The `SIMILAR_TO` definition and Cypher query
stay the same. For same-engine data, the search portion has this SQL shape
(aliases and score guards simplified):

```sql
SELECT source.id AS source_id, hits.*
FROM documents AS source
CROSS JOIN LATERAL (
  SELECT target.id AS target_id,
         1.0 - (target.embedding <=> source.embedding) AS score
  FROM documents AS target
  WHERE target.id <> source.id
  ORDER BY target.embedding <=> source.embedding ASC NULLS LAST
  LIMIT 10
) AS hits
WHERE source.id = $1;
```

Descending similarity becomes ascending distance so PostgreSQL can use the
index. The remaining graph joins can stay in the same SQL island. Dot product
uses `<#>` and L2 distance uses `<->`; choose the matching index operator class
and mapping metric. See [pgvector indexing](https://github.com/pgvector/pgvector#hnsw)
for index options. `exact` deliberately avoids ANN index selection.

## Lance through DuckDB

Use a Lance dataset at `/data/documents.lance` with the mapped columns and
fixed-size embeddings, such as `FLOAT[384]`. In the DuckDB connection that will
execute the searches, load duckdb-lance and expose the dataset as `documents`:

```sql
INSTALL lance FROM community;
LOAD lance;
CREATE VIEW documents AS SELECT * FROM '/data/documents.lance';
```

Create a cosine index if the dataset does not already have one. This small-data
example uses one partition; size the index for your dataset:

```sql
CREATE INDEX documents_cosine ON '/data/documents.lance' (embedding)
  USING IVF_FLAT WITH (num_partitions = 1, metric_type = 'cosine');
```

Use this binding instead of the pgvector binding:

```toml
[[search_indexes]]
table = "documents"
column = "embedding"
metric = "cosine"
backend = { kind = "lance", uri = "/data/documents.lance", nprobes = 1, refine_factor = 2 }
```

The table and URI must identify the same dataset. The binding metric must match
the index. `nprobes` controls IVF partitions searched; `refine_factor` controls
candidate refinement. [duckdb-lance's SQL reference](https://github.com/lance-format/lance-duckdb/blob/main/docs/sql.md)
describes these backend options and index creation.

Assign `documents` to a DuckDB engine. OrchidDB reads the selected seed, binds its
embedding into `lance_vector_search`, and joins the hits into the graph query.
For a three-dimensional example seed with ID 7, the search statement has this
shape; actual vector dimensions must match your dataset:

```sql
SELECT target.id,
       list_cosine_similarity([0.1, 0.9, 0.2], target.embedding) AS score
FROM lance_vector_search(
  '/data/documents.lance', 'embedding', [0.1, 0.9, 0.2]::FLOAT[3],
  k = 10, use_index = true, nprobs = 1, refine_factor = 2, prefilter = true
) AS target
WHERE target.id <> 7;
```

The Cypher query and relationship rule stay unchanged. OrchidDB uses the DuckDB
extension, with no Lance SDK or separate storage adapter. Load an extension
matching your DuckDB version: live integration was verified with DuckDB 1.5.0;
the embedded 1.5.2 build requires a matching extension artifact.

Candidate filters must be pushable comparisons, null checks, or conjunctions.
Lance `exact` currently supports L2; exact cosine/dot requests fail planning
because the extension's non-indexed search does not expose a metric selector.

## BM25 full-text search

Use document text as the query to define a lexical relationship over the same
nodes:

```toml
[edge.TEXT_MATCH]
source = "Document"
target = "Document"
predicate = "target.id <> source.id"
order_by = [{ expression = "score", direction = "desc" }]
limit_per_source = 10

[edge.TEXT_MATCH.properties]
score = "text.bm25(source.body, target.body)"
```

For indexed Lance retrieval, create a text index and add its binding alongside
the vector binding:

```sql
CREATE INDEX documents_text ON '/data/documents.lance' (body) USING INVERTED;
```

```toml
[[search_indexes]]
table = "documents"
column = "body"
metric = "bm25"
backend = { kind = "lance", uri = "/data/documents.lance" }
```

Query the relationship normally:

```cypher
MATCH (seed:Document {id: $id})-[r:TEXT_MATCH]->(related)
RETURN related.title, r.score
ORDER BY r.score DESC
```

For a seed whose body is `graph retrieval`, the backend search is shaped like:

```sql
SELECT target.id, target._score AS score
FROM lance_fts('/data/documents.lance', 'body', 'graph retrieval',
               k = 10, prefilter = true) AS target
WHERE target.id <> 7;
```

Lance supplies its indexed corpus statistics, tokenizer, and scores. Without a
Lance text binding, native, PostgreSQL, and ordinary DuckDB execution compute
portable BM25 over the mapped target corpus before pair filters and ranking.
That mode is exhaustive scoring. pgvector does not provide a BM25 index, and
OrchidDB does not substitute PostgreSQL `ts_rank` for BM25.

## Retrieve candidates, then rerank with MaxSim

Map a `tokens` property to a column containing each document's token vectors
(for example, add `tokens = "token_embeddings"` under
`[node.Document.properties]`). Supply embeddings from your model; normalize them
if your ColBERT scoring requires cosine token similarity.

This relationship retrieves up to 200 BM25 candidates with the text binding
above, then selects ten using MaxSim:

```toml
[edge.RELEVANT_TO]
source = "Document"
target = "Document"
order_by = [{ expression = "score", direction = "desc" }]
limit_per_source = 10

[edge.RELEVANT_TO.candidates]
predicate = "source.id <> target.id"
order_by = [{ expression = "lexical", direction = "desc" }]
limit_per_source = 200

[edge.RELEVANT_TO.candidates.properties]
lexical = "text.bm25(source.body, target.body)"

[edge.RELEVANT_TO.properties]
score = "vector.maxsim(source.tokens, target.tokens)"
```

Traverse `RELEVANT_TO` in place of `SIMILAR_TO` in the opening query. Both `score`
and `lexical` are available as edge properties. To retrieve vector candidates
instead, replace the candidate's `lexical` expression with
`vector.cosine_similarity(source.embedding, target.embedding)` and use the
pgvector or Lance vector binding. Candidate property names must be distinct
from final property names.

MaxSim sums the best document-token inner product for each query token.
Eligibility filters belong in the candidate predicate when they must apply
before retrieval. Final-stage predicates filter only retrieved candidates.
Removing the candidate stage makes MaxSim exhaustive over the target relation.

## Native scoring and function mappings

All built-in scores have native implementations and PostgreSQL/DuckDB SQL
mappings:

| Expression | Preferred order | Input |
| --- | --- | --- |
| `vector.cosine_similarity(a, b)` | Descending | Two vectors |
| `vector.dot(a, b)` | Descending | Two vectors |
| `vector.l2_distance(a, b)` | Ascending | Two vectors |
| `text.bm25(query, target.body)` | Descending | Query text and a mapped text property |
| `vector.maxsim(query_tokens, document_tokens)` | Descending | Two lists of token vectors |

For native execution, register Arrow table providers on `GraphMapping`, lower
with `RelBackend`, and call `ir::rel::execute_lowered`. Omit backend index bindings
when you want native pair scoring. Plain DuckDB SQL uses list functions such as
`list_cosine_similarity` and `list_distance`; an index binding is what selects
Lance retrieval. Vectors may be float32/float64 lists or fixed-size lists. Outer
nulls produce null scores; zero-norm cosine inputs also produce null. Indexed
search requires valid query vectors, which you can enforce with source predicates.

Custom functions carry their native UDF and SQL expressions together. For example,
register an absolute-value function before planning relationships that use it:

```rust
use std::collections::BTreeMap;
use orchiddb::ir::functions::logical::{LogicalFunction, SqlFunctionMapping};

mapping.register_logical_function(LogicalFunction::new(
    "business.absolute",
    datafusion::functions::math::abs(),
    BTreeMap::from([
        ("postgres".into(), SqlFunctionMapping {
            value: "abs(__arg0)".into(), ordering: None,
        }),
        ("duckdb".into(), SqlFunctionMapping {
            value: "abs(__arg0)".into(), ordering: None,
        }),
    ]),
))?;
```

An optional `SqlOrderingMapping { expression, reverse }` describes an equivalent
backend sort key and whether to reverse its direction. A function without a SQL
mapping creates a native execution boundary. A custom function can declare a
supported indexed metric with `with_search_metric` when it meets that metric's
query/document and ordering contract. Arbitrary combined scores do not imply an
index access path; use a candidate stage before reranking them.

## Execute across SQL islands

The [JSON compiler interface](sql-compiler.md) accepts `computed_relationships`
and `search_indexes` alongside its table schemas, nodes, query, and engine
ownership. Use `list:float32` for vector schema metadata and
`list:list:float32` for token matrices.

Same-engine PostgreSQL search can stay with surrounding graph joins in one SQL
island. Lance search and cross-engine pgvector search use dependent transfers:
execute the source SQL, bind source values, run search on the target's owner,
then join the hits. `federation::execute` performs these transfers using
caller-owned sessions. JSON clients call `bind_search` with source rows, execute
its returned statements, and pass hit rows to the ordinary `bind` operation.
Backend failures propagate; searches are not retried as scans.

The [detailed search reference](https://github.com/OrchidDB/OrchidDB/blob/main/docs/computed-relationships-and-search.md)
provides the complete JSON shapes, native BM25 formula, input constraints, and
additional backend details.
