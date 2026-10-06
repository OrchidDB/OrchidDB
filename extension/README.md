# Orchid graph language extension for DuckDB

A DuckDB extension pinned to **DuckDB 1.5.6**. Define a graph with one statement
and query it with native `CYPHER` or `GREMLIN` syntax. DuckDB executes the
relational plans and schedules Orchid’s existing graph kernels, including
scans supplied by the Iceberg and Lance extensions.

The extension reuses Orchid's existing Cypher parser, semantic analysis,
Graph IR, mappings, relational lowering, graph kernels, persistence codecs,
and DuckDB SQL generator. It does not
open a second connection or database, run DataFusion's executor, or include the
PostgreSQL driver. DataFusion remains a **compiler dependency** during this
stage. The existing library is retained as shared implementation and regression coverage.

## Build locally

Requirements: Python 3.12+, curl, Cargo/Rust supporting edition 2024, and a C++17
compiler. Builds and tests run locally, never in GitHub Actions.

```sh
python3 extension/scripts/build.py
```

The script caches pinned DuckDB headers, the official CLI, and the nlohmann JSON
header under `extension/vendor`. The Rust compiler uses its checked-in lockfile
and the repository's target directory. Set `CARGO_TARGET_DIR` to change that
cache, or pass `--release` for an optimized build. The result is:

```text
extension/build/orchid.duckdb_extension
extension/vendor/cli/duckdb
```

This development artifact is unsigned. Launch the matching CLI with:

```sh
extension/vendor/cli/duckdb -unsigned graph.duckdb \
  -cmd "LOAD 'extension/build/orchid.duckdb_extension'"
```

Load Orchid before submitting native graph statements. Loading it enables
DuckDB's `allow_parser_override_extension=fallback` setting so the extension
can translate its statements while leaving ordinary SQL to DuckDB. Applications
using the Python client can set `allow_unsigned_extensions=true` when opening
the connection and then `LOAD` the artifact. Other clients need the same DuckDB
version and development loading configuration.

## Define and query a graph

```sql
CREATE TABLE people(id BIGINT PRIMARY KEY, name VARCHAR, age INTEGER);
CREATE TABLE follows(id BIGINT, src BIGINT, dst BIGINT, since INTEGER);
INSERT INTO people VALUES (1, 'Alice', 30), (2, 'Bob', 40);
INSERT INTO follows VALUES (10, 1, 2, 2020);

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

CYPHER social
MATCH (a:Person)-[e:FOLLOWS]->(b:Person)
RETURN a.name AS person, b.name AS friend, e.since AS since;

EXPLAIN CYPHER social
MATCH (p:Person)
WHERE p.age > 35
RETURN p.name;

DESCRIBE PROPERTY GRAPH social;
```

Both vertices and edges require explicit `KEY (...)` in this version. Ordered
composite keys work. `AS` assigns a source alias for endpoint references. A
physical table can supply both vertices and edges when the elements have distinct
aliases. Endpoints must reference a vertex's complete ordered key, with matching
types. Keys identify elements; they do not create constraints or certify the
source data's uniqueness. Separate edge keys preserve parallel relationships.

`PROPERTIES (column AS property, ...)` selects and renames properties. Omitting
it, or writing `PROPERTIES ALL COLUMNS`, exposes the current columns. `NO
PROPERTIES` exposes none. Each label maps to one source in this first version.

Mapped definitions support `CREATE OR REPLACE PROPERTY GRAPH`,
`CREATE PROPERTY GRAPH IF NOT EXISTS`, and `DROP PROPERTY GRAPH [IF EXISTS]`. Definitions use transactionally
managed DuckDB views named `__orchid_graph_<name>` as their persistence mechanism;
no graph rows are copied. Keep that prefix reserved for Orchid. Reopen the
database, load Orchid and any source extensions, and reattach external catalogs
under the same names to query the saved graph.

Names support `catalog.schema.graph`. Unqualified sources are relative to the
graph's own catalog/schema, including after `USE` changes. Use fully qualified
names for attached sources. Put persistent graph definitions in a writable
DuckDB database, independently of where the source data lives.

## Parameters and SQL composition

Native Cypher parameters become DuckDB named parameters, with values supplied
through the regular driver:

```python
rows = connection.execute(
    "CYPHER social MATCH (p:Person) WHERE p.name = $name RETURN p.age",
    {"name": "Alice"},
).fetchall()
```

The compiler specializes to those values during binding. Tests check that
repeated executions rebind values and do not treat them as SQL text. Supported
parameter values are null, Boolean, signed integers, finite floating-point
numbers, strings, and nested lists/structs. Out-of-range integers and unsupported
types fail explicitly.

For composition inside SQL, the same binder is available as a table function:

```sql
SELECT q.name, p.age
FROM orchid_cypher('social', 'MATCH (p:Person) RETURN p.name AS name') AS q
JOIN people AS p USING (name)
WHERE p.age > 35;
```

The function binds a relational plan with existing kernel operators where needed.
`EXPLAIN` shows the actual scans, joins, filters, and residual graph operators. Nested unquoted `(CYPHER ...)` syntax is not implemented yet.

## Iceberg and Lance

Load and configure these extensions through their existing DuckDB interfaces.
Orchid does not install extensions, own credentials, read file formats, or
implement indexes. Graph sources can be attached tables or ordinary views:

```sql
INSTALL iceberg;
LOAD iceberg;
INSTALL lance;
LOAD lance;

ATTACH '/data/lance' AS vectors (TYPE lance);

-- An attached Iceberg catalog table can be used directly instead of this view.
CREATE VIEW authorship AS
SELECT * FROM iceberg_scan('/data/authorship/metadata/v1.metadata.json');

CREATE PROPERTY GRAPH knowledge
VERTEX TABLES (
    people KEY (id) LABEL Person,
    vectors.main.documents AS docs KEY (id)
        LABEL Document PROPERTIES (title)
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
RETURN d.title;
```

Existing Lance search functions can supply a graph source through a view, e.g.
`SELECT * FROM lance_vector_search(...)`. This preserves Lance's search and
index execution. The storage integration test creates an IVF_FLAT index and
queries such a source joined to Iceberg relationships. Dynamic per-row graph
search and a computed-relationship DDL clause are not implemented yet; they need
adapters for the existing Orchid search planner.

Schema discovery binds `SELECT * ... LIMIT 0` through DuckDB without executing
source scans. Binding external sources may fetch their metadata. Every graph
query revalidates source schemas and endpoints. Metadata is not treated as
proof of enforced keys, nor as a distributed snapshot across storage systems.

## Current scope

Cypher and Gremlin use the existing language implementations with DuckDB as
execution host. SQL-compatible operations remain ordinary DuckDB plans;
residual graph operations use the existing compiled kernels. Managed graphs
support mutations, dynamic labels and properties, multi-properties and
meta-properties. Writes participate in the caller’s transaction, and prepared
queries acquire fresh execution state. `EXPLAIN` and `PREPARE` do not apply writes.

For a graph whose storage Orchid manages, create it without source mappings:

```sql
CREATE PROPERTY GRAPH workspace;
CYPHER workspace CREATE (:Person {name: 'Ada', age: 37});
CYPHER workspace MATCH (p:Person) RETURN p.name, p.age;
GREMLIN workspace g.V().hasLabel('Person').values('name');
```

Graph and heterogeneous results preserve native identity and value types through
the shared value transport. `__orchiddb_value_json(value)` exposes its typed JSON
representation for SQL clients. Mapped scalar queries retain ordinary SQL types.

SPARQL retains its existing embedded query and update functionality through the
shared mapping API below. Graph DDL is inspired by `CREATE PROPERTY GRAPH`;
it does not claim full SQL/PGQ DDL or query compatibility.

Types supported by the bridge include Boolean, integer widths, float/double,
decimal, strings/binary, date, microsecond time/timestamp, interval, JSON,
lists/arrays and structs. Unsupported source types require a cast in a source
view. Array columns enter the compiler as lists; the bound DuckDB source retains
its actual type. Writes to mapped sources require explicitly writable mappings
and the source extension’s write support. Cross-catalog atomicity and consistent
multi-format snapshots depend on the underlying storage systems.

## Tests

```sh
python3 -m venv extension/vendor/test-env
extension/vendor/test-env/bin/pip install -r extension/tests/requirements.txt

CARGO_TARGET_DIR="$PWD/target" cargo test \
  --locked --manifest-path extension/compiler/Cargo.toml --lib

ORCHID_EXTERNAL_TESTS=1 \
  extension/vendor/test-env/bin/python -m unittest discover -s extension/tests -p 'test_*.py'
```

Run the pinned upstream assertions against the loaded extension:

```sh
extension/vendor/test-env/bin/python extension/conformance/run.py \
  --suite opencypher --output target/conformance/extension/cypher.json
CONFORMANCE_JAVA=/path/to/java extension/vendor/test-env/bin/python \
  extension/conformance/run.py --suite tinkerpop \
  --output target/conformance/extension/gremlin.json
```

Reports identify the extension artifact, pinned catalogs, and assertion sources.
The Gremlin provider executes the original Java assertions through the same
extension instance. JVM algorithm kernels require the repository’s existing
`jvm/target/orchiddb-jvm-0.1.0.jar` and dependency classpath.

The external tests use PyIceberg solely to create a real local fixture. Production
execution goes through DuckDB's Iceberg extension. Lance fixtures and indexes are
created through the Lance extension. Neither is mocked. These tests check actual
results and scan visibility in `EXPLAIN`; they are not a lakehouse performance
benchmark or a complete storage transaction certification.

Validated locally on macOS ARM64 with DuckDB 1.5.6, Iceberg `890b78a9c`, and Lance
`2913169`. The build script also has Linux/macOS x86_64 and Linux ARM64 routes;
those targets have not been validated in this work. C++ extension builds remain
specific to DuckDB versions/platforms. Downloaded dependencies retain their
upstream licenses: DuckDB and nlohmann JSON are MIT; Orchid is GPL-3.0-only.

## Gremlin and SPARQL through the shared compiler

Gremlin reuses the same property graph definition and host transaction:

```sql
GREMLIN social g.V().hasLabel('Person').out('FOLLOWS').values('name');
EXPLAIN GREMLIN social g.V().values('name');
```

`orchid_gremlin(graph, query)` provides the corresponding SQL table function.
Typed Gremlin bindings are available in the mapping API. Native graph
statements share the same managed and mapped execution paths.

The advanced `orchid_query(request_json)` table function accepts the existing
version-1 compiler mapping protocol for Cypher, Gremlin and SPARQL. DuckDB binds
each declared table/view and supplies its actual schema, ignoring supplied
column declarations. The extension requires the host DuckDB as execution engine.
`orchid_compile(request_json)` exposes the same compilation metadata for
inspection; compiler failures return an `error` plus any authoritative frontend
`classification`. Schema/binding errors still raise DuckDB exceptions.

```python
import json

request = {
    "version": 1,
    "language": "sparql",
    "query": "SELECT ?name WHERE { ?person <urn:name> ?name }",
    "tables": [{"name": "people"}],
    "rdf": [{
        "table": "people",
        "subject": {"kind": "template", "prefix": "urn:person:", "columns": ["id"]},
        "predicate": {"kind": "constant", "value": "urn:name"},
        "object": {"kind": "literal", "column": "name"},
    }],
}
rows = connection.execute("SELECT * FROM orchid_query(?)", [json.dumps(request)]).fetchall()
```

SPARQL mappings preserve RDF lexical values and identity columns (kind,
datatype, language). `rdf_sources` additionally accepts the existing typed
`IriQuadSource` representation; `rdf_graph_names` maps named graph registries.
The extension registers the existing SPARQL scalar kernel for regexes, URI
resolution, date comparisons, exact decimal division and related operations.
No language semantics were copied into a second evaluator.

## Upstream conformance runner

Run the original assertions through the host extension with:

```sh
extension/vendor/test-env/bin/pip install -r conformance/requirements.txt
extension/vendor/test-env/bin/python extension/conformance/run.py \
  --suite opencypher --output target/conformance/extension/opencypher.json
```

Use `--suite tinkerpop` or `--suite rdf` for the other languages. Java 21 and the
existing pinned Java assertion adapter are required for TinkerPop. Reports
record the loaded extension and assertion source hashes. See the setup commands
in the main conformance guide for installing the pinned test corpora.
