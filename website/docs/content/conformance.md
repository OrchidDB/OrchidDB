# Conformance

The pinned upstream assertions run against the loaded DuckDB extension. Reports
identify the artifact, catalog, assertion sources, and actual individual outcomes.
These results do not claim coverage beyond the pinned corpora.

## Cypher

All **3,897 / 3,897** openCypher TCK 2024.3 cases pass.
[Full report](downloads/conformance/cypher.json.gz).

## Gremlin

All **1,511 / 1,511** TinkerPop 3.7.4 cases pass, including the original 15 Java
provider assertions on the same extension instance.
[Full report](downloads/conformance/gremlin.json.gz).

## SPARQL

The existing embedded baseline has **974 passes**, **77 skipped**, and **74 not
applicable**. The migration preserves its existing scope; excluded cases are not
counted as passes. [Full report](downloads/conformance/rdf.json.gz).

[Artifact and count summary](downloads/conformance/summary.json) ·
[Reproduction instructions](https://github.com/OrchidDB/OrchidDB/blob/main/conformance/README.md)

Historical peer comparisons remain in the repository as historical evidence and
are not merged into these extension results. Validation is local on macOS ARM64,
DuckDB 1.5.6. Reports are recorded evidence, not a live status dashboard.
