# Native traversal-produced cardinality values

Native Rust merge execution supports `single(value)`, `list(value)`, and
`set(value)` wrappers inside traversal-produced `Merge.onMatch` and
`Merge.onCreate` maps. Qualified `Cardinality.single/list/set` forms work too.
For example:

```groovy
g.mergeV(['name':'a']).option(Merge.onMatch,
    constant(['reading':list(2)]))
```

The parser constructs a dedicated `GValue::CardinalityValue`; lowering and
execution preserve it as `Value::CardinalityValue`. It remains distinct from
an ordinary map with `cardinality` and `value` keys. Maps can arrive from
`select()`, side effects, or branches, so different input rows can supply
different cardinalities. Merge consumes the wrapper and stores its payload
using the requested native vertex-property cardinality. List-valued payloads
remain one property value, and null payloads honor the null-property profile.

Wrappers are execution values rather than ordinary graph property data.
Property, meta-property, and edge setters reject them, including wrappers
nested inside containers. Edge merge does not accept vertex cardinality.
A wrapper payload that is a nested traversal object is rejected explicitly;
a nested traversal is not evaluated as a cardinality payload.

The runtime type has explicit equality, hashing, output, and binary snapshot
encoding. The typed wire discriminator is `cardinality_value`; no ordinary map
shape acts as a sentinel. Existing literal merge option maps remain supported.

Supplemental native coverage is in `tests/gremlin_dynamic_cardinality.rs` and
`tests/gremlin_merge_validation.rs`, with representation/codec unit tests in
`src/ir/value.rs`, interpreter output, and snapshot encoding. This extends the
native semantic coverage beyond the pinned Gherkin scenario count; JVM-backed
provider tests remain separate execution evidence.
