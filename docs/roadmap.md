# Maintainer roadmap

This replaces historical handoffs and completion plans. Old failure counts are
not current bugs: use [committed evidence](verification.md) and reproduce the
relevant query before opening work.

- Expand standalone SQL compilation independently of managed-runtime conformance.
  Writes and opaque/residual operations currently fail. SPARQL SERVICE is
  unsupported across compiler and managed execution.
  Extend coverage with expected-result comparisons against caller-owned databases.
- Add Gremlin/SPARQL parameter binding and broader mapped identity/schema types.
  Preserve the [Postgres and DuckDB execution checks](sql-engines.md) as coverage
  grows. ClickHouse needs a dialect and capability contract before support.
- Measure the existing federation coordinator's buffered transfers on application
  workloads before extending execution. Cross-engine joins already use SQL islands
  and typed exchanges; distributed snapshots, distributed transactions, and
  streaming exchanges remain outside that contract.
- Separate the optional bundled runtime into its own crate if it needs an
  independent release lifecycle. Today the default dependency graph excludes its
  drivers, while the explicit `duckdb` feature preserves engine and CLI users.
- Address quarantined Cartesian/count-only corpus queries without materializing
  enormous intermediate joins; keep fixture omissions and memory limits separate
  from language failures. See `MEMORY_QUARANTINE` in the corpus runner.
- Extend [supplied relational constraints](relational-constraints.md) with nested-field
  metadata and broader proven containment rules. Keep validated constraints separate
  from estimates and preserve duplicate input occurrence identity.
- Measure SQL boundary costs and physical work. Assess correlated scalar/lateral
  plans, shared aggregates/windows and repeated subplans with null/error/effect
  equivalence tests. Temporal ASOF specialization needs actual query semantics,
  not merely timestamp columns. Retain the existing performance evidence files.
- Managed and mapped writes now share the execution runtime and transaction owner,
  including RDF row updates. Extend supported storage capabilities deliberately:
  mapped primary-key generation, volatile mapped defaults, and cyclic writes
  requiring deferred constraints remain unsupported. Collection expansions and
  physical-layout logical sources are read-only.

Detailed historical reasoning remains in Git history before this cleanup:
[374977f](https://github.com/OrchidDB/OrchidDB/tree/374977f985869f795ef628e2e37c566dafefe19a/docs).
