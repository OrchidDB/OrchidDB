# Development and release work

- Run builds, tests, documentation builds, and packaging locally. Never dispatch
  GitHub Actions builds or tests. GitHub is only a publication destination.
- The product is the DuckDB extension. Reuse the existing compiler, kernels,
  codecs, and JVM modules; do not create parallel language implementations.
- Use `scripts/release/Makefile` and the root README for local extension packaging.
- Preserve completed platform artifacts and caches. Retry only failed work.
- Artifacts are specific to DuckDB version and platform. Validate each target
  before publishing it. Do not build or package Windows artifacts.
- Never move published tags or replace a published artifact with different bytes.
- Keep product documentation in the root README and runnable DuckDB examples in
  `examples/`. Compiler probes belong in `tools/diagnostics/`.
- Retire obsolete client/CLI repositories only when explicitly authorized; the
  main extension, landing page, and brand repositories remain separate.
