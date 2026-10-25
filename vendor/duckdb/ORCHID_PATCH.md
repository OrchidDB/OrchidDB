Vendored from crates.io duckdb 1.10502.0 (duckdb/duckdb-rs).
The opt-in `download` feature forwards to the patched sys crate's library downloader.
The sys dependency uses an explicit sibling path so external path consumers also
receive the patched driver and do not depend on workspace-only Cargo overrides.
Upstream revision: `8aa176add468ce2f7ada2d9bfd46f4c13509f198`.
`LICENSE` is copied from that revision's repository root (the crate archive omits it).
Original license and source are retained. Orchid patch adds
`ArrowStream::next_result` and `RawStatement::streaming_step_result` to propagate
late DuckDB execution and Arrow conversion errors instead of returning EOF.
Legacy iterator behavior is unchanged for compatibility; Orchid engine execution
uses the fallible method. Remove this patch when the upstream driver exposes an
equivalent fallible streaming API.

Arrow input nested types: recursively construct Map key/value logical types, preserve Union member types, and write dense/sparse unions using DuckDB's tag/member vectors. Struct/list writers recursively dispatch through the same converter so nested maps and unions remain typed. Inactive union members are null; Arrow Null children use nullable INTEGER storage. Integration coverage: `nested_map_union_preserves_members_offsets_and_nulls` in `tests/duckdb_streaming_session.rs`.

Dependency scope: the driver no longer enables Arrow pretty printing or declares an unused direct `comfy-table` dependency. Pretty printing is retained only as a driver dev dependency for its standalone examples. The introductory example prints Arrow batches with `Debug`; Arrow FFI and the streaming/scalar adapters remain enabled as before.
