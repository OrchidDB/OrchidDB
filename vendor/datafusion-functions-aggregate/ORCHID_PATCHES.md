# Upstream provenance and local patch

Vendored from crates.io `datafusion-functions-aggregate` version **53.1.0**.

- Upstream repository: https://github.com/apache/datafusion
- Published crate: https://crates.io/crates/datafusion-functions-aggregate/53.1.0
- Published crate SHA-256: `00aa6217e56098ba84e0a338176fe52f0a84cca398021512c6c8c5eff806d0ad`
- `Cargo.toml.orig`, `LICENSE.txt`, and `NOTICE.txt` are retained from upstream.

The active `Cargo.toml` removes the `datafusion-execution` dependency.
`src/lib.rs` imports `FunctionRegistry` (and its documentation example, when
present) directly from `datafusion_expr::registry`, which owns the same trait.
No function implementation, signature, coercion, optimizer metadata, or runtime
behavior is changed. The redundant upstream lockfile and Cargo registry metadata
are omitted; dependency resolution belongs to the consuming workspace.
