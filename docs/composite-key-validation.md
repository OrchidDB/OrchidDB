# Composite-key validation

Validated locally on 2026-09-27 against `389c9edb`, the composite-key patch, and
the compiler fixes in the working tree. Builds use the development profile;
these results are correctness checks, not release performance measurements.
[Machine-readable evidence](composite-key-validation.json) records binary hashes,
source identity, and full local result paths.

## Fixes

DuckDB SQL emission now preserves struct, binary, and time casts through the
existing typed SQL cast adapter. This fixes composite endpoint joins,
heterogeneous-label scans, mixed unsigned identities, and binary/time keys.
Gremlin identity filters compare the appropriate typed variants directly,
including string-form ID filters and negation, without requiring native ID
projection first. Existing invalid-key validation retains its assertions with
the updated component-specific diagnostic.

`tests/sql_compiler_composite_keys.rs` executes Cypher and Gremlin FK joins with
repeated numeric IDs across tenants and partially NULL foreign keys. Additional
identity regressions verify numeric/string distinctions, multiple requested IDs,
and negation. No failing assertion was removed or skipped.

## Conformance and core checks

| Upstream suite | Passed | Failed | Other outcomes |
| --- | ---: | ---: | --- |
| openCypher | 3,897 | 0 | None |
| TinkerPop | 1,511 | 0 | None |
| RDF | 974 | 0 | 74 not applicable, 77 skipped |

All 6,382 passes match the committed baseline. Complete case-ID sets, case hashes,
and outcomes are unchanged. The suites use freshly rebuilt `target/debug`
executables and local socket access for Java fixture lookups. Historical release
artifacts were not overwritten.

All **63 targeted core tests pass**, covering compiler identities and traversals,
composite FK joins, ordinary mappings, scalar-key validation, foreign-key
relationships, mapped writes, and the unified mapped runtime.

```sh
RUST_MIN_STACK=16777216 cargo test --features duckdb --no-fail-fast \
  --test sql_compiler --test sql_compiler_identities \
  --test sql_compiler_composite_keys --test sql_compiler_traversals \
  --test scalar_primary_keys --test mapped_writes \
  --test foreign_key_relationships --test byos_smoke --test unified_mapped_engine
```

## Clients and examples

All **93 client/integration test cases pass**:

| Suite | Passes |
| --- | ---: |
| Workspace compiler/caller-owned DuckDB | 13 |
| Java and Java Gremlin | 48 |
| Rust client | 13 |
| CLI, including Arrow and Iceberg | 4 |
| Native C ABI | 2 |
| Python | 6 |
| JavaScript | 3 |
| Elixir | 3 |
| C++ | 1 |

All six Java examples, the Rust borrowed-DuckDB example, Python people example,
JavaScript DuckDB-Wasm example, and both Elixir examples run successfully. The
C++ integration executable is also its standalone example source. CLI tests
exercise its sample mapping/setup and real Iceberg loading. The core composite
runtime example and downloadable mapped-graph tutorial also run. The tutorial
returns `alice | 120.0` and `carol | 500.0` as documented.

The Python composite-client check also passes for both Cypher and Gremlin against
the rebuilt native compiler: tenant keys remain separate and partially NULL FKs
produce no relationships.

Validation explicitly selects rebuilt local compilers. Temporary Rust/CLI
manifests point at sibling sources; temporary Python/Elixir copies carry matching
development provenance pins; the JavaScript example imports the local built
client. Java uses the rebuilt JNI library and local class files. Published
package pins and sibling example sources were not changed. Client logs and
reproduction harnesses are under `../target/composite-validation/`.

## Documentation

The compiler reference and client guide describe ordered key arrays, FK ownership,
NULL behavior, and source-build usage. The temporary failure notes were removed
after the affected queries passed. The mdBook build and link/search checker pass;
the downloadable mapped-graph example is compiled and executed.

Core logs and complete conformance artifacts are under
`target/composite-validation/`. Nothing was published during validation.
