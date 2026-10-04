# Quickwit and Elasticsearch

```cypher
MATCH (question:Question {id: $id})-[match:SIMILAR_TO]->(document:Document)
MATCH (document)-[:WRITTEN_BY]->(author:Person)
RETURN document.title, author.name, match.score
ORDER BY match.score DESC
```

Use Quickwit or Elasticsearch to retrieve documents, then follow ordinary graph
relationships stored in SQL. Both are optional engines. The search relationship
describes scoring and ranking; OrchidDB constructs the remote requests and joins
their results into the remaining SQL islands.

Enable the engine you use:

```toml
[dependencies]
orchiddb = { path = "../orchiddb", features = ["quickwit", "elasticsearch"] }
```

The features are independent and disabled by default. Applications that execute
DuckDB locally also enable `duckdb`; applications supplying their own SQL
sessions do not need it.

## Define the relationship

Map `Question` to your query-input table and `Document` to your document table or
index. This example assumes both expose `body` and `tenant` properties, and that
`WRITTEN_BY` is already mapped as a stored relationship.

```toml
[edge.SIMILAR_TO]
source = "Question"
target = "Document"
predicate = "source.tenant = target.tenant"
order_by = [{ expression = "score", direction = "desc" }]
limit_per_source = 10

[edge.SIMILAR_TO.properties]
score = "text.bm25(source.body, target.body)"
```

This selects up to ten matching documents from the same tenant for each question.
The tenant condition applies before retrieval. A `WHERE` clause on the consuming
Cypher query filters the selected edges without refilling their top ten.

Quickwit and Elasticsearch supply their own indexed corpus statistics and text
analysis. Scores are backend scores; their numeric values need not match across
engines. Indexed retrieval returns matching documents, not unrelated documents
with zero scores. A null query produces no matches.

Gremlin traverses the same relationship:

```gremlin
g.V().has('Question', 'id', 7).
  outE('SIMILAR_TO').as('match').
  inV().as('document').select('document', 'match')
```

## Register an index as an engine-owned table

In the [compiler request](sql-compiler.md), register the engine and assign the
index table to it. This excerpt uses Quickwit; replace `quickwit` with
`elasticsearch` to select Elasticsearch.

```json
{
  "engines": {
    "local": {"dialect": "duckdb"},
    "text": {"dialect": "quickwit"}
  },
  "execution_engine": "local",
  "tables": [{
    "name": "documents",
    "engine": "text",
    "columns": [
      {"name": "id", "data_type": "int64", "nullable": false},
      {"name": "body", "data_type": "string"},
      {"name": "tenant", "data_type": "string"},
      {"name": "title", "data_type": "string"}
    ]
  }],
  "source_metadata": [{
    "table": "documents",
    "format": "quickwit",
    "options": {"index": "documents"},
    "indexes": [{"column": "body", "metric": "bm25"}]
  }]
}
```

The complete request also includes the query, node and edge mappings, and the
SQL tables used for questions and authors. Table names for engine-owned indexes
identify their actual remote indexes. Stored fields must match the declared
column types, and graph IDs must be unique and non-null.

Quickwit needs a stored key, a text field with term frequencies and
`fieldnorms: true`, and appropriate indexed/fast fields for filtering and
ordering. Use a `raw` tokenizer for exact string comparisons such as tenant IDs.
See [Quickwit index configuration](https://quickwit.io/docs/configuration/index-config).
For Elasticsearch, use `text` for BM25 content and `keyword` for exact string
filters. Keep the required fields available in `_source`.

## Use a remote index over SQL-owned documents

Keep the `documents` table assigned to your SQL engine and name the index engine
in its source metadata:

```toml
[[source_metadata]]
table = "documents"
format = "quickwit"
options = { engine = "text", index = "document_index", key_field = "document_id", text_field = "body" }
indexes = [{ column = "body", metric = "bm25" }]
```

Use `format = "elasticsearch"` with an Elasticsearch engine. The remote key
must correspond to the mapped document ID. OrchidDB retrieves keys and scores,
then joins the selected keys to the authoritative SQL table. Titles and other
properties therefore come from SQL, even if older copies exist in the index.
Fields used for eligibility must be indexed consistently with their SQL values.
Maintaining index freshness and unique document keys is the application's
responsibility. Missing SQL documents do not refill the ranked candidate set.

## Connect and execute

Connections and credentials belong to the application, not the compiled plan.
The HTTP sessions implement `federation::Session` alongside your SQL session:

```rust,ignore
use orchiddb::remote::transport::{
    Authentication, HttpOptions, QuickwitSession,
};

let mut options = HttpOptions::new("https://quickwit.example.com");
options.authentication = Authentication::Bearer(token);
let text = QuickwitSession::with_options(options)?;

sessions.insert("text".into(), Box::new(text));
// Register your SQL session under "local" as well.
let batches = orchiddb::federation::execute(&compiled, &mut sessions).await?;
```

For Elasticsearch, use `ElasticsearchSession` and the same options. Basic,
bearer-token, and API-key authentication are supported. A caller-supplied
`reqwest::Client` can provide custom certificate roots. Configure request
timeouts, page size, and result limits through `HttpOptions`.

Ordinary remote scans use paginated snapshot reads. Configured limits reject
oversized results rather than silently truncating them. HTTP errors, malformed
responses, and partial search failures propagate to the caller.

## Language clients

Python, JavaScript, JVM, C++, and Elixir provide `RemoteEngine` adapters for these
services. Register one alongside your application-owned SQL adapter and use the
client's federated-query entry point. Keep the remote engine open across queries
and close it when the application no longer needs it. These clients use the same
validated HTTP transport as Rust, including pagination, typed results, and error
handling. Each client README includes connection and query examples.

The Rust client exposes the adapters through its independent `quickwit` and
`elasticsearch` features. The CLI accepts a separate engine options file:

```json
{
  "text": {"endpoint": "http://localhost:7280", "page_size": 1000}
}
```

```sh
orchiddb query request.json --engines engines.json --init setup.sql --no-iceberg
```

The request's `engines` registry selects Quickwit or Elasticsearch; the options
file supplies connection details for that engine ID. Credentials belong in this
application configuration, never in the compiled query.

## Compose retrieval and reranking

Use the [candidate-stage relationship syntax](search.md#retrieve-candidates-then-rerank-with-maxsim)
to retrieve candidates from either engine and rerank their token embeddings with
MaxSim. The candidate stage keeps the indexed BM25 score, while the final stage
selects the highest reranked scores. Token embeddings must be available from the
mapped document source; OrchidDB does not generate embeddings.

## API choice and supported operations

Portable BM25 relationships use each engine's structured search endpoint.
Quickwit's Elasticsearch-compatible endpoint exposes the score metadata and
non-scoring Boolean filters required by these relationships. Elasticsearch is
not a dependency of Quickwit.

Quickwit's native search API remains available for explicitly requested native
queries. Its hit response does not expose the same score metadata; native text
queries and scored relationship retrieval therefore have distinct interfaces.

The adapters push supported projections, comparisons, Boolean filters, ordering,
and limits into remote requests. SQL null semantics are preserved, including
missing fields under negation. Expressions that cannot be pushed stay in a valid
SQL island where possible. Unsupported eligibility expressions inside indexed
top-k retrieval fail explicitly.

Advanced native queries can return a typed JSON response through the request
operation interface and then compose with [JSON functions](json.md). This makes
backend query DSL features and aggregations available without claiming that
every backend operation has an automatic Cypher-to-query-DSL translation.

The Rust relational API exposes `quickwit.search` and `elasticsearch.search`
as typed `TableFunction` operations, and `quickwit.query` and
`elasticsearch.query` as complete JSON responses. Quickwit additionally provides
`native_search` and `native_query` for its native API. These names are relational
API operations, not new Cypher scalar functions. Native search requires an
explicit finite `max_hits`; use the structured search operation for paginated
enumeration. Automatic aggregate pushdown and automatic vector relationship
lowering are not currently provided by these adapters.

For an `exact` Quickwit relationship, a single target key supplies the secondary
tie-break order: Quickwit permits at most two sort criteria, including the score.
Index aliases or mappings changed during a session require
`clear_metadata_cache()` before their new schema is used.

## Verify against running services

The repository includes pinned local services and live tests that create their
own indexes:

```sh
docker compose -f tests/remote-engines.compose.yml up -d
QUICKWIT_URL=http://127.0.0.1:17280 \
ELASTICSEARCH_URL=http://127.0.0.1:19200 \
cargo test --features duckdb,quickwit,elasticsearch \
  --test remote_engines -- --ignored
```

The tests require reachable services; they do not silently pass when one is
absent. The compose configuration binds unauthenticated test services to
localhost and is intended only for local testing.
