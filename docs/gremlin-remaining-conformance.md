# Remaining Gremlin conformance work

## Architectural changes first

These changes address shared causes rather than adding one-off scenario handling. They are an implementation plan, not a claim that a rewrite is necessary for every failure. Extend the existing Graph IR and interpreter where possible; keep language-specific semantics from changing Cypher/SPARQL behavior.

### A1. Persist native properties with identity and cardinality

**Required for full property conformance.** The current runtime constructs property-shaped maps (`element`, `key`, `value`, `__id`, `__order`); `property_order()` derives IDs from row position. That cannot reliably model independently addressable multi-properties, meta-properties or native VertexProperty output.

- Introduce property identity and ownership in storage and runtime values. Distinguish VertexProperty from edge/meta Property, ordinary Map and Map.Entry.
- Store multiple property records under one key. Implement `single` replacement, `list` append, `set` uniqueness and the provider-default cardinality; preserve a list-valued property as distinct from multiple properties.
- Support meta-property reads/writes/deletes and property removal without deleting the owner. Update indexes, snapshots and transactional rollback.
- Carry native property types/IDs through SQL boundaries, fixture loading, Arrow/native output and the Java adapter. Remove synthetic position-based property identity.
- Extend merge cardinality parsing and execution only after that storage contract exists.

**Primary work:** F02/F03; prerequisites for property ordering in F19 and 23 currently fixture-blocked cases. A scalar property-deletion fix can ship before the complete storage migration.

### A2. Make traversal state and barriers explicit throughout execution

**Required for the remaining interacting stateful traversals.** `Row` already has `bulk`, repeat has state frames, and `ExecutionContext` already has group-count state. Extend these mechanisms; do not add a second independent execution engine. Some side-effect lowering still reconstructs writer plans or stores projections in row bindings.

- Define one contract for current object, bulk, path objects plus label sets, loop stack, sack, and traversal-scoped side effects. Preserve it at SQL/interpreter and child-traversal boundaries.
- Give side effects shared storage with registered reducers. Separate side-effect lifetime from row lifetime; `cap()` reads completed state and does not replay mutations.
- Give child traversals explicit input/scope rules: per-traverser probes versus whole-group reductions, inherited label bindings versus isolated child path history.
- Implement barriers with bulk/sack merging, correct eager/lazy side-effect visibility, and exactly-once mutations.
- Bulk equivalent frontier entries where future-observable state permits. Retain distinct paths when downstream steps observe them. Stream or short-circuit existence probes when semantics permit.

**Primary work:** F06–F09 and F24; also reduces the four timeout risks. Profile before choosing a broader repeat execution redesign.

### A3. Preserve Gremlin types and centralize semantic operations

**Required cross-cutting contract; not a blanket replacement of shared Value semantics.** Use a Gremlin semantic layer for numeric promotion, equality, hashing, comparability, total ordering, collection kinds and reducer types. SQL lowering must prove it preserves that contract or keep the affected operation in the interpreter.

- Keep Byte/Short/Int/Long/Float/Double/BigInteger/BigDecimal distinctions in results. Fix decimal/double promotion using the pinned upstream contract, without rounding all numeric values to f64.
- Keep Set, BulkSet, List, Map, typed token keys, Map.Entry, Property, VertexProperty and Path distinct. Ordinary user maps must never gain engine semantics merely from their field names.
- Use equality-consistent hash/dedup rules while preserving output numeric types. Predicate errors/unknown and total sort order remain separate concepts.
- Carry heterogeneous and nested type metadata through relational operations and serialization. Preserve aggregate return widths even when nested inside group/cap.

**Primary work:** F01/F12/F19/F20/F22; supports A1 and option matching.

### A4. Separate public element IDs from internal storage addresses

**Required for supplied-ID creation and stable provider identity.** Keep internal row IDs for joins/storage, but add typed public-ID lookup/allocation for vertices and edges. Define uniqueness, collision behavior, generated IDs, endpoint resolution and snapshot persistence. Importers and fixtures must preserve IDs consistently. Do not derive public IDs from a label/row-position string when upstream requires an existing ID.

**Primary work:** F04; supports F15/F19/F23. Distinguish a real engine identity defect from a fixture mapping defect before changing storage.

### A5. Add real stateful/operator implementations where placeholders remain

**Required additions, not a global engine rewrite.** Seeded sampling needs per-step RNG state and weighted selection; import needs a transactional write operator plus format readers; search needs a graph-backed service rather than fixed row IDs. Route these through explicit IR operators/procedures and shared execution state so optimizations cannot discard or duplicate their effects.

**Primary work:** F10/F16/F23. The GraphComputer-only skipped profile would additionally require an OLAP/vertex-program execution contract; that is separate from finishing the currently executed OLTP failures.

## Scope and evidence

This plan covers **all 112 currently failing Gremlin cases: 108 failures and 4 timeouts**, not only cases that SQLg or PuppyGraph pass. It also inventories **all 106 skipped cases** separately. The 1,511-case pinned TinkerPop suite currently records **1,293 passes**. A skip is not a pass or proof that an engine operation is broken.

- Result artifact: [crabgraph-tinkerpop.json](../conformance/upstream-results/crabgraph-tinkerpop.json).
- Scenario catalog, exact fixture setup, parameters and assertions: [catalog.json](../conformance/upstream/catalog.json).
- Upstream: Apache TinkerPop **3.7.4**, revision `fa698ba2aba8967dcd17eb61cb13648b934fab5b`.
- Recorded local sweep: **2026-09-24 06:07:00–06:10:00 UTC**, with three subsequent fresh-process fixture replays retained in `attempts`.
- Source revision recorded by runner: `21d83285628fbdec441c960ce8a7eb7f08f0dc1a`; published result commit: `4510350`.
- The artifact records `working_tree_modified: true`; use its binary hash and exact artifact as the evidence identity, not an assertion that this was a clean checkout build.
- Binary SHA-256: `ee6164534fc645d789953244e1a4ce2928f06f2261f2f62ea7274bfcd1d361d2`.
- SQLg: 1,322 passes; PuppyGraph: 972. **68** cases are currently not passing in Crabgraph and pass in at least one of those peers. A total-score difference of 29 from SQLg does not imply only 29 missing behaviors.

The diagnoses below combine the committed run, exact catalog assertions, local engine code and pinned upstream source. “Confirmed” describes an observed implementation or type mismatch; other causes are explicitly hypotheses to verify. This document does not rerun tests or claim fixes have been made.

### Implementation order

1. Fix the 18 SubgraphStrategy regressions via F01, confirming attribution case by case. Also address compact fixes F05/F11/F12/F17/F20/F21/F22.
2. Implement A1 and A4 together with fixture/native transport support; then complete property/merge behavior and ordering.
3. Complete A2, then correlated reducers, paths, sacks and termination/performance. Avoid optimizing an incorrect state model.
4. Implement real sampling, search and graph import. Diagnose F14/F15 at their relevant execution/identity boundaries.
5. Resolve harness capability gaps and choose which additional skipped profiles to support. Keep OLTP, remote-lambda and GraphComputer results distinguishable.

Workstreams can overlap where ownership is separate. No deadline or pass gain is guaranteed by a group count: initialization failures can hide later failures.

## Remediation workstreams

Each failing case has exactly one primary assignment below; architectural dependencies may overlap. Per-case evidence follows the workstream list.


<a id="f01"></a>

### F01. Cross-type numeric comparison and SubgraphStrategy (18 cases)

**Evidence strength:** Conversion mismatch confirmed in source; attribution of all 18 cases remains a hypothesis.

**Change:** Keep exact BigDecimal literals in the transport. Align Gremlin floating-point/decimal promotion with pinned TinkerPop NumberHelper: its BigDecimal conversion uses BigDecimal.valueOf(double), while our equality/order paths use BigDecimal::from_f64. The latter preserves the binary approximation, so decimal 0.4 and double 0.4 can disagree. Centralize the Gremlin promotion rule across predicate equality, order, hash/dedup and backend boundaries. Trace these strategy predicates after that fix; do not assume every strategy failure has the same cause.

**Implementation locations:** [src/ir/value.rs](../src/ir/value.rs), [src/ir/interpreter/expr/compare.rs](../src/ir/interpreter/expr/compare.rs), [src/ir/interpreter/runtime/hashing.rs](../src/ir/interpreter/runtime/hashing.rs), [src/language/gremlin/planner/lowering/sources.rs](../src/language/gremlin/planner/lowering/sources.rs), [conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java](../conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java).

**Acceptance:** All 18 regressions pass with original decimal transport; add mixed numeric predicates on ordinary and strategy-filtered graphs, including 0.2/0.4, large integers, NaN and infinities. Retain the existing typed Equality passes.

**Cases:** `integrated/SubgraphStrategy.feature:200`, `integrated/SubgraphStrategy.feature:216`, `integrated/SubgraphStrategy.feature:232`, `integrated/SubgraphStrategy.feature:340`, `integrated/SubgraphStrategy.feature:441`, `integrated/SubgraphStrategy.feature:456`, `integrated/SubgraphStrategy.feature:485`, `integrated/SubgraphStrategy.feature:514`, `integrated/SubgraphStrategy.feature:530`, `integrated/SubgraphStrategy.feature:559`, `integrated/SubgraphStrategy.feature:745`, `integrated/SubgraphStrategy.feature:762`, `integrated/SubgraphStrategy.feature:778`, `integrated/SubgraphStrategy.feature:809`, `integrated/SubgraphStrategy.feature:840`, `integrated/SubgraphStrategy.feature:857`, `integrated/SubgraphStrategy.feature:888`, `integrated/SubgraphStrategy.feature:919`.

<a id="f02"></a>

### F02. First-class properties and cardinality (24 cases)

**Evidence strength:** Structural limitation confirmed by explicit rejection paths and map-based property representation.

**Change:** Implement A1 below end to end: property records with owner, identity, key, typed value and meta-properties; single/list/set cardinalities and provider-default cardinality. Repeated name writes must preserve the scenario-required values. Support cardinality wrappers inside merge option maps and the option(map, cardinality) overload, including precedence between per-key and default cardinality. Preserve vertex-property identity in fixture import, output and sort rather than returning ordinary maps or deriving colliding IDs. Cases blocked during graph initialization must be rerun through their later merge assertions.

**Implementation locations:** [src/ir/catalog/](../src/ir/catalog/), [src/ir/value.rs](../src/ir/value.rs), [src/ir/interpreter/runtime/property_object.rs](../src/ir/interpreter/runtime/property_object.rs), [src/ir/interpreter/runtime/mutations.rs](../src/ir/interpreter/runtime/mutations.rs), [src/language/gremlin/parser/mutations.rs](../src/language/gremlin/parser/mutations.rs), [src/language/gremlin/parser/merge.rs](../src/language/gremlin/parser/merge.rs), [src/language/gremlin/planner/lowering/merge.rs](../src/language/gremlin/planner/lowering/merge.rs), [conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java](../conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java).

**Acceptance:** All listed cases and the 23 fixture-blocked cases run; verify single replacement, list duplicates, set uniqueness, meta-property update/removal, unique property IDs, stable snapshot round trips, and correct VertexProperty transport.

**Cases:** `filter/Drop.feature:92`, `map/AddVertex.feature:150`, `map/AddVertex.feature:178`, `map/AddVertex.feature:206`, `map/AddVertex.feature:259`, `map/AddVertex.feature:287`, `map/AddVertex.feature:453`, `map/AddVertex.feature:471`, `map/MergeVertex.feature:401`, `map/MergeVertex.feature:515`, `map/MergeVertex.feature:567`, `map/MergeVertex.feature:621`, `map/MergeVertex.feature:642`, `map/MergeVertex.feature:830`, `map/MergeVertex.feature:848`, `map/MergeVertex.feature:866`, `map/MergeVertex.feature:884`, `map/MergeVertex.feature:902`, `map/MergeVertex.feature:920`, `map/MergeVertex.feature:939`, `map/MergeVertex.feature:973`, `map/MergeVertex.feature:987`, `semantics/Orderability.feature:44`, `semantics/Orderability.feature:67`.

<a id="f03"></a>

### F03. Property deletion (3 cases)

**Evidence strength:** Diagnosis to verify.

**Change:** Dispatch drop by native input kind. Property drop must remove the referenced vertex/edge property without deleting its owner, and work inside onMatch side effects. Use the property owner/key/identity to perform a transactional write and leave drop output empty. A1 supplies stable addressing for multi-properties; the current single-property failures can be fixed before that larger migration.

**Implementation locations:** [src/ir/interpreter/runtime/mutations.rs](../src/ir/interpreter/runtime/mutations.rs), [src/ir/interpreter/runtime/property_object.rs](../src/ir/interpreter/runtime/property_object.rs), [src/language/gremlin/planner/lowering/mutations.rs](../src/language/gremlin/planner/lowering/mutations.rs).

**Acceptance:** Observe the same graph after the write: vertices/edges remain, targeted properties are absent, unrelated properties remain, and rollback restores them.

**Cases:** `filter/Drop.feature:51`, `filter/Drop.feature:66`, `map/MergeEdge.feature:616`.

<a id="f04"></a>

### F04. User-supplied element identity (6 cases)

**Evidence strength:** Explicit allocator / supplied-ID rejection confirmed.

**Change:** Implement A4: separate user-visible T.id from internal row offsets, allocate/index supplied IDs transactionally and resolve existing IDs before deciding to create. Parse property(T.id, ...) as an element token on creation. Apply the same identity rules to merge criteria, onCreate options, edge endpoints and native element references.

**Implementation locations:** [src/ir/catalog/](../src/ir/catalog/), [src/ir/interpreter/element_id.rs](../src/ir/interpreter/element_id.rs), [src/ir/interpreter/runtime/mutations.rs](../src/ir/interpreter/runtime/mutations.rs), [src/language/gremlin/parser/mutations.rs](../src/language/gremlin/parser/mutations.rs), [src/language/gremlin/planner/lowering/merge.rs](../src/language/gremlin/planner/lowering/merge.rs).

**Acceptance:** Create/re-find vertices and edges by supplied IDs, match already existing IDs, reject collisions according to the provider contract, and persist identity across reloads.

**Cases:** `map/MergeEdge.feature:681`, `map/MergeEdge.feature:703`, `map/MergeEdge.feature:809`, `map/MergeVertex.feature:661`, `map/MergeVertex.feature:680`, `map/MergeVertex.feature:695`.

<a id="f05"></a>

### F05. Merge validation and onMatch execution (6 cases)

**Evidence strength:** Expected error contracts confirmed from catalog; side-effect failure needs runtime trace.

**Change:** Validate all merge/option maps at the upstream-required stage, including hidden keys in an onMatch map even when no match exists. Report the required missing-endpoint and conflicting onCreate diagnostics. Reject the illegal incoming Element shape in MergeEdge.feature:845. For :862, execute onMatch side effects against the matched edge even when the returned option map is empty; ensure one execution and persistence. Do not broadly permit or reject all Element inputs: use the exact pinned MergeEdgeStep contract.

**Implementation locations:** [src/language/gremlin/planner/lowering/merge.rs](../src/language/gremlin/planner/lowering/merge.rs), [src/ir/interpreter/runtime/mutations.rs](../src/ir/interpreter/runtime/mutations.rs), [src/ir/interpreter/run.rs](../src/ir/interpreter/run.rs).

**Acceptance:** Upstream negative tests receive their expected errors; :862 changes weight from 1 to 0; errors produce no partial writes. Validate conflicting T.id before allocator checks.

**Cases:** `map/MergeEdge.feature:323`, `map/MergeEdge.feature:334`, `map/MergeEdge.feature:830`, `map/MergeEdge.feature:845`, `map/MergeEdge.feature:862`, `map/MergeVertex.feature:722`.

<a id="f06"></a>

### F06. Traversal-scoped side effects (4 cases)

**Evidence strength:** Diagnosis to verify.

**Change:** Complete A2: shared typed side-effect state with reducer registration, correct eager/lazy barriers and lifecycle through repeat/child traversals. cap must read accumulated state after exhaustion; it must not reconstruct/re-execute a writer subtree. groupCount(x) must expose finalized counts at the correct barrier, including null keys. The integrated Paths case also needs path-label state and typed composite map keys; solve its shared-state prerequisites before tracing the whole traversal.

**Implementation locations:** [src/language/gremlin/planner/lowering/side_effects.rs](../src/language/gremlin/planner/lowering/side_effects.rs), [src/language/gremlin/planner/lowering/group.rs](../src/language/gremlin/planner/lowering/group.rs), [src/ir/interpreter/run.rs](../src/ir/interpreter/run.rs), [src/ir/interpreter/ops/repeat.rs](../src/ir/interpreter/ops/repeat.rs).

**Acceptance:** Two bags after addV contain the same three labels without extra writes; groupCount yields 3/2/1; repeat group cap retains all loop contributions; integrated Paths returns all 30 expected paths.

**Cases:** `integrated/Paths.feature:22`, `map/AddVertex.feature:570`, `sideEffect/Inject.feature:89`, `sideEffect/SideEffectCap.feature:67`.

<a id="f07"></a>

### F07. Repeat expansion, bulking and termination (4 cases)

**Evidence strength:** Timeouts observed; individual performance/termination causes require profiling.

**Change:** Profile loop frontiers, bulk, path retention, shared side effects and SQL/interpreter boundaries separately. For Count cases, combine equivalent traversers and propagate multiplicities instead of materializing every walk; only merge when future-observable labels/path/sack/state agree. For Where, first fix aggregate(e) visibility so without(e) actually prevents edge reuse. For Repeat, preserve nested loop counters and emit semantics; use an existence probe that can short-circuit where valid. Increasing deadlines is not a semantic fix.

**Implementation locations:** [src/ir/interpreter/ops/repeat.rs](../src/ir/interpreter/ops/repeat.rs), [src/ir/interpreter/ops/expand.rs](../src/ir/interpreter/ops/expand.rs), [src/ir/interpreter/ops/aggregate.rs](../src/ir/interpreter/ops/aggregate.rs), [src/ir/interpreter/run.rs](../src/ir/interpreter/run.rs), [src/language/gremlin/planner/lowering/branch.rs](../src/language/gremlin/planner/lowering/branch.rs).

**Acceptance:** All four cases finish inside the current adapter deadline with exact upstream results. Record elapsed time, peak frontier size and bulk compression; verify path-observing traversals remain distinct.

**Cases:** `branch/Repeat.feature:323`, `filter/Where.feature:185`, `map/Count.feature:91`, `map/Count.feature:102`.

<a id="f08"></a>

### F08. Path history through branches and path labels (3 cases)

**Evidence strength:** Diagnosis to verify.

**Change:** Carry scalar and element history through inject, mid-traversal V, coalesce and constant without appending artificial child-probe steps. Track the set of labels attached to each path position, including as(a,b), and make path().select(Column.keys) return those label sets. Preserve branch output history while keeping predicate/projection child histories isolated.

**Implementation locations:** [src/ir/interpreter/ops/coalesce.rs](../src/ir/interpreter/ops/coalesce.rs), [src/ir/interpreter/expr/path_predicate.rs](../src/ir/interpreter/expr/path_predicate.rs), [src/language/gremlin/planner/lowering/path.rs](../src/language/gremlin/planner/lowering/path.rs), [src/language/gremlin/planner/lowering/sub_traversal.rs](../src/language/gremlin/planner/lowering/sub_traversal.rs), [src/ir/value.rs](../src/ir/value.rs).

**Acceptance:** CyclicPath returns 12, SimplePath 6, and Select returns the two expected label-set paths; add scalar-equality and multiple-label path checks.

**Cases:** `filter/CyclicPath.feature:70`, `filter/SimplePath.feature:76`, `map/Select.feature:733`.

<a id="f09"></a>

### F09. Correlated reducers and match bindings (5 cases)

**Evidence strength:** Diagnosis to verify.

**Change:** Evaluate each group value traversal over its full correlated group, retaining labels and reducer output shape. Nested group returns a map, not a list containing maps. Scope dedup and fold to the intended group. In match, bind child reduction outputs (mean/count) as scalar labels and make them visible to subsequent where predicates and repeated-label unification; do not require every bound label to be an element.

**Implementation locations:** [src/language/gremlin/planner/lowering/group.rs](../src/language/gremlin/planner/lowering/group.rs), [src/language/gremlin/planner/lowering/branch.rs](../src/language/gremlin/planner/lowering/branch.rs), [src/language/gremlin/planner/lowering/sub_traversal.rs](../src/language/gremlin/planner/lowering/sub_traversal.rs), [src/ir/interpreter/ops/apply.rs](../src/ir/interpreter/ops/apply.rs), [src/ir/interpreter/ops/aggregate.rs](../src/ir/interpreter/ops/aggregate.rs).

**Acceptance:** Dedup emits 6 groups, the two Miscellaneous cases produce nested maps, Match :483 produces 8 rows and :548 produces count 6 with original expectations.

**Cases:** `filter/Dedup.feature:255`, `integrated/Miscellaneous.feature:21`, `integrated/Miscellaneous.feature:52`, `map/Match.feature:483`, `map/Match.feature:548`.

<a id="f10"></a>

### F10. Seeded random sampling (4 cases)

**Evidence strength:** Global sample implementation confirmed to be a limit.

**Change:** Implement actual coin and local/global sample operators with per-step seeded RNG, weighted by() evaluation, nonproductive-weight handling and bulk-aware draws. Propagate SeedStrategy to children using pinned TinkerPop behavior and Java Random-compatible draws for these deterministic tests. Global sample currently discards by() and lowers to a limit. Preserve the input ordering consumed by the seeded algorithm.

**Implementation locations:** [src/language/gremlin/planner/lowering/slice.rs](../src/language/gremlin/planner/lowering/slice.rs), [src/language/gremlin/planner/lowering/dispatch.rs](../src/language/gremlin/planner/lowering/dispatch.rs), [src/language/gremlin/planner/lowering.rs](../src/language/gremlin/planner/lowering.rs), [src/ir/interpreter/run.rs](../src/ir/interpreter/run.rs).

**Acceptance:** All four seeded expected outputs match; weighted sample never chooses software with no age. Different seeds and graphs must exercise real sampling, not fixture-specific selections.

**Cases:** `filter/Coin.feature:47`, `filter/Sample.feature:49`, `filter/Sample.feature:62`, `filter/Sample.feature:75`.

<a id="f11"></a>

### F11. Choice option matching (1 case)

**Evidence strength:** Diagnosis to verify.

**Change:** Compare each choice traversal result with option keys using Gremlin numeric equality; dispatch only matching options. A scalar option key is not a truth test. Evaluate the child count once per incoming traverser and preserve the parent context when invoking the selected branch.

**Implementation locations:** [src/language/gremlin/planner/lowering/branch.rs](../src/language/gremlin/planner/lowering/branch.rs), [src/ir/interpreter/ops/choose.rs](../src/ir/interpreter/ops/choose.rs), [src/ir/value.rs](../src/ir/value.rs).

**Acceptance:** Exactly the two upstream rows, including correct Long option matching; retain Pick.none/any and traversal-option cases.

**Cases:** `branch/Choose.feature:21`.

<a id="f12"></a>

### F12. Local dedup native set type (1 case)

**Evidence strength:** Expected set type confirmed from catalog.

**Change:** Return a native set with set equality/serialization from dedup(local), rather than a List whose order is significant. The expected s[...] values are sets: sorting the returned list to match one printed ordering is not a fix. Check that the runtime set representation is not confused with Gremlin BulkSet multiplicities.

**Implementation locations:** [src/ir/interpreter/runtime/lists.rs](../src/ir/interpreter/runtime/lists.rs), [src/ir/value.rs](../src/ir/value.rs), [src/ir/interpreter/output.rs](../src/ir/interpreter/output.rs), [conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java](../conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java).

**Acceptance:** All six result sets compare as sets, independent of element iteration order.

**Cases:** `filter/Dedup.feature:35`.

<a id="f13"></a>

### F13. where predicate by-ring (1 case)

**Evidence strength:** Diagnosis to verify.

**Change:** Apply cyclic by modulators to the start and referenced label values according to WherePredicateStep, including nested and/or predicates. Preserve projection productivity and the scalar min child traversal result; do not apply one by expression to every operand.

**Implementation locations:** [src/language/gremlin/planner/lowering/filter.rs](../src/language/gremlin/planner/lowering/filter.rs), [src/language/gremlin/planner/lowering/predicates.rs](../src/language/gremlin/planner/lowering/predicates.rs), [src/language/gremlin/planner/lowering/helpers.rs](../src/language/gremlin/planner/lowering/helpers.rs).

**Acceptance:** The four expected a/c/d maps survive the mixed age/weight/min predicate.

**Cases:** `filter/Where.feature:309`.

<a id="f14"></a>

### F14. PartitionStrategy edge visibility (2 cases)

**Evidence strength:** Diagnosis to verify.

**Change:** Trace V().bothE() strategy rewriting: filter starting vertices by read partitions and edges by their own partition. Do not inadvertently require an edge's opposite endpoint to be visible when returning the edge itself. Apply endpoint visibility only to operations where upstream strategy semantics require it.

**Implementation locations:** [src/language/gremlin/planner/lowering/sources.rs](../src/language/gremlin/planner/lowering/sources.rs), [src/language/gremlin/planner/lowering.rs](../src/language/gremlin/planner/lowering.rs), [src/ir/rel/expansion.rs](../src/ir/rel/expansion.rs).

**Acceptance:** Partition a yields weight 1.0 and partition b weight 2.0; retain vertex-navigation visibility checks.

**Cases:** `integrated/PartitionStrategy.feature:72`, `integrated/PartitionStrategy.feature:92`.

<a id="f15"></a>

### F15. Element string representation (3 cases)

**Evidence strength:** Diagnosis to verify.

**Change:** Format vertices and edges using their external identity and the pinned TinkerPop element string contract, consistently in scalar and local forms. Investigate fixture external-ID mapping before changing graph storage. Remove literal str[...] wrappers; do not special-case modern-graph IDs or names.

**Implementation locations:** [src/ir/interpreter/runtime/strings.rs](../src/ir/interpreter/runtime/strings.rs), [src/ir/interpreter/runtime/graph.rs](../src/ir/interpreter/runtime/graph.rs), [src/ir/interpreter/output.rs](../src/ir/interpreter/output.rs), [conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java](../conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java).

**Acceptance:** Vertex strings, edge strings (including endpoints), and local list strings match the originals on imported and newly created graphs.

**Cases:** `map/AsString.feature:119`, `map/AsString.feature:136`, `map/AsString.feature:148`.

<a id="f16"></a>

### F16. Graph-backed search service (2 cases)

**Evidence strength:** Hard-coded row selection confirmed in source.

**Change:** Replace fixed person-row IDs in lower_call_source(tinker.search) with graph-backed property search or a registered provider implementation. Return actual Property/VertexProperty owners from element(); honor type filters and arguments. The current fixed-ID mapping explains why a search can return vadas for mar.

**Implementation locations:** [src/language/gremlin/planner/lowering/procedures.rs](../src/language/gremlin/planner/lowering/procedures.rs), [src/ir/interpreter/runtime/property_object.rs](../src/ir/interpreter/runtime/property_object.rs), [src/ir/interpreter/run.rs](../src/ir/interpreter/run.rs).

**Acceptance:** Both cases resolve marko; verify reordered IDs, different labels and non-modern input graphs.

**Cases:** `map/Call.feature:102`, `map/Call.feature:147`.

<a id="f17"></a>

### F17. Index configuration (2 cases)

**Evidence strength:** Shape mismatch confirmed.

**Change:** Honor the numeric indexer option (~tinkerpop.index.indexer=1) and emit a typed map keyed by integer indexes, for scalar input and local collections. Current output is the default list-of-pairs shape.

**Implementation locations:** [src/language/gremlin/planner/lowering/dispatch.rs](../src/language/gremlin/planner/lowering/dispatch.rs), [src/language/gremlin/planner/lowering.rs](../src/language/gremlin/planner/lowering.rs), [src/ir/interpreter/runtime/lists.rs](../src/ir/interpreter/runtime/lists.rs).

**Acceptance:** Results are {0:vertex} and {0:josh,1:marko,2:peter,3:vadas}, with integer map keys.

**Cases:** `map/Index.feature:33`, `map/Index.feature:58`.

<a id="f18"></a>

### F18. Remote lambda harness failure (1 case)

**Evidence strength:** Upstream scenario and pre-execution failure confirmed.

**Change:** The @RemoteOnly scenario embeds Lambda.function directly; the traversal is null before execution. Repair harness error classification first. Supporting this scenario requires a deliberate remote-lambda execution capability (or an upstream-supported profile exclusion), not a Rust map-step patch. Keep the original scenario and report an explicit capability boundary until it can actually execute; do not treat reclassification as a newly passing test.

**Implementation locations:** [conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java](../conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java), [conformance/upstream/run.py](../conformance/upstream/run.py).

**Acceptance:** No NullPointerException; either the original remote lambda executes and returns 3/5/4, or the recorded outcome explicitly identifies the unsupported profile capability.

**Cases:** `map/Map.feature:66`.

<a id="f19"></a>

### F19. Typed token and path ordering (6 cases)

**Evidence strength:** Diagnosis to verify.

**Change:** Implement the pinned total ordering for token/string keys and lexicographic paths using their typed objects and external element/property identity. Current token tags and node ordering by (label, internal offset) are not a substitute for upstream comparisons. Property-path cases depend on A1; value-path cases still need correct element identity ordering. Keep predicate comparability distinct from total orderability.

**Implementation locations:** [src/ir/interpreter/expr/compare.rs](../src/ir/interpreter/expr/compare.rs), [src/ir/interpreter/runtime/graph.rs](../src/ir/interpreter/runtime/graph.rs), [src/ir/value.rs](../src/ir/value.rs).

**Acceptance:** Both elementMap key directions and all four path ordering cases match; compare the first differing typed component when debugging.

**Cases:** `map/Order.feature:575`, `map/Order.feature:590`, `semantics/Orderability.feature:312`, `semantics/Orderability.feature:328`, `semantics/Orderability.feature:343`, `semantics/Orderability.feature:358`.

<a id="f20"></a>

### F20. Heterogeneous values preserve types (1 case)

**Evidence strength:** Observed ages are strings, expected ages are Int32.

**Change:** Preserve individual value types when values(name,age,null) combines property columns. Ignore/filter a null property-name argument according to upstream without converting numeric ages to strings. Trace SQL union/common-type coercion and Arrow/native output metadata; disable this lowering only where the backend cannot preserve the heterogeneous type.

**Implementation locations:** [src/language/gremlin/planner/lowering/sources.rs](../src/language/gremlin/planner/lowering/sources.rs), [src/ir/rel/projection.rs](../src/ir/rel/projection.rs), [src/ir/interpreter/output.rs](../src/ir/interpreter/output.rs).

**Acceptance:** Names remain strings and ages remain Int32 in the same result stream.

**Cases:** `map/Properties.feature:91`.

<a id="f21"></a>

### F21. ValueMap options and productive modulators (3 cases)

**Evidence strength:** Diagnosis to verify.

**Change:** Interpret the tokens bitmask independently for id and label. Apply by modulators cyclically to property values; remove an entry when its by traversal is nonproductive and retain productive scalar outputs. Do not treat any with(tokens,...) as both tokens or ignore failed child projections.

**Implementation locations:** [src/language/gremlin/planner/lowering/dispatch.rs](../src/language/gremlin/planner/lowering/dispatch.rs), [src/language/gremlin/planner/lowering.rs](../src/language/gremlin/planner/lowering.rs), [src/ir/interpreter/runtime/property_object.rs](../src/ir/interpreter/runtime/property_object.rs).

**Acceptance:** :117 includes label only, :133 id only, :221 age-only scalar maps and empty maps for software.

**Cases:** `map/ValueMap.feature:117`, `map/ValueMap.feature:133`, `map/ValueMap.feature:221`.

<a id="f22"></a>

### F22. Group reduction shape, productivity and width (3 cases)

**Evidence strength:** Width error confirmed by GraphSON; productivity diagnosis needs trace.

**Change:** Preserve the specialized Gremlin reducer contract when group and group(sideEffect) lower child sum traversals. :206 and SideEffectCap :122 have numerically correct maps but Int32 values instead of expected Int64. :165 must not create sungBy/writtenBy groups from a nonproductive weight traversal. Distinguish no input from a genuine sum identity; do not globally drop numeric zero.

**Implementation locations:** [src/language/gremlin/planner/lowering/group.rs](../src/language/gremlin/planner/lowering/group.rs), [src/language/gremlin/planner/lowering/reduce.rs](../src/language/gremlin/planner/lowering/reduce.rs), [src/ir/interpreter/ops/aggregate.rs](../src/ir/interpreter/ops/aggregate.rs), [src/ir/interpreter/output.rs](../src/ir/interpreter/output.rs).

**Acceptance:** Nested group contains only productive labels; age sums are native Long 96 and 32 in direct and capped maps.

**Cases:** `sideEffect/Group.feature:165`, `sideEffect/Group.feature:206`, `sideEffect/SideEffectCap.feature:122`.

<a id="f23"></a>

### F23. Graph import is a real write (6 cases)

**Evidence strength:** Diagnosis to verify.

**Change:** Implement io(path).read() as an explicit import operation with reader selection/autodetection for Gryo, GraphSON and GraphML. Resolve fixture paths in the local adapter, preserve element IDs/types/properties, persist the imported graph in the same query session, and surface parser/format errors. A Java-backed Gryo codec is an implementation option; imported mutations must still go through the graph transaction path. Returning no rows without importing is not success.

**Implementation locations:** [src/language/gremlin/parser/](../src/language/gremlin/parser/), [src/language/gremlin/planner/lowering/dispatch.rs](../src/language/gremlin/planner/lowering/dispatch.rs), [src/ir/interpreter/run.rs](../src/ir/interpreter/run.rs), [src/ir/catalog/](../src/ir/catalog/), [conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java](../conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java).

**Acceptance:** All six reader variants create the expected 6 vertices/6 edges with typed properties, observed by subsequent traversals; malformed imports roll back.

**Cases:** `sideEffect/Read.feature:21`, `sideEffect/Read.feature:32`, `sideEffect/Read.feature:43`, `sideEffect/Read.feature:54`, `sideEffect/Read.feature:65`, `sideEffect/Read.feature:76`.

<a id="f24"></a>

### F24. Sack barriers and merge functions (3 cases)

**Evidence strength:** Diagnosis to verify.

**Change:** Parse SackFunctions.Barrier.normSack and represent it explicitly in the AST/IR. Implement normalized sack barriers, sack merge operators and their interaction with withBulk(false). :74 already parses but emits 6 instead of 4, so parsing alone is insufficient. Extend existing Row.bulk and execution state under A2 rather than implementing sack as an unrelated row projection.

**Implementation locations:** [src/language/gremlin/parser/](../src/language/gremlin/parser/), [src/language/gremlin/ast/step.rs](../src/language/gremlin/ast/step.rs), [src/language/gremlin/planner/lowering/side_effects.rs](../src/language/gremlin/planner/lowering/side_effects.rs), [src/ir/interpreter/run.rs](../src/ir/interpreter/run.rs).

**Acceptance:** Both normSack cases parse and yield exact original numeric outputs; :74 merges sacks into four results under withBulk(false).

**Cases:** `sideEffect/Sack.feature:61`, `sideEffect/Sack.feature:74`, `sideEffect/Sack.feature:88`.

## Complete failure inventory

Every case below links to its pinned upstream scenario. The complete setup/assertion is in that scenario and the local catalog; the shown traversal is the original scenario traversal, including symbolic parameters (not a replacement reproducer). Multiple assertions may follow one traversal. Peer/reference outcomes are historical recorded results, not the semantic oracle. Times are per-case wall time and can include setup and retained replays; they are not comparable query benchmarks.


### 001. branch/Choose.feature:21

**Assignment:** [F11](#f11). **Status:** fail. **Recorded time:** 104.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/branch/Choose.feature#L21).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<3>
```

**Failed at:** the result should be unordered

```gremlin
g.V().choose(__.out().count()).
  option(xx1, __.values("name")).
  option(xx2, __.values("age"))
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  d[29].i
  josh
  ```

### 002. branch/Repeat.feature:323

**Assignment:** [F07](#f07). **Status:** timeout. **Recorded time:** 15,019.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/branch/Repeat.feature#L323).

**Recorded comparison:** sqlg=timeout, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: adapter-timeout: Crabgraph adapter deadline
```

**Failed at:** the result should be unordered

```gremlin
g.V(vid3).repeat(__.both("created")).until(__.loops().is(40)).emit(__.repeat(__.in("knows")).emit(__.loops().is(1))).dedup().values("name")
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  josh
  lop
  ripple
  ```

### 003. filter/Coin.feature:47

**Assignment:** [F10](#f10). **Status:** fail. **Recorded time:** 83.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Coin.feature#L47).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<1> but was:<0>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SeedStrategy(seed: 999999)).V().order().by("name").coin(0.5)
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[josh]
  ```

### 004. filter/CyclicPath.feature:70

**Assignment:** [F08](#f08). **Status:** fail. **Recorded time:** 49.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/CyclicPath.feature#L70).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<12> but was:<3>
```

**Failed at:** the result should be unordered

```gremlin
g.inject(0).V().both().coalesce(has('name','marko').both(),constant(0)).cyclicPath().path()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  p[d[0].i,v[marko],v[lop],d[0].i]
  p[d[0].i,v[marko],v[vadas],d[0].i]
  p[d[0].i,v[marko],v[josh],d[0].i]
  p[d[0].i,v[vadas],v[marko],v[vadas]]
  p[d[0].i,v[lop],v[marko],v[lop]]
  p[d[0].i,v[lop],v[josh],d[0].i]
  p[d[0].i,v[lop],v[peter],d[0].i]
  p[d[0].i,v[josh],v[ripple],d[0].i]
  p[d[0].i,v[josh],v[lop],d[0].i]
  p[d[0].i,v[josh],v[marko],v[josh]]
  p[d[0].i,v[ripple],v[josh],d[0].i]
  p[d[0].i,v[peter],v[lop],d[0].i]
  ```

### 005. filter/Dedup.feature:35

**Assignment:** [F12](#f12). **Status:** fail. **Recorded time:** 39.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Dedup.feature#L35).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<[peter, josh, marko]>, <[peter, josh, marko]>, <[peter, josh, marko]>, <[marko]>, <[josh]>, <[marko]>] in any order
     but: not matched: <[marko, josh, peter]>
```

**Failed at:** the result should be unordered

```gremlin
g.V().out().map(__.in().values("name").fold().dedup(Scope.local))
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  s[peter,marko,josh]
  s[marko,peter,josh]
  s[josh,marko,peter]
  s[marko]
  s[josh]
  s[marko]
  ```

**Type diagnostic:** `s[...]` in the expected table is a Set, not an ordered List.

### 006. filter/Dedup.feature:255

**Assignment:** [F09](#f09). **Status:** fail. **Recorded time:** 37.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Dedup.feature#L255).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<6> but was:<27>
```

**Failed at:** the result should be unordered

```gremlin
g.V().as("a").repeat(__.both()).times(3).emit().values("name").as("b").group().by(__.select("a")).by(__.select("b").dedup().order().fold()).select(Column.values).unfold().dedup()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  josh
  lop
  marko
  peter
  ripple
  vadas
  ```

### 007. filter/Drop.feature:51

**Assignment:** [F03](#f03). **Status:** fail. **Recorded time:** 45.9 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Drop.feature#L51).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<0> but was:<2>
```

**Failed at:** the graph should return 0 for count of "g.V().properties()"

```gremlin
g.V().properties().drop()
```

**Required outcome from original scenario:**

- the result should be empty
- the graph should return 2 for count of "g.V()"
- the graph should return 0 for count of "g.V().properties()"

### 008. filter/Drop.feature:66

**Assignment:** [F03](#f03). **Status:** fail. **Recorded time:** 88.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Drop.feature#L66).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<0> but was:<6>
```

**Failed at:** the graph should return 0 for count of "g.E().properties()"

```gremlin
g.E().properties("weight").drop()
```

**Required outcome from original scenario:**

- the result should be empty
- the graph should return 0 for count of "g.E().properties()"

### 009. filter/Drop.feature:92

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 122.4 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Drop.feature#L92).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.V().properties().properties("startTime").drop()
```

**Required outcome from original scenario:**

- the result should be empty
- the graph should return 2 for count of "g.V().properties().properties()"
- the graph should return 0 for count of "g.V().properties().properties(\"startTime\")"

### 010. filter/Sample.feature:49

**Assignment:** [F10](#f10). **Status:** fail. **Recorded time:** 32.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Sample.feature#L49).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{software=[1.0, 0.4]}>, <{person=[0.5, 1.0]}>] in any order
     but: not matched: <{person=[0.2, 0.4]}>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SeedStrategy(seed: 999999)).V().group().by(T.label).by(__.bothE().values("weight").order().sample(2).fold()).unfold()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"software":"l[d[1.0].d,d[0.4].d]"}]
  m[{"person":"l[d[0.5].d,d[1.0].d]"}]
  ```

### 011. filter/Sample.feature:62

**Assignment:** [F10](#f10). **Status:** fail. **Recorded time:** 33.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Sample.feature#L62).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{software=[0.2, 0.4, 0.4, 1.0]}>, <{person=[0.5, 1.0, 0.4, 0.2, 1.0]}>] in any order
     but: not matched: <{person=[0.2, 0.4, 0.4, 0.5, 0.5]}>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SeedStrategy(seed: 999999)).V().group().by(T.label).by(__.bothE().values("weight").order().fold().sample(Scope.local, 5)).unfold()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"software":"l[d[0.2].d,d[0.4].d,d[0.4].d,d[1.0].d]"}]
  m[{"person":"l[d[0.5].d,d[1.0].d,d[0.4].d,d[0.2].d,d[1.0].d]"}]
  ```

### 012. filter/Sample.feature:75

**Assignment:** [F10](#f10). **Status:** fail. **Recorded time:** 54.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Sample.feature#L75).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: every item is one of {<v[person#3]>, <v[person#0]>, <v[person#2]>, <v[person#1]>}
     but: an item was <v[software#0]>
```

**Failed at:** the result should be of

```gremlin
g.withStrategies(new SeedStrategy(seed: 999999)).V().order().by(T.label, Order.desc).sample(1).by("age")
```

**Required outcome from original scenario:**

- the result should be of

  ```text
  result
  v[peter]
  v[marko]
  v[josh]
  v[vadas]
  ```
- the result should have a count of 1

### 013. filter/SimplePath.feature:76

**Assignment:** [F08](#f08). **Status:** fail. **Recorded time:** 24.1 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/SimplePath.feature#L76).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<6> but was:<15>
```

**Failed at:** the result should be unordered

```gremlin
g.inject(0).V().both().coalesce(has('name','marko').both(),constant(0)).simplePath().path()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  p[d[0].i,v[vadas],v[marko],v[lop]]
  p[d[0].i,v[vadas],v[marko],v[josh]]
  p[d[0].i,v[lop],v[marko],v[vadas]]
  p[d[0].i,v[lop],v[marko],v[josh]]
  p[d[0].i,v[josh],v[marko],v[lop]]
  p[d[0].i,v[josh],v[marko],v[vadas]]
  ```

### 014. filter/Where.feature:185

**Assignment:** [F07](#f07). **Status:** timeout. **Recorded time:** 15,026.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Where.feature#L185).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: adapter-timeout: Crabgraph adapter deadline
```

**Failed at:** the result should be unordered

```gremlin
g.V(vid1).repeat(__.bothE("created").where(P.without("e")).aggregate("e").otherV()).emit().path()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  p[v[marko],e[marko-created->lop],v[lop]]
  p[v[marko],e[marko-created->lop],v[lop],e[josh-created->lop],v[josh]]
  p[v[marko],e[marko-created->lop],v[lop],e[peter-created->lop],v[peter]]
  p[v[marko],e[marko-created->lop],v[lop],e[josh-created->lop],v[josh],e[josh-created->ripple],v[ripple]]
  ```

### 015. filter/Where.feature:309

**Assignment:** [F13](#f13). **Status:** fail. **Recorded time:** 39.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Where.feature#L309).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<4> but was:<0>
```

**Failed at:** the result should be unordered

```gremlin
g.V().as("a").outE("created").as("b").inV().as("c").in("created").as("d").where("a", P.lt("b").or(P.gt("c")).and(P.neq("d"))).by("age").by("weight").by(__.in("created").values("age").min()).select("a", "c", "d").by("name")
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"a": "josh", "c": "lop", "d": "marko"}]
  m[{"a": "josh", "c": "lop", "d": "peter"}]
  m[{"a": "peter", "c": "lop", "d": "marko"}]
  m[{"a": "peter", "c": "lop", "d": "josh"}]
  ```

### 016. integrated/Miscellaneous.feature:21

**Assignment:** [F09](#f09). **Status:** fail. **Recorded time:** 296.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/Miscellaneous.feature#L21).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{peter={josh={numCoCreated=1, coCreated=[lop]}, marko={numCoCreated=1, coCreated=[lop]}}}>, <{josh={peter={numCoCreated=1, coCreated=[lop]}, marko={numCoCreated=1, coCreated=[lop]}}}>, <{marko={peter={numCoCreated=1, coCreated=[lop]}, josh={numCoCreated=1, coCreated=[lop]}}}>] in any order
     but: not matched: <{josh=[{marko={coCreated=[], numCoCreated=0}}, {peter={coCreated=[], numCoCreated=0}}]}>
```

**Failed at:** the result should be unordered

```gremlin
g.V().hasLabel("person").
  filter(__.outE("created")).
  aggregate("p").as("p1").
  values("name").as("p1n").
  select("p").unfold().
  where(neq("p1")).as("p2").values("name").as("p2n").
  select("p2").
  out("created").
  choose(__.in("created").where(P.eq("p1")), __.values("name"), __.constant(xx1)).
  group().
    by(__.select("p1n")).
    by(__.group().
            by(__.select("p2n")).
            by(__.unfold().fold().
               project("numCoCreated", "coCreated").
                 by(__.count(local)).by())).
  unfold()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"peter": {"josh": {"numCoCreated":"d[1].l", "coCreated":["lop"]}, "marko": {"numCoCreated":"d[1].l", "coCreated":["lop"]}}}]
  m[{"josh": {"peter": {"numCoCreated":"d[1].l", "coCreated":["lop"]}, "marko": {"numCoCreated":"d[1].l", "coCreated":["lop"]}}}]
  m[{"marko": {"peter": {"numCoCreated":"d[1].l", "coCreated":["lop"]}, "josh": {"numCoCreated":"d[1].l", "coCreated":["lop"]}}}]
  ```

### 017. integrated/Miscellaneous.feature:52

**Assignment:** [F09](#f09). **Status:** fail. **Recorded time:** 79.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/Miscellaneous.feature#L52).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{peter={josh={numCoCreated=1, coCreated=[lop]}, marko={numCoCreated=1, coCreated=[lop]}}}>, <{josh={peter={numCoCreated=1, coCreated=[lop]}, marko={numCoCreated=1, coCreated=[lop]}}}>, <{marko={peter={numCoCreated=1, coCreated=[lop]}, josh={numCoCreated=1, coCreated=[lop]}}}>] in any order
     but: not matched: <{josh=[{marko={coCreated=[lop], numCoCreated=1}}, {peter={coCreated=[lop], numCoCreated=1}}]}>
```

**Failed at:** the result should be unordered

```gremlin
g.V().hasLabel("person").
  filter(__.outE("created")).as("p1").
  V().hasLabel("person").
  where(P.neq("p1")).
  filter(__.outE("created")).as("p2").
  map(__.out("created").where(__.in("created").as("p1")).values("name").fold()).
  group().
    by(__.select("p1").by("name")).
    by(__.group().by(select("p2").by("name")).
    by(__.project("numCoCreated", "coCreated").
            by(__.count(local)).by())).
  unfold()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"peter": {"josh": {"numCoCreated":"d[1].l", "coCreated":["lop"]}, "marko": {"numCoCreated":"d[1].l", "coCreated":["lop"]}}}]
  m[{"josh": {"peter": {"numCoCreated":"d[1].l", "coCreated":["lop"]}, "marko": {"numCoCreated":"d[1].l", "coCreated":["lop"]}}}]
  m[{"marko": {"peter": {"numCoCreated":"d[1].l", "coCreated":["lop"]}, "josh": {"numCoCreated":"d[1].l", "coCreated":["lop"]}}}]
  ```

### 018. integrated/PartitionStrategy.feature:72

**Assignment:** [F14](#f14). **Status:** fail. **Recorded time:** 200.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/PartitionStrategy.feature#L72).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<1> but was:<0>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new PartitionStrategy(partitionKey: "_partition", writePartition: "a", readPartitions: ["a"])).V().
  bothE().values("weight")
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  d[1.0].d
  ```

### 019. integrated/PartitionStrategy.feature:92

**Assignment:** [F14](#f14). **Status:** fail. **Recorded time:** 183.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/PartitionStrategy.feature#L92).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<1> but was:<0>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new PartitionStrategy(partitionKey: "_partition", writePartition: "a", readPartitions: ["b"])).V().
  bothE().values("weight")
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  d[2.0].d
  ```

### 020. integrated/Paths.feature:22

**Assignment:** [F06](#f06). **Status:** fail. **Recorded time:** 173.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/Paths.feature#L22).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<30> but was:<0>
```

**Failed at:** the result should be unordered

```gremlin
g.V().as("v").both().as("v").
  project("src", "tgt", "p").
    by(__.select(first, "v")).
    by(__.select(last, "v")).
    by(__.select(Pop.all, "v")).as("triple").
  group("x").
    by(__.select("src", "tgt")).
    by(__.select("p").fold()).select("tgt").barrier().
  repeat(__.both().as("v").
          project("src", "tgt", "p").
            by(__.select(first, "v")).
            by(__.select(last, "v")).
            by(__.select(Pop.all, "v")).as("t").
          filter(__.select(Pop.all, "p").count(local).as("l").
                 select(Pop.last, "t").select(Pop.all, "p").dedup(Scope.local).count(Scope.local).where(P.eq("l"))).
          select(Pop.last, "t").
          not(__.select(Pop.all, "p").as("p").count(local).as("l").
              select(Pop.all, "x").unfold().filter(select(keys).where(P.eq("t")).by(select("src", "tgt"))).
              filter(__.select(Column.values).unfold().or(__.count(Scope.local).where(P.lt("l")), __.where(P.eq("p"))))).
          barrier().
          group("x").
            by(__.select("src", "tgt")).
            by(__.select(Pop.all, "p").fold()).select("tgt").barrier()).
  cap("x").select(Column.values).unfold().unfold().map(__.unfold().values("name").fold())
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  l[josh,marko,vadas]
  l[ripple,josh,lop]
  l[josh,lop]
  l[peter,lop,marko]
  l[ripple,josh,marko]
  l[josh,marko]
  l[marko,lop]
  l[lop,marko]
  l[josh,lop,peter]
  l[peter,lop,josh]
  l[vadas,marko]
  l[ripple,josh]
  l[marko]
  l[josh]
  l[ripple]
  l[josh,ripple]
  l[peter,lop]
  l[vadas,marko,josh]
  l[lop,josh,ripple]
  l[marko,josh]
  l[lop,marko,vadas]
  l[lop]
  l[peter]
  l[vadas]
  l[marko,josh,ripple]
  l[marko,vadas]
  l[vadas,marko,lop]
  l[lop,peter]
  l[lop,josh]
  l[marko,lop,peter]
  ```

### 021. integrated/SubgraphStrategy.feature:200

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 57.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L200).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<3> but was:<2>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(edges: __.or(__.has("weight", 1.0).hasLabel("knows"),
                                                   __.has("weight", 0.4).hasLabel("created").outV().has("name", "marko"),
                                                   __.has("weight", 1.0).hasLabel("created")))).E()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  e[marko-knows->josh]
  e[marko-created->lop]
  e[josh-created->ripple]
  ```

### 022. integrated/SubgraphStrategy.feature:216

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 66.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L216).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(edges: __.or(__.has("weight", 1.0).hasLabel("knows"),
                                                   __.has("weight", 0.4).hasLabel("created").outV().has("name", "marko"),
                                                   __.has("weight", 1.0).hasLabel("created")))).V(vid1).outE()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  e[marko-knows->josh]
  e[marko-created->lop]
  ```

### 023. integrated/SubgraphStrategy.feature:232

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 65.4 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L232).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(edges: __.or(__.has("weight", 1.0).hasLabel("knows"),
                                                   __.has("weight", 0.4).hasLabel("created").outV().has("name", "marko"),
                                                   __.has("weight", 1.0).hasLabel("created")))).V(vid1).out()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[josh]
  v[lop]
  ```

### 024. integrated/SubgraphStrategy.feature:340

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 91.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L340).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(edges: __.or(__.has("weight", 1.0).hasLabel("knows"),
                                                   __.has("weight", 0.4).hasLabel("created").outV().has("name", "marko"),
                                                   __.has("weight", 1.0).hasLabel("created")))).E(eid8).outV().outE()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  e[marko-knows->josh]
  e[marko-created->lop]
  ```

### 025. integrated/SubgraphStrategy.feature:441

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 61.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L441).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).E()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  e[josh-created->lop]
  e[josh-created->ripple]
  ```

### 026. integrated/SubgraphStrategy.feature:456

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 68.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L456).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).V(vid4).outE()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  e[josh-created->lop]
  e[josh-created->ripple]
  ```

### 027. integrated/SubgraphStrategy.feature:485

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 67.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L485).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).V(vid4).out()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[lop]
  v[ripple]
  ```

### 028. integrated/SubgraphStrategy.feature:514

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 71.4 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L514).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).V(vid4).both()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[lop]
  v[ripple]
  ```

### 029. integrated/SubgraphStrategy.feature:530

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 67.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L530).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).V(vid4).bothE()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  e[josh-created->lop]
  e[josh-created->ripple]
  ```

### 030. integrated/SubgraphStrategy.feature:559

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 90.1 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L559).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<0>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).E(eid11).bothV()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[josh]
  v[lop]
  ```

### 031. integrated/SubgraphStrategy.feature:745

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 58.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L745).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<3> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(checkAdjacentVertices: false,
                                      vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).E()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  e[marko-created->lop]
  e[josh-created->lop]
  e[josh-created->ripple]
  ```

### 032. integrated/SubgraphStrategy.feature:762

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 59.1 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L762).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(checkAdjacentVertices: true,
                                      vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).E()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  e[josh-created->lop]
  e[josh-created->ripple]
  ```

### 033. integrated/SubgraphStrategy.feature:778

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 68.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L778).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(checkAdjacentVertices: false,
                                      vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).V(vid4).outE()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  e[josh-created->lop]
  e[josh-created->ripple]
  ```

### 034. integrated/SubgraphStrategy.feature:809

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 67.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L809).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(checkAdjacentVertices: false,
                                      vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).V(vid4).out()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[lop]
  v[ripple]
  ```

### 035. integrated/SubgraphStrategy.feature:840

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 69.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L840).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(checkAdjacentVertices: false,
                                      vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).V(vid4).both()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[lop]
  v[ripple]
  ```

### 036. integrated/SubgraphStrategy.feature:857

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 69.1 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L857).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<1>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(checkAdjacentVertices: false,
                                      vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).V(vid4).bothE()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  e[josh-created->lop]
  e[josh-created->ripple]
  ```

### 037. integrated/SubgraphStrategy.feature:888

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 83.4 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L888).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<0>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(checkAdjacentVertices: false,
                                      vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).E(eid11).bothV()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[josh]
  v[lop]
  ```

### 038. integrated/SubgraphStrategy.feature:919

**Assignment:** [F01](#f01). **Status:** fail. **Recorded time:** 83.4 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L919).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<1> but was:<0>
```

**Failed at:** the result should be unordered

```gremlin
g.withStrategies(new SubgraphStrategy(checkAdjacentVertices: false,
                                      vertices: __.has("name", P.within("josh", "lop", "ripple")),
                                      edges: __.or(__.has("weight", 0.4).hasLabel("created"),
                                                   __.has("weight", 1.0).hasLabel("created")))).E(eid9).bothV()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[lop]
  ```

### 039. map/AddVertex.feature:150

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 115.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L150).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property meta-properties
```

**Failed at:** the result should have a count of 1

```gremlin
g.addV("person").property(Cardinality.single, "name", "stephen").property(Cardinality.single, "name", "stephenm", "since", 2010)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 0 for count of "g.V().has(\"person\",\"name\",\"stephen\")"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"stephenm\")"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"stephenm\").properties(\"name\").has(\"since\",2010)"

### 040. map/AddVertex.feature:178

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 89.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L178).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property meta-properties
```

**Failed at:** the result should have a count of 1

```gremlin
g.V().has("name", "marko").property("friendWeight", __.outE("knows").values("weight").sum(), "acl", "private")
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"friendWeight\", 1.5)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").properties(\"friendWeight\").has(\"acl\",\"private\")"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").properties(\"friendWeight\").count()"

### 041. map/AddVertex.feature:206

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 105.9 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L206).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<1> but was:<0>
```

**Failed at:** the graph should return 1 for count of "g.V().hasLabel(\"animal\").has(\"name\",\"mateo\").has(\"name\", \"gateo\").has(\"name\", \"cateo\").has(\"age\",5)"

```gremlin
g.addV("animal").property("name", "mateo").property("name", "gateo").property("name", "cateo").property("age", 5)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().hasLabel(\"animal\").has(\"name\",\"mateo\").has(\"name\", \"gateo\").has(\"name\", \"cateo\").has(\"age\",5)"

### 042. map/AddVertex.feature:259

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 101.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L259).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property meta-properties
```

**Failed at:** the result should have a count of 1

```gremlin
g.addV("person").property(Cardinality.single, "name", "stephen").property(Cardinality.single, "name", "stephenm", "since", 2010)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 0 for count of "g.V().has(\"name\",\"stephen\")"
- the graph should return 1 for count of "g.V().has(\"name\",\"stephenm\")"
- the graph should return 1 for count of "g.V().has(\"name\",\"stephenm\").properties(\"name\").has(\"since\",2010)"

### 043. map/AddVertex.feature:287

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 118.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L287).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<6> but was:<0>
```

**Failed at:** the result should have a count of 6

```gremlin
g.V().addV("animal").property("name", __.values("name")).property("name", "an animal").property(__.values("name"), __.label())
```

**Required outcome from original scenario:**

- the result should have a count of 6
- the graph should return 1 for count of "g.V().hasLabel(\"animal\").has(\"name\",\"marko\").has(\"name\",\"an animal\").has(\"marko\",\"person\")"
- the graph should return 1 for count of "g.V().hasLabel(\"animal\").has(\"name\",\"vadas\").has(\"name\",\"an animal\").has(\"vadas\",\"person\")"
- the graph should return 1 for count of "g.V().hasLabel(\"animal\").has(\"name\",\"lop\").has(\"name\",\"an animal\").has(\"lop\",\"software\")"
- the graph should return 1 for count of "g.V().hasLabel(\"animal\").has(\"name\",\"josh\").has(\"name\",\"an animal\").has(\"josh\",\"person\")"
- the graph should return 1 for count of "g.V().hasLabel(\"animal\").has(\"name\",\"ripple\").has(\"name\",\"an animal\").has(\"ripple\",\"software\")"
- the graph should return 1 for count of "g.V().hasLabel(\"animal\").has(\"name\",\"peter\").has(\"name\",\"an animal\").has(\"peter\",\"person\")"

### 044. map/AddVertex.feature:453

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 45.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L453).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the result should have a count of 1

```gremlin
g.V().has('name','foo').property(["name": Cardinality.set("bar"), "age": 43 ])
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"name\",\"foo\")"
- the graph should return 1 for count of "g.V().has(\"name\",\"bar\")"
- the graph should return 1 for count of "g.V().has(\"age\",43)"
- the graph should return 0 for count of "g.V().has(\"age\",42)"

### 045. map/AddVertex.feature:471

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 48.1 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L471).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the result should have a count of 1

```gremlin
g.V().has('name','foo').property(Cardinality.set, ["name":"bar", "age": Cardinality.single(43) ])
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"name\",\"foo\")"
- the graph should return 1 for count of "g.V().has(\"name\",\"bar\")"
- the graph should return 1 for count of "g.V().has(\"age\",43)"
- the graph should return 0 for count of "g.V().has(\"age\",42)"

### 046. map/AddVertex.feature:570

**Assignment:** [F06](#f06). **Status:** fail. **Recorded time:** 104.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L570).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<1> but was:<0>
```

**Failed at:** the result should be unordered

```gremlin
g.V().limit(3).addV("software").aggregate("a1").by(T.label).aggregate("a2").by(T.label).cap("a1", "a2").
  select("a1","a2").by(unfold().fold())
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"a1": ["software", "software", "software"], "a2": ["software", "software", "software"]}]
  ```

### 047. map/AsString.feature:119

**Assignment:** [F15](#f15). **Status:** fail. **Recorded time:** 16.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AsString.feature#L119).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items ["v[1]", "v[2]", "v[3]", "v[4]", "v[5]", "v[6]"] in any order
     but: not matched: "person#0"
```

**Failed at:** the result should be unordered

```gremlin
g.V().asString()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  str[v[1]]
  str[v[2]]
  str[v[3]]
  str[v[4]]
  str[v[5]]
  str[v[6]]
  ```

### 048. map/AsString.feature:136

**Assignment:** [F15](#f15). **Status:** fail. **Recorded time:** 20.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AsString.feature#L136).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<[v[1], v[2], v[3], v[4], v[5], v[6]]>] in any order
     but: not matched: <[str[v[1]], str[v[2]], str[v[3]], str[v[4]], str[v[5]], str[v[6]]]>
```

**Failed at:** the result should be unordered

```gremlin
g.V().fold().asString(Scope.local).order(local)
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  l[str[v[1]],str[v[2]],str[v[3]],str[v[4]],str[v[5]],str[v[6]]]
  ```

### 049. map/AsString.feature:148

**Assignment:** [F15](#f15). **Status:** fail. **Recorded time:** 16.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AsString.feature#L148).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items ["e[7][1-knows->2]", "e[8][1-knows->4]", "e[9][1-created->3]", "e[10][4-created->5]", "e[11][4-created->3]", "e[12][6-created->3]"] in any order
     but: not matched: "created#0"
```

**Failed at:** the result should be unordered

```gremlin
g.E().asString()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  str[e[7][1-knows->2]]
  str[e[8][1-knows->4]]
  str[e[9][1-created->3]]
  str[e[10][4-created->5]]
  str[e[11][4-created->3]]
  str[e[12][6-created->3]]
  ```

### 050. map/Call.feature:102

**Assignment:** [F16](#f16). **Status:** fail. **Recorded time:** 18.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Call.feature#L102).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<v[person#0]>] in any order
     but: not matched: <v[person#1]>
```

**Failed at:** the result should be unordered

```gremlin
g.call("tinker.search", xx1).element()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[marko]
  ```

### 051. map/Call.feature:147

**Assignment:** [F16](#f16). **Status:** fail. **Recorded time:** 16.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Call.feature#L147).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<v[person#0]>] in any order
     but: not matched: <v[person#1]>
```

**Failed at:** the result should be unordered

```gremlin
g.call("tinker.search", xx1).with("type", "Vertex").element()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  v[marko]
  ```

### 052. map/Count.feature:91

**Assignment:** [F07](#f07). **Status:** timeout. **Recorded time:** 15,249.9 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Count.feature#L91).

**Recorded comparison:** sqlg=fail, puppygraph=skipped, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: adapter-timeout: Crabgraph adapter deadline
```

**Failed at:** the result should be ordered

```gremlin
g.V().repeat(__.out()).times(8).count()
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  d[2505037961767380].l
  ```

### 053. map/Count.feature:102

**Assignment:** [F07](#f07). **Status:** timeout. **Recorded time:** 15,539.2 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Count.feature#L102).

**Recorded comparison:** sqlg=fail, puppygraph=skipped, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: adapter-timeout: Crabgraph adapter deadline
```

**Failed at:** the result should be ordered

```gremlin
g.V().
  repeat(__.out()).
    times(5).as("a").
  out("writtenBy").as("b").
  select("a", "b").
  count()
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  d[24309134024].l
  ```

### 054. map/Index.feature:33

**Assignment:** [F17](#f17). **Status:** fail. **Recorded time:** 38.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Index.feature#L33).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: every item is one of {<{0=v[software#0]}>, <{0=v[software#1]}>}
     but: an item was <[[v[software#0], 0]]>
```

**Failed at:** the result should be of

```gremlin
g.V().hasLabel("software").order().by("name").index().with(WithOptions.indexer, WithOptions.map)
```

**Required outcome from original scenario:**

- the result should be of

  ```text
  result
  m[{"d[0].i": "v[lop]"}]
  m[{"d[0].i": "v[ripple]"}]
  ```
- the result should have a count of 2

### 055. map/Index.feature:58

**Assignment:** [F17](#f17). **Status:** fail. **Recorded time:** 15.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Index.feature#L58).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{0=josh, 1=marko, 2=peter, 3=vadas}>] in any order
     but: not matched: <[[josh, 0], [marko, 1], [peter, 2], [vadas, 3]]>
```

**Failed at:** the result should be unordered

```gremlin
g.V().hasLabel("person").values("name").fold().order(Scope.local).index().with(WithOptions.indexer, WithOptions.map)
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"d[0].i": "josh", "d[1].i": "marko", "d[2].i": "peter", "d[3].i": "vadas"}]
  ```

### 056. map/Map.feature:66

**Assignment:** [F18](#f18). **Status:** fail. **Recorded time:** 17.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L66).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=skipped.

**Observed:**

```text
java.lang.RuntimeException: java.lang.NullPointerException: Cannot invoke "org.apache.tinkerpop.gremlin.process.traversal.Traversal.toList()" because "this.traversal" is null
```

**Failed at:** the result should be unordered

```gremlin
g.V(vid1).out().map(Lambda.function("it.get().value('name')")).map(Lambda.function("it.get().toString().length()"))
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  d[3].i
  d[5].i
  d[4].i
  ```

**Boundary:** failure occurs before the lambda traversal executes; see F18.

### 057. map/Match.feature:483

**Assignment:** [F09](#f09). **Status:** fail. **Recorded time:** 305.2 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Match.feature#L483).

**Recorded comparison:** sqlg=pass, puppygraph=skipped, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<8> but was:<0>
```

**Failed at:** the result should be unordered

```gremlin
g.V().match(__.as("a").has("song", "name", "HERE COMES SUNSHINE"),
            __.as("a").map(__.inE("followedBy").values("weight").mean()).as("b"),
            __.as("a").inE("followedBy").as("c"),
            __.as("c").filter(__.values("weight").where(P.gte("b"))).outV().as("d")).
      select("d").by("name")
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  THE MUSIC NEVER STOPPED
  PROMISED LAND
  PLAYING IN THE BAND
  CASEY JONES
  BIG RIVER
  EL PASO
  LIBERTY
  LOOKS LIKE RAIN
  ```

### 058. map/Match.feature:548

**Assignment:** [F09](#f09). **Status:** fail. **Recorded time:** 352.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Match.feature#L548).

**Recorded comparison:** sqlg=pass, puppygraph=skipped, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable containing [<6L>]
     but: item 0: was <0L>
```

**Failed at:** the result should be ordered

```gremlin
g.V().match(__.as("a").out("followedBy").count().is(P.gt(10)).as("b"),
            __.as("a").in("followedBy").count().is(P.gt(10)).as("b")).count()
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  d[6].l
  ```

### 059. map/MergeEdge.feature:323

**Assignment:** [F05](#f05). **Status:** fail. **Recorded time:** 12.2 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L323).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: a string containing "Vertex id could not be resolved from mergeE" ignoring case
     but: was "org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: Vertex with id String("100") does not exist"
```

**Failed at:** the traversal will raise an error with message containing text of "Vertex id could not be resolved from mergeE"

```gremlin
g.mergeE(xx1)
```

**Required outcome from original scenario:**

- the traversal will raise an error with message containing text of "Vertex id could not be resolved from mergeE"

### 060. map/MergeEdge.feature:334

**Assignment:** [F05](#f05). **Status:** fail. **Recorded time:** 45.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L334).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: a string containing "Vertex id could not be resolved from mergeE" ignoring case
     but: was "org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: Vertex with id String("100") does not exist"
```

**Failed at:** the traversal will raise an error with message containing text of "Vertex id could not be resolved from mergeE"

```gremlin
g.mergeE(xx1).option(Merge.onCreate,xx2).option(Merge.onMatch,xx3)
```

**Required outcome from original scenario:**

- the traversal will raise an error with message containing text of "Vertex id could not be resolved from mergeE"

### 061. map/MergeEdge.feature:616

**Assignment:** [F03](#f03). **Status:** fail. **Recorded time:** 186.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L616).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<0> but was:<1>
```

**Failed at:** the graph should return 0 for count of "g.E().hasLabel(\"knows\").has(\"weight\")"

```gremlin
g.withSideEffect("c",xx2).withSideEffect("m",xx3).
  mergeE(xx1).
    option(Merge.onCreate, __.select("c")).
    option(Merge.onMatch, __.sideEffect(__.properties("weight").drop()).select("m"))
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 2 for count of "g.V()"
- the graph should return 0 for count of "g.E().hasLabel(\"knows\").has(\"created\",\"Y\")"
- the graph should return 1 for count of "g.E().hasLabel(\"knows\").has(\"created\",\"N\")"
- the graph should return 0 for count of "g.E().hasLabel(\"knows\").has(\"weight\")"

### 062. map/MergeEdge.feature:681

**Assignment:** [F04](#f04). **Status:** fail. **Recorded time:** 43.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L681).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: Creating user-supplied element IDs is not supported
```

**Failed at:** the result should have a count of 1

```gremlin
g.mergeE(xx1).option(onCreate, xx2)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 2 for count of "g.V()"
- the graph should return 1 for count of "g.E()"
- the graph should return 1 for count of "g.E(\"201\")"
- the graph should return 1 for count of "g.V().has(\"name\",\"marko\").out(\"knows\").has(\"name\",\"vadas\")"

### 063. map/MergeEdge.feature:703

**Assignment:** [F04](#f04). **Status:** fail. **Recorded time:** 44.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L703).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: Creating user-supplied element IDs is not supported
```

**Failed at:** the result should have a count of 1

```gremlin
g.mergeE(xx1).option(onCreate, xx2)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 2 for count of "g.V()"
- the graph should return 1 for count of "g.E()"
- the graph should return 1 for count of "g.E(\"201\")"
- the graph should return 1 for count of "g.V().has(\"name\",\"marko\").out(\"knows\").has(\"name\",\"vadas\")"

### 064. map/MergeEdge.feature:809

**Assignment:** [F04](#f04). **Status:** fail. **Recorded time:** 58.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L809).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: mergeV user-supplied T.id requires an element identity allocator
```

**Failed at:** the result should have a count of 1

```gremlin
g.mergeV(xx1).as("outV").mergeV(xx2).as("inV").mergeE(xx3).option(outV, select("outV")).option(inV, select("inV"))
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 2 for count of "g.V()"
- the graph should return 1 for count of "g.E()"
- the graph should return 1 for count of "g.V().has(\"name\",\"marko\").out(\"knows\").has(\"name\",\"vadas\")"

### 065. map/MergeEdge.feature:830

**Assignment:** [F05](#f05). **Status:** fail. **Recorded time:** 50.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L830).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError
```

**Failed at:** the traversal will raise an error with message containing text of "Property key can not be a hidden key: ~label"

```gremlin
g.V().as("v").mergeE(xx1).option(Merge.onMatch,xx2).option(Merge.outV,select("v")).option(Merge.inV,select("v"))
```

**Required outcome from original scenario:**

- the traversal will raise an error with message containing text of "Property key can not be a hidden key: ~label"

### 066. map/MergeEdge.feature:845

**Assignment:** [F05](#f05). **Status:** fail. **Recorded time:** 108.9 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L845).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError
```

**Failed at:** the traversal will raise an error with message containing text of "The incoming traverser for MergeEdgeStep cannot be an Element"

```gremlin
g.V().mergeE(xx1).
        option(Merge.onMatch, __.sideEffect(__.property("weight", 0)).constant([:]))
```

**Required outcome from original scenario:**

- the traversal will raise an error with message containing text of "The incoming traverser for MergeEdgeStep cannot be an Element"

### 067. map/MergeEdge.feature:862

**Assignment:** [F05](#f05). **Status:** fail. **Recorded time:** 113.9 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L862).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<0> but was:<1>
```

**Failed at:** the graph should return 0 for count of "g.E().hasLabel(\"knows\").has(\"weight\",1)"

```gremlin
g.mergeE(xx1).
    option(Merge.onMatch, __.sideEffect(__.property("weight", 0)).constant([:]))
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 2 for count of "g.V()"
- the graph should return 0 for count of "g.E().hasLabel(\"knows\").has(\"weight\",1)"
- the graph should return 1 for count of "g.E().hasLabel(\"knows\").has(\"weight\",0)"
- the graph should return 0 for count of "g.V().has(\"weight\")"

### 068. map/MergeVertex.feature:401

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 26.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L401).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property meta-properties
```

**Failed at:** the result should have a count of 1

```gremlin
g.mergeV(xx1).property("name","vadas","acl","public")
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().properties(\"name\").hasValue(\"vadas\").has(\"acl\",\"public\")"

### 069. map/MergeVertex.feature:515

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 27.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L515).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property meta-properties
```

**Failed at:** the result should have a count of 1

```gremlin
g.inject(0).mergeV(xx1).property("name","vadas","acl","public")
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().properties(\"name\").hasValue(\"vadas\").has(\"acl\",\"public\")"

### 070. map/MergeVertex.feature:567

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 33.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L567).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.mergeV(xx1).property(Cardinality.list,"name","steve")
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V()"
- the graph should return 1 for count of "g.V().properties(\"name\").hasValue(\"steve\")"
- the graph should return 1 for count of "g.V().properties(\"name\").hasValue(\"stephen\")"
- the graph should return 2 for count of "g.V().properties(\"name\")"

### 071. map/MergeVertex.feature:621

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 39.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L621).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.withSideEffect("c", xx1).
  withSideEffect("m", xx2).
  mergeV(__.select("c")).
    option(Merge.onMatch, __.sideEffect(__.properties("age").drop()).select("m"))
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\", 19)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\")"

### 072. map/MergeVertex.feature:642

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 29.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L642).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.withSideEffect("m", xx1).
  V().has("person", "name", "marko").
  mergeV([:]).
    option(Merge.onMatch, __.sideEffect(__.properties("age").drop()).select("m"))
```

**Required outcome from original scenario:**

- the traversal will raise an error with message containing text of "The incoming traverser for MergeVertexStep cannot be an Element"

### 073. map/MergeVertex.feature:661

**Assignment:** [F04](#f04). **Status:** fail. **Recorded time:** 12.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L661).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property key must be a string
```

**Failed at:** the graph initializer of

```gremlin
g.mergeV(xx1).option(Merge.onCreate, xx2)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V()"
- the graph should return 1 for count of "g.V(\"1\").has(\"person\",\"name\",\"mike\")"

### 074. map/MergeVertex.feature:680

**Assignment:** [F04](#f04). **Status:** fail. **Recorded time:** 6.4 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L680).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: mergeV user-supplied T.id requires an element identity allocator
```

**Failed at:** the result should have a count of 1

```gremlin
g.mergeV(xx1).option(Merge.onCreate, xx2)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V()"
- the graph should return 1 for count of "g.V(\"1\").has(\"person\",\"name\",\"mike\")"

### 075. map/MergeVertex.feature:695

**Assignment:** [F04](#f04). **Status:** fail. **Recorded time:** 6.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L695).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: mergeV user-supplied T.id requires an element identity allocator
```

**Failed at:** the result should have a count of 1

```gremlin
g.mergeV(xx1).option(Merge.onCreate, xx2)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V()"
- the graph should return 1 for count of "g.V(\"1\").has(\"person\",\"name\",\"mike\")"

### 076. map/MergeVertex.feature:722

**Assignment:** [F05](#f05). **Status:** fail. **Recorded time:** 6.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L722).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: a string containing "option(onCreate) cannot override values from merge() argument" ignoring case
     but: was "org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: mergeV user-supplied T.id requires an element identity allocator"
```

**Failed at:** the traversal will raise an error with message containing text of "option(onCreate) cannot override values from merge() argument"

```gremlin
g.mergeV(xx1).option(onCreate, xx2)
```

**Required outcome from original scenario:**

- the traversal will raise an error with message containing text of "option(onCreate) cannot override values from merge() argument"

### 077. map/MergeVertex.feature:830

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 28.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L830).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.mergeV([name: "marko"]).
    option(Merge.onMatch, [age: Cardinality.list(33)])
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\", 33)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\")"
- the graph should return 4 for count of "g.V().has(\"person\",\"name\",\"marko\").properties(\"age\")"

### 078. map/MergeVertex.feature:848

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 27.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L848).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.mergeV([name: "marko"]).
    option(Merge.onMatch, [age: Cardinality.set(33)])
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\", 33)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\")"
- the graph should return 4 for count of "g.V().has(\"person\",\"name\",\"marko\").properties(\"age\")"

### 079. map/MergeVertex.feature:866

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 26.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L866).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.mergeV([name: "marko"]).
    option(Merge.onMatch, [age: Cardinality.set(31)])
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\", 31)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\")"
- the graph should return 3 for count of "g.V().has(\"person\",\"name\",\"marko\").properties(\"age\")"

### 080. map/MergeVertex.feature:884

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 28.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L884).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.mergeV([name: "marko"]).
    option(Merge.onMatch, [age: Cardinality.single(33)])
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\", 33)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\")"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").properties(\"age\")"

### 081. map/MergeVertex.feature:902

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 27.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L902).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.mergeV([name: "marko"]).
    option(Merge.onMatch, [age: 33], Cardinality.single)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\", 33)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").has(\"age\")"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"marko\").properties(\"age\")"

### 082. map/MergeVertex.feature:920

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 28.2 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L920).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.mergeV([name: "marko"]).
    option(Merge.onMatch, [name: "allen", age: Cardinality.set(31)], single)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 0 for count of "g.V().has(\"person\",\"name\",\"marko\")"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"allen\").has(\"age\", 31)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"allen\").has(\"age\")"
- the graph should return 3 for count of "g.V().has(\"person\",\"name\",\"allen\").properties(\"age\")"

### 083. map/MergeVertex.feature:939

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 27.9 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L939).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: property cardinality requires single
```

**Failed at:** the graph initializer of

```gremlin
g.mergeV([name: "marko"]).
    option(Merge.onMatch, [name: "allen", age: Cardinality.single(31)], single)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 0 for count of "g.V().has(\"person\",\"name\",\"marko\")"
- the graph should return 0 for count of "g.V().has(\"person\",\"name\",\"allen\").has(\"age\", 33)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"allen\").has(\"age\", 31)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"allen\").has(\"age\")"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"allen\").properties(\"age\")"

### 084. map/MergeVertex.feature:973

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 14.1 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L973).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: mergeV cardinality value requires single in an option map
```

**Failed at:** the result should have a count of 1

```gremlin
g.mergeV([name: "alice", (T.label): "person"]).
    option(Merge.onCreate, [age: Cardinality.set(81)])
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"alice\").has(\"age\", 81)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"alice\").has(\"age\")"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"alice\").properties(\"age\")"

### 085. map/MergeVertex.feature:987

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 7.6 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L987).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: unsupported gremlin construct: mergeV option requires a static map
```

**Failed at:** the result should have a count of 1

```gremlin
g.mergeV([name: "alice", (T.label): "person"]).
    option(Merge.onCreate, [age: 81], Cardinality.set)
```

**Required outcome from original scenario:**

- the result should have a count of 1
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"alice\").has(\"age\", 81)"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"alice\").has(\"age\")"
- the graph should return 1 for count of "g.V().has(\"person\",\"name\",\"alice\").properties(\"age\")"

### 086. map/Order.feature:575

**Assignment:** [F19](#f19). **Status:** fail. **Recorded time:** 39.1 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Order.feature#L575).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable containing [<{name=marko}>, <{label=person}>, <{id=person#0}>, <{age=29}>]
     but: item 0: was <{label=person}>
```

**Failed at:** the result should be ordered

```gremlin
g.V(vid1).elementMap().order(Scope.local).by(Column.keys, Order.desc).unfold()
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  m[{"name":"marko"}]
  m[{"t[label]":"person"}]
  m[{"t[id]":"v[marko].id"}]
  m[{"age":29}]
  ```

### 087. map/Order.feature:590

**Assignment:** [F19](#f19). **Status:** fail. **Recorded time:** 31.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Order.feature#L590).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable containing [<{age=29}>, <{id=person#0}>, <{label=person}>, <{name=marko}>]
     but: item 1: was <{name=marko}>
```

**Failed at:** the result should be ordered

```gremlin
g.V(vid1).elementMap().order(Scope.local).by(Column.keys, Order.asc).unfold()
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  m[{"age":29}]
  m[{"t[id]":"v[marko].id"}]
  m[{"t[label]":"person"}]
  m[{"name":"marko"}]
  ```

### 088. map/Properties.feature:91

**Assignment:** [F20](#f20). **Status:** fail. **Recorded time:** 19.4 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Properties.feature#L91).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items ["marko", <29>, "vadas", <27>, "josh", <32>, "peter", <35>, "lop", "ripple"] in any order
     but: not matched: "29"
```

**Failed at:** the result should be unordered

```gremlin
g.V().values("name", "age", null)
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  marko
  d[29].i
  vadas
  d[27].i
  josh
  d[32].i
  peter
  d[35].i
  lop
  ripple
  ```

### 089. map/Select.feature:733

**Assignment:** [F08](#f08). **Status:** fail. **Recorded time:** 15.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Select.feature#L733).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<2> but was:<0>
```

**Failed at:** the result should be ordered

```gremlin
g.V().as("a", "b").out().as("c").path().select(Column.keys)
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  s[a,b]
  s[c]
  ```
- the graph should return 6 for count of "g.V().as(\"a\", \"b\").out().as(\"c\").path().select(Column.keys)"

### 090. map/ValueMap.feature:117

**Assignment:** [F21](#f21). **Status:** fail. **Recorded time:** 14.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ValueMap.feature#L117).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{name=marko, age=29, label=person}>, <{name=josh, age=32, label=person}>, <{name=peter, age=35, label=person}>, <{name=vadas, age=27, label=person}>, <{name=lop, label=software}>, <{name=ripple, label=software}>] in any order
     but: not matched: <{id=person#0, label=person, age=29, name=marko}>
```

**Failed at:** the result should be unordered

```gremlin
g.V().valueMap("name", "age").with(WithOptions.tokens, WithOptions.labels).by(__.unfold())
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"t[label]": "person", "name": "marko", "age": 29}]
  m[{"t[label]": "person", "name": "josh", "age": 32}]
  m[{"t[label]": "person", "name": "peter", "age": 35}]
  m[{"t[label]": "person", "name": "vadas", "age": 27}]
  m[{"t[label]": "software", "name": "lop"}]
  m[{"t[label]": "software", "name": "ripple"}]
  ```

### 091. map/ValueMap.feature:133

**Assignment:** [F21](#f21). **Status:** fail. **Recorded time:** 66.2 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ValueMap.feature#L133).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{id=person#0, name=marko, age=29}>, <{id=person#2, name=josh, age=32}>, <{id=person#3, name=peter, age=35}>, <{id=person#1, name=vadas, age=27}>, <{id=software#0, name=lop}>, <{id=software#1, name=ripple}>] in any order
     but: not matched: <{id=person#0, label=person, age=29, name=marko}>
```

**Failed at:** the result should be unordered

```gremlin
g.V().valueMap("name", "age").with(WithOptions.tokens, WithOptions.ids).by(__.unfold())
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"t[id]": "v[marko].id", "name": "marko", "age": 29}]
  m[{"t[id]": "v[josh].id", "name": "josh", "age": 32}]
  m[{"t[id]": "v[peter].id", "name": "peter", "age": 35}]
  m[{"t[id]": "v[vadas].id", "name": "vadas", "age": 27}]
  m[{"t[id]": "v[lop].id", "name": "lop"}]
  m[{"t[id]": "v[ripple].id", "name": "ripple"}]
  ```

### 092. map/ValueMap.feature:221

**Assignment:** [F21](#f21). **Status:** fail. **Recorded time:** 22.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ValueMap.feature#L221).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{age=29}>, <{age=32}>, <{age=35}>, <{age=27}>, <{}>, <{}>] in any order
     but: not matched: <{name=[marko], age=[29]}>
```

**Failed at:** the result should be unordered

```gremlin
g.V().valueMap("name", "age").by(__.is("x")).by(__.unfold())
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"age": 29}]
  m[{"age": 32}]
  m[{"age": 35}]
  m[{"age": 27}]
  m[{}]
  m[{}]
  ```

### 093. semantics/Orderability.feature:44

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 35.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/semantics/Orderability.feature#L44).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.ClassCastException: class java.util.LinkedHashMap cannot be cast to class org.apache.tinkerpop.gremlin.structure.VertexProperty (java.util.LinkedHashMap is in module java.base of loader 'bootstrap'; org.apache.tinkerpop.gremlin.structure.VertexProperty is in unnamed module of loader 'app')
```

**Failed at:** the result should be ordered

```gremlin
g.V().properties().order()
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  vp[marko-name->marko]
  vp[marko-age->d[29].i]
  vp[vadas-name->vadas]
  vp[vadas-age->d[27].i]
  vp[lop-name->lop]
  vp[lop-lang->java]
  vp[josh-name->josh]
  vp[josh-age->d[32].i]
  vp[ripple-name->ripple]
  vp[ripple-lang->java]
  vp[peter-name->peter]
  vp[peter-age->d[35].i]
  ```

### 094. semantics/Orderability.feature:67

**Assignment:** [F02](#f02). **Status:** fail. **Recorded time:** 15.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/semantics/Orderability.feature#L67).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable containing [<0L>, <1L>, <2L>, <3L>, <4L>, <5L>, <6L>, <7L>, <8L>, <9L>, <10L>, <11L>]
     but: item 1: was <0L>
```

**Failed at:** the result should be ordered

```gremlin
g.V().properties().order().id()
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  d[0].l
  d[1].l
  d[2].l
  d[3].l
  d[4].l
  d[5].l
  d[6].l
  d[7].l
  d[8].l
  d[9].l
  d[10].l
  d[11].l
  ```

### 095. semantics/Orderability.feature:312

**Assignment:** [F19](#f19). **Status:** fail. **Recorded time:** 17.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/semantics/Orderability.feature#L312).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable containing ["lop", "java", "ripple", "java"]
     but: item 0: was "java"
```

**Failed at:** the result should be ordered

```gremlin
g.V().out().out().properties().as("head").path().order().by(asc).select("head").value()
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  lop
  java
  ripple
  java
  ```

### 096. semantics/Orderability.feature:328

**Assignment:** [F19](#f19). **Status:** fail. **Recorded time:** 17.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/semantics/Orderability.feature#L328).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable containing ["java", "ripple", "java", "lop"]
     but: item 1: was "java"
```

**Failed at:** the result should be ordered

```gremlin
g.V().out().out().properties().as("head").path().order().by(desc).select("head").value()
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  java
  ripple
  java
  lop
  ```

### 097. semantics/Orderability.feature:343

**Assignment:** [F19](#f19). **Status:** fail. **Recorded time:** 18.1 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/semantics/Orderability.feature#L343).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable containing ["java", "lop", "java", "ripple"]
     but: item 1: was "java"
```

**Failed at:** the result should be ordered

```gremlin
g.V().out().out().values().as("head").path().order().by(asc).select("head")
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  java
  lop
  java
  ripple
  ```

### 098. semantics/Orderability.feature:358

**Assignment:** [F19](#f19). **Status:** fail. **Recorded time:** 18.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/semantics/Orderability.feature#L358).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable containing ["ripple", "java", "lop", "java"]
     but: item 0: was "java"
```

**Failed at:** the result should be ordered

```gremlin
g.V().out().out().values().as("head").path().order().by(desc).select("head")
```

**Required outcome from original scenario:**

- the result should be ordered

  ```text
  result
  ripple
  java
  lop
  java
  ```

### 099. sideEffect/Group.feature:165

**Assignment:** [F22](#f22). **Status:** fail. **Recorded time:** 11,581.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Group.feature#L165).

**Recorded comparison:** sqlg=pass, puppygraph=skipped, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{cover={followedBy=777982}, ={followedBy=179350}, original={followedBy=2185613}}>] in any order
     but: not matched: <{={followedBy=179350}, cover={followedBy=777982, sungBy=0, writtenBy=0}, original={followedBy=2185613, sungBy=0, writtenBy=0}}>
```

**Failed at:** the result should be unordered

```gremlin
g.V().out("followedBy").group().by("songType").by(__.bothE().group().by(T.label).by(__.values("weight").sum()))
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"cover":{"followedBy":"d[777982].l"}, "":{"followedBy":"d[179350].l"}, "original":{"followedBy":"d[2185613].l"}}]
  ```

### 100. sideEffect/Group.feature:206

**Assignment:** [F22](#f22). **Status:** fail. **Recorded time:** 17.7 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Group.feature#L206).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{ripple=32, lop=96}>] in any order
     but: not matched: <{lop=96, ripple=32}>
```

**Failed at:** the result should be unordered

```gremlin
g.V().hasLabel("person").as("p").out("created").group().by("name").by(__.select("p").values("age").sum())
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"ripple":"d[32].l", "lop":"d[96].l"}]
  ```

**Type diagnostic:** recorded GraphSON map values are `g:Int32`; expected `d[96].l` and `d[32].l` are Long. Equal printed numbers do not satisfy this assertion.

### 101. sideEffect/Inject.feature:89

**Assignment:** [F06](#f06). **Status:** fail. **Recorded time:** 26.3 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Inject.feature#L89).

**Recorded comparison:** sqlg=fail, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{a=10, b=3}>, <{a=20, b=2}>, <{a=null, b=1}>] in any order
     but: not matched: <{a=10, b=1}>
```

**Failed at:** the result should be unordered

```gremlin
g.inject(10,20,null,20,10,10).groupCount("x").dedup().as("y").project("a","b").by().by(__.select("x").select(__.select("y")))
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"a":"d[10].i", "b":"d[3].l"}]
  m[{"a":"d[20].i", "b":"d[2].l"}]
  m[{"a":null, "b":"d[1].l"}]
  ```

### 102. sideEffect/Read.feature:21

**Assignment:** [F23](#f23). **Status:** fail. **Recorded time:** 22.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Read.feature#L21).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<6> but was:<0>
```

**Failed at:** the graph should return 6 for count of "g.V()"

```gremlin
g.io("data/tinkerpop-modern.kryo").read()
```

**Required outcome from original scenario:**

- the result should be empty
- the graph should return 6 for count of "g.V()"
- the graph should return 6 for count of "g.E()"

### 103. sideEffect/Read.feature:32

**Assignment:** [F23](#f23). **Status:** fail. **Recorded time:** 25.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Read.feature#L32).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<6> but was:<0>
```

**Failed at:** the graph should return 6 for count of "g.V()"

```gremlin
g.io("data/tinkerpop-modern.kryo").with(IO.reader, IO.gryo).read()
```

**Required outcome from original scenario:**

- the result should be empty
- the graph should return 6 for count of "g.V()"
- the graph should return 6 for count of "g.E()"

### 104. sideEffect/Read.feature:43

**Assignment:** [F23](#f23). **Status:** fail. **Recorded time:** 20.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Read.feature#L43).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<6> but was:<0>
```

**Failed at:** the graph should return 6 for count of "g.V()"

```gremlin
g.io("data/tinkerpop-modern.json").read()
```

**Required outcome from original scenario:**

- the result should be empty
- the graph should return 6 for count of "g.V()"
- the graph should return 6 for count of "g.E()"

### 105. sideEffect/Read.feature:54

**Assignment:** [F23](#f23). **Status:** fail. **Recorded time:** 26.2 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Read.feature#L54).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<6> but was:<0>
```

**Failed at:** the graph should return 6 for count of "g.V()"

```gremlin
g.io("data/tinkerpop-modern.json").with(IO.reader, IO.graphson).read()
```

**Required outcome from original scenario:**

- the result should be empty
- the graph should return 6 for count of "g.V()"
- the graph should return 6 for count of "g.E()"

### 106. sideEffect/Read.feature:65

**Assignment:** [F23](#f23). **Status:** fail. **Recorded time:** 20.0 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Read.feature#L65).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<6> but was:<0>
```

**Failed at:** the graph should return 6 for count of "g.V()"

```gremlin
g.io("data/tinkerpop-modern.xml").read()
```

**Required outcome from original scenario:**

- the result should be empty
- the graph should return 6 for count of "g.V()"
- the graph should return 6 for count of "g.E()"

### 107. sideEffect/Read.feature:76

**Assignment:** [F23](#f23). **Status:** fail. **Recorded time:** 21.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Read.feature#L76).

**Recorded comparison:** sqlg=fail, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<6> but was:<0>
```

**Failed at:** the graph should return 6 for count of "g.V()"

```gremlin
g.io("data/tinkerpop-modern.xml").with(IO.reader, IO.graphml).read()
```

**Required outcome from original scenario:**

- the result should be empty
- the graph should return 6 for count of "g.V()"
- the graph should return 6 for count of "g.E()"

### 108. sideEffect/Sack.feature:61

**Assignment:** [F24](#f24). **Status:** fail. **Recorded time:** 31.1 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Sack.feature#L61).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: parse: line 1:91 no viable alternative at input 'g.withBulk(false).withSack(1.0D,Operator.sum).V("person#0").local(__.outE("knows").barrier(SackFunctions' near [@32,91:103='SackFunctions',<318>,1:91]
```

**Failed at:** the result should be unordered

```gremlin
g.withBulk(false).withSack(1.0d, Operator.sum).V(vid1).local(__.outE("knows").barrier(Barrier.normSack).inV()).in("knows").barrier().sack()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  d[1.0].d
  ```

### 109. sideEffect/Sack.feature:74

**Assignment:** [F24](#f24). **Status:** fail. **Recorded time:** 34.1 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Sack.feature#L74).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.AssertionError: expected:<4> but was:<6>
```

**Failed at:** the result should be unordered

```gremlin
g.withBulk(false).withSack(1, Operator.sum).V().out().barrier().sack()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  d[3].i
  d[1].i
  d[1].i
  d[1].i
  ```

### 110. sideEffect/Sack.feature:88

**Assignment:** [F24](#f24). **Status:** fail. **Recorded time:** 15.4 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Sack.feature#L88).

**Recorded comparison:** sqlg=pass, puppygraph=fail, reference=pass.

**Observed:**

```text
java.lang.RuntimeException: java.lang.IllegalStateException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: org.apache.tinkerpop.gremlin.process.remote.RemoteConnectionException: parse: line 1:74 no viable alternative at input 'g.withSack(1.0D,Operator.sum).V("person#0").local(__.out("knows").barrier(SackFunctions' near [@27,74:86='SackFunctions',<318>,1:74]
```

**Failed at:** the result should be unordered

```gremlin
g.withSack(1.0d, Operator.sum).V(vid1).local(__.out("knows").barrier(Barrier.normSack)).in("knows").barrier().sack()
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  d[1.0].d
  d[1.0].d
  ```

### 111. sideEffect/SideEffectCap.feature:67

**Assignment:** [F06](#f06). **Status:** fail. **Recorded time:** 9.8 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/SideEffectCap.feature#L67).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{josh=1, ripple=2, lop=4, vadas=1}>] in any order
     but: not matched: <{}>
```

**Failed at:** the result should be unordered

```gremlin
g.V().repeat(__.out().group("a").by("name").by(__.count())).times(2).cap("a")
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"ripple":"d[2].l", "vadas":"d[1].l", "josh":"d[1].l", "lop":"d[4].l"}]
  ```

### 112. sideEffect/SideEffectCap.feature:122

**Assignment:** [F22](#f22). **Status:** fail. **Recorded time:** 26.5 ms. [Pinned scenario](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/SideEffectCap.feature#L122).

**Recorded comparison:** sqlg=pass, puppygraph=pass, reference=pass.

**Observed:**

```text
java.lang.AssertionError:
Expected: iterable with items [<{ripple=32, lop=96}>] in any order
     but: not matched: <{lop=96, ripple=32}>
```

**Failed at:** the result should be unordered

```gremlin
g.V().hasLabel("person").as("p").out("created").group("a").by("name").by(__.select("p").values("age").sum()).cap("a")
```

**Required outcome from original scenario:**

- the result should be unordered

  ```text
  result
  m[{"ripple":"d[32].l", "lop":"d[96].l"}]
  ```

**Type diagnostic:** recorded GraphSON map values are `g:Int32`; expected `d[96].l` and `d[32].l` are Long. Equal printed numbers do not satisfy this assertion.

## Skipped cases: separate capability backlog

These **106 cases are not part of the 112-failure count**. They must remain visible when claiming completeness. The following actions cover every recorded skip reason. Retain upstream expectations and exact skip reasons; enable cases only when the corresponding adapter/engine capability exists.

| Work item | Recorded category | Count | What needs to change |
|---|---|---:|---|
| S01 | GraphComputer-only profile | 31 | Provide actual GraphComputer/vertex-program execution and algorithm state, or retain a clearly separate unsupported execution profile. Existing fixture-name-based virtual algorithm values in `runtime/property_object.rs` cannot establish generic algorithm support. Test on other graphs before enabling these cases. |
| S02 | Lambda parameters excluded by gremlin-language | 27 | Provide a deliberate supported lambda execution path (including language/runtime and closure semantics), or retain the upstream language-profile exclusion. Translating a few known lambdas into special cases is insufficient. Related direct-lambda failure: F18. |
| S03 | Fixture multi/meta-properties lost by mapping bridge | 23 | Complete A1 and load property IDs, cardinalities and meta-properties without flattening. Re-enable the original fixtures and execute all later assertions. |
| S04 | Remote file-write assertions unavailable | 6 | Add local writer/round-trip evidence with real Gryo/GraphSON/GraphML codecs. Preserve the upstream Gherkin skip unless using an upstream-supported execution path that can assert the written file; supplemental tests are not replacement upstream passes. |
| S05 | Null property values profile excluded | 5 | Define provider null-property capability and distinguish missing from present-null in storage, mutation, has/values/valueMap and transport. Enable the profile only after its contract is implemented. |
| S06 | Edge parameter transport unsupported | 3 | Preserve native Edge identity, endpoints and label in parameter translation through gremlin-language or another upstream-supported runner path. |
| S07 | Property identifiers/assertions unsupported by GLV harness | 2 | Complete A1 and add an upstream-compatible property parameter/assertion transport; leave original assertions intact. |
| S08 | Empty Set parameter unsupported | 2 | Preserve a typed empty Set through the adapter instead of treating it as an empty List. |
| S09 | Other upstream Gherkin/GLV limitations | 7 | See each exact reason below: native vertex-property results, nested typed map assertions, BigInteger assignment, JVM UnaryOperator sack overload, and numeric rounding. First establish whether an upstream-supported runner can execute the scenario unchanged; otherwise retain the exclusion and report supplemental tests separately. |

### Every skipped scenario

| Scenario | Work item | Recorded reason |
|---|---|---|

| [branch/Branch.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/branch/Branch.feature#L21) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [branch/Choose.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/branch/Choose.feature#L37) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [branch/Local.feature:22](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/branch/Local.feature#L22) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [branch/Repeat.feature:215](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/branch/Repeat.feature#L215) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [branch/Repeat.feature:338](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/branch/Repeat.feature#L338) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [filter/Dedup.feature:95](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Dedup.feature#L95) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Dedup.feature:323](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Dedup.feature#L323) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [filter/Filter.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L21) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Filter.feature:31](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L31) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Filter.feature:48](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L48) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Filter.feature:61](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L61) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Filter.feature:72](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L72) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Filter.feature:85](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L85) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Filter.feature:98](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L98) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Filter.feature:111](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L111) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Filter.feature:121](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L121) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Has.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Has.feature#L21) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [filter/Has.feature:249](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Has.feature#L249) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [filter/HasLabel.feature:46](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/HasLabel.feature#L46) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [integrated/SubgraphStrategy.feature:615](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L615) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [integrated/SubgraphStrategy.feature:631](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L631) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [integrated/SubgraphStrategy.feature:647](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L647) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [integrated/SubgraphStrategy.feature:664](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L664) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [integrated/SubgraphStrategy.feature:681](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L681) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [integrated/SubgraphStrategy.feature:697](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L697) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [integrated/SubgraphStrategy.feature:713](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/integrated/SubgraphStrategy.feature#L713) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/AddEdge.feature:422](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddEdge.feature#L422) | S05 | Upstream execution profile excludes @AllowNullPropertyValues |
| [map/AddVertex.feature:532](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L532) | S05 | Upstream execution profile excludes @AllowNullPropertyValues |
| [map/AddVertex.feature:543](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L543) | S05 | Upstream execution profile excludes @AllowNullPropertyValues |
| [map/Combine.feature:162](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Combine.feature#L162) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/Conjoin.feature:104](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Conjoin.feature#L104) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/ConnectedComponent.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ConnectedComponent.feature#L21) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ConnectedComponent.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ConnectedComponent.feature#L37) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ConnectedComponent.feature:53](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ConnectedComponent.feature#L53) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ConnectedComponent.feature:65](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ConnectedComponent.feature#L65) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/Difference.feature:146](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Difference.feature#L146) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/Disjunct.feature:133](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Disjunct.feature#L133) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/Element.feature:101](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Element.feature#L101) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/Element.feature:114](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Element.feature#L114) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/Intersect.feature:146](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Intersect.feature#L146) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/Map.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L21) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [map/Map.feature:34](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L34) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [map/Map.feature:49](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L49) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [map/Map.feature:80](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L80) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [map/Map.feature:97](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L97) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [map/Merge.feature:177](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Merge.feature#L177) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/MergeEdge.feature:953](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L953) | S05 | Upstream execution profile excludes @AllowNullPropertyValues |
| [map/MergeVertex.feature:1054](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L1054) | S05 | Upstream execution profile excludes @AllowNullPropertyValues |
| [map/Order.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Order.feature#L37) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [map/Order.feature:103](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Order.feature#L103) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [map/Order.feature:172](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Order.feature#L172) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [map/Order.feature:388](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Order.feature#L388) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [map/PageRank.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L21) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PageRank.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L37) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PageRank.feature:51](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L51) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PageRank.feature:67](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L67) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PageRank.feature:79](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L79) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PageRank.feature:95](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L95) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PageRank.feature:109](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L109) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PageRank.feature:121](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L121) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PageRank.feature:135](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L135) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PeerPressure.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PeerPressure.feature#L21) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PeerPressure.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PeerPressure.feature#L37) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/PeerPressure.feature:48](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PeerPressure.feature#L48) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/Product.feature:162](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Product.feature#L162) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/Properties.feature:111](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Properties.feature#L111) | S09 | This test is not supported by Gherkin because: The test suite doesn't do well with vertex property values. |
| [map/Properties.feature:118](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Properties.feature#L118) | S07 | This test is not supported by Gherkin because: GLV suite doesn't support property identifiers and related assertions |
| [map/Properties.feature:125](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Properties.feature#L125) | S07 | This test is not supported by Gherkin because: GLV suite doesn't support property identifiers and related assertions |
| [map/Select.feature:132](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Select.feature#L132) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/ShortestPath.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L21) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:67](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L67) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:113](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L113) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:159](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L159) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:182](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L182) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:205](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L205) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:230](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L230) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:246](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L246) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:262](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L262) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:278](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L278) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:290](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L290) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:304](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L304) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:318](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L318) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:334](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L334) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/ShortestPath.feature:348](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L348) | S01 | Upstream execution profile excludes @GraphComputerOnly |
| [map/Unfold.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Unfold.feature#L37) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [map/ValueMap.feature:190](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ValueMap.feature#L190) | S03 | adapter-skip: fixture has multi/meta-properties that the mapping bridge cannot preserve |
| [map/Vertex.feature:190](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Vertex.feature#L190) | S06 | This test uses a Edge as a parameter which is not supported by gremlin-language |
| [map/Vertex.feature:202](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Vertex.feature#L202) | S06 | This test uses a Edge as a parameter which is not supported by gremlin-language |
| [map/Vertex.feature:216](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Vertex.feature#L216) | S06 | This test uses a Edge as a parameter which is not supported by gremlin-language |
| [sideEffect/Aggregate.feature:175](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Aggregate.feature#L175) | S08 | This test uses a empty Set as a parameter which is not supported by gremlin-language |
| [sideEffect/Group.feature:118](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Group.feature#L118) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [sideEffect/Group.feature:151](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Group.feature#L151) | S09 | This test is not supported by Gherkin because: The result is [a:[32:[v[4]],27:[v[2]],josh:[v[4]],ripple:[v[5],v[5]],lop:[v[3],v[3],v[3],v[3]],vadas:[v[2]]],b:[ripple:2,lop:4]] which is not supported by this test suite |
| [sideEffect/Group.feature:158](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Group.feature#L158) | S09 | This test is not supported by Gherkin because: The result is [1:[software:[v[5]],person:[v[2],v[6]]],3:[software:[v[3]],person:[v[1],v[4]]]] which is not supported by this test suite |
| [sideEffect/Inject.feature:39](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Inject.feature#L39) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [sideEffect/Sack.feature:115](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Sack.feature#L115) | S09 | This test is not supported by Gherkin because: GLV Suite does not support BigInteger assignments at this time. |
| [sideEffect/Sack.feature:122](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Sack.feature#L122) | S09 | This test is not supported by Gherkin because: This test is bound pretty tightly to the JVM in that it requires a UnaryOperator cast to get the right withSack() method called. Not sure how that would work with a GLV. |
| [sideEffect/Sack.feature:130](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Sack.feature#L130) | S09 | This test is not supported by Gherkin because: Something strange happens with rounding that prevents GLVs from asserting this result properly. |
| [sideEffect/SideEffectCap.feature:100](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/SideEffectCap.feature#L100) | S02 | This test uses a lambda as a parameter which is not supported by gremlin-language |
| [sideEffect/SideEffectCap.feature:156](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/SideEffectCap.feature#L156) | S09 | This test is not supported by Gherkin because: The result is [a:[32:1,35:1,27:1,29:1],b:[ripple:1,lop:1]] which is not supported by this test suite |
| [sideEffect/Store.feature:52](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Store.feature#L52) | S08 | This test uses a empty Set as a parameter which is not supported by gremlin-language |
| [sideEffect/Write.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L21) | S04 | This test is not supported by Gherkin because: We don't have a nice way to assert the remotely written file with this framework - just testing compilation. |
| [sideEffect/Write.feature:28](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L28) | S04 | This test is not supported by Gherkin because: We don't have a nice way to assert the remotely written file with this framework - just testing compilation. |
| [sideEffect/Write.feature:35](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L35) | S04 | This test is not supported by Gherkin because: We don't have a nice way to assert the remotely written file with this framework - just testing compilation. |
| [sideEffect/Write.feature:42](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L42) | S04 | This test is not supported by Gherkin because: We don't have a nice way to assert the remotely written file with this framework - just testing compilation. |
| [sideEffect/Write.feature:49](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L49) | S04 | This test is not supported by Gherkin because: We don't have a nice way to assert the remotely written file with this framework - just testing compilation. |
| [sideEffect/Write.feature:56](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L56) | S04 | This test is not supported by Gherkin because: We don't have a nice way to assert the remotely written file with this framework - just testing compilation. |

## Local verification and completion criteria

1. Start with a clean, identified source snapshot and record the runner binary hash, source revision, upstream pin and fixture/adapter versions. Keep the current JSON as the before artifact.
2. For each workstream, run focused regressions plus its exact upstream scenarios locally, using unchanged catalog expectations. Test the general behavior on an additional graph where fixture-specific identity or values could conceal a shortcut.
3. For a case blocked in setup, continue through the complete scenario after the setup fix. For error scenarios, verify error timing/message and graph rollback, not just an exception of any kind.
4. After integrating the workstreams, run the full 1,511-case Gremlin suite locally. Compare case IDs/statuses against this snapshot and report new passes, lost passes, timeouts, reclassifications and skips separately. Retain replay attempts for genuine fixture/adapter failures.
5. Preserve SQLg/PuppyGraph edition/version evidence. Consult pinned upstream implementation and original expected types when products disagree; majority behavior alone does not establish correctness.
6. Commit result artifacts and publish the static matrix/leaderboard through GitHub Actions. **Never run the conformance tests in GitHub Actions.** Keep the immutable original parity baseline.

A current-failure workstream is complete only when its original cases pass, focused semantic checks pass and no existing passes regress. The 106 skips require their own capability decisions; changing a failure to a skip does not close the functionality gap. These results describe the pinned suite, not every possible Gremlin program.

## Primary implementation references

The upstream checkout under `conformance/upstream/cache/tinkerpop/` is pinned to the revision above. Useful sources for implementation decisions:

- `gremlin-core/.../gremlin/util/NumberHelper.java`: numeric promotion, including `bigDecimalValue`.
- `gremlin-core/.../gremlin/util/GremlinValueComparator.java`: comparability/orderability; also inspect the relevant step comparator and element ID contracts.
- `gremlin-core/.../process/traversal/step/filter/CoinStep.java`, `SampleGlobalStep.java`, and `step/map/SampleLocalStep.java`: random state, weighted/bulk-aware sampling and local shape.
- `gremlin-core/.../process/traversal/step/map/MergeVertexStep.java`, `MergeEdgeStep.java`: validation order, cardinality, incoming traverser and option semantics.
- `gremlin-core/.../process/traversal/step/filter/WherePredicateStep.java`: predicate by-ring.
- `gremlin-core/.../process/traversal/step/map/GroupStep.java` and `step/sideEffect/GroupSideEffectStep.java`: reducer and productivity contracts.
- `gremlin-test/.../features/`: every exact linked scenario, setup, tags and expected result.

The paths abbreviated with `...` are navigation hints under `src/main/java/org/apache/tinkerpop` (or `src/main/resources/org/apache/tinkerpop/gremlin/test` for feature files), not additional evidence artifacts.
