# Native Gremlin semantic backlog evidence

These changes execute in the Rust traversal interpreter over Crabgraph storage.
The supplemental tests do not alter the pinned Gherkin catalog or its expected
answers. Reference probes used TinkerPop 3.7.4, revision
`fa698ba2aba8967dcd17eb61cb13648b934fab5b`, to establish behavior; native execution
does not consult a reference engine.

## Implemented behavior

- `project()` cycles its `by()` traversal ring across keys, resets for each
  traverser, consumes surplus modulators, and preserves productive null versus
  an unproductive child.
- Named `groupCount()` starts from the registered typed map. Its long counts
  and typed keys survive empty input, repeat accumulation, cache invalidation,
  `select()` and repeated `cap()` calls without adding the seed twice.
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

## Upstream lifecycle details

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

## Local validation

Regression sources:

- `tests/gremlin_semantic_backlog.rs`: cyclic modulators, typed seeds, empty
  streams, repeated publication, filtered/ranged lazy consumption, choices,
  graph writes and unproductive projections.
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

## Remaining boundaries

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
