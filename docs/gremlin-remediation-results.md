# Gremlin remediation results — 2026-09-24

## Verified result

The combined implementation completes the executable failures in the [original remediation plan](gremlin-remaining-conformance.md). The pinned 1,511-case TinkerPop 3.7.4 suite ran locally against clean source `6c67ebc4b3aff08547017b828c1554c82778d411`.

| Outcome | Previous published result | Integrated result |
|---|---:|---:|
| Pass | 1,293 | **1,427** |
| Fail | 108 | **0** |
| Timeout | 4 | **0** |
| Adapter error | 0 | **0** |
| Skipped | 106 | 83 |
| Unsupported | 0 | 1 |

**134 new passes, zero lost passes.** Transitions: 107 failures → pass, 4 timeouts → pass, 23 fixture skips → pass. One remote-lambda failure is now an explicit unsupported capability; it is **not** a pass gain. All 1,511 case IDs and hashes are unchanged. This final run needed no retries or result normalization.

| Recorded comparison | Passes | Cases passing there but not in OrchidDB |
|---|---:|---:|
| OrchidDB | **1,427** | — |
| SQLg | 1,322 | **0** |
| PuppyGraph | 972 | **0** |
| Pinned reference runner | 1,427 | **0** |

OrchidDB's passing case set exactly matches the recorded reference runner's passing set and contains every recorded SQLg/PuppyGraph pass. Peer results retain their existing versions, dates and artifacts; peers were not rerun in this round. This establishes parity on this pinned execution profile, not every possible Gremlin program or execution profile.

## Architectural changes delivered

- Native Property/VertexProperty values and persisted property records, cardinality, meta-properties, stable public IDs, full/incremental snapshots and rollback.
- Shared traversal side effects, explicit barriers, exactly-once branch inputs, correlated group reducers, weighted bulk execution and corrected repeat scheduling/label liveness.
- Gremlin numeric promotion, native Set identity, typed heterogeneous values, productive cyclic modulators and graph-aware public-ID ordering.
- Real seeded sampling, graph-backed search and transactional Gryo/GraphSON/GraphML imports.
- Native Java property transport and ordered remote result delivery without re-bulking duplicate rows.

These changes preserve the shared engine and isolate Gremlin-specific semantics where needed. No workflow was added to execute tests in GitHub.

## Integration verification

- **272 Rust tests passed** across library, native property, storage, import, merge, search, grouping, traversal state, sampling, ordering, options and mutation targets.
- **24 additional cross-language checks passed** (15 Cypher DML and 9 SPARQL smoke tests).
- **15 Java tests passed** across native properties, fixtures, numeric literals, ordered transport and capability diagnostics.
- Full upstream sweep: **1,427 pass / 83 skipped / 1 unsupported**, with zero failures, timeouts or adapter errors.
- Original catalog, upstream expectations and immutable parity baseline are unchanged.

The property-only branch exposed three large-graph runtime regressions. All were retested on the combined implementation inside the original deadlines:

| Original case | Integrated query time (ms) | Scenario time including fresh-process setup (ms) |
|---|---:|---:|
| Recommendation.feature:21 | 278.570 | 8,056.233 |
| sideEffect/Group.feature:107 | 465.766 | 1,887.392 |
| SideEffectCap.feature:89 | 470.041 | 1,895.886 |

These are diagnostic local timings, not controlled cross-product benchmarks. The full suite uses its own per-case timings in the committed artifact.

## Evidence identity

- [Committed full result artifact](../conformance/upstream-results/orchiddb-tinkerpop.json).
- Source: `6c67ebc4b3aff08547017b828c1554c82778d411`, `working_tree_modified: false`.
- Runner SHA-256: `f2b351682752446f7e1f20f09eacb38c1ad42266c291c230079a9b09fecdef59`.
- Result artifact SHA-256: `997ce3ae9b04272e0e28a697884b148885026c38ab4a81ca41c3b15fd4cb0ef0`.
- Java classes archive SHA-256: `9d27ce6f9ace703de90fcf6b96a7daf03592cc1aeb74050bf3c4eaa13512882b`.
- Java bridge source SHA-256: `27e534f2a262e670a2ad3a9a26e9960c6ddb49e1c4d5abeb3ee8ba90a49954af`.
- Upstream TinkerPop revision: `fa698ba2aba8967dcd17eb61cb13648b934fab5b` (3.7.4).
- Full run: 2026-09-24 **07:35:32–07:37:13 UTC**, Darwin arm64, approximately 101 seconds.

The full result is published by the existing static-site GitHub Action. All test execution occurred locally.

## Remaining capability boundaries

No recorded `fail`, `timeout` or `adapter-error` remains in the Gremlin artifact. `map/Map.feature:66` requires a remote lambda execution capability and remains `unsupported`. The 83 skips retain upstream language/profile/assertion exclusions. The 23 previous multi/meta-property fixture exclusions now pass.

The following general behaviors were identified outside the completed pinned cases and remain separate follow-up work; they are not hidden conformance passes:

1. A single `project().by()` modulator does not cycle across all projected keys in every form.
2. A pre-seeded map for `groupCount()` is not merged into the new counter state.
3. General lazy aggregate consumption through arbitrary intervening operators is not a complete pull-based execution model.
4. A named-group value writer after its reducing barrier is explicitly rejected to avoid replaying writes during finalization.
5. The new per-seed repeat scheduling guarantee covers unbounded stateful repeat without emit/until; other combinations require their own behavioral checks.
6. Traversal-produced cardinality wrapper values, unsupported Java-regex constructs in search, and JVM identity-dependent HashMap tree-bin ordering are not established by this result.

The original plan remains as a historical case-by-case diagnosis; use this result and the committed artifact for current status.
