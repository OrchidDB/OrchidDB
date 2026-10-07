# OrchidDB native compiler

Source builds use the shared core in this monorepo. See [build and test instructions](../README.md).

A shared C ABI for the OrchidDB graph-to-SQL compiler. Python, JavaScript,
Elixir and C++ clients can load this library without a DuckDB dependency.
Java retains its existing JNI compiler binding; Rust uses the Rust API.

The contract is **query + schema/mapping metadata → SQL + output field names**.
Database connections remain application-owned. Compilation returns SQL and
plan diagnostics. Explicit statistics generation accepts bounded Arrow IPC
samples from the application session; the shared Rust coordinator collects and
retains a reusable statistics snapshot. Dialect support and read-only compiler limitations are
in the [compiler guide](../../README.md#standalone-compilation-and-engine-adapters).

## Build

From the repository root, run `make native` to build and stage the shared library.
Requires Rust with edition 2024 support and a C toolchain. Builds compile
DataFusion but do not compile DuckDB. The library is in `target/debug/` (or the
chosen `CARGO_TARGET_DIR`); use `liborchiddb_compiler.dylib` on macOS and
`liborchiddb_compiler.so` on Linux. No runtime downloads are performed.

## ABI 1

The [header](include/orchiddb.h) declares:

```c
uint32_t orchiddb_abi_version(void);
const char *orchiddb_version(void);
const char *orchiddb_core_revision(void);
char *orchiddb_compile_json(const char *input);
char *orchiddb_bind_arrow_json(const char *input, struct ArrowArrayStream *stream);
char *orchiddb_statistics_json(const char *input);
void orchiddb_string_free(char *response);
```

`input` is a NUL-terminated UTF-8 JSON compiler request, protocol version 1.
The result is an owned UTF-8 JSON envelope:

```json
{"ok":true,"result":{"version":1,"dialect":"duckdb","sql":"SELECT 42 AS answer","fields":["answer"]}}
```

Errors use `{"ok":false,"error":"..."}`. Free every response exactly once with
`orchiddb_string_free`; that function accepts null. Never free borrowed version
or revision strings. Invalid raw pointers and double frees are caller errors.
Statistics submissions contain sampled source data; compilation requests may
contain sensitive parameter values. Treat both as application data.

Mixed-engine compilation returns SQL islands in `transfers`. The `bind` JSON
operation accepts `plan`, `relation`, and either typed `rows` or base64 Arrow
`ipc`. `orchiddb_bind_arrow_json` instead accepts `plan` and `relation` with a live
Arrow C stream. With non-null arguments it consumes the stream, including on
errors. Both interfaces bind values into query-scoped CTEs and execute no SQL.
See the [SQL engines guide](../../README.md#standalone-compilation-and-engine-adapters).

Calls may be concurrent. A process-wide two-worker runtime gives recursive
planning a 16 MiB native stack. Calls synchronously wait for compilation; language
bindings must schedule them appropriately (for example an Elixir dirty scheduler).
Admission limits/cancellation belong to the embedding application. Process abort
and allocation failure cannot be recovered across this ABI.

## Release

`CORE_REVISION` is generated from the actual monorepo build. Tagged `vX.Y.Z` releases must match
Cargo's version. Build locally and use `scripts/package.py` for the selected
validated Linux or macOS library. Publish finished assets to the main repository.
Windows is excluded. Clean pinned source
is mandatory under `ORCHIDDB_RELEASE_BUILD=1`; development builds report `-dirty`
when appropriate instead of claiming clean provenance.

Each `orchiddb-compiler-vX.Y.Z-TARGET.tar.gz` contains `lib/`, `include/orchiddb.h`,
`LICENSE.md` and `manifest.json` (ABI, package version, core revision, target,
library name and library checksum). `SHA256SUMS` covers all archives. Clients
should package the matching library into their own platform artifacts or use an
explicit local-library override. Source pins and checksum validation must match;
ABI compatibility alone does not establish a release's provenance.

This is a native runtime artifact, not a crates.io package. The local publisher
uploads the release assets; client registries require their own publishing
credentials or trusted-publisher configuration. No release is published merely
by pushing the source repository.

See [LICENSE.md](LICENSE.md) for the applicable license.

## One-time statistics

`orchiddb_statistics_json` uses the same UTF-8 envelope and string ownership as
compilation. Send `begin` with the normal request metadata, then execute each
returned collection request through the existing application session. Submit
bounded samples as base64 Arrow IPC (`ipc`) or row objects (`rows`). Batch
submissions use `done:false`; a final submission completes the request. Report
execution failures with `error` so the coordinator can record partial coverage.
Honor every request's timeout, row, and byte limits; unsupported adapters must
report an error rather than execute an unbounded fallback.

`finish` returns a portable `snapshot`, a retained `catalog_id`, and a coverage
`report`. Use `compile` with this handle and normal query request to avoid
repeated snapshot serialization. `install` restores a saved snapshot; `release`
frees a catalog; `cancel` discards unfinished collection. The coordinator owns
statistics algorithms, collection planning, and budgets. Clients own database
execution. There are no collection tiers or background refreshes.
