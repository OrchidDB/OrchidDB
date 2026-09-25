# SPARQL conformance implementation

## Execution and write contracts

Preserve the production execution model: SPARQL → Graph IR → SQL IR DAG,
with DuckDB executing eligible regions and DataFusion executing residual
operators. RDF terms carry lexical value, kind, datatype, and language through
each boundary. SPARQL expression errors become unbound values according to the
operator's contract; they must not change Cypher or Gremlin error behavior.

Writes must invert the declared read mapping. Predicate/class mappings identify
the existing table, key, property column or relationship endpoint columns.
Compile `INSERT`, `DELETE`, and `DELETE/INSERT WHERE` into ordered SQL IR effects;
evaluate WHERE once against its input snapshot, then apply deletes and inserts
atomically. Reject unmapped predicates, ambiguous destinations, and read-only
query mappings before effects. Do not create a generic triple/property table as
a fallback. RDF quad-table mappings and ontology-to-relational mappings need
explicit writable-source contracts, including named-graph ownership and key
allocation. Conformance fixtures must declare one fixed mapping configuration.

Graph-management operations (`CREATE`, `CLEAR`, `DROP`, `COPY`, `MOVE`, `ADD`)
also need declared graph ownership and an empty-graph registry; row existence
alone cannot represent an empty named graph. `LOAD` and `SERVICE` require an
explicit resource resolver so local fixtures and production network access have
the same execution contract.

## Comparison and starting evidence

- W3C SPARQL 1.0/1.1, pinned revision
  `369a90d1a60c021b746df2e411da0ff36258a758`: **1,125 scenarios**.
- Fresh starting Crabgraph run on 2026-09-25: **732 pass, 90 fail,
  149 unsupported, 80 skipped, 74 not applicable**.
- Peer: **Apache Jena 6.2.0 / TDB2**, Apache 2.0 license. It executes the same
  source fixtures and expected artifacts. Query and update syntax outcomes are
  distinct from execution evidence; no parser-only pass implies working writes.
- Keep result literals, duplicate solutions, and blank-node identity intact.
  Investigate adapter differences for either engine before attributing failures
  to engine behavior. A shared file-URI base must cover queries, fixture data,
  named graph identifiers, and expected results.

## Work groups, in implementation order

1. **Adapter fidelity and peer evidence.** Correct relocated fixture bases, add
   Jena query/update transport, and compare entire update datasets. Preserve all
   pinned scenario IDs, expectations, and recorded non-passing outcomes.
2. **Scalar functions.** URI encoding, SHA-384/512, timezone/NOW, regex flags and
   replacement groups; keep language errors local to the expression.
3. **Numeric and temporal terms.** Decimal precision, numeric promotion, casts,
   lexical serialization, fractional seconds, and timezone comparison.
4. **Solution algebra.** Grouping errors, OPTIONAL/FILTER placement, correlated
   EXISTS, ordering and slicing through subqueries, empty binding projections.
5. **Dataset and path semantics.** FROM/FROM NAMED, graph scopes, zero-length
   paths, merged default graphs, and blank-node identity in CONSTRUCT.
6. **Mapped updates.** Introduce the writable mapping/SQL IR effect contracts
   above, then implement data updates and WHERE-driven updates together with
   multi-table rollback and read-after-write tests.
7. **Named graph lifecycle and bulk operations.** Registered empty graphs,
   clear/drop/copy/move/add, and resolver-backed load operations.
8. **Federation and entailment.** Install upstream SERVICE fixtures and explicit
   reasoning profiles; do not simulate expected results in the adapter.
9. **Protocol and serialization.** Exercise HTTP and TSV/CSV scenarios through
   their actual interfaces when provided; keep embedded API scope explicit.
10. **Final evidence.** Run complete suites locally on a committed build,
    preserving Cypher 3,897/3,897 and Gremlin 1,511/1,511. Publish measured JSON
    through the existing static-site action, with one Crabgraph result per case.

Batch implementation changes before full sweeps. Focused regressions validate
new contracts; full suites run in the background after each substantial batch.
