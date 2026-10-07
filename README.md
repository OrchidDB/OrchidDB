# OrchidDB

**Cypher, Gremlin, and SPARQL over multiple execution engines.**

OrchidDB is a shared graph compiler and runtime. Language frontends lower to
Graph IR, then relational plans and graph kernels. Applications register graph schema on a connection and execute query text with
separate parameters, or load Orchid as a DuckDB extension. The extension hosts the same compiler and kernels inside
DuckDB, with access to the caller's catalog, transactions, and storage.

## Clients and CLI

All client source now lives in this repository and builds against the same core:

| Component | Source |
| --- | --- |
| Rust | [clients/rust](clients/rust) |
| Java and Gremlin | [clients/java](clients/java) |
| Python | [clients/python](clients/python) |
| JavaScript / TypeScript | [clients/js](clients/js) |
| C++ | [clients/cpp](clients/cpp) |
| Elixir | [clients/elixir](clients/elixir) |
| Shared C ABI | [clients/native](clients/native) |
| Standalone CLI | [cli](cli) |

See [client development](clients/README.md) for local builds and tests. `make cli`
builds the standalone CLI; `make native` builds and stages one shared library for
the foreign-language clients. The DuckDB extension remains a separate integration.

## Connection-based execution

Register source schemas and graph mappings once on a connection, then submit
query text and parameters separately. Schema can be loaded from JSON or YAML
into an SDK configuration object. Query text, language, parameters, and principal
are never part of that schema. The CLI accepts JSON schema files.

```python
from orchiddb import Connection, DuckDBEngine

# database is your existing DuckDB connection; schema is mapping configuration.
with Connection(DuckDBEngine(database), schema) as graph:
    with graph.query("MATCH (p:Person) WHERE p.name=$name RETURN p.name AS name",
                     parameters={"name": "Ada"}) as result:
        print(result.read_all().to_pylist())
```

```sh
make cli
./target/debug/orchiddb query 'MATCH (p:Person) RETURN p.name AS name ORDER BY name' \
  --schema cli/examples/people.json --init cli/examples/setup.sql --no-iceberg --format table
```

No public compile-only API or CLI route remains. SQL lowering, portable function
mappings, and engine routing stay internal to execution. Schema validation rejects
combined query/schema documents. Existing application connections, transactions,
and driver configuration remain caller-owned. Results are rows or Arrow batches.

The shared `fn.*` library still lowers to engine SQL or shared runtime kernels.
Backend-specific functions use declared signatures; the DuckDB extension can
discover them through its caller's catalog. DuckDB and PostgreSQL adapters share
the same language frontends and IR. StarRocks remains a planned adapter.

See [client interfaces and source builds](clients/README.md) for each language.
This API change is in source; it does not republish older registry packages.

## Optional DuckDB extension

The following guide and SQL examples use the extension interface. DuckDB owns
the connection, transactions, relational execution, and storage access in this mode.

## Build and load

The extension targets **DuckDB 1.5.6**. Requirements: Python 3.12+, curl, Rust
supporting edition 2024, and a C++17 compiler. Run from the repository root:

```sh
python3 extension/scripts/build.py
extension/vendor/cli/duckdb -unsigned graph.duckdb \
  -cmd "LOAD '$PWD/extension/build/orchid.duckdb_extension'"
```

Use `--release` for optimized Rust code. The script caches pinned DuckDB headers,
the matching official DuckDB CLI, and build dependencies under `extension/vendor`.
Rust uses the checked-in lockfile and the root `target` cache; `CARGO_TARGET_DIR`
can override that cache.

The artifact is an **unsigned development extension**, specific to its DuckDB
version and platform. Load Orchid before submitting graph statements. Loading
enables DuckDB's parser extension fallback for native `CYPHER` and `GREMLIN` syntax.
The release pipeline builds and validates macOS ARM64, Linux ARM64, and Linux
x86-64. Signed extension distribution is not configured yet.

## End-to-end examples

In the DuckDB shell opened above, run these against a fresh database:

```text
.read examples/01_social.sql
.read examples/02_managed.sql
.read examples/03_sparql.sql
.read examples/05_rag.sql
```

| Example | What it demonstrates |
| --- | --- |
| [01_social.sql](examples/01_social.sql) | Tables, one-statement graph mapping, Cypher, Gremlin, SQL composition, rollback, EXPLAIN |
| [02_managed.sql](examples/02_managed.sql) | Managed graph creation, mutations, transactions, typed results |
| [03_sparql.sql](examples/03_sparql.sql) | Existing RDF mappings and SPARQL through DuckDB |
| [04_iceberg_lance.sql](examples/04_iceberg_lance.sql) | Graph queries across existing Iceberg and Lance sources; replace the example paths first |
| [05_rag.sql](examples/05_rag.sql) | Computed RAG edge: tenant filtering, BM25 candidates, MaxSim reranking, and traversal to authors |
| [06_authorization.sql](examples/06_authorization.sql) | Optional SpiceDB channel permissions, implicit session identity, chunks, and computed edges |
| [07_rag_setup.sql](examples/07_rag_setup.sql) + [07_parameterized_rag.sql](examples/07_parameterized_rag.sql) | Ordinary Cypher hybrid search, MaxSim reranking, and traversal to source documents |

The local examples create their own data; SPARQL uses the `people` table from
the first example. The RAG example is self-contained. The storage example expects
existing external datasets.

## Define a graph over tables

```sql
CREATE TABLE people(id BIGINT PRIMARY KEY, name VARCHAR, age INTEGER);
CREATE TABLE follows(id BIGINT PRIMARY KEY, src BIGINT, dst BIGINT, since INTEGER);
INSERT INTO people VALUES (1, 'Alice', 30), (2, 'Bob', 40), (3, 'Cara', 35);
INSERT INTO follows VALUES (10, 1, 2, 2020), (11, 2, 3, 2022);

CREATE PROPERTY GRAPH social
VERTEX TABLES (
    people KEY (id) LABEL Person PROPERTIES (name, age)
)
EDGE TABLES (
    follows KEY (id)
        SOURCE KEY (src) REFERENCES people (id)
        DESTINATION KEY (dst) REFERENCES people (id)
        LABEL FOLLOWS PROPERTIES (since)
);
```

DuckDB discovers column names and types by binding the sources. You explicitly
declare keys, labels, properties, and relationship endpoints. Orchid validates the
mapping; it does not infer relationships from column names or copy source rows.

- Every mapped vertex and edge needs a `KEY`. Composite keys are ordered; endpoint
  columns must match the referenced vertex's complete key and types.
- Keys identify elements but do not create constraints or certify uniqueness.
  Separate edge keys preserve parallel relationships.
- `AS` gives a source an alias for endpoint references. Each label maps to one source.
- `PROPERTIES (column AS property, ...)` selects and renames properties. Omitting
  it, or using `PROPERTIES ALL COLUMNS`, exposes current columns. `NO PROPERTIES`
  exposes none. Cast unsupported source types in a DuckDB view.
- Graph names support `catalog.schema.graph`. Unqualified sources resolve relative
  to the graph's catalog and schema; qualify attached sources explicitly.

Mapped definitions support `CREATE OR REPLACE PROPERTY GRAPH`,
`CREATE PROPERTY GRAPH IF NOT EXISTS`, `DESCRIBE PROPERTY GRAPH`, and
`DROP PROPERTY GRAPH [IF EXISTS]`. Definitions persist as transactionally managed
`__orchid_graph_<name>` views; reserve that prefix for Orchid. Reopen the database,
load Orchid and source extensions, and reattach external catalogs under the same
names to reuse the saved graph. Source schemas are revalidated on each query.

## Query with Cypher and Gremlin

Native statements require no quoted query wrapper:

```sql
CYPHER social
MATCH (a:Person)-[r:FOLLOWS]->(b:Person)
RETURN a.name AS person, b.name AS friend, r.since AS since
ORDER BY person;
-- Alice | Bob  | 2020
-- Bob   | Cara | 2022

CYPHER social
MATCH (:Person {name: 'Alice'})-[:FOLLOWS*1..2]->(friend)
RETURN DISTINCT friend.name AS name ORDER BY name;
-- Bob, Cara

GREMLIN social
 g.V().has('Person', 'name', 'Alice')
  .out('FOLLOWS').out('FOLLOWS').values('name');
-- Cara

EXPLAIN CYPHER social MATCH (p:Person) WHERE p.age > 35 RETURN p.name;
EXPLAIN GREMLIN social g.V().values('name');
```

For composition inside SQL, use `orchid_cypher(graph, query)` or
`orchid_gremlin(graph, query)` as table functions:

```sql
SELECT q.name, p.age
FROM orchid_cypher('social', 'MATCH (p:Person) RETURN p.name AS name') AS q
JOIN people AS p USING (name)
WHERE p.age > 35;
-- Bob | 40
```

Nested unquoted `(CYPHER ...)` SQL subqueries are not implemented. The table
functions bind DuckDB relational plans and existing graph kernel operators.

## Optional row authorization with SpiceDB

Orchid can filter mapped graph rows using SpiceDB permissions. The trusted
embedding application authenticates the caller and supplies a connection-local
subject. Queries inherit that identity automatically. No MCP server or federation
service is required or included.

Configure a DuckDB secret for SpiceDB's **HTTP gateway** (not its gRPC port):

```sql
CREATE SECRET company_auth (
    TYPE SPICEDB,
    ENDPOINT 'https://permissions.example.com',
    TOKEN 'replace-with-service-token'
);
```

HTTPS verifies certificates; plain HTTP is accepted only on loopback for local
testing. Tokens are redacted by DuckDB's secret manager. Secrets are temporary by
DuckDB's default; use its `CREATE PERSISTENT SECRET` explicitly if desired.

Add a policy to an existing mapped graph, or append the same `AUTHORIZATION`
block to `CREATE PROPERTY GRAPH`, after any computed edges:

```sql
ALTER PROPERTY GRAPH slack SET AUTHORIZATION (
    PROVIDER company_auth, DEFAULT DENY,
    VERTEX Message RESOURCE channel KEY(channel_id) REQUIRE view,
    VERTEX Chunk RESOURCE channel KEY(channel_id) REQUIRE view
);

SET GRAPH AUTHORIZATION (SUBJECT_TYPE 'user', SUBJECT_ID 'alice');
CYPHER slack MATCH (m:Message) RETURN m.body;
GREMLIN slack g.V().hasLabel('Message').values('body');
RESET GRAPH AUTHORIZATION;
```

A message with `channel_id = 'engineering'` requires `view` on SpiceDB object
`channel:engineering`. All messages and chunks in that channel share its decision;
there is no extra authorization-ID column or per-message grant required. Resource
keys must be string/integer columns containing the canonical SpiceDB object ID.
Use workspace-qualified IDs when IDs are not globally unique. Authorization key
columns need not be exposed as graph properties.

SpiceDB owns user/team membership and grant semantics. For example:

```zed
definition user {}
definition team {
    relation member: user
}
definition channel {
    relation viewer: user | team#member
    permission view = viewer
}
```

With these relationships, Alice sees engineering messages and Bob sees private
messages:

```text
team:engineering#member@user:alice
channel:engineering#viewer@team:engineering#member
channel:private#viewer@user:bob
```

Your existing application/ingestion process maintains those relationships in
SpiceDB. Orchid reads permissions; it does not synchronize source ACLs or change
SpiceDB's schema. [06_authorization.sql](examples/06_authorization.sql) exercises
this model, including chunks and computed edges. Its local configuration expects
SpiceDB's HTTP gateway at `127.0.0.1:8448` with token `orchid-local-test` and the
schema/relationships above.

For parameter binding, the trusted host can use:

```sql
CALL orchid_set_authorization($context);
```

`$context` is a JSON string such as
`{"subject_type":"user","subject_id":"alice","context":{"region":"us"}}`.
The optional `context` object supplies SpiceDB caveat inputs. An optional
`at_least_as_fresh` string supplies a ZedToken. The native statement supports the
same options as string literals:

```sql
SET GRAPH AUTHORIZATION (
    SUBJECT_TYPE 'user', SUBJECT_ID 'alice', CONTEXT '{"region":"us"}'
);
```

Identity is connection-local, is not persisted, and is not rolled back by a data
transaction. Clear it before returning a connection to a pool. Prepared graph
queries rebind and use the current identity and policy. Permission decisions are
batched and deduplicated within one statement, with no cache across statements.
The first check requests fully consistent permissions (or the supplied freshness
bound); subsequent checks use the same SpiceDB revision. This does not create a
shared transaction across SpiceDB, DuckDB, and external storage.

Unlisted vertex labels are denied; use `VERTEX SomeLabel PUBLIC` to expose one
explicitly. Stored edges require both endpoints to be visible, including standalone
edge scans. Computed edges filter before candidate limits and reranking, and BM25
uses the authorized target corpus. Missing identity/secret, permission-service
errors, and unresolved caveat context fail the query. NULL/empty resource keys are
denied. Protected graphs are read-only.

Authorization currently applies to mapped property graphs through native Cypher
and Gremlin. Authorized sessions reject advanced caller-supplied mappings and
SPARQL/update entry points; managed graphs do not support these policies. This
preserves the existing SPARQL functionality without inventing a new RDF policy
model. Scalar macros remain usable, but macros containing subqueries (including
transitive calls) are rejected in authorized execution.

The embedding application controls identity, graph definitions, secrets, and
installed functions/extensions. Arbitrary host SQL remains trusted and can read
base tables; this feature is not database-wide DuckDB SQL RLS or a sandbox for
untrusted UDFs. Expose graph-query execution through the application's controlled
interface rather than exposing administrative operations.

SpiceDB support is enabled in the normal build and makes no network calls for
unprotected graphs. To build without its HTTP client:

```sh
CARGO_TARGET_DIR="$PWD/target" cargo build --locked \
  --manifest-path extension/compiler/Cargo.toml --no-default-features
python3 extension/scripts/build.py --skip-rust
```

## DuckDB functions

Ordinary scalar and aggregate functions come from the **current DuckDB connection**.
Orchid uses DuckDB's catalog and binder for overloads, argument checking, and return
types. Built-ins, loaded extension functions, SQL macros, and registered UDFs are
available without declaring signatures in Orchid.

```sql
CREATE MACRO title_slug(s) AS lower(replace(s, ' ', '-'));

CYPHER social
MATCH (p:Person)
RETURN title_slug(p.name) AS slug, version() AS duckdb_version;

CYPHER social
MATCH (p:Person)
RETURN product(p.age) AS age_product;
```

The same calls work in managed graphs and graph kernels that cannot become a SQL
region. Those kernels pass typed values or batches to DuckDB through the existing
host interface. DuckDB executes the function on the caller's connection and
transaction; Orchid does not emulate it. Binding discovers types without executing
user calls. Catalog changes are picked up when queries are rebound.

Language-defined operations retain their semantics: Cypher `labels()` and
`toInteger()`, Gremlin traversal steps, and SPARQL datatype/error behavior still
use the shared language implementation. Use a schema-qualified DuckDB function
such as `main.log(x)` when its name overlaps a language function. Orchid's RAG
scoring functions and the portable `fn.*` catalog also remain available. Portable
calls use explicit per-engine mappings; ordinary backend calls use DuckDB's catalog.

Table functions remain relational sources: expose `read_parquet(...)`,
`iceberg_scan(...)`, or another table function through a DuckDB view and map that
view into a graph. DuckDB SQL remains the interface for SQL window syntax and
other SQL-only calling forms; catalog inheritance does not change graph-language
grammars.

## Use an ordinary DuckDB client

For example, install the matching Python driver in a virtual environment:

```sh
python3 -m venv extension/vendor/demo-env
extension/vendor/demo-env/bin/pip install duckdb==1.5.6
```

Close the DuckDB shell before opening its database file from another process.
Run this Python code from the repository root:

```python
from pathlib import Path
import duckdb

with duckdb.connect(
    'graph.duckdb', config={'allow_unsigned_extensions': True}
) as db:
    db.load_extension(str(Path('extension/build/orchid.duckdb_extension').resolve()))
    rows = db.execute('''
        CYPHER social
        MATCH (p:Person)
        WHERE p.name = $name
        RETURN p.age AS age
    ''', {'name': 'Alice'}).fetchall()
    print(rows)  # [(30,)]
```

Parameters are ordinary DuckDB named bindings, never query-text substitution.
Supported values include null, Boolean, signed integers, finite floating-point
numbers, strings, and nested lists/structs. Repeated executions bind fresh values.
Use the regular DuckDB APIs for rows, Arrow batches, or dataframes.

## Managed graphs and writes

Create a graph without source mappings to use Orchid-managed storage in DuckDB:

```sql
CREATE PROPERTY GRAPH workspace;
CYPHER workspace
CREATE (a:Person {name: 'Ada'}), (b:Person {name: 'Grace'}), (a)-[:KNOWS]->(b);

BEGIN;
CYPHER workspace MATCH (p:Person {name: 'Ada'}) SET p.age = 37;
COMMIT;

GREMLIN workspace g.V().has('Person', 'name', 'Ada').out('KNOWS').values('name');
```

Managed graphs support existing Cypher/Gremlin mutations, dynamic labels and
properties, multi-properties, and meta-properties. Records persist across reopen.
Reads and writes use the caller's DuckDB transaction; failed effects roll back
statement changes. `EXPLAIN` and `PREPARE` do not apply writes.

Mapped graphs also see ordinary SQL updates immediately within the transaction:

```sql
BEGIN;
UPDATE people SET age = 41 WHERE name = 'Bob';
CYPHER social MATCH (p:Person {name: 'Bob'}) RETURN p.age AS age;
-- 41
ROLLBACK;
CYPHER social MATCH (p:Person {name: 'Bob'}) RETURN p.age AS age;
-- 40
```

Graph-language writes to mapped sources require explicitly writable advanced
mappings and source write support. Graph DDL alone does not make external sources
writable. Cross-catalog atomicity depends on the underlying storage extensions.

## Results and native values

Mapped scalar projections retain ordinary SQL types when possible. Managed dynamic
values, graph elements, paths, and heterogeneous collections use the shared typed
carrier. Its raw client display is not yet a plain application value. Inspect it
with the existing SQL helper:

```sql
SELECT __orchiddb_value_json(q.name) AS name
FROM orchid_cypher('workspace', $$
    MATCH (:Person {name: 'Ada'})-[:KNOWS]->(p)
    RETURN p.name AS name
$$) AS q;
-- {"type":"string","value":"Grace"}
```

The bridge supports Boolean and numeric types, decimal, strings/binary, date,
microsecond time/timestamp, interval, JSON, lists/arrays, and structs. Native graph
identity, property records, and nested values retain their types. RDF mappings
preserve term kind, datatype, and language metadata.

## Iceberg, Lance, and search

Use their existing DuckDB interfaces to configure storage and credentials. Given
an existing Lance `documents` dataset (`id`, `title`) and Iceberg `authorship`
table (`id`, `person_id`, `document_id`), replace these example paths:

```sql
INSTALL iceberg;
LOAD iceberg;
INSTALL lance;
LOAD lance;
ATTACH '/data/lance' AS vectors (TYPE lance);

CREATE VIEW authorship AS
SELECT * FROM iceberg_scan('/data/authorship/metadata/v1.metadata.json');

CREATE PROPERTY GRAPH knowledge
VERTEX TABLES (
    people KEY (id) LABEL Person PROPERTIES (name),
    vectors.main.documents AS docs KEY (id) LABEL Document PROPERTIES (title)
)
EDGE TABLES (
    authorship KEY (id)
        SOURCE KEY (person_id) REFERENCES people (id)
        DESTINATION KEY (document_id) REFERENCES docs (id)
        LABEL AUTHORED
);

CYPHER knowledge
MATCH (p:Person)-[:AUTHORED]->(d:Document)
WHERE p.name = 'Alice'
RETURN d.title AS title;
```

Attached Iceberg catalog tables can be used directly instead of a scan view.
Schema discovery binds `SELECT * ... LIMIT 0` without executing source scans;
external binding can fetch metadata. This is not a distributed storage snapshot.

Expose an existing `lance_vector_search(...)` invocation through a view to map
indexed search candidates as graph vertices. Lance retains index and scoring
execution. Integration tests create a real IVF_FLAT index and join its results
with Iceberg relationships. Computed relationships can also rank candidates per source using the DDL below.
That declaration currently uses relational scoring over mapped sources; it does not
automatically register Lance search-index bindings. A mapped Lance search view
continues to use the index selected by its DuckDB search function.

## Retrieval with ordinary Cypher

Search inputs are query parameters. Use `MATCH`, scoring expressions, and successive
`WITH ... ORDER BY ... LIMIT` stages to select candidates, rerank them, and traverse
their relationships. Run [the setup](examples/07_rag_setup.sql), then execute
[the query file](examples/07_parameterized_rag.sql) with named parameters through
DuckDB. It returns `Detailed guide | duckdb | 2.0`.

Against that graph, an application binds text, an embedding, and a matrix of token
vectors through its ordinary DuckDB connection:

```python
rows = connection.execute("""
    CYPHER knowledge
    MATCH (c:Chunk)
    WITH c,
         text.bm25($query_text, c.text) AS lexical,
         vector.cosine_similarity($query_embedding, c.embedding) AS semantic
    WITH c, lexical / (1 + lexical) + (semantic + 1) / 2 AS candidate_score
    ORDER BY candidate_score DESC
    LIMIT 2
    WITH c, vector.maxsim($query_tokens, c.token_vectors) AS score
    ORDER BY score DESC
    LIMIT 1
    MATCH (document:Content)-[:HAS_CHUNK]->(c)
    RETURN document.title AS title, c.text AS text, score
    ORDER BY score DESC
""", {
    "query_text": "duckdb",
    "query_embedding": [1.0, 0.0],
    "query_tokens": [[1.0, 0.0], [0.0, 1.0]],
}).fetchall()
```

The fixture uses two candidates and one result; choose larger limits for your
application. Each limit applies at its position in the query. Traversals and filters
after a limit do not refill the candidate set. Add an ID as a secondary ordering key
when you need deterministic ties. Embeddings and token vectors come from the
application's models.

`text.bm25(query, node.text)` uses all rows of the mapped vertex source for its corpus,
after graph authorization and before query filters, joins, or limits. `WITH` aliases
preserve this source association; duplicated traversal rows do not duplicate corpus
documents. The text argument must retain its mapped vertex property provenance.
To score a transformed text field, expose the transformation in a mapped DuckDB view.
The existing scorer uses case-insensitive ASCII alphanumeric terms, `k1=1.2` and
`b=0.75`. Empty queries score zero; null input documents score null. Empty and null
documents still participate in corpus statistics.

`vector.cosine_similarity`, `vector.dot`, `vector.l2_distance`, and `vector.maxsim`
reuse the same scoring implementations as computed edges. MaxSim sums each query
token's best document-token inner product. Supply normalized token vectors for cosine
scoring. Cosine similarity returns null for a zero-norm vector; incompatible vector
dimensions fail the query.

Session SpiceDB policies apply automatically to candidate scans, BM25 statistics,
and subsequent graph traversal. These queries also work over mapped Lance tables
and Iceberg views. This path performs exact scoring with DuckDB; the example does
not provision a full-text or approximate-nearest-neighbor index. BM25 corpus
aggregation and vector scans must be considered when sizing large deployments.

## Computed RAG edges

A computed edge defines a relationship from expressions and ranking rules instead
of an edge table. Its endpoints are existing vertex labels. The shared compiler
constructs the joins, eligibility filters, scoring, and per-source ranking.

[The complete RAG example](examples/05_rag.sql) creates questions, documents,
authors, and stored `WRITTEN_BY` relationships, then declares this graph:

```sql
CREATE PROPERTY GRAPH rag
VERTEX TABLES (
    questions KEY (id) LABEL Question PROPERTIES (id, tenant_id, text, tokens),
    documents KEY (id) LABEL Document PROPERTIES (id, tenant_id, title, body, tokens),
    authors KEY (id) LABEL Author PROPERTIES (name)
)
EDGE TABLES (
    written_by KEY (id)
        SOURCE KEY (document_id) REFERENCES documents (id)
        DESTINATION KEY (author_id) REFERENCES authors (id)
        LABEL WRITTEN_BY
)
COMPUTED EDGES (
    RELEVANT_TO SOURCE Question DESTINATION Document
        CANDIDATES (
            WHERE (source.tenant_id = target.tenant_id)
            PROPERTIES (text.bm25(source.text, target.body) AS lexical)
            ORDER BY (lexical DESC)
            LIMIT PER SOURCE 2
        )
        PROPERTIES (vector.maxsim(source.tokens, target.tokens) AS score)
        ORDER BY (score DESC)
        LIMIT PER SOURCE 1
);

```

For each question, this takes the two highest BM25 candidates from the same tenant,
then reranks those candidates with MaxSim and retains one document. Candidate
properties such as `lexical` remain available on the final edge.

```sql
CYPHER rag
MATCH (q:Question)-[r:RELEVANT_TO]->(d:Document)-[:WRITTEN_BY]->(a:Author)
WHERE q.id = 1
RETURN d.title AS title, a.name AS author, r.score AS score;
-- Detailed guide | Bob | 2.0

GREMLIN rag g.V().has('Question', 'id', 1).out('RELEVANT_TO').values('title');
-- Detailed guide
```

The fixture intentionally includes a higher-MaxSim document outside the BM25
candidate set and another tenant's document. Neither can displace the eligible
candidate. Questions and token vectors are supplied by the application; the edge
retrieves context without loading an embedding model or invoking an LLM.

Computed-edge declarations follow `VERTEX TABLES` and optional `EDGE TABLES`:

- `SOURCE` and `DESTINATION` refer to vertex **labels**. Expressions use mapped
  graph properties through `source.property` and `target.property`.
- `WHERE (predicate)` filters eligible pairs. Place tenant/access filters in the
  `CANDIDATES` stage to apply them before candidate selection.
- `PROPERTIES (expression AS name, ...)` defines edge properties. Existing scalar
  expressions and immutable functions are reused. Candidate and final property
  names must be distinct.
- `ORDER BY (expression DESC, ...)` supports ASC/DESC and NULLS FIRST/LAST.
  `LIMIT PER SOURCE n` requires ordering; zero produces no edges. Candidate stages
  require a per-source limit. Omit `CANDIDATES` for single-stage scoring.
- Optional `RETRIEVAL EXACT` or `RETRIEVAL APPROXIMATE_ALLOWED` selects the existing
  retrieval policy; the default is `APPROXIMATE_ALLOWED`. An index still requires
  a supported source/index binding. Native declarations currently use table scans.

Functions include `text.bm25`, `vector.cosine_similarity`, `vector.dot`,
`vector.l2_distance`, and `vector.maxsim`. BM25's portable scorer uses the mapped
target corpus before pair filtering. MaxSim sums each query token's best document
token inner product; supply normalized token vectors when cosine scoring is wanted.

Computed edges are read-only and evaluated from current source data. Their identity
comes from both endpoint keys, including composite keys. Exact ranking breaks ties
by destination key. Reverse traversal reads the same directed edges, and filtering
a result after ranking does not refill the source's top-k. Definitions persist,
participate in DDL transactions, and are revalidated against source schemas.

The same declaration works over DuckDB tables/views, Lance documents, and Iceberg
stored relationships. Integration checks cover the two-stage RAG query across
actual Lance and Iceberg sources.

## SPARQL and advanced mappings

Register advanced schema configuration once on the DuckDB connection, then pass
query text separately. For the `people` table above:

```sql
-- Uses the people table created by 01_social.sql.
-- Register the RDF schema once on this connection.
CALL orchid_register_schema('people_rdf', $$
{
  "tables": [{"name": "people"}],
  "rdf": [{
    "table": "people",
    "subject": {"kind": "template", "prefix": "urn:person:", "columns": ["id"]},
    "predicate": {"kind": "constant", "value": "urn:name"},
    "object": {"kind": "literal", "column": "name"}
  }]
}
$$);
SELECT * FROM orchid_query('people_rdf',
  'SELECT ?name WHERE { ?person <urn:name> ?name } ORDER BY ?name',
  language := 'sparql');
-- Alice, Bob, Cara, each with RDF kind/datatype/language metadata.
```

`orchid_register_schema(name, schema_json)` retains a named schema for the current
connection. It rejects query text and query options inside the schema document.
`orchid_query(name, query_text, language := 'cypher', parameters := {name: 'Ada'})`
executes against that schema; typed Gremlin bindings use the separate `bindings`
argument. Source column types are resolved from the host DuckDB catalog.

SPARQL updates use `CALL orchid_sparql_update(name, query_text)` with explicitly
writable RDF mappings. `rdf_sources` supports typed quad sources and
`rdf_graph_names` maps named graph registries. Native `CYPHER` and `GREMLIN`
statements over property graphs remain available. SQL lowering is internal;
there is no `orchid_compile` function or combined query/schema request interface.

## Execution architecture

All integrations share language frontends, graph IR, and relational/kernel programs.
Standalone callers choose their SQL dialect and session or native execution. In
extension mode, DuckDB optimizes SQL regions and schedules physical graph kernel operators.
Multi-input stateful branches run in declared order. Nested kernels execute compiled
subplans on the same connection and transaction, with query-scoped state and
cancellation. Immutable prepared subplans may be reused within a query.

DataFusion provides relational planning and native execution in the shared core;
its executor does not run extension queries. PostgreSQL execution is an optional
feature. The extension opens no second database connection. Iceberg and Lance
retain their own scans and indexes. Existing JVM
algorithms, callbacks, and graph file codecs remain internal reusable components.
The retained `orchiddb-jvm-store` binary is an internal helper, not a public CLI.

Host integration lives in `extension/src` and `extension/compiler`; shared kernel
programs, native value transport, and storage access live in `src/ir/rel`.
Compiler probes and historical implementation diagnostics live in `tools/diagnostics`.
They are developer tooling, not product usage examples or a client SDK.

## Conformance and local verification

| Pinned corpus | Recorded result |
| --- | ---: |
| openCypher TCK 2024.3 | **3,897 / 3,897 passed** |
| TinkerPop 3.7.4 | **1,511 / 1,511 passed** |
| Existing SPARQL baseline | **974 passed**, 77 skipped, 74 not applicable |

[Committed evidence](conformance/extension-results/summary.json) identifies the
artifact and compressed full reports. Gremlin includes the original 15 Java
provider assertions on one extension instance. Excluded SPARQL cases are not
passes. These are pinned-corpus results, not universal language certification.
Older peer evidence remains historical and is not combined with extension results.

Build once, then validate that unchanged artifact locally:

```sh
python3 extension/scripts/build.py
python3 -m venv extension/vendor/test-env
extension/vendor/test-env/bin/pip install -r extension/tests/requirements.txt
make extension-test
```

The integration suite includes actual Iceberg/Lance scans and indexed search,
managed storage, native values, parameters, rollback, ordered effects, and
cancellation. Storage integration was validated with Iceberg `890b78a9c` and Lance
`2913169`. These checks are not a cross-platform or distributed transaction certification.

To exercise authorization against a **real SpiceDB server**, supply an installed
SpiceDB binary. The tests start and stop their own isolated memory-datastore server:

```sh
ORCHID_SPICEDB_BINARY=/absolute/path/to/spicedb ORCHID_EXTERNAL_TESTS=1 \
  extension/vendor/test-env/bin/python -m unittest discover \
  -s extension/tests -p test_authorization.py -v
```

Local authorization validation uses SpiceDB **1.56.2** and includes user/team
channel grants, revocation, caveats, prepared statements, source-reading macro
rejection, stored/computed edges, and actual Iceberg/Lance sources.

To reproduce upstream reports, install the existing assertion dependencies
(Java 21 and Maven required for Gremlin):

```sh
extension/vendor/test-env/bin/pip install -r conformance/requirements.txt
extension/vendor/test-env/bin/python conformance/upstream/fetch.py
extension/vendor/test-env/bin/python conformance/upstream/catalog.py
mvn -q -f jvm-codecs/pom.xml install
mvn -q -f jvm/pom.xml install dependency:build-classpath -Dmdep.outputFile=target/classpath.txt
mvn -q -f conformance/adapters/sqlg/pom.xml package dependency:build-classpath -Dmdep.outputFile=classpath.txt

CONFORMANCE_JAVA=/path/to/java extension/vendor/test-env/bin/python extension/conformance/run.py \
  --suite tinkerpop --output target/conformance/extension/gremlin.json
extension/vendor/test-env/bin/python extension/conformance/run.py \
  --suite opencypher --output target/conformance/extension/cypher.json
extension/vendor/test-env/bin/python extension/conformance/run.py \
  --suite rdf --output target/conformance/extension/rdf.json
```

Run suites sequentially without filters for complete catalog coverage. Reports
record case IDs, assertion identities, source hashes, and the loaded artifact.
The RDF runner reports its full catalog, including the existing exclusions.

### Standalone SQL backend conformance

The migrated shared core passes the full pinned matrix on **both DuckDB and
PostgreSQL**: 3,897 Cypher, 1,511 Gremlin, and 974 SPARQL cases per backend.
The same 77 skipped and 74 not-applicable SPARQL cases remain omitted.
[Standalone execution evidence](conformance/sql-engine-results/summary.json)
records full reports, executable identity, selected-engine SQL region counts,
and uninterrupted Gremlin instances.

After installing the assertion dependencies above, build the shared native runner
and run all three pinned suites on DuckDB and PostgreSQL:

```sh
bash conformance/build-orchiddb.sh
export CONFORMANCE_ORCHIDDB_BINARY="$PWD/target/release/upstream"
export ORCHIDDB_JVM_STORE="$PWD/target/release/orchiddb-jvm-store"
export ORCHIDDB_JVM_CLASSPATH="$PWD/jvm/target/classes:$(cat jvm/target/classpath.txt)"
export CONFORMANCE_TINKERPOP_SOURCE="$PWD/conformance/upstream/cache/tinkerpop"
export CONFORMANCE_JAVA=/path/to/java
export ORCHIDDB_TEST_PG_URL='host=localhost dbname=orchiddb_test'
extension/vendor/test-env/bin/python conformance/upstream/engine_matrix.py
```

The matrix requires complete catalogs, one uninterrupted Gremlin instance per
backend, SQL regions executed by the selected backend, and no unexpected failures
or omissions. Only the existing SPARQL omissions with unchanged case hashes are
allowed. Reports are written under `target/conformance/sql-engines/`.

## Build the complete GitHub release

From a clean, committed checkout on **macOS ARM64**, run:

```sh
make release
```

The version defaults to the root Cargo.toml. For a new version, run
`make release VERSION=0.4.0`. From a clean checkout, Make updates the core, CLI,
and client manifests and lockfiles, commits the version change, prepares the
Python release environment, and runs the release pipeline. Existing uncommitted
work is never included in an automatic release commit. Nothing is uploaded, no tag is
created, and nothing is published to npm, PyPI, Maven Central, Hex, or crates.io.
The command prints the directory to upload manually to your GitHub release:

```text
target/releases/orchiddb-v<version>/<commit>/upload/
```

The pipeline runs client tests, the full DuckDB and PostgreSQL conformance matrix,
and the full extension conformance suites. Only the existing, exact SPARQL
omissions are accepted. PostgreSQL runs in an isolated temporary Docker container,
which is removed on exit. It then builds optimized artifacts for **macOS ARM64,
Linux ARM64, and Linux x86-64**, validates native ABI and actual CLI/extension
queries on each platform, and checks the Java classifiers in host/Linux JVMs.
Linux validation uses local Docker containers; builds use Cargo and Zig locally.
No GitHub Actions jobs are dispatched.

| Component | GitHub assets |
| --- | --- |
| DuckDB extension | Three platform archives, pinned to DuckDB 1.5.6 |
| CLI | Three archives with the standalone executable and examples |
| Native runtime | Three shared-library/header archives plus JVM kernel/dependency ZIP |
| Python | Three wheels containing the matching native runtime |
| JavaScript/TypeScript | One local-install tarball with all three native runtimes |
| Java/Gremlin | One ZIP with API, dependency, and native classifier JARs |
| C++ | Three CMake/header/library archives, including JSON headers |
| Elixir | Three source/runtime archives; the small NIF builds against the user's Erlang |
| Rust | Complete source archive preserving local workspace dependencies |
| Verification | Full conformance reports, release manifest, and SHA256SUMS |

Upload **every file** in the printed `upload/` directory. The manifest maps each
client to its assets and records the source commit and checksums. The Rust client
is under `clients/rust` in the source archive. Language package tools may install
the downloaded assets or resolve third-party dependencies; this release process
requires no registry publishing accounts or credentials. Extension binaries are
unsigned and require DuckDB's `allow_unsigned_extensions` setting.

### Prerequisites and retries

Install Rust/rustup, Zig, cargo-zigbuild, Docker with ARM64/x86-64 execution,
Java 21, Maven, Node 20+, npm, Elixir/Erlang, CMake, and a C++17 compiler. Configure
`JAVA_HOME` for Java 21. Use the existing test environment or prepare it with:

```sh
python3 -m venv extension/vendor/test-env
extension/vendor/test-env/bin/pip install -r extension/tests/requirements.txt \
  -r conformance/requirements.txt pytest build wheel setuptools
make release-check
make release
```

`TEST_PYTHON=/absolute/path/to/python` selects another prepared environment.
Build tools can download their normal dependencies; generated packages are staged
under `target/`. Source files and published tags/assets are never overwritten.

Completed tests, platform builds, packages, and validation receipts are reused
only for the same commit/version with matching hashes. Rerun `make release` to
retry a failed stage. Individual stages are available for diagnosis:

```sh
make release-test
make release-build
make release-package
make release-verify
```

`make -n release` previews the command. Packaging requires successful tests and
builds; only successful verification prints the upload-ready result. The existing
extension development targets are also in the root `Makefile`: `extension`,
`extension-release`, `extension-test`, and `extension-package`.

## License

[GPL-3.0-only](LICENSE.md). Third-party components retain their upstream licenses
and notices.
