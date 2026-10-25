# Upstream provenance and local patch

Vendored from crates.io `libduckdb-sys` version **1.10502.0**.

- Upstream repository: https://github.com/duckdb/duckdb-rs
- Published crate: https://crates.io/crates/libduckdb-sys/1.10502.0
- Published crate SHA-256: `8d7401630ae2abcff642f7156294289e50f2d222e061c026ad797b01bf20c215`
- Upstream `LICENSE`, `Cargo.toml.orig`, bundled `duckdb.tar.gz`, OpenSSL files,
  bindings, and source build backends are retained unchanged.

The explicit `download` feature enables the build-time `reqwest` and `zip`
dependencies. Its cfg-gated downloader preserves upstream cache, platform,
archive extraction, linking, and runtime library-copy behavior. Setting
`DUCKDB_DOWNLOAD_LIB=1` or `true` without that feature produces an actionable
build error only when library discovery reaches the download branch. Explicit
`DUCKDB_LIB_DIR` and bundled builds retain precedence and ignore the download
request. Enabling the feature alone does not download a library.

The archive/manifest dependencies `flate2`, `tar`, `serde`, and `serde_json`
are optional and activated by `bundled`. `bundled-cmake` continues to include
`bundled`, as upstream requires; existing feature implications are preserved.
Normal external-library builds do not compile these download/extraction build
dependencies. External `DUCKDB_LIB_DIR`, pkg-config, and system-library discovery
retain their upstream precedence.

The redundant upstream lockfile and Cargo registry metadata are omitted; the
consuming workspace owns dependency resolution.
