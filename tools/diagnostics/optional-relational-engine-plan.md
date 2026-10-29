# Source-first execution design, superseded

The current design and audit are consolidated in
[Cypher SQL lowering audit](sql-lowering-audit.md), particularly L31–L32.
DuckDB is the sole completion engine. Source pushdown and narrow Arrow projections
can eliminate all completion work, including DuckDB session initialization.

The previous engine-selection and multi-client plan is retired. Rust and Go are
the supported bindings; the separate DuckDB extension registers scalar UDFs only.
See [execution testing scope](execution-testing-and-scope-plan.md) for remaining
integration-test proposals and the audit for actual validation evidence.
