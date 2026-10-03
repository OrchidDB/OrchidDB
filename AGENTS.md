# Release work

- Run all runtime tests locally. GitHub Actions may only build, package, sign,
  and publish artifacts. Never add or dispatch GitHub test jobs.
- Use the coordinated process in `OrchidDB/scripts/release/README.md`
  (the sibling `orchiddb` checkout in a multi-repository workspace).
- Dispatch audited workflows from `main`, passing the immutable source tag as
  an input. Historical tag workflows can contain obsolete test steps.
- Preserve completed platform artifacts. Retry only missing or failed work;
  do not cancel successful builds or restart an entire release matrix.
- Python, JavaScript, and C++ packages reuse the shared native compiler.
  Do not rebuild that compiler separately for each client.
- Keep Linux x86_64, macOS ARM64, and macOS x86_64 artifacts where supported.
  Preserve existing Windows support for native, Python, and Java packages.
- Never move published tags or replace a published package with different bytes.
