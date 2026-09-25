# Gremlin conformance completion plan

> Implementation checkpoint: the 84 originally identified gaps now have passing evidence across the native, JVM OLTP, GraphComputer and original Java test profiles. See [implementation results](gremlin-final-results.md) and the [case-to-evidence mapping](../conformance/upstream-results/gremlin-final-gap-evidence.json). The inventory below preserves the original plan and baseline; it is not the current failure list. Broader Java suite results are reported separately.

## Architectural work required

The remaining work needs **an upstream Java provider test path, a general lambda execution contract, and a real GraphComputer execution profile**. Extending the current string-based Gremlin adapter alone will not cover all three.

1. **Add a Java provider bridge over OrchidDB.** Implement the TinkerPop Graph/Vertex/Edge/VertexProperty/Property interfaces needed by the pinned upstream Java tests, with real native identity, property cardinality, mutation, transaction and feature contracts. Reuse the current native storage and transport. Keep execution modes explicit: Java reference traversal over OrchidDB storage tests provider/storage behavior; execution through OrchidDB's planner tests its traversal engine. A Java interpreter computing answers over a detached fixture does not establish OrchidDB engine conformance.
2. **Add a typed submission path.** Preserve native elements, properties, empty sets, arbitrary-precision numbers, callbacks and nested maps without round-tripping everything through a grammar string. Prefer the pinned upstream traversal/bytecode or provider interfaces where supported. Define a versioned typed boundary into native execution; retain the current grammar path as its own tested profile.
3. **Support lambdas as a deliberate execution capability.** Supply real traverser context, callback types, property access, paths, labels, bulk and sack lifecycle. Choose either a JVM callback bridge invoked by native execution or an explicitly identified JVM traversal profile backed by OrchidDB. These establish different claims. The remote-lambda case additionally needs a working remote submission/runtime path.
4. **Implement GraphComputer semantics.** Add supersteps, message scopes, per-vertex compute state, memory/reducers, termination, traversal integration and result/persist behavior. A correct single-process implementation is a useful first target; distributed execution is not required merely to begin testing these cases. Algorithm wrappers must compute from arbitrary graph data.
5. **Complete stream-consumption and state contracts.** Extend the existing evaluator's barriers, lazy consumption, seeded side effects, cyclic modulators and repeat scheduling. Use explicit effects and traversal requirements to decide when materialization, reordering or bulking is legal. A wholesale engine rewrite is not assumed; shared behavior must remain compatible with Cypher and SPARQL.

**Suggested order:** Java harness and typed transport first; close small semantic gaps in parallel; then callbacks and GraphComputer as separate substantial workstreams. Preserve the 1,427 current passes throughout.

## Baseline and definition of done

Original baseline source: `6c67ebc4b3aff08547017b828c1554c82778d411`; published checkpoint: `b1c05ac`.

- **1,427 pass, 83 skipped, 1 unsupported**, out of 1,511 pinned TinkerPop 3.7.4 scenarios.
- **Zero failures, timeouts, adapter errors or peer-only passing gaps** against the recorded SQLg/PuppyGraph runs.
- All 112 previously failing/timing-out cases are resolved as 111 passes and one explicit unsupported remote-lambda case.
- All 23 previously blocked multi/meta-property fixture cases now pass.

Sources: [current result artifact](../conformance/upstream-results/orchiddb-tinkerpop.json), [scenario catalog](../conformance/upstream/catalog.json), [completed remediation results](gremlin-remediation-results.md).

### Two completion targets

**Target A — finish the currently identified gaps.** Implement and verify the behavior behind all 84 non-passing scenarios. Keep an exact case-to-evidence mapping. For executable scenarios, run original assertions. For upstream placeholders, use the corresponding pinned upstream Java tests where available; retain the original Gherkin exclusion and report the Java evidence separately.

**Target B — broader TinkerPop provider conformance.** Run the applicable upstream `ProcessStandardSuite`, `ProcessComputerSuite` and `StructureStandardSuite`, plus the supported remote/serialization profiles. Record every feature requirement and exclusion. This is broader than the current 1,511-scenario catalog and can reveal new requirements; its test count and scope must be inventoried before estimating completion.

Fifteen of the remaining Gherkin scenarios literally say `Given an unsupported test` and have no real query/assertion body. **A literal 1,511/1,511 passing result cannot be obtained from this unchanged Gherkin catalog merely by fixing the engine.** Do not convert those placeholders into fake passes. Report complete behavioral evidence across identified upstream suites instead, keeping each suite's denominator and execution profile visible.

## Remaining scope: all 84 cases

Each scenario appears once in the inventory at the end. Counts describe current recorded blockers, not guaranteed pass gains after one change; an early blocker may conceal later semantic failures.

| ID | Workstream | Cases | Main boundary | Relative scope |
|---|---|---:|---|---|
| G1 | GraphComputer and graph algorithms | 31 | Execution architecture | Very large |
| G2 | Lambda parameters and remote lambda | 28 | Execution/runtime and transport | Large |
| G3 | Null-valued properties | 5 | Storage/provider semantics | Medium |
| G4 | Native Edge parameters | 3 | Typed input transport | Small–medium |
| G5 | Empty Set parameters | 2 | Typed input transport and side-effect seeds | Small–medium |
| G6 | Native property parameters and identifiers | 3 | Upstream assertion/Java provider path | Medium |
| G7 | Nested typed-map assertions | 3 | Upstream assertion path; potentially state semantics | Medium |
| G8 | Sack numeric precision and JVM split callbacks | 3 | Upstream assertion path and runtime | Medium–large |
| G9 | File writers and round-trip verification | 6 | Export implementation and upstream assertion path | Medium |
| | **Total** | **84** | | |

Relative scope is a planning estimate, not a time commitment. G1/G2 depend on architectural choices; use an executable vertical slice before estimating duration.

## G1. GraphComputer and algorithms — 31 cases

**Breakdown:** ConnectedComponent 4, PageRank 9, PeerPressure 3, ShortestPath 15.

### Changes

- Implement the provider's `compute()`/GraphComputer execution profile and its advertised feature contract. Establish compute graph access, legal mutations, iteration state, messaging, memory reducers, worker lifecycle and termination.
- Support traversal requirements for graph filtering, per-vertex state, barriers, paths, global results, algorithm-generated properties and chaining algorithms with ordinary traversal steps.
- ConnectedComponent: arbitrary topology, selected edges, chosen output property and stable component identity according to upstream.
- PageRank: damping, iteration count including zero, edge traversal/direction, initial rank configuration, output property, chaining and result precision.
- PeerPressure: cluster propagation, edge selection, iteration/termination behavior and composition with PageRank.
- ShortestPath: selected sources/targets, direction, edge inclusion, weighted distance, maximum distance and tied paths. Verify the full upstream behavior, not just path length on the modern graph.
- Audit algorithm lowering and virtual property helpers for fixture-name/ID shortcuts. Remove any remaining shortcuts before accepting results. A fixture-specific computed property is not an algorithm implementation.

### Acceptance

All 31 original scenarios execute in the GraphComputer profile, followed by applicable upstream Java computer tests. Also test disconnected graphs, cycles, ties, renamed vertices, reordered IDs and varied weights. Capture result graphs, compute properties, memory and convergence where relevant. Keep GraphComputer results distinct from OLTP results.

**Start in:** `src/language/gremlin/planner/lowering/procedures.rs`, IR plan/execution context, catalog computed properties, and a new computer/provider adapter. Inventory existing algorithm support before deciding which operators can be reused.

## G2. Lambdas and remote execution — 28 cases

**27 lambda-parameter exclusions plus `map/Map.feature:66`, the one unsupported inline remote lambda.**

### Changes

- Define supported callback forms: predicates, mapping functions, branch/choice functions, comparators, repeat termination, sack initialization/split/merge, and traverser/path access.
- Add typed callback submission and invoke it at the proper execution point with current object, labels, path, loops, bulk and side-effect/sack context. Preserve exception propagation, callback ordering and exactly-once side effects.
- Choose and document the runtime route. For native planner coverage, JVM callbacks need a native executor bridge; a JVM-only provider test is valuable but must be labeled as provider execution rather than evidence for Rust callback execution.
- Use the pinned upstream script/lambda language for original scenarios. Do not implement a lookup table for the 27 known lambda strings or translate only those strings into canned traversals.
- Add actual remote request/session execution for the inline `Lambda.function` scenario. The current explicit unsupported diagnostic should become a real execution result, not simply a different status label.
- Define lifecycle and isolation for executable callbacks, including cancellation and resource cleanup. Treat these as runtime requirements rather than weakening assertion or timeout behavior.

### Acceptance

All 28 original cases execute unchanged in an identified capable profile. Add general callbacks with captured values, native properties, nested traversals and error propagation. Verify callback results cross the typed boundary without losing number widths or graph identity.

**Dependencies:** Java provider/typed boundary; state and traversal-context semantics. **Start in:** `conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java`, a separate provider/callback adapter, Gremlin AST/IR and native executor callback operators.

## G3. Null-valued properties — 5 cases

### Changes

- Decide and implement the provider feature for stored null values consistently across vertex, edge and meta-properties.
- Represent **absent property** separately from **present property whose value is null**. Preserve that distinction in native property records, indexes, snapshots, incremental replay and transaction rollback.
- Update `property()`, `has()`, `hasNot()`, `values()`, `properties()`, `valueMap()` and merge onMatch writes accordingly. A stored null must not silently disappear because a generic runtime helper treats `Value::Null` as absence.
- Verify meta-property nulls and replacement/cardinality behavior against the exact pinned upstream expectations.
- Enable `@AllowNullPropertyValues` only for a provider profile that actually supports the feature; retain separate semantics if other profiles use null-as-removal.

### Acceptance

The five original AddVertex/AddEdge/MergeVertex/MergeEdge cases pass, plus missing-vs-null tests on reads, deletes, full/incremental persistence and rollback. Re-run existing null/productivity tests so a valid null traversal result remains distinct from no result.

**Start in:** `src/ir/catalog/properties.rs`, native mutation/property helpers, output codecs, adapter feature declarations.

## G4. Edge parameters — 3 cases

### Changes

- Transport a native Edge parameter with typed public identity, label and endpoints, including lists of edges, without requiring a grammar literal that the language translator cannot express.
- Resolve detached/reference edges against the target graph consistently; keep edge-object identity distinct from an ID string or an ordinary map.
- Preserve parameter handling through `g.E(edge)`, multiple arguments and list arguments. Use an upstream-supported submission path where possible.

### Acceptance

The three original Vertex.feature cases execute and return the expected native edges. Add typed-ID, reordered storage, unknown-ID and cross-session reference checks according to the provider contract.

**Dependency:** typed submission boundary. This may be mostly adapter work because native Edge results already exist, but later engine failures remain possible after removing the transport blocker.

## G5. Empty Set parameters — 2 cases

### Changes

- Preserve a typed empty Set through parameter decoding/submission; do not substitute `[]` or an empty Map.
- Ensure `withSideEffect()` initializes the actual shared container and its collection/reducer semantics. `aggregate(local, ...)` and `store(...)` must update the expected set while preserving native typed membership identity.
- Reuse current canonical set-member equality for numeric widths, NaN, signed zero, decimal scale and nested values.

### Acceptance

Both original cases pass, with set type preserved through `cap()`. Verify duplicate insertion and seeded non-empty sets as follow-up checks. A successful serialization round trip alone does not verify accumulation.

## G6. Property parameters and property-ID assertions — 3 cases

These Gherkin entries are placeholders. Native property storage/transport now exists, but these entries cannot execute merely by removing a skip check.

### Changes

- Locate the corresponding pinned `PropertiesTest` methods and run their real assertions through the Java provider bridge.
- Support native VertexProperty injection and property-ID parameters with original typed identity. Cover string and numeric ID paths according to the upstream test/provider feature contract.
- Preserve owner access, property value type, stable IDs and detached/reference property behavior across the bridge.

### Acceptance

Record original Java test identity, assertion outcome, feature requirements and source hash for each linked placeholder. Keep Gherkin status unchanged; expose separate Java-provider evidence and do not count the same behavioral coverage twice in one suite total.

## G7. Nested typed maps and side-effect assertions — 3 cases

Affected placeholders: Group.feature:151, Group.feature:158, SideEffectCap.feature:156.

### Changes

- Locate their corresponding Java tests and assert nested native maps, collection multiplicities, mixed key types and graph elements directly.
- Preserve typed map keys and nested lists/maps through serialization; avoid JSON stringification of keys or flattening map/list shape.
- If assertions reveal engine defects, fix nested grouping, shared reducer state and `cap()` behavior. Do not assume that an assertion-layer exclusion implies the runtime already works.

### Acceptance

The original upstream Java assertions pass with intact types, duplicates and numeric widths. Include actual typed output in result evidence so equal-looking printed maps cannot hide mismatched values.

## G8. Sack precision and split callbacks — 3 cases

Affected placeholders: Sack.feature:115, :122 and :130.

### Changes

- Execute the pinned Java tests, which include huge BigInteger sack initialization, a JVM `UnaryOperator` clone/split callback and a division/rounding assertion.
- Preserve arbitrary precision through sack initialization, normalization, arithmetic, split and merge. Do not coerce huge integers to f64 for convenience.
- Ensure sack splitting clones mutable state with upstream aliasing semantics; child traversers must not accidentally share the same mutable map.
- Implement the pinned numeric promotion/rounding contract for division and compare using the upstream assertion/tolerance. Do not loosen tolerances just to pass.

### Acceptance

All three Java scenarios pass; add independent-branch mutation and exact large-number transport checks. Identify whether each test ran through native callback execution or the Java-provider profile.

**Dependencies:** G2 for JVM callbacks, typed transport and shared sack state.

## G9. File writers — 6 cases

The current implementation has verified readers/import. Writer Gherkin entries are upstream placeholders, so export must be tested through actual writer tests and round trips.

### Changes

- Implement `io(path).write()` with Gryo, GraphSON and GraphML writer selection/autodetection, using the supported upstream codec version.
- Export from a consistent native graph snapshot with public IDs, labels, endpoints, typed values and each format's supported property/meta-property representation.
- Reuse the Java codec bridge where appropriate. Keep format capability differences explicit; do not promise a lossless round trip for a feature the chosen format cannot represent.
- Surface writer errors, close files reliably and avoid treating a created empty file as success. Handle destination replacement through a defined completion/atomicity contract.
- Run applicable pinned Java WriteTest/IO tests, and load output using an independent compatible reader as well as OrchidDB's reader.

### Acceptance

Default and explicit writer selection for all three formats are verified. Validate graph content and supported type/identity preservation after round trip, not byte-for-byte file identity or file existence alone. Report the six linked Gherkin placeholders separately from executable Java/IO test counts.

## General semantic work beyond the 84 catalog entries

These were identified during the previous implementation. They need explicit regressions even if closing them does not increase the current Gherkin pass count.

| Area | Required change | Completion check |
|---|---|---|
| Cyclic project modulators | Cycle a single `by()` over every projected key; preserve nonproductive versus null results. | `g.inject(1).project('a','b').by(__.constant(7))` yields both values 7, plus multi-modulator cases. |
| Seeded groupCount | Initialize the reducer from the typed `withSideEffect` map rather than starting at zero. | Seed `{1:10L}` plus one occurrence produces `{1:11L}` without changing key/value types. |
| Lazy aggregate consumption | Honor partial downstream consumption through arbitrary intervening operators, not just an immediate slice. | Upstream comparisons with filters, limits, branches and early termination; effects run exactly as many times as consumed. |
| Writes after group reductions | Define and implement exactly-once execution after a reducing barrier; remove the current explicit restriction only when correct. | No writer replay during finalization/cache invalidation, including repeated `cap()` and multiple readers. |
| Repeat scheduling combinations | Extend seed/frontier scheduling rules to stateful repeats with emit/until and nesting where required. | Original upstream behavior for shared effects, counters, cancellation, barriers and retained paths. |
| Dynamic cardinality wrappers | Support traversal-produced cardinality wrapper values through typed AST/runtime, not only literal option maps. | Same behavior for equivalent literal and dynamically generated inputs. |
| Search regex profile | Decide whether Java-compatible constructs such as lookaround/backreferences are supported; implement a matching runtime if claimed. | Explicit supported syntax and upstream-compatible behavior; no silent pattern reinterpretation. |
| Seeded grouped sampling | Remove dependency on emulated JVM identity-based map iteration, or use a JVM-backed execution profile when that exact behavior is required. | Adversarial hash collisions/tree bins and graph insertion orders, not only modern-graph sampling. |

Review these with upstream source before implementing. Existing concrete examples establish the current gaps; they do not establish a complete inventory of all language behavior.

## Harness and evidence design

Use upstream tests as the authority. The new harness should orchestrate upstream suites and expose OrchidDB's implementation, rather than invent a parallel set of expected answers.

1. Pin the same TinkerPop revision first: `fa698ba2aba8967dcd17eb61cb13648b934fab5b` (3.7.4). Treat a version upgrade as separate work.
2. Inventory Java test methods, feature annotations and execution requirements. Link each of the 15 Gherkin placeholders to actual Java test methods where available. If no executable upstream equivalent exists, label a supplemental test separately.
3. Keep suite/profile identifiers explicit: existing Gherkin native traversal, Java provider/storage, Java native-traversal bridge, GraphComputer and remote callbacks.
4. Record source revision/cleanliness, runner and bridge hashes, fixture hashes, provider features, upstream test ID and assertion result. Preserve typed expected/actual output and timing.
5. Store original exclusions and reasons. Do not rewrite upstream files, replace assertions, count classification changes as gains or merge duplicate tests into an inflated total.
6. Retain raw timeout/error attempts. Use fresh-process replays to diagnose fixture failures while keeping query timeouts visible; preserve existing deadlines unless separately justified and reported.
7. Keep peer comparisons on matching suites/profiles and recorded editions. A new Java/GraphComputer suite should not be compared against a peer's old OLTP-only denominator.
8. Run everything **locally**. GitHub Actions only builds/publishes static docs and committed artifacts. Preserve `conformance/data/parity-baseline.json`.

## Delivery milestones

| Milestone | Deliverable | Exit gate |
|---|---|---|
| M0 | Java test inventory and provider/typed-boundary design | Every non-pass has an execution route, feature contract and upstream assertion source; 15 placeholders explicitly identified. |
| M1 | Typed Edge/Set parameters and null-property behavior | G3/G4/G5 original executable cases pass; current 1,427 passes retained. |
| M2 | Java property/nested-map/sack assertions and real writers | G6/G7/G8/G9 behavioral evidence recorded under correct suites; no fake Gherkin passes. |
| M3 | Lambda runtime and remote callback path | All 28 G2 cases execute with real callbacks; profile differences explicit. M2's split-callback test may depend on this milestone. |
| M4 | GraphComputer vertical slice, then algorithms | First one arbitrary-graph algorithm through real supersteps; then all 31 G1 cases and applicable Java computer suite. |
| M5 | General semantic closure and full regression | Above state/streaming gaps fixed, applicable provider/process suites green, no current-pass regressions, residual feature exclusions fully enumerated. |

M1, writer implementation and harness inventory can overlap. GraphComputer and callbacks can use separate teams after their interfaces are agreed. Avoid concurrent ownership of shared Value, traversal-state and provider-feature contracts.

## Final acceptance

- All 69 currently executable-but-blocked scenario behaviors run successfully in their appropriate profiles, with original assertions retained.
- All 15 placeholder behaviors have executable upstream Java evidence where available, or an explicit remaining evidence gap. Original placeholder statuses stay faithful to upstream.
- The relevant pinned Java process, computer and structure suites are executed; feature exclusions are visible and correspond to genuine provider capabilities.
- All 1,427 existing passes remain passing, and known general semantic regressions are fixed.
- Results are reproducible from clean source with native output types, unchanged case expectations, original deadlines and complete provenance.
- The final documentation names the precise supported profiles and suites. A broad “full Gremlin conformance” claim requires that declared scope; one aggregate number cannot establish every JVM, remote, OLAP and storage capability.

## Exact remaining scenario inventory

Every current non-pass appears exactly once below. Links point to the pinned upstream revision. “Placeholder” means the original Gherkin body supplies no executable traversal/assertion, not that its intended behavior is unimportant.

| Scenario | Workstream | Current status | Execution route |
|---|---|---|---|
| [branch/Branch.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/branch/Branch.feature#L21) | G2 | skipped | Lambda-capable native/remote profile |
| [branch/Choose.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/branch/Choose.feature#L37) | G2 | skipped | Lambda-capable native/remote profile |
| [branch/Repeat.feature:215](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/branch/Repeat.feature#L215) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Dedup.feature:95](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Dedup.feature#L95) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Filter.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L21) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Filter.feature:31](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L31) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Filter.feature:48](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L48) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Filter.feature:61](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L61) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Filter.feature:72](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L72) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Filter.feature:85](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L85) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Filter.feature:98](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L98) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Filter.feature:111](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L111) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Filter.feature:121](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Filter.feature#L121) | G2 | skipped | Lambda-capable native/remote profile |
| [filter/Has.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/filter/Has.feature#L21) | G2 | skipped | Lambda-capable native/remote profile |
| [map/AddEdge.feature:422](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddEdge.feature#L422) | G3 | skipped | Native provider with null-property feature |
| [map/AddVertex.feature:532](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L532) | G3 | skipped | Native provider with null-property feature |
| [map/AddVertex.feature:543](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/AddVertex.feature#L543) | G3 | skipped | Native provider with null-property feature |
| [map/ConnectedComponent.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ConnectedComponent.feature#L21) | G1 | skipped | GraphComputer profile |
| [map/ConnectedComponent.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ConnectedComponent.feature#L37) | G1 | skipped | GraphComputer profile |
| [map/ConnectedComponent.feature:53](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ConnectedComponent.feature#L53) | G1 | skipped | GraphComputer profile |
| [map/ConnectedComponent.feature:65](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ConnectedComponent.feature#L65) | G1 | skipped | GraphComputer profile |
| [map/Map.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L21) | G2 | skipped | Lambda-capable native/remote profile |
| [map/Map.feature:34](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L34) | G2 | skipped | Lambda-capable native/remote profile |
| [map/Map.feature:49](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L49) | G2 | skipped | Lambda-capable native/remote profile |
| [map/Map.feature:66](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L66) | G2 | unsupported | Lambda-capable native/remote profile |
| [map/Map.feature:80](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L80) | G2 | skipped | Lambda-capable native/remote profile |
| [map/Map.feature:97](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Map.feature#L97) | G2 | skipped | Lambda-capable native/remote profile |
| [map/MergeEdge.feature:953](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeEdge.feature#L953) | G3 | skipped | Native provider with null-property feature |
| [map/MergeVertex.feature:1054](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/MergeVertex.feature#L1054) | G3 | skipped | Native provider with null-property feature |
| [map/Order.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Order.feature#L37) | G2 | skipped | Lambda-capable native/remote profile |
| [map/Order.feature:103](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Order.feature#L103) | G2 | skipped | Lambda-capable native/remote profile |
| [map/Order.feature:172](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Order.feature#L172) | G2 | skipped | Lambda-capable native/remote profile |
| [map/Order.feature:388](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Order.feature#L388) | G2 | skipped | Lambda-capable native/remote profile |
| [map/PageRank.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L21) | G1 | skipped | GraphComputer profile |
| [map/PageRank.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L37) | G1 | skipped | GraphComputer profile |
| [map/PageRank.feature:51](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L51) | G1 | skipped | GraphComputer profile |
| [map/PageRank.feature:67](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L67) | G1 | skipped | GraphComputer profile |
| [map/PageRank.feature:79](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L79) | G1 | skipped | GraphComputer profile |
| [map/PageRank.feature:95](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L95) | G1 | skipped | GraphComputer profile |
| [map/PageRank.feature:109](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L109) | G1 | skipped | GraphComputer profile |
| [map/PageRank.feature:121](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L121) | G1 | skipped | GraphComputer profile |
| [map/PageRank.feature:135](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PageRank.feature#L135) | G1 | skipped | GraphComputer profile |
| [map/PeerPressure.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PeerPressure.feature#L21) | G1 | skipped | GraphComputer profile |
| [map/PeerPressure.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PeerPressure.feature#L37) | G1 | skipped | GraphComputer profile |
| [map/PeerPressure.feature:48](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/PeerPressure.feature#L48) | G1 | skipped | GraphComputer profile |
| [map/Properties.feature:111](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Properties.feature#L111) | G6 | skipped | Placeholder → pinned Java property tests |
| [map/Properties.feature:118](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Properties.feature#L118) | G6 | skipped | Placeholder → pinned Java property tests |
| [map/Properties.feature:125](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Properties.feature#L125) | G6 | skipped | Placeholder → pinned Java property tests |
| [map/ShortestPath.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L21) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:67](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L67) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:113](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L113) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:159](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L159) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:182](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L182) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:205](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L205) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:230](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L230) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:246](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L246) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:262](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L262) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:278](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L278) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:290](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L290) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:304](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L304) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:318](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L318) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:334](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L334) | G1 | skipped | GraphComputer profile |
| [map/ShortestPath.feature:348](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/ShortestPath.feature#L348) | G1 | skipped | GraphComputer profile |
| [map/Unfold.feature:37](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Unfold.feature#L37) | G2 | skipped | Lambda-capable native/remote profile |
| [map/Vertex.feature:190](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Vertex.feature#L190) | G4 | skipped | Typed parameter transport |
| [map/Vertex.feature:202](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Vertex.feature#L202) | G4 | skipped | Typed parameter transport |
| [map/Vertex.feature:216](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/map/Vertex.feature#L216) | G4 | skipped | Typed parameter transport |
| [sideEffect/Aggregate.feature:175](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Aggregate.feature#L175) | G5 | skipped | Typed parameter transport + shared seed |
| [sideEffect/Group.feature:118](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Group.feature#L118) | G2 | skipped | Lambda-capable native/remote profile |
| [sideEffect/Group.feature:151](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Group.feature#L151) | G7 | skipped | Placeholder → pinned Java typed assertions |
| [sideEffect/Group.feature:158](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Group.feature#L158) | G7 | skipped | Placeholder → pinned Java typed assertions |
| [sideEffect/Inject.feature:39](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Inject.feature#L39) | G2 | skipped | Lambda-capable native/remote profile |
| [sideEffect/Sack.feature:115](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Sack.feature#L115) | G8 | skipped | Placeholder → pinned Java sack tests |
| [sideEffect/Sack.feature:122](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Sack.feature#L122) | G8 | skipped | Placeholder → pinned Java sack tests |
| [sideEffect/Sack.feature:130](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Sack.feature#L130) | G8 | skipped | Placeholder → pinned Java sack tests |
| [sideEffect/SideEffectCap.feature:100](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/SideEffectCap.feature#L100) | G2 | skipped | Lambda-capable native/remote profile |
| [sideEffect/SideEffectCap.feature:156](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/SideEffectCap.feature#L156) | G7 | skipped | Placeholder → pinned Java typed assertions |
| [sideEffect/Store.feature:52](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Store.feature#L52) | G5 | skipped | Typed parameter transport + shared seed |
| [sideEffect/Write.feature:21](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L21) | G9 | skipped | Placeholder → pinned Java writer/IO tests |
| [sideEffect/Write.feature:28](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L28) | G9 | skipped | Placeholder → pinned Java writer/IO tests |
| [sideEffect/Write.feature:35](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L35) | G9 | skipped | Placeholder → pinned Java writer/IO tests |
| [sideEffect/Write.feature:42](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L42) | G9 | skipped | Placeholder → pinned Java writer/IO tests |
| [sideEffect/Write.feature:49](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L49) | G9 | skipped | Placeholder → pinned Java writer/IO tests |
| [sideEffect/Write.feature:56](https://github.com/apache/tinkerpop/blob/fa698ba2aba8967dcd17eb61cb13648b934fab5b/gremlin-test/src/main/resources/org/apache/tinkerpop/gremlin/test/features/sideEffect/Write.feature#L56) | G9 | skipped | Placeholder → pinned Java writer/IO tests |

## Source navigation

- [Current native Gremlin result](../conformance/upstream-results/orchiddb-tinkerpop.json) and [catalog](../conformance/upstream/catalog.json): exact statuses, steps and upstream links.
- [Previous implementation/results](gremlin-remediation-results.md): verified behavior and remaining semantic boundaries.
- Pinned local upstream checkout: `conformance/upstream/cache/tinkerpop/`.
- Under upstream `gremlin-test/src/main/java/org/apache/tinkerpop/gremlin/`: `process/ProcessStandardSuite.java`, `process/ProcessComputerSuite.java`, `structure/StructureStandardSuite.java`, and the corresponding traversal/structure test classes.
- Current adapter: `conformance/adapters/sqlg/src/main/java/UpstreamGremlin.java`; extend with a separately identified provider/Java-suite runner as needed.

This is a draft implementation plan based on the published checkpoint, not a claim that the remaining capabilities have been implemented or that the broader Java suites have already been run.
