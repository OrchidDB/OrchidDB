# Upstream provenance and local patch

Vendored from crates.io `datafusion-functions-nested` version **53.1.0**.

- Upstream repository: https://github.com/apache/datafusion
- Published crate: https://crates.io/crates/datafusion-functions-nested/53.1.0
- Published crate SHA-256: `ef13a858e20d50f0a9bb5e96e7ac82b4e7597f247515bccca4fdd2992df0212a`
- `Cargo.toml.orig`, `LICENSE.txt`, and `NOTICE.txt` are retained from upstream.

The active `Cargo.toml` removes the `datafusion-execution` dependency.
`src/lib.rs` imports `FunctionRegistry` (and its documentation example, when
present) directly from `datafusion_expr::registry`, which owns the same trait.
No function implementation, signature, coercion, optimizer metadata, or runtime
behavior is changed. The redundant upstream lockfile and Cargo registry metadata
are omitted; dependency resolution belongs to the consuming workspace.

The scalar and aggregate function dependencies use adjacent vendored packages
with exact 53.1.0 versions. These explicit paths preserve the compiler-only
graph for downstream workspaces without relying on root Cargo patches.
