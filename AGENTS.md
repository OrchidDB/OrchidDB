# Release work

- Run all tests, builds, and packaging locally. Never dispatch GitHub Actions
  builds or tests. GitHub is only an upload/publication destination.
- Use the local release Makefile: `make release VERSION=X.Y.Z` from the workspace
  (or `make -f orchiddb/scripts/release/Makefile release VERSION=X.Y.Z`).
- Use the coordinated process in `OrchidDB/scripts/release/README.md`
  (the sibling `orchiddb` checkout in a multi-repository workspace).
- Build immutable source tags locally. Do not use historical remote-build
  workflow instructions or the legacy workflow-dispatch coordinator.
- Preserve completed local platform artifacts and caches. Retry only missing
  or failed work; never restart a complete release matrix.
- Python, JavaScript, and C++ packages reuse the shared native compiler.
  Do not rebuild that compiler separately for each client.
- Keep Linux x86_64, macOS ARM64, and macOS x86_64 artifacts where supported.
  Do not build or package Windows artifacts.
- Never move published tags or replace a published package with different bytes.
