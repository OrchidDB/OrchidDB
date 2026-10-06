# OrchidDB

**Cypher, Gremlin, and SPARQL in DuckDB.** Query existing tables, Iceberg data,
and Lance datasets as graphs, using DuckDB's execution engine and storage extensions.

Orchid is a DuckDB extension. Define relationships once, then write native graph
queries through your existing DuckDB connection. No separate server, Orchid CLI,
or language-specific Orchid client is required.

[Extension guide](extension/README.md) · [Documentation source](website/docs/content/index.md) ·
[Architecture](docs/architecture.md) · [Conformance](conformance/README.md)

## Build and load

The extension targets **DuckDB 1.5.6**. Build locally with Python 3.12+, Rust
(edition 2024), curl, and a C++17 compiler:

```sh
python3 extension/scripts/build.py
extension/vendor/cli/duckdb -unsigned graph.duckdb \
  -cmd "LOAD 'extension/build/orchid.duckdb_extension'"
```

This produces an unsigned development extension. Applications can load the same
artifact using a matching DuckDB client with `allow_unsigned_extensions=true`.
See the [build guide](extension/README.md#build-locally) for details.

## Tables become a graph with one statement

```sql
CREATE TABLE people(id BIGINT PRIMARY KEY, name VARCHAR, age INTEGER);
CREATE TABLE follows(id BIGINT, src BIGINT, dst BIGINT, since INTEGER);
INSERT INTO people VALUES (1, 'Alice', 30), (2, 'Bob', 40);
INSERT INTO follows VALUES (10, 1, 2, 2020);

CREATE PROPERTY GRAPH social
VERTEX TABLES (people KEY (id) LABEL Person PROPERTIES (name, age))
EDGE TABLES (
    follows KEY (id)
    SOURCE KEY (src) REFERENCES people (id)
    DESTINATION KEY (dst) REFERENCES people (id)
    LABEL FOLLOWS PROPERTIES (since)
);

CYPHER social
MATCH (a:Person)-[e:FOLLOWS]->(b:Person)
RETURN a.name AS person, b.name AS friend, e.since AS since;

GREMLIN social g.V().hasLabel('Person').out('FOLLOWS').values('name');
```

DuckDB discovers the source schemas. You declare element keys, labels, properties,
and relationship endpoints; Orchid validates their columns and types. Graph
metadata persists in the database, while source rows stay in their original tables.

Use attached Iceberg and Lance tables directly, or expose their scans and indexed
search functions through DuckDB views. Their extensions continue to perform
storage access and search. See the [Iceberg and Lance example](extension/README.md#iceberg-and-lance).

## Or let Orchid manage the graph

```sql
CREATE PROPERTY GRAPH workspace;
CYPHER workspace CREATE (:Person {name: 'Ada', age: 37});
CYPHER workspace MATCH (p:Person) RETURN p.name, p.age;
GREMLIN workspace g.V().hasLabel('Person').values('name');
```

Managed graphs support mutations, dynamic properties, and Gremlin multi-properties
and meta-properties. Queries and writes use the calling DuckDB transaction.
Native Cypher accepts ordinary DuckDB named parameters. For SQL composition, use
`orchid_cypher(graph, query)` or `orchid_gremlin(graph, query)` as table functions.

SPARQL retains the existing RDF query and update implementation through
`orchid_query(request_json)` and `orchid_sparql_update(request_json)`.
See [RDF mappings and examples](extension/README.md#gremlin-and-sparql-through-the-shared-compiler).

## One compiler, DuckDB execution

Orchid reuses its language frontends, graph IR, relational lowering, value codecs,
and graph kernels. DuckDB optimizes and executes relational operations and hosts
residual graph kernels. The extension neither runs DataFusion's executor nor
includes a PostgreSQL driver; DataFusion remains a compiler dependency.

`EXPLAIN CYPHER ...` and `EXPLAIN GREMLIN ...` expose the execution plan. Source
scans remain visible to DuckDB, including those supplied by Iceberg and Lance.

## Verified against pinned upstream suites

| Suite | Result |
| --- | ---: |
| openCypher TCK 2024.3 | **3,897 / 3,897 passed** |
| TinkerPop 3.7.4 | **1,511 / 1,511 passed** |
| Existing SPARQL baseline | **974 passed**, 77 skipped, 74 not applicable |

These are results for the pinned corpora, not a claim of universal language
coverage. Gremlin includes the original 15 Java provider assertions. SPARQL scope
is unchanged. [Evidence and reproduction](conformance/README.md).

Local integration tests cover actual Iceberg and Lance scans, indexed Lance
search, transaction rollback, prepared queries, cancellation, and native values.
Validated on macOS ARM64; artifacts are specific to the DuckDB version and platform.

## Development

```sh
make -f scripts/release/Makefile test
python3 website/docs/build.py
python3 website/docs/check.py
```

See [verification](docs/verification.md), [local packaging](scripts/release/README.md),
and the [implementation plan](extension/IMPLEMENTATION_PLAN.md). All builds and
tests run locally. Existing shared library and JVM modules remain implementation
components; this repository no longer packages standalone clients.

## License

[GPL-3.0-only](LICENSE.md).
