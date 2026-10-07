# Development and release work

- Run builds, tests, documentation builds, and packaging locally. Never dispatch
  GitHub Actions builds or tests. GitHub is only a publication destination.
- OrchidDB is an engine-independent graph compiler and runtime. Keep language
  frontends, Graph IR, relational lowering, kernels, codecs, and JVM modules
  shared. SQL dialects, function catalogs, and execution belong to engine adapters.
- The DuckDB extension is an optional host of the shared core, not a requirement
  for default library compilation. Preserve its caller-owned catalog and session
  integration alongside other engine adapters. Keep SQL lowering internal.
- Customer APIs register schema separately from query text and parameters and
  return execution results. Do not add public compile-only APIs or combined
  query/schema request documents. JSON/YAML schema configuration is allowed.
- Use the root `Makefile` and the root README for local extension packaging.
  Keep extension packaging separate from default library use.
- Preserve completed platform artifacts and caches. Retry only failed work.
- Artifacts are specific to DuckDB version and platform. Validate each target
  before publishing it. Do not build or package Windows artifacts.
- Never move published tags or replace a published artifact with different bytes.
- Keep product documentation in the root README. `examples/` is for runnable,
  customer-facing usage examples. Compiler probes and internal diagnostics belong
  in `tools/diagnostics/`; do not present them as customer examples.
- All language clients and native bindings live in `clients/`; the CLI lives in
  `cli/`. Keep their core dependencies local to this repository. The landing page
  and brand repositories remain separate.
