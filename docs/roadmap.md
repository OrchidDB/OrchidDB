# Maintainer roadmap

This replaces historical handoffs and completion plans. Old failure counts are
not current bugs: use [committed evidence](verification.md) and reproduce the
relevant query before opening work.

- Expand standalone SQL compilation independently of managed-runtime conformance.
  Writes, external SPARQL SERVICE and opaque/residual operations currently fail.
  Extend coverage with expected-result comparisons against caller-owned databases.
- Add Gremlin/SPARQL parameter binding, broader mapped identity/schema types and
  explicitly tested PostgreSQL execution. ClickHouse needs a dialect and capability
  contract before it can be advertised as a supported engine.
- Design federation around engine-specific fragments and typed exchanges; specify
  consistency, cancellation and partial-failure behavior before cross-engine joins.
- Separate the optional bundled runtime into its own crate if it needs an
  independent release lifecycle. Today the default dependency graph excludes its
  drivers, while the explicit `duckdb` feature preserves engine and CLI users.
- Address quarantined Cartesian/count-only corpus queries without materializing
  enormous intermediate joins; keep fixture omissions and memory limits separate
  from language failures. See `MEMORY_QUARANTINE` in the corpus runner.
- Improve metadata for unique keys, relationship multiplicity and nested types.
  Distinguish validated constraints from estimates; estimates cannot justify
  removing rows or errors. Preserve duplicate input occurrence identity.
- Measure SQL boundary costs and physical work. Assess correlated scalar/lateral
  plans, shared aggregates/windows and repeated subplans with null/error/effect
  equivalence tests. Temporal ASOF specialization needs actual query semantics,
  not merely timestamp columns. Retain the existing performance evidence files.
- Managed writes still use a native graph overlay and persisted records. Extending
  write-through to external mappings needs explicit mutation lowering and shared
  transaction ownership; the managed runtime does not imply general mapped writes.

Detailed historical reasoning remains in Git history before this cleanup:
[374977f](https://github.com/OrchidDB/OrchidDB/tree/374977f985869f795ef628e2e37c566dafefe19a/docs).
