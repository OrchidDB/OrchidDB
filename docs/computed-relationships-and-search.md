# Computed relationships and portable search

OrchidDB represents computed relationships as endpoint labels, scalar expressions,
predicates, and ranking behavior. It constructs the relational plan itself.
There is no relationship query parameter, embedded SELECT, or semantic builder DSL.

The implementation adds a ranked-search boundary to the existing relational
plan, alongside joins, projections, filters, aggregates, and windows. Both Cypher and Gremlin consume these relationships through ordinary
edge traversal. Function definitions carry native execution and per-engine SQL
mappings, so scoring can execute inside SQL islands or in the native engine.

## Declare a relationship

Start with ordinary node mappings:

```toml
[node.Document]
table = "documents"
id = "document_id"

[node.Document.properties]
id = "document_id"
tenant_id = "tenant_id"
title = "title"
body = "body"
embedding = "embedding"
tokens = "token_embeddings"

[edge.SIMILAR_TO]
source = "Document"
target = "Document"
order_by = [{ expression = "score", direction = "desc", nulls = "last" }]
limit_per_source = 10
retrieval = "approximate_allowed"

[edge.SIMILAR_TO.properties]
score = "vector.cosine_similarity(source.embedding, target.embedding)"
```

`predicate` is optional. Omit it to allow all endpoint pairs, including a node
paired with itself. To restrict eligibility before ranking, add a condition under
`[edge.SIMILAR_TO]`, for example:

```toml
predicate = "target.tenant_id = source.tenant_id AND target.id <> source.id"
```

This condition expresses application behavior; OrchidDB constructs the retrieval
query from the score, ordering, limit, and any eligibility filter.

Expressions reference **mapped graph properties**, not physical table columns.
They support ordinary scalar expression syntax: arithmetic, comparisons, Boolean
logic, CASE, casts, and registered functions. Property expressions can reference
other computed properties; dependencies are resolved independently of declaration
order. Unknown references and cycles fail planning. User-defined aggregates,
windows, subqueries, extra statements, and non-immutable functions are rejected.

`source` and `target` name mapped node labels. Their existing keys determine the
endpoints, including composite keys. The concatenated tuple of source and target
key components determines edge identity within the relationship type. Rows with
null key components do not produce edges. Computed edges are read-only.

`limit_per_source` requires `order_by`. Zero produces no edges; omission keeps
all qualifying pairs. Direction defaults to `asc`, null placement to `last`.
The default retrieval mode is `approximate_allowed`: use the backend's search
access path. `exact` explicitly requests exhaustive vector ranking. Destination
keys break ties in exact ranking and when ranking retrieved candidates; an ANN
index may choose different tied candidates at its cutoff. Mapped keys must
uniquely identify nodes, as with ordinary mappings.

Ranking creates a relationship, rather than a query-specific neighbor search:
reverse traversal reads those same directed edges. A consuming query that filters
a destination does not refill its source's top-k with different neighbors.

## Query it

```cypher
MATCH (seed:Document {id: $id})-[similarity:SIMILAR_TO]->(related)
MATCH (related)-[:WRITTEN_BY]->(author)
RETURN related.title, author.name, similarity.score
ORDER BY similarity.score DESC
```

```groovy
g.V().has('Document', 'id', id).
  outE('SIMILAR_TO').as('similarity').
  inV().as('document').
  out('WRITTEN_BY').as('author').
  select('document', 'author', 'similarity')
```

Reverse traversal is ordinary syntax too:

```cypher
MATCH (target:Document {id: $id})<-[r:SIMILAR_TO]-(source)
RETURN source.id, r.score
```

The rule composes with stored edges, other computed edges, predicates, and graph
traversals. There is no `edge.SIMILAR_TO(query)` call for users to implement.

## Built-in logical functions

| Function | Meaning | Preferred order |
| --- | --- | --- |
| `vector.cosine_similarity(a, b)` | Normalized inner product | Descending |
| `vector.dot(a, b)` | Inner product | Descending |
| `vector.l2_distance(a, b)` | Euclidean distance | Ascending |
| `vector.maxsim(query_tokens, document_tokens)` | Sum of maximum token inner products | Descending |
| `text.bm25(query, target.body)` | BM25 using the mapped target corpus | Descending |

Vector inputs are float32/float64 lists, large lists, or fixed-size lists. MaxSim
uses lists of token vectors. Embedding generation and token-vector normalization
remain the caller's responsibility; no model is loaded. ColBERT cosine MaxSim
requires caller-normalized token vectors.

Outer null inputs produce null scores. Zero-norm cosine inputs produce null.
Empty vectors/matrices, null components/token vectors, dimension mismatches, and
non-finite components are invalid. PostgreSQL pgvector uses float32 arithmetic;
its numeric precision and supported dimensions still apply. Near-tied floating
point scores may differ from float64 native/DuckDB scoring. Tie breakers resolve
equal backend scores, not cross-backend rounding differences.

### BM25 corpus and analyzer

Within a relationship, the two-argument form requires the document argument to
be a `target` property. With a declared Lance BM25 index, retrieval and scoring
use `lance_fts`; the index owns corpus statistics and its analyzer. OrchidDB does
not collect that corpus. Without an indexed text binding, native/portable scoring
collects the property's values from the mapped target relation **before pair
predicates, candidate selection, and ranking**.
To use a restricted corpus, define a correspondingly restricted target node
mapping. Null-key rows are excluded with the other invalid endpoints.

The reference profile uses ASCII alphanumeric tokenization, ASCII lowercase,
deduplicated query terms, `k1 = 1.2`, `b = 0.75`, and:

```text
idf(term) = ln(1 + (N - df(term) + 0.5) / (df(term) + 0.5))
score = sum(idf(term) * tf(term) * 2.2 /
            (tf(term) + 1.2 * (0.25 + 0.75 * document_length / average_length)))
```

Null/empty documents count in corpus size and contribute zero length. Empty
queries, empty corpora, and corpora with zero average length score zero. Null
query/document inputs score null. The scalar function also exposes a three-
argument form, `text.bm25(query, document, corpus_texts)`, for explicit corpus use.

PostgreSQL and ordinary DuckDB SQL implement this reference formula. An explicit
Lance BM25 binding selects Lance's index-defined tokenizer and BM25 scoring;
its numbers and match set need not equal the reference profile. In particular,
Lance returns matching documents rather than filling top-k with zero-score
nonmatches. PostgreSQL `ts_rank` is not BM25 and is never substituted. pgvector
provides vector indexes, not BM25 indexes; PostgreSQL's portable BM25 expression
is not advertised as an indexed text implementation.

## Candidate retrieval followed by ColBERT scoring

This example assumes a mapped `Question` node with an ID and `text`, `tenant_id`,
and `tokens` properties, alongside the `Document` mapping above. `tokens` contains
the question's token vectors.

```toml
[edge.RELEVANT_TO]
source = "Question"
target = "Document"
order_by = [{ expression = "score", direction = "desc" }]
limit_per_source = 10

[edge.RELEVANT_TO.candidates]
predicate = "source.tenant_id = target.tenant_id"
order_by = [{ expression = "lexical", direction = "desc" }]
limit_per_source = 200

[edge.RELEVANT_TO.candidates.properties]
lexical = "text.bm25(source.text, target.body)"

[edge.RELEVANT_TO.properties]
score = "vector.maxsim(source.tokens, target.tokens)"
```

The candidate stage computes its properties, applies its predicate, and selects
its per-source candidates. The final stage then computes its properties, applies
its predicate, and selects its final top-k. Candidate properties remain available
to the final stage and as edge properties. Their names must be distinct from
final property names. Put eligibility filters in the candidate predicate if they
must apply before candidate selection.

This means BM25-top-200 followed by MaxSim-top-10. Removing the candidate stage
means exhaustive MaxSim ranking. These are intentionally distinct relationships.

## Rust and compiler APIs

`GraphMapping::from_toml` and `to_toml` round-trip these declarations. Providers
remain separate: register Arrow providers for native execution, or table schemas
for compilation against caller-owned databases.

Rust callers can construct `ComputedRelationship` and call:

```rust
mapping.map_computed_relationship(rule)?;
```

Register endpoint node mappings first. Duplicate relationship names are rejected.
Custom portable functions are registered with:

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

The native UDF owns type checking and execution. SQL templates are parsed as
expressions with AST argument substitution, not string substitution of values.
Templates can use `__local0` through `__local7` for collision-free local aliases.
An optional `SqlOrderingMapping { expression, reverse }` supplies an equivalent
sort key. Reversal changes direction while preserving explicit null placement.
Arithmetic around a score does not acquire an assumed monotonic mapping.
`LogicalFunction::with_search_metric(SearchMetric::...)` can additionally declare
that a custom function follows a built-in metric's indexed-access contract:
argument zero is the query, argument one is the document, and ranking follows
that metric. This metadata participates in planning; recognition does not depend
on hard-coded function names. Arbitrary weighted expressions remain scalar
expressions; put an indexed candidate stage before them when appropriate.

Definitions travel on the logical UDF; they are not installed in a process-global
mutable registry. Omitting a dialect implementation creates a native execution
boundary. Registration rejects volatile functions and name collisions.

The JSON compiler request adds a `computed_relationships` array:

```json
{
  "computed_relationships": [{
    "name": "SIMILAR_TO",
    "source": "Document",
    "target": "Document",
    "predicate": "source.id <> target.id",
    "properties": {
      "score": "vector.cosine_similarity(source.embedding, target.embedding)"
    },
    "order_by": [{"expression": "score", "direction": "desc", "nulls": "last"}],
    "limit_per_source": 10,
    "retrieval": "approximate_allowed"
  }]
}
```

This is an addition to the existing request's tables, nodes, physical edges,
query, and engine ownership. Schema types use the existing notation, such as
`list:float32` and `list:list:float32`.

## Lowered SQL and SQL islands

With the optional tenant/self-exclusion filter above, a descending cosine top-k
becomes an index-usable pgvector query of this shape (aliases are shortened):

```sql
SELECT source.document_id AS source_id, hits.*
FROM documents AS source
CROSS JOIN LATERAL (
  SELECT target.document_id AS target_id,
         1.0 - (target.embedding <=> source.embedding) AS score
  FROM documents AS target
  WHERE target.tenant_id = source.tenant_id AND target.document_id <> source.document_id
  ORDER BY target.embedding <=> source.embedding ASC NULLS LAST
  LIMIT 10
) AS hits
```

The value expression includes a zero-norm guard in generated SQL. The access
ordering stays a bare ascending distance operator so PostgreSQL can use HNSW
or IVFFlat. Dot product uses `<#>` for access ordering and negates it for score;
L2 uses `<->`. Native `vector` columns with matching operator-class indexes are
required for index use; casting ordinary array columns does not create an index.
For `exact`, the access ordering deliberately prevents ANN-index selection.

The surrounding graph joins and final property projections remain part of the
same PostgreSQL island. Native execution uses per-source ranking over pairs.
Final reranking uses a partitioned window and respects composite endpoint keys.
Source filters move before per-source top-k. The planner also restricts source
keys from the graph join with a semijoin, so a query for one seed issues one
bound search. Target/score filters in the consuming graph query do not change
which neighbors the relationship selects. Reverse traversal still reads the
forward relationship's selected edges.

DuckDB cosine uses:

```sql
CASE WHEN list_inner_product(source.embedding, source.embedding) = 0
       OR list_inner_product(target.embedding, target.embedding) = 0
     THEN NULL
     ELSE CAST(list_cosine_similarity(source.embedding, target.embedding) AS DOUBLE)
END
```

DuckDB dot/L2 use `list_inner_product`/`list_distance`. MaxSim expands token
matrices, takes the maximum token inner product for each query token, and sums
those maxima. PostgreSQL's existing nested-list adapter represents token vectors
as JSONB children in a one-dimensional array before casting them to pgvector.

Backend selection occurs over the composed plan. A closed same-engine subtree
can include scoring, eligibility, ranking, traversal joins, and projection in one
SQL island. Mixed ownership keeps the existing exchange boundaries. Unsupported
logical functions execute natively; execution failures are propagated rather
than retried in another engine.

## Lance through DuckDB

OrchidDB has **no Lance SDK dependency or Lance storage adapter**. Load a
compatible `duckdb-lance` extension in the caller's DuckDB connection. Expose
nodes using an attached Lance table or an ordinary view:

```sql
LOAD lance;
CREATE VIEW documents AS SELECT * FROM '/data/documents.lance';
```

Bind physical search capabilities separately from relationship expressions.
These top-level TOML entries can precede the node/edge sections:

```toml
[[source_metadata]]
table = "documents"
format = "lance"
options = { uri = "/data/documents.lance" }
indexes = [
  { column = "embedding", metric = "cosine", options = { nprobes = 8, refine_factor = 2 } },
  { column = "body", metric = "bm25" },
]
```

The URI/table must refer to the same dataset, and the declared vector metric
must match its existing index. Supported metrics are `cosine`, `dot`, `l2`, and
`bm25`. For PostgreSQL, `format = "pgvector"` explicitly requires
PostgreSQL placement; ordinary ranked vector expressions also map to pgvector
when their SQL island belongs to PostgreSQL.

Lance binds the search input when preparing the statement. OrchidDB therefore
plans a dependent SQL island: execute the source island, bind each source row as
typed SQL AST literals, execute search on the owning DuckDB session, and return
hits to the surrounding graph plan. It never emits a lateral Lance call with
unbound source columns. Representative generated search statements are:

```sql
SELECT target.document_id, list_cosine_similarity([0.1, 0.9], target.embedding) AS score
FROM lance_vector_search('/data/documents.lance', 'embedding', [0.1, 0.9],
                         k = 10, use_index = true, nprobs = 8,
                         refine_factor = 2, prefilter = true) AS target
WHERE target.tenant_id = 42 AND target.document_id <> 7;

SELECT target.document_id, target._score AS score
FROM lance_fts('/data/documents.lance', 'body', 'graph retrieval',
               k = 200, prefilter = true) AS target
WHERE target.tenant_id = 42;
```

Retrieved vectors are scored with the mapped logical function. Lance text search
uses `_score`. ColBERT MaxSim and other final expressions run over these hits.
Candidate predicates must be pushable target-column comparisons, null checks,
and conjunctions after source values are bound. Unsupported predicates fail
planning; put post-retrieval conditions in the final stage explicitly.

`exact` maps to `use_index = false` for Lance L2. The current duckdb-lance search
API does not expose a metric parameter for non-indexed cosine/dot search, so
those combinations fail planning rather than changing the metric or scanning
through a different adapter. Install/create indexes using backend tooling;
OrchidDB performs no automatic extension installation or index DDL.

## Federation and JSON execution

The compiler request accepts `source_metadata` alongside `computed_relationships`.
Legacy `search_indexes` inputs are normalized into this metadata at registration.
For dependent search, supply the existing `engines`, table ownership, and
`execution_engine` fields. This also covers cross-engine pgvector search: source
rows are bound into top-k PostgreSQL statements on the vector table's owner.
Same-engine PostgreSQL search stays in one correlated SQL island. A compiled transfer can carry:

```json
{
  "source_engine": "local",
  "source_dialect": "duckdb",
  "sql": "SELECT ...source rows...",
  "target_relation": "__orchiddb_search_...",
  "columns": ["...output column descriptors..."],
  "operation": {
    "engine": "lance",
    "input_columns": ["...source column descriptors..."],
    "template": {"sql": "SELECT ... FROM lance_vector_search(..., $3, ...)", "parameters": 4, "dialect": "duckdb"}
  }
}
```

`federation::execute` runs dependent transfers automatically using caller-owned
sessions. Clients using the JSON protocol execute a transfer's source SQL,
then call `{"op":"bind_operation","plan":...,"relation":...,"rows":[...]}`.
Rows follow `operation.input_columns` order. Execute the returned SQL statements on
the returned engine, concatenate their hit rows in transfer-column order, and
complete the ordinary `bind` operation. Bind updates both final SQL and pending
transfer dependencies. Source data and search hits cross existing SQL-island
boundaries; target datasets are not copied into a private Lance adapter.

## Engine integration and general relational lowering

Source metadata records physical format, source options, and index capabilities.
It does not choose an execution backend. The owner engine's registered
`DialectAdapter` (also exported as `EngineAdapter`) lowers the typed relational
operation using Rust code. Built-in PostgreSQL and DuckDB rules implement the
same interface. There is no separate search-backend trait or closed backend enum.

The `lower_relation` hook returns a SQL AST, an equivalent logical-plan rewrite,
or a dependent operation with source inputs, a typed SQL template, and an output
schema. `TableFunction` is a general row-producing logical node: argument types
are checked against its source schema; `Correlated` inputs can remain in a SQL
island and `PrepareTime` inputs require binding first. Predicates remain above a
table-function boundary unless its lowering explicitly implements a valid pushdown.
Schema-changing rewrites and mismatched dependent parameter counts are rejected.

Search retains its own logical ranking semantics: eligibility before retrieval,
per-source top-k, scoring direction, and exact/approximate requirements. The
engine rules implement these with index-usable PostgreSQL ordering or Lance
search calls. Indexed BM25 has an explicit index-owned corpus; it does not
construct a dummy corpus or silently invoke native scoring.

See the [engine adapter guide](../website/docs/content/engine-adapters.md) and
`tests/engine_adapter.rs` for registration, AST transformations, function
mappings, table-function lowering, and typed literal codecs. Custom formats and
options are retained for the owning adapter to validate. Unsupported engine
operations fail explicitly.

The JSON execution protocol now emits `Transfer.operation` and accepts
`op: "bind_operation"`. The former `search` descriptor and `bind_search` command
remain accepted on input. These names also apply to non-search operations.
Typed expressions, row-producing operations, and codec hooks are extension
points for future JSON lowering; this change does not implement full JSON
relational semantics.

## Verification and limits

`tests/computed_relationships.rs` covers native scoring, Cypher/Gremlin traversal,
reverse edges, composite keys, candidate/MaxSim composition, TOML/JSON metadata,
backend SQL, PostgreSQL HNSW plan verification, and real Lance vector/BM25
retrieval through the compile/bind/final-join protocol.

```sh
GRAPH_PG_URL='host=/tmp dbname=postgres user=YOUR_USER' \
ORCHIDDB_LANCE_DUCKDB_CLI=/path/to/compatible/duckdb \
ORCHIDDB_LANCE_EXTENSION=/path/to/lance.duckdb_extension \
cargo test --features duckdb,postgres --test computed_relationships
```

External integration tests require those environment variables. The published
Lance binary tested here targets DuckDB 1.5.0; this repository embeds DuckDB 1.5.2.
For embedded Lance execution, supply an extension built for the embedded version.
Indexed queries require valid backend query inputs. Use source predicates such
as `source.embedding IS NOT NULL` to exclude missing embeddings before binding
search. Scalar null behavior does not fabricate a query vector for an index.
The engine propagates missing-extension, binder, and execution errors. It does
not retry failed search by scanning the dataset or changing engines.

Native BM25 and exhaustive MaxSim still need corpus/pair work when explicitly
used without an indexed candidate stage. MaxSim scores supplied token embeddings;
it does not load a ColBERT model. Approximate retrieval preserves the backend's
candidate/recall behavior and cannot guarantee the same neighbors as exact mode.

The computed-edge interface is currently Cypher/Gremlin. RDF ontology generation
requires a separately mapped relational RDF view for computed relationships.

Backend API references: [duckdb-lance SQL](https://github.com/lance-format/lance-duckdb/blob/main/docs/sql.md),
[pgvector index ordering](https://github.com/pgvector/pgvector#querying), and
[Lance text indexes](https://lance.org/quickstart/full-text-search/).
