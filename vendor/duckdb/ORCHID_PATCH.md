Vendored from crates.io duckdb 1.10502.0 (duckdb/duckdb-rs).
Upstream revision: `8aa176add468ce2f7ada2d9bfd46f4c13509f198`.
`LICENSE` is copied from that revision's repository root (the crate archive omits it).
Original license and source are retained. Orchid patch adds
`ArrowStream::next_result` and `RawStatement::streaming_step_result` to propagate
late DuckDB execution and Arrow conversion errors instead of returning EOF.
Legacy iterator behavior is unchanged for compatibility; Orchid engine execution
uses the fallible method. Remove this patch when the upstream driver exposes an
equivalent fallible streaming API.

Arrow input nested types: recursively construct Map key/value logical types, preserve Union member types, and write dense/sparse unions using DuckDB's tag/member vectors. Struct/list writers recursively dispatch through the same converter so nested maps and unions remain typed. Inactive union members are null; Arrow Null children use nullable INTEGER storage. Integration coverage: `nested_map_union_preserves_members_offsets_and_nulls` in `tests/duckdb_streaming_session.rs`.
