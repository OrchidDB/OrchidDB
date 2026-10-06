# Ordinary Cypher retrieval

Implement parameterized retrieval with MATCH, WITH, ORDER BY, LIMIT and subsequent
graph traversal. Reuse the existing parameter codec, scoring UDFs, SQL templates,
mapped scans and session authorization. No retrieval declarations or procedures.

- Resolve two-argument BM25's document property back to its mapped vertex source,
  through aliases and intervening filters/joins/limits. Aggregate its authorized
  corpus independently of candidates, once per source row, using the existing
  computed-edge corpus construction. Reject expressions without source lineage.
- Keep vector scoring on the existing typed expression path, including nested
  token-matrix parameters. Preserve normal Cypher stage boundaries.
- Add end-to-end tests for parameter rebinding, hybrid candidates, MaxSim reranking,
  traversal, corpus stability across aliases/filters/joins/limits, empty/null data,
  invalid arguments, real SpiceDB identity/revocation and Iceberg/Lance sources.
- Document a complete DuckDB example in the README and runnable SQL in examples.
- Build locally, run targeted tests while implementing, then the extension suite
  with real storage and SpiceDB enabled. Commit only these changes and push main.

## Completed

- Reused scoring kernels, SQL templates, parameter binding, mapped endpoint scans,
  and computed-edge corpus aggregation. Added only corpus provenance resolution
  and preservation of typed mapped properties through node grouping.
- Added `examples/07_parameterized_rag.sql` and README parameterized usage.
- Local DuckDB v1.5.6 build succeeds. All 95 extension integration tests pass,
  including real SpiceDB v1.56.2 and Iceberg/Lance fixtures. All five extension
  compiler unit tests pass. Existing conformance records were not rerun.
- Final targeted retrieval checks also cover the vertex/edge label collision
  guard added during review. Retrieval uses exact scans; no indexes are provisioned.
