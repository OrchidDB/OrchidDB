# Gremlin runtime semantics

These contracts describe native graph execution. They do not expand the read-only SQL compiler API.

## Gremlin file import

`g.io(path).read()` is a graph write. It returns no rows and appends the file's vertices and edges to the current graph. Managed `GraphEngine` queries use the same persistence and statement rollback path as other mutations. Imports inside an explicit transaction are visible to subsequent queries and are undone by rollback.

The `.json`/`.graphson`, `.xml`/`.graphml`, and `.kryo`/`.gryo` extensions select GraphSON, GraphML, and Gryo respectively. Use `.with(IO.reader, IO.graphson)`, `.with(IO.reader, IO.graphml)`, or `.with(IO.reader, IO.gryo)` to override the extension. Paths are local to the engine process. The conformance adapter resolves upstream data paths before sending a query.

The native GraphSON reader accepts the TinkerPop adjacency-list format (one JSON vertex object per record). It preserves declared numeric widths and validates the complete input before graph writes. Unknown typed values, duplicate IDs, missing endpoints, invalid JSON, and unsupported readers produce errors. Imported edge records are read from `outE`; the corresponding `inE` copies do not create duplicate edges.

GraphML and Gryo use the pinned TinkerPop 3.7.4 Java readers in the production [`jvm-codecs`](../jvm-codecs/README.md) module. The codec decodes a file into GraphSON; Rust validates and applies the resulting records through the graph transaction. It does not replace the engine graph or provide query results.

For standalone engine use, compile the codec with the adapter's Maven dependencies and set:

- `ORCHIDDB_GREMLIN_IO_JAVA`: the Java executable (defaults to `java`).
- `ORCHIDDB_GREMLIN_IO_CLASSPATH`: a Java classpath containing `io.orchiddb.gremlin.codec.ImportGraph` and TinkerPop's dependencies.

The local conformance Java bridge supplies both variables using its current JVM and classpath. Java codec failures are returned as import errors. The engine does not invoke a shell to run the codec.

`io(path)` requires exactly one of `read()` or `write()`.

### File export

`g.io(path).write()` reads a private native graph snapshot and returns no rows. The same extensions select the writer; `.with(IO.writer, IO.graphson)`, `.with(IO.writer, IO.graphml)`, or `.with(IO.writer, IO.gryo)` overrides selection. Options may follow `write()`. Export is allowed by `ReadOnlyStrategy` because it does not mutate the graph.

GraphSON is written natively in the TinkerPop 3.7.4 GraphSON 3 adjacency format. It preserves public vertex/edge/property IDs, labels, both edge directions, property cardinality, meta-properties, numeric widths, arbitrary precision numbers, lists, sets and typed maps. Unsupported native value kinds produce an error instead of being converted to strings. A valid empty graph has an empty GraphSON file.

Gryo and GraphML use `ExportGraph` on the same codec classpath as `ImportGraph`. The Java codec reads the actual native GraphSON snapshot solely to convert its file representation. It does not execute traversals or supply graph query results. GraphML requires single boolean/string/Int32/Int64/Float/Double properties, one scalar type per key, and rejects nulls, collections, multi-properties and meta-properties. GraphML IDs become strings and vertex-property IDs are not represented. Choose GraphSON or Gryo when those identities/types must survive.

The writer stages files in the destination directory, flushes completed output, then atomically renames the output over the destination. Serialization, codec and rename failures leave an existing destination intact and remove staging files. The parent directory must exist. Exported files reflect graph state visible at invocation, including uncommitted graph changes; subsequent transaction rollback does not remove an already exported file. Rename completion is the file visibility contract; this does not promise directory-entry durability across a machine crash.

Supplemental Rust tests are in `tests/gremlin_export.rs`. The ignored codec test must be run explicitly with the pinned classpath configured; it checks all six default/explicit writer choices using the independent upstream reader and the native importer. These executable round trips are separate from upstream Gherkin writer placeholders.

Null values in GraphSON and Gryo retain presence when importing into a graph configured with `enable_null_property_values(true)`. The codec emits its adjacency envelope directly with the pinned typed mapper because the upstream 3.7.4 GraphSON graph writer cannot serialize null edge/meta-property values.


## Native Gremlin null-valued properties

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


## Native traversal-produced cardinality values

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


## Native Gremlin semantic backlog evidence

These changes execute in the Rust traversal interpreter over OrchidDB storage.
The supplemental tests do not alter the pinned Gherkin catalog or its expected
answers. Reference probes used TinkerPop 3.7.4, revision
`fa698ba2aba8967dcd17eb61cb13648b934fab5b`, to establish behavior; native execution
does not consult a reference engine.

### Implemented behavior

- `project()` cycles its `by()` traversal ring across keys, resets for each
  traverser, consumes surplus modulators, and preserves productive null versus
  an unproductive child.
- Named `groupCount()` starts from the registered typed map. Its long counts
  and typed keys survive empty input, repeat accumulation, cache invalidation,
  `select()` and repeated `cap()` calls without adding the seed twice.
- Direct `select(...).by('property')` uses case-sensitive `Map.get()` semantics
  for ordinary and typed maps, including productive null for an absent key.
  Element reads preserve a present null property and drop a truly absent
  property. Consequently a downstream range counts a missing-map-key null,
  while continuing past a vertex whose property is absent.
- Bounded lazy aggregation consumes through filters, projections, pure
  per-traverser children, nonbarrier choices and native mutations. Whole-stream
  barriers still consume their inputs. The range boundary respects movable
  scalar/side-effect stages and the upstream range step's extra input request.
- Choices without barriers emit in incoming traverser order. Options containing
  barriers retain whole-option input streams.
- Named groups whose first reducing barrier is `count()` or `fold()` support
  post-reduction writers and transformations. `select()` reads the pending
  barrier map. Each explicit `cap()` executes the finalizer once per key and
  installs its result as the seed for subsequent contributions. Cached reads
  and cache invalidation do not execute finalizer writers. Errors preserve the
  engine's transaction rollback behavior.

### Upstream lifecycle details

The implementation follows `ProjectStep.map`, `BranchStep.standardAlgorithm`,
`EarlyLimitStrategy`, `RangeGlobalStep.filter`, `GroupSideEffectStep`,
`Grouping.doFinalReduction`, and `SideEffectCapStep.supply` in the pinned source.

For `g.inject(1,2).group('m').by(__.constant('k'))`
`.by(__.count().sideEffect(__.addV('done')))`, the upstream and native profiles
agree on these graph mutations:

| Final steps | Created vertices |
| --- | ---: |
| `select('m')` | 0 |
| `cap('m')` | 1 |
| `cap('m').cap('m')` | 2 |
| `select('m').cap('m').select('m')` | 1 |

Two explicit caps therefore intentionally execute two finalizations; preventing
that would contradict the upstream behavior. A cap that publishes `7L` followed
by another count contribution yields `8L`, rather than recounting the original
input. A cap that publishes a vertex causes a type error if later count
contributions try to merge into that vertex.

For `g.inject(1,2,3,4,5).store('x').is(P.gt(2)).limit(1).cap('x')`, upstream
consumes `[1,2,3,4]`: the range requests one extra traverser before stopping.
In contrast, `g.inject(1,2,3).store('x').limit(1).cap('x')` returns `[1]`, because
the upstream early-limit strategy moves the range before the store.

### Local validation

Regression sources:

- `tests/gremlin_semantic_backlog.rs`: cyclic modulators, typed seeds, empty
  streams, repeated publication, filtered/ranged lazy consumption, choices,
  graph writes, unproductive projections and map/element null productivity.
- `tests/gremlin_group_finalization.rs`: pending reads, explicit repeated caps,
  prefix versus suffix writes, published reducer seeds, fold accumulation,
  native element results and rollback.
- Existing `gremlin_correlated_groups`, `gremlin_shared_side_effects`,
  `gremlin_upstream_regressions`, and `gremlin_where_choice_partition` suites.

Run all six targets locally with:

```sh
cargo test --test gremlin_semantic_backlog --test gremlin_group_finalization \
  --test gremlin_correlated_groups --test gremlin_shared_side_effects \
  --test gremlin_upstream_regressions --test gremlin_where_choice_partition
```

### Remaining boundaries

This is not a general pull-based evaluator. Bounded pipelines stop at unsupported
stateful child operators and barriers. Lowering can erase strategy-ordering
details: for example, an explicit `identity()` between a store and `limit(0)`
prevents upstream early-limit movement before that identity is subsequently
removed. Such strategy fences and arbitrary stateful repeat/branch scheduling
still require broader work.

Post-barrier writers with first barriers other than count/fold remain explicitly
unsupported until their intermediate merge-state contracts are represented.
A finalizer cannot recursively finalize or update its own named group. These
exclusions remain errors, rather than falling back to reference answers.
