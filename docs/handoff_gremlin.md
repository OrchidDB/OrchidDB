# Gremlin Conformance Handoff (2026-07-26)

State at wind-down of the Gremlin tail push. The working agent was terminated
by session limits mid-cleanup; this document was assembled from its reports
and a final verification run.

## Current state (verified)

- `cargo test --release --test gremlin_tinkerpop_cases`: **Accurate 1578/1667
  runnable = 94.7%**, 89 incorrect, 0 parse/plan/run errors, 42 skipped
  (missing `grateful`/`sink` harness datasets). The harness intentionally
  exits FAILED while any incorrect cases remain.
- `gremlin_planner` unit tests green (the `choose_option_dispatch_...`
  expectation was updated: choose/branch options now dispatch per input
  traverser — TinkerPop order — instead of grouped-by-arm).
- History: 90.1% at the start of the day → 92.7% after the first push
  (15 clusters, see commit 2cb1999) → 94.7% now (+32 cases in the second
  push, uncommitted at the time of writing).

## Second-push work (files under src/language/gremlin/)

Edits landed across parser.rs, ast/step.rs, semantics.rs, and
planner/lowering/{dispatch, branch, filter, select, side_effects,
match_step, predicates, local_scope, slice, helpers, literals, context}.rs,
plus gremlin-side interpreter touches (runtime/property_object.rs,
type_check.rs, strings.rs, ops/unwind.rs). The agent was killed before
writing its own cluster list; consult `git diff` of those files for
specifics. Known targets it was working through, in order: repeat()
side-effect/emit internals, choose/branch traversal-valued option keys,
orderability corner types, set-semantics side effects, match() solver
reordering, sack bulk semantics, meta-properties, misc formatting.

## Remaining gaps (89 incorrect, from the last triage)

1. repeat() internals: per-iteration side-effect bags, emit ordering,
   nested repeat, aggregate/range inside repeat.
2. Graph algorithms: pageRank iteration fidelity/options, weighted
   shortestPath tie handling.
3. Orderability corner types: UUID/Set/±Infinity/NaN in `inject` mixed-type
   ordering (needs value-model additions).
4. Set-semantics side effects: `withSideEffect("a", {set})` — set-ness is
   lost; `GValue` has no Set variant.
5. match() solver: pattern reordering, where-clauses inside match.
6. Sack bulk semantics: `withBulk(false)` barrier merge, `Barrier.normSack`,
   BigInteger sacks, scientific-notation float rendering.
7. Meta-properties: `properties().properties()` on crew-style
   multi-properties is unmodeled.
8. Misc formatting: `valueMap().select(key)` list rendering, `asDate`
   timezone offsets, `select(b).by(T.id)` id format.

## Harness limitations (fix in tests/gremlin_case_runner/, not the engine)

- `xx`-binding extraction in parse.rs misses `optionX2L_nameX` /
  `isX1X__`-style tokens (~2+ cases).
- The case importer drops some `withSideEffect(...)` seed preambles.
- The actual-side formatter does not emit `vp[...]` for property objects in
  all paths.
- `grateful` (770 vertices) and `sink` datasets are not provided (42 skips).
