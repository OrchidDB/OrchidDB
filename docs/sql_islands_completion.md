# SQL Islands Completion Tracker

This is the working checklist for finishing the DuckDB SQL backend and
retiring the graph interpreter from supported execution paths.

Background and technique: [`handoff_sql_islands.md`](handoff_sql_islands.md).
Read that first — it explains why the reference-agreement number, not the
expected-output pass rate, is the one to optimize.

## Baseline

Verified at commit `dec9eae` (2026-07-27) on the first 1,600 Cypher cases,
islands mode against DuckDB:

| Metric | Value |
| --- | --- |
| Fully pushed down | 991 / 1,030 (96.2%) |
| SQL/interpreter agreement | 1,002 / 1,009 (99.3%) |
| Correctness divergences | 7 |
| Cases retaining residual interpreter work | 39 |
| Matched expected output | 999 / 1,600 (62.4%) |

Two denominators are in play and they are not interchangeable. *Pushdown* is
measured over cases that produced a plan (1,030); *agreement* over cases where
both paths returned an answer (1,009); *matched* over every case in the slice
including unparseable and unimported ones (1,600). Quote the denominator or
the number is meaningless.

### How to reproduce

```
GRAPH_REL_CROSSCHECK=1 GRAPH_REL_EXEC=islands GRAPH_REL_LANG=cypher \
  GRAPH_REL_LIMIT=1600 cargo test --release --test graph_rel_backend_cases \
  -- --ignored --nocapture
```

`--nocapture` is mandatory: cargo swallows stdout for suites that pass, and
this harness reports by printing. Divergences, with the query and both
answers, land in `target/graph_rel_backend_cases/summary.txt`. Drop
`GRAPH_REL_LIMIT` for the full corpus, at roughly twenty minutes per run.

To isolate one case, add `GRAPH_REL_SUITE=<path substring>` and run it under
each `GRAPH_REL_EXEC` mode: only `islands` wrong means the partitioner
(`src/ir/exec.rs`); all three wrong means the lowering (`src/ir/rel/`);
`datafusion` right and `duckdb` wrong means SQL generation
(`src/ir/rel/sql/`).

## 1. Correctness

- [x] Make `size()` use collection cardinality for list-valued expressions.
- [x] Preserve first-appearance ordering for `collect(DISTINCT ...)`.
- [x] Match graph-engine byte collation for non-ASCII blob `MIN`/`MAX`.
- [x] Preserve exact INT128/UINT128 values through SQL execution and comparison.
- [x] Fix independent `UNWIND ... MATCH` Cartesian multiplication.
- [x] Assemble named paths across recursive and fixed path segments.
- [x] Fix remaining relationship collection/grouping rendering differences.
- [ ] Normalize aggregate numeric output without changing native value semantics.
- [ ] Render floats inside cast list results with graph-engine precision.
- [ ] Compare out-of-range integer literals exactly instead of through a float.
- [ ] Emit `_NODES` for a recursive path as intermediate nodes only.
- [ ] Preserve the all-null grouping row for dynamic list group keys.

Exit criterion: 100% SQL/interpreter agreement on the eligible corpus —
cases that parse, plan, and produce an answer on both paths. Cases where the
interpreter itself is wrong are out of scope here and belong to corpus work.

### The 7 open divergences

Each maps to an unchecked item above. Listed so the next session can pick one
without re-deriving the set.

| Query shape | SQL | Reference |
| --- | --- | --- |
| `CAST(p.workedHours, "DOUBLE[]")` | `[1,9]` | `[1.000000,9.000000]` |
| `CAST(p.workedHours, "FLOAT[]")` | `[1,9]` | `[1.000000,9.000000]` |
| `sum(row.doubleColumn)` over empty | `0.000000` | `0` |
| `RETURN [p.language], [p.language, p.content], COUNT(p)` | missing the all-null group row | `[]\|[,]\|120256` |
| `-[e:Follows*1..2]->` `RETURN e` | `_NODES` holds both endpoints | `_NODES` holds intermediates only |
| `WHERE t.id = 170141183460469231731687303715884105727` | matches, returns `…728.000000` | no rows |
| `WHERE t.id = 340282366920938463463374607431768211455` | matches, returns `…456.000000` | no rows |

The last two are the same bug: the literal is round-tripped through a float,
so a value one above `i128::MAX` compares equal to it. Note this is a
*different* path from the `function/comparison` cases recorded as done below —
those go through folded comparison, these through a scan predicate.

The `_NODES` row is a regression introduced with the recursive lowering: the
accumulator appends the node it arrives at without excluding the endpoints,
which are already bound as source and target.

## 2. Residual Island Failures

- [x] Encode NUL-bearing strings without placing NUL bytes in SQL text.
- [ ] Repair SELECT-alias scope boundaries emitted by the unparser.
- [ ] Stop casting encoded graph values through incorrect narrow integer types.
- [ ] Implement graph-compatible overflow/error behavior for integer arithmetic.
- [ ] Fix remaining binding, parser, and path-to-scalar SQL failures.

Exit criterion: no read query retains interpreter work because SQL preparation
or DuckDB execution declined.

### What the 39 residual cases are

| Count | Cause |
| --- | --- |
| 13 | Island result did not match the graph column encoding |
| ~16 | Narrow-integer cast failures on encoded properties |
| 3 | DuckDB binder/parser errors (`"inf"` not found, syntax near `cd`) |
| 1 | `DECIMAL(4,1)` range failure |
| rest | Long tail |

The 13 encoding declines are the partitioner in `src/ir/exec.rs` refusing to
fabricate a value it cannot represent, which is correct behaviour and yields a
right answer by fallback. They are counted here because they still cost
pushdown, not because they are wrong.

The ~16 narrow-integer failures are one bug with many faces: properties stored
as encoded text are cast to the declared column width (`INT8`, `INT16`) rather
than to the width the value needs, so `9223372036854775808`, `32800`, `33768`
and `250` all fail. Fixing the cast target fixes the cluster.

## 3. Read Lowering Coverage

- [x] Lower `union_tag`, including stored union-valued properties.
- [x] Lower dynamic list expressions.
- [x] Lower supported UUID/random-function semantics (`UUID` normalization
      and row-volatile `gen_random_uuid`).
- [ ] Complete remaining `GraphApply` correlation shapes.
- [ ] Complete remaining `GraphRepeat`, `GraphChoose`, and `GraphCoalesce` shapes.
- [ ] Define stateful `nextval` semantics on the persistent SQL session.
- [ ] Audit every remaining `RelError::Unsupported` against supported language
      features and either lower it or give it an intentional public boundary.

Exit criterion: every supported read plan lowers as one complete SQL query.

## 4. Interpreter-Free Read Execution

- [ ] Make SQL `GraphReturn` shaping agree for every scalar, graph, error, and
      result-form case.
- [x] Preserve declared field ordering and result-form metadata at the SQL
      result boundary.
- [x] Return `ReturnedBatches` directly for fully lowered plans.
- [x] Remove `GraphValues` round-tripping from the complete-read path.
- [ ] Switch the default executor from hybrid islands to the direct-read API
      after the result-shaping gate is green.
- [x] Keep hybrid island execution available as a migration/debugging
      facility.

Exit criterion: a successful supported read never calls the interpreter.

`execute_with_islands` is the direct-read entry point: when a read lowers
completely it returns the engine's batches with `residual_ops == 0`, so no
residual plan is built and the interpreter is not called at all. Anything less
than complete falls back to hybrid island execution. Pinned by
`complete_reads_return_target_batches_without_a_residual_plan` in
`tests/exec_islands.rs`.

## 5. Persistent DuckDB

- [x] Own a reusable DuckDB connection/session per SQL target.
- [x] Reuse unchanged table-materialization blocks per SQL target, replace
      changed blocks safely, and address tables through `GraphMapping`.
- [ ] Parameterize query inputs and literals.
- [ ] Add transaction, cancellation, timeout, and resource controls.
- [ ] Remove per-island graph re-creation and hard plan-size safety caps.

Exit criterion: production reads execute against stable DuckDB tables without
per-query catalog reload.

## 6. Transactional Mutations

- [ ] Lower `CREATE`.
- [ ] Lower `MERGE`.
- [ ] Lower `SET` and property-map replacement/merge.
- [ ] Lower relationship and node `DELETE`, including detach semantics.
- [ ] Make generated IDs, sequences, and mutation visibility transactional.
- [ ] Add rollback, isolation, and mixed read/write tests.

Exit criterion: supported mutations execute in DuckDB without interpreter
state.

Until this section lands, mutations must stay excluded from islanding.
A relational run computes a result set without ever writing to the catalog,
so an islanded write is silently lost — `contains_mutation` in
`src/ir/exec.rs` is the guard, and `mutations_are_never_islanded` is the test.

## 7. Release Gates

- [ ] First 1,600 Cypher cases: 100% pushdown and 100% reference agreement.
- [ ] Full Cypher corpus gate.
- [ ] Gremlin corpus gate with an explicit supported-feature manifest.
- [ ] BYOS and Postgres compatibility gates where advertised.
- [ ] Freeze golden results before removing the interpreter reference path.
- [ ] Remove or quarantine obsolete interpreter-only production entry points.

The freeze gate is ordering-critical. The interpreter is currently the only
reference for what a correct answer looks like; once it is removed, a
regression in the SQL path has nothing to disagree with. Golden results must
exist before the reference path goes, not after.

## Verification Notes

- `agg/distinct_agg`: all 11 currently lowerable cases execute in DuckDB;
  first-appearance ordering and the two independent `UNWIND ... MATCH`
  `COUNT(*)` cases no longer fall back.
- `function/comparison`: all four INT128 comparison cases agree with the
  reference path; DuckDB HUGEINT results are retained as exact decimal text.
  This does *not* cover out-of-range literals in scan predicates — see the
  open divergences in section 1.
- `agg/hash` cases 1–4: 100% SQL/reference agreement after decoding Kuzu map
  properties in relationship grouping keys.
- Dedicated DuckDB smoke coverage now checks recursive paths, undirected
  length, `count(DISTINCT path)`, mixed variable/fixed named paths, dynamic
  list `UNWIND`, persistent setup replacement, exact HUGEINT transport, and
  tags retained alongside normalized stored union values.
- Constant wide-integer arithmetic and overflow expressions are folded with
  graph-engine semantics before entering DataFusion's narrower type system.
  All 49 `arithmetic/add` cases now lower; remaining differences in that
  imported suite are decimal error/scale compatibility and two expected files
  that contain only six rows for a nine-row Cartesian `UNWIND`.
- The `projection/single_label` audit now lowers 90 / 91 planned cases. UUID
  functions, dynamic string predicates, dynamic map construction,
  struct-literal field access, catalog struct-star expansion, and list slicing
  no longer require residual evaluation. The sole lowering boundary there is
  a row-dependent duplicate-map-key error, deliberately left to the fallback
  until SQL error expressions can preserve the exact public error text.
- The imported binary-demo cases cannot run in this checkout because the
  `LBUG binary-demo` fixture is absent; their mixed-path shape is covered by a
  self-contained graph fixture.
- A debug-build audit of deeply nested display expressions currently needs a
  larger worker stack. Treat that as a release-gate issue, not as permission
  to raise a production semantic depth ceiling.

## Standing Invariants

These hold across every section and should not be traded away for pushdown.

- **Decline, never fabricate.** A value the SQL path cannot represent must
  make the island decline so the subtree is evaluated directly. Substituting
  `NULL` converts an unsupported type into a silently wrong answer; that is
  how every `collect()` result in the corpus was once emptied.
- **Unexpressible SQL is an error, not a guess.** Constructs the unparser
  cannot express surface as `SqlError::Unsupported`. The aggregate `ORDER BY`
  splice refuses when the call text is not unique for exactly this reason.
- **No silent approximation.** Bounded unrolling once capped unbounded
  `-[*]->` at six hops without saying so. Limits must fail loudly or not
  exist; recursive lowering relies on trail semantics for termination rather
  than a cap.
- **Both paths must agree on element identity.** `interpreter::element_id`
  and `rel_index_case` encode the same table numbering. Change one and the
  same element prints differently depending on which path ran it.
