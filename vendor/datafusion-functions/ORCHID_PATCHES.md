# Upstream provenance and local patch

Vendored from crates.io `datafusion-functions` version **53.1.0**.

- Upstream repository: https://github.com/apache/datafusion
- Published crate: https://crates.io/crates/datafusion-functions/53.1.0
- Published crate SHA-256: `f28aa4e10384e782774b10e72aca4d93ef7b31aa653095d9d4536b0a3dbc51b6`
- `Cargo.toml.orig`, `LICENSE.txt`, and `NOTICE.txt` are retained from upstream.

The active `Cargo.toml` removes the `datafusion-execution` dependency.
`src/lib.rs` imports `FunctionRegistry` (and its documentation example, when
present) directly from `datafusion_expr::registry`, which owns the same trait.
No function implementation, signature, coercion, optimizer metadata, or runtime
behavior is changed. The redundant upstream lockfile and Cargo registry metadata
are omitted; dependency resolution belongs to the consuming workspace.
