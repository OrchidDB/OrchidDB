# Crabgraph handoff

Date: 2026-08-31

## Product direction

Crabgraph is a graph-language data layer built on DataFusion. It is not a
database. Cypher, Gremlin, and SPARQL frontends produce a shared Graph IR.
Relational regions lower to SQL islands and execute in DuckDB today, with
other SQL engines intended later.

Users bring their existing tables, views, and lakehouse schemas. A graph
schema mapping connects node labels, relationship types, identities, and
properties to those relations. SPARQL adds an ontology mapping before the
same graph schema mapping. RDF storage columns are not required.

The intended lakehouse path is DuckDB with `cache_httpfs`, Iceberg, and a
Polaris catalog. Users expose suitable views, map those views once, and run
graph queries against them.

## Implemented in this workstream

- Added the SPARQL frontend using the standards-oriented `spargebra` parser.
- Added ontology mappings for RDF classes, predicates, identities, graph
  properties, and property-graph relationships.
- Added bring-your-own-schema mappings for user tables, queries, and views.
- Added mapped SPARQL-to-DuckDB execution examples that contain no RDF
  storage machinery in generated SQL.
- Expanded Cypher relational lowering for `count_if`, dynamic list append and
  prepend, `list_unique`, temporal extraction and truncation, temporal casts,
  and subscripting.
- Expanded Gremlin relational lowering around native list values, `unfold`,
  local list operations, set operations, casts, string operations, and math.
- Added DuckDB temporal result conversion and dialect fixups for nested list
  functions and `UNNEST` aliases.
- Added real DuckDB timeout handling across setup and query execution.
- Added a sharded, full-denominator DuckDB corpus runner with machine-readable
  metrics, blockers, per-case outcomes, outcome transitions, and regression
  reports.
- Added the one-page Crabgraph website, language tabs with generated SQL,
  bring-your-own-schema messaging, coverage details, and project links.

## Last verified denominators

### Cypher through DuckDB

Command:

```sh
scripts/run-rel-coverage.sh cypher 4
```

Current full checkpoint:

| Measure | Result |
| --- | ---: |
| Total | 5,593 |
| Runnable | 5,446 |
| Matched | 3,392 |
| Parsed | 5,427 |
| Planned | 5,399 |
| Lowered | 4,816 |
| Executed | 4,634 |
| Mismatches | 1,242 |
| Skipped fixture data | 147 |

The original checkpoint was 3,238 matches, 4,610 lowered, and 4,443
executed. The verified net movement is therefore +154 matches, +206 lowered,
and +191 executed.

### Gremlin through DuckDB

Command:

```sh
scripts/run-rel-coverage.sh gremlin 4
```

Last full checkpoint:

| Measure | Result |
| --- | ---: |
| Total | 1,709 |
| Runnable | 1,667 |
| Matched | 578 |
| Parsed | 1,667 |
| Planned | 1,667 |
| Lowered | 888 |
| Executed | 782 |
| Mismatches | 204 |
| Skipped fixture data | 42 |

This is +42 matches and +149 lowered cases over the initial DuckDB checkpoint.
The final transition report records zero matched-to-failed regressions.

### SPARQL

The pinned W3C query-file benchmark is documented in
`docs/sparql_coverage.md`. Its current parser and Graph IR planning snapshot is
408 of 425 query files, with all 408 parsed files reaching Graph IR.

`tests/sparql_smoke.rs` contains the mapped DuckDB execution matrix. It covers
identity and property mappings, filters, ordering and limits, distinct
projection, relationships, and required mapped numeric properties over
user-owned views.

The current matrix result is 6 / 6 planned, lowered, executed, and matched.

## Current verification boundary

The final batched edits add relational lowering for:

- `cast_number`, `cast_bigint`, `cast_bigdecimal`, and `gremlin_cast_date`;
- `gremlin_math_bin`;
- `format_concat` and `conjoin`;
- `null_to_sentinel`, `list_restore_null_sentinels`, and
  `gremlin_dedup_key`;
- dynamic list membership and `list_has_all`;
- local trim and reverse string aliases;
- DuckDB list-replacement naming.

They have received the focused SPARQL execution check and full Gremlin and
Cypher DuckDB denominators. Both transition reports contain zero
matched-to-failed regressions. Reproduce the verification with:

```sh
cargo fmt --all -- --check
cargo test --test sparql_smoke mapped_sparql_duckdb_execution_matrix -- --nocapture
scripts/run-rel-coverage.sh gremlin 4
scripts/run-rel-coverage.sh cypher 4
```

The coverage runner retains the preceding report at a `-previous` path and
writes `transitions.tsv` plus `regressions.tsv`. A matched-to-failed transition
should block a coverage claim.

## Highest-value remaining gaps

Gremlin's largest relational gaps are labeled binding preservation,
`properties_list` fan-out, path projections, fold reductions, sacks and side
effects, correlated apply ambiguity, repeat termination, and shortest paths.
The next useful structural change is a typed property-object representation
that can fan out in SQL without passing through display strings.

Cypher's largest actionable lowering gaps include sequence functions,
`properties`, stored union tags, correlated apply keys, dynamic unwind and
quantifiers, and arithmetic over catalog values that currently cross the
boundary as text. Many zero-row mismatches are missing or incomplete imported
fixtures, so keep fixture coverage separate from language lowering coverage.

SPARQL needs a larger mapped execution denominator with expected rows. Extend
the matrix around `OPTIONAL`, unions, values, aggregates, and property paths.
The W3C query-file benchmark measures parsing and planning only and must not be
presented as execution conformance.

## Website and copy

The landing page is under `website/`. Its language should continue to follow
`website/STYLE.md`: direct technical prose, concrete nouns, measured
claims, and no em dashes. Keep DataFusion described as the foundation of the
planner, not as a database. Keep the graph schema and ontology mapping layers
separate in diagrams and examples.

Repository links currently use the existing GitHub repository path
`henneberger/new-graph`, while the product name is Crabgraph. Rename the
repository link only after the GitHub repository itself is renamed.
