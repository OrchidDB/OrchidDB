# Native Gremlin null-valued properties

`PropertyGraph` supports an explicit null-valued-property feature. The default
is disabled, preserving Gremlin's existing null-as-removal profile. Enable the
feature before loading or writing a null-capable graph:

```rust
let graph = PropertyGraph::new();
graph.enable_null_property_values(true);
```

`supports_null_property_values()` reports the setting. The native conformance
runner accepts `allow_null_property_values: true` on `fixture` and `reset`
requests; the caller selects the provider profile. Supporting this profile does
not change the expectations of upstream `@DisallowNullPropertyValues` scenarios.

Use `set_vertex_property` for vertex cardinality and metadata, or
`set_gremlin_property` for ordinary vertex, edge, and meta-property writes.
These methods honor the setting. `set_property` retains scalar-language
null-as-removal semantics. `remove_property` explicitly removes a vertex
property record, edge property, or meta-property, regardless of the setting.

With the feature enabled, property presence is independent of its value:

- `has('x')` and `has('x', null)` match a stored null; `hasNot('x')` does not.
- `values('x')` emits a null traverser for a stored null and emits nothing for
  absence. `properties()`, `valueMap()`, metadata reads, and merge predicates
  retain that distinction.
- List cardinality can contain repeated nulls. Set cardinality reuses the
  existing null record; single cardinality replaces prior records.
- Full snapshots, incremental entity records, graph checkpoints, and durable
  transaction rollback preserve presence and the configured write policy.
  Existing snapshots without the feature sections default to disabled.
- Null overrides on Arrow-backed graph rows shadow the base value without
  assigning a property to other rows with a null column cell.

The feature is implemented in native Rust storage and traversal execution. It
is available to a JVM provider through these same graph APIs; a JVM traversal
run remains a separately identified execution profile.

Local regression evidence lives in `tests/gremlin_null_properties.rs` (including
DuckDB reopen/commit/rollback), `tests/gremlin_native_properties.rs`, and the
catalog's `null_presence_incremental_replay_and_removal` unit test. These are
supplemental regressions, separate from the unchanged five upstream scenarios
requiring `@AllowNullPropertyValues`.
