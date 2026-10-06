# OrchidDB

**Cypher, Gremlin, and SPARQL in DuckDB.** Map existing tables, Iceberg data, and
Lance datasets into graphs, then query them through your normal DuckDB connection.

Orchid is a DuckDB extension. DuckDB owns the connection, transactions, relational
execution, and storage access. Orchid reuses its existing language frontends,
graph compiler, kernels, and value codecs. There is no separate Orchid server,
public CLI, or client SDK. This README is the project guide; runnable examples
use the DuckDB interface exclusively.

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
Local validation covers macOS ARM64. Other Linux/macOS build routes still need
platform validation before distribution. Signed extension distribution is not
configured yet.

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
scoring functions also remain available. The former `fn.*` portable catalog and
its cross-engine function mappings have been removed.

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

The existing version-1 mapping protocol runs through DuckDB's
`orchid_query(request_json)` table function. For the `people` table above:

```sql
SELECT * FROM orchid_query($$
{
  "version": 1,
  "language": "sparql",
  "query": "SELECT ?name WHERE { ?person <urn:name> ?name } ORDER BY ?name",
  "tables": [{"name": "people"}],
  "rdf": [{
    "table": "people",
    "subject": {"kind": "template", "prefix": "urn:person:", "columns": ["id"]},
    "predicate": {"kind": "constant", "value": "urn:name"},
    "object": {"kind": "literal", "column": "name"}
  }]
}
$$);
```

SPARQL currently uses this function interface, not a native unquoted statement.
The same protocol accepts Cypher and typed Gremlin bindings. DuckDB supplies actual
source schemas; caller-provided column declarations are not authoritative.
`orchid_compile(request_json)` exposes compilation metadata and frontend errors.
The extension requires its host DuckDB as the execution engine.

Existing SPARQL updates use `CALL orchid_sparql_update(request_json)` with explicitly
writable RDF mappings. `rdf_sources` accepts typed quad sources, including kind,
datatype, and language columns; `rdf_graph_names` maps named graph registries.
The request types are defined in [src/compiler.rs](src/compiler.rs). SPARQL scope
is unchanged: this migration adds no SERVICE execution, reasoning, or HTTP endpoint.

## Execution architecture

Existing language frontends compile to the shared graph IR and relational/kernel
program. DuckDB optimizes SQL regions and schedules physical graph kernel operators.
Multi-input stateful branches run in declared order. Nested kernels execute compiled
subplans on the same connection and transaction, with query-scoped state and
cancellation. Immutable prepared subplans may be reused within a query.

DataFusion remains a compiler dependency; its executor does not run extension
queries. The PostgreSQL executor, session implementation, and driver dependency have been
removed. The extension opens no second database connection. Iceberg and Lance retain their own scans and indexes. Existing JVM
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
make -f scripts/release/Makefile test
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

## Local packaging

```sh
make -f scripts/release/Makefile build
make -f scripts/release/Makefile test
make -f scripts/release/Makefile package
```

Packaging copies the existing extension and license into `target/packages/<sha256>`
with a checksum and manifest identifying its DuckDB version, platform, and source
revision. Build and validate the intended revision before packaging. Signing and
publication are separate operations. All builds and tests run locally; no client
packages or GitHub Actions build matrix are involved.

## License

[GPL-3.0-only](LICENSE.md). Third-party components retain their upstream licenses
and notices.
