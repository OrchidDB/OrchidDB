# Development and release work

- Run builds, tests, documentation builds, and packaging locally. Never dispatch
  GitHub Actions builds or tests. GitHub is only a publication destination.
- OrchidDB is a Cypher lowering and Arrow execution library with Rust and Go
  bindings. Built-in SQL sources are PostgreSQL, DuckDB, and Trino. DuckDB is the sole
  completion engine; source pushdown can eliminate its work entirely. Keep shared Graph IR, lowering, and kernels
  separate from source dialects and engine-extension rewrite hooks.
- The optional DuckDB extension is a separate scalar-UDF module. Do not put
  query execution, parser hooks, schema registration, or catalog management in
  the extension. Keep SQL lowering internal to the library.
- Customer APIs register schema separately from query text and parameters and
  return execution results. Do not add public compile-only APIs or combined
  query/schema request documents. JSON/YAML schema configuration is allowed.
- Use the root `Makefile` and the root README for local extension packaging.
  Keep extension packaging separate from default library use.
- Preserve completed platform artifacts and caches. Retry only failed work.
- Artifacts are specific to DuckDB version and platform. Validate each target
  before publishing it. Do not build or package Windows artifacts.
- Never move published tags or replace a published artifact with different bytes.
- Keep product documentation and terminal usage examples in the root README.
  Use Rust and Go library examples; the DuckDB shell exercises scalar UDFs only.
  Keep tests independent of usage examples. Compiler probes and internal
  diagnostics belong in `tools/diagnostics/`, with generated reports under
  `target/`; do not present them as customer examples.
- All language clients and native bindings live in `clients/`. Keep their core
  dependencies local to this repository. The landing page and brand repositories
  remain separate; language reference PDFs live in the parent workspace's `specs/`.
