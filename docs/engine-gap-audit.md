# Graph Engine on DuckDB — Gap Audit

Baseline audit on 2026-09-19, before the managed engine, parameter binding,
and session work in this change. The findings below describe that starting
point. See [engine.md](engine.md) for the current APIs and remaining limits.

## 1. What actually executes today (four distinct paths)

There is no single "DuckDB engine". There are four layers, and "DuckDB"
appears only in the last one:

| Path | Entry point | Engine | Durable? |
| --- | --- | --- | --- |
| Interpreter | `src/ir/interpreter/mod.rs` `execute` / `execute_rows` | in-process, Arrow `RecordBatch` + `PropertyGraph` overlay | no |
| Relational (in-proc) | `src/ir/rel/mod.rs` `RelBackend::execute` → `execute_lowered:235` | DataFusion `SessionContext` | no |
| SQL island | `src/ir/rel/sql/mod.rs` `prepare`/`execute_prepared` + `duckdb_exec.rs` | DuckDB via `duckdb` crate | **no** (`open_in_memory`) |
| Hybrid partitioner | `src/ir/exec.rs` `execute_with_islands:270` / `plan_with_islands:241` | mixes all of the above | no |

The routing in `execute_with_islands` (`src/ir/exec.rs:276-296`):

- A read plan that fully lowers AND executes on the target returns the
  target's batches directly (`residual_ops == 0`); otherwise it partitions
  into maximal SQL islands and runs the residual through the interpreter.
- `IslandTarget` (`src/ir/exec.rs:53`) is the engine seam; `default_target`
  (`src/ir/exec.rs:180`) picks DuckDB when the `duckdb` feature is on,
  otherwise DataFusion, overridable via `GRAPH_ISLAND_TARGET`.

So "running on DuckDB" is currently the **hybrid path**: a fraction of each
query is real SQL, the rest falls back to the Arrow interpreter over an
in-memory graph. Nothing is written to disk anywhere in the crate.

## 2. Durable storage: none

`DuckDbExecutor` opens `Connection::open_in_memory()` and never persists
(`src/ir/rel/sql/duckdb_exec.rs:57-62`). The "persistent session" that exists
is a cache of applied `CREATE TABLE`/`INSERT` setup blocks keyed by the first
statement (`applied_setup`, `setup_table_key`, `duckdb_exec.rs:218-227`) — an
in-memory dedup optimization, not durability.

Catalog storage is the in-memory `PropertyGraph` (`src/ir/catalog.rs:54-83`):
Arrow batches for base tables plus a `RefCell<GraphOverlay>` of insert/
override/delete sets (`catalog.rs:110-147`). `graph_setup_sql`
(`sql/mod.rs:688`) can render the catalog as `CREATE TABLE` DDL, but it is
only used to re-materialize per-run fixtures, and the target database is
in-memory.

Gap D-1 (blocking): no file-backed or externally-attached DuckDB database.
`duckdb::Config`/`Connection::open(path)` is never used. This is the single
hardest blocker to "a graph engine on DuckDB": today the engine forgets the
graph when the process exits.

## 3. Catalog mutation: interpreter-only overlay, never relational

Mutation semantics live entirely in the interpreter overlay:

- Writes: `PropertyGraph::insert_node:710`, `insert_edge:622`,
  `set_property:735`, `set_properties:788`, `delete_value:855`
  (`src/ir/catalog.rs`).
- Operators: `create_op`, `merge_op`, `set_property_op`, `delete_op`
  (`src/ir/interpreter/ops/mutation.rs:15-133`).
- The overlay is not transactional: no rollback, no snapshot, no isolation.
  It is a `RefCell` mutated in place.

Relational/SQL lowering has **no** mutation support. In
`LoweringContext::lower_node` (`src/ir/rel/mod.rs:404`) the mutation nodes
(`GraphCreate`, `GraphMerge`, `GraphSetProperty`, `GraphDelete`) fall through
to `other => RelError::Unsupported` (`rel/mod.rs:820-825`), named by
`unsupported_node_name` (`rel/mod.rs:7764-7789`). `GraphMerge` is also
unreachable there because it only appears in `graph_plan_stats` traversal
(`rel/mod.rs:297`), never in the lowering match.

The partitioner hard-guards this: `contains_mutation` (`src/ir/exec.rs:655`)
and the two `Decline::Ineligible` checks (`exec.rs:276`, `exec.rs:351`) keep
any write-bearing subtree with the interpreter, pinned by
`mutations_are_never_islanded` (`tests/exec_islands.rs:148`). This is correct
behaviour today — a relational run would compute rows without writing the
catalog — but it means `docs/sql_islands_completion.md` §6 "Transactional
Mutations" is entirely unchecked.

Gap D-2 (blocking): transactional `CREATE`/`MERGE`/`SET`/`DELETE` in DuckDB,
including detach semantics, generated ids/sequences, and mixed read/write
visibility. See `docs/sql_islands_completion.md:170-185`.

## 4. Parameters: parsed and represented, never bound

Parameter syntax is parsed and lowered end-to-end, then stubbed to `Null`:

- Parser: `lower_parameter` (`src/language/cypher/parser/lowering/expressions.rs:738-747`)
  → `Expr::Parameter(name)`.
- Planner: `Expr::Parameter(name) => IrExpr::Call { name: "parameter", args: [String(name)] }`
  (`src/language/cypher/planner/lowering/project.rs:4604-4607`); pattern-side
  parameters become `cypher_properties_match` predicates
  (`src/language/cypher/planner/lowering/pattern.rs:703-719`, `1253-1261`).
- Interpreter: `("parameter", [Value::String(_)]) => Ok(Some(Value::Null))`
  (`src/ir/interpreter/runtime/mod.rs:880`). A `Null` parameter in a pattern
  predicate matches **everything**:
  `cypher_properties_match` with `Value::Null` returns `true`
  (`runtime/mod.rs:867-872`).
- SQL path: no `"parameter"` handling anywhere under `src/ir/rel/`; a
  parameter reference cannot lower to SQL. `docs/sql_islands_completion.md`
  §5 "Parameterize query inputs and literals" is unchecked.

There is no API surface to pass a `name → value` map into
`CypherPlanner::plan` (`src/planner/cypher.rs:20`), `RelBackend::lower`, or
`execute_with_islands`. Parameters are the only Cypher feature that is
silently lossy rather than an explicit boundary: a query with `$p` "succeeds"
with a fabricated answer.

Gap D-3 (blocking): a parameter dictionary threaded through planning,
interpreter evaluation, and SQL lowering (as bound parameters, not literal
splicing — `docs/sql_islands_completion.md:163`). Until then parameterized
queries should error loudly, not return `Null`.

## 5. Prioritized gaps

Priority order reflects what blocks "an engine on DuckDB" versus what blocks
"a *complete* engine".

### P0 — the engine is not durable, cannot write, cannot take parameters

1. **Durable storage** (D-1). `duckdb_exec.rs:59` is `open_in_memory` only.
   No file/URL connection, no `cache_httpfs`/Iceberg/Polaris despite the
   product direction in `docs/HANDOFF.md:17-19`. Unblock: connection config +
   persistence, then `graph_setup_sql` against a persistent DB.
2. **Transactional mutations** (D-2). No relational `CREATE`/`MERGE`/`SET`/
   `DELETE`; overlay has no transactions. Unblock: a DuckDB-backed write path
   plus rollback/isolation tests (`docs/sql_islands_completion.md:172-180`).
3. **Parameters** (D-3). `runtime/mod.rs:880` returns `Null`;
   `runtime/mod.rs:867-872` makes `$p` match all rows. Silent wrong answers.
   Unblock: parameter map + bound SQL parameters, or fail loudly.

### P1 — correctness and completion of the read path (SQL vs interpreter)

4. **7 open SQL/reference divergences** (`docs/sql_islands_completion.md:67-89`):
   float precision inside `CAST(... FLOAT[]/DOUBLE[])`; `SUM` empty-group
   identity; missing all-null group row for dynamic list group keys;
   `_NODES` includes endpoints (should be intermediates only — regression from
   the recursive lowering, `src/ir/rel/varlen.rs:392-405`); out-of-range
   integer literals round-tripped through `f64` in scan predicates. Exit
   criterion: 100% agreement on the eligible corpus.
5. **~39 residual island failures** (`docs/sql_islands_completion.md:92-120`):
   ~13 encoding declines in `decode_value` (`src/ir/exec.rs:583-631`, correct
   but costs pushdown); ~16 narrow-integer cast failures on encoded
   properties (one cast-target bug); 3 DuckDB binder/parser errors. Also the
   SELECT-alias scope and overflow-behaviour items at
   `sql_islands_completion.md:94-97`.
6. **Remaining `RelError::Unsupported` in read lowering**
   (`docs/sql_islands_completion.md:123-134`):
   - `GraphApply` correlated shapes beyond Inner/Semi/Anti/Optional/Scalar
     (`lower_apply`, `src/ir/rel/mod.rs:1562`); `ApplyKind::Scalar` shares the
     left-join path and its single-row contract is not enforced
     (`lower_left_apply`, `rel/mod.rs:1664`).
   - `GraphRepeat` is unrolled with `REPEAT_CAP = 8`
     (`rel/mod.rs:2087`) and declines on `until`/`until_traversal`/`emit`
     sub-traversal/conditional emit/path (`rel/mod.rs:2088-2129`).
   - `GraphCoalesce` only two arms with a pass-through correlate fallback
     (`rel/mod.rs:2282-2297`); `GraphChoose unmatched=Error` declined
     (`rel/mod.rs:2391`).
   - `GraphShortestPath`, `GraphSelect`, `GraphCap`,
     `GraphGroupCountSideEffect`, `GraphCollect`, `GraphListComprehension`,
     `GraphBarrier` all `Unsupported` (`unsupported_node_name`,
     `rel/mod.rs:7764-7789`).
7. **Result shaping / interpreter-free read** (`docs/sql_islands_completion.md`
   §4). `execute_with_islands` already returns target batches for fully
   lowered reads (`exec.rs:276`), but the default executor is still hybrid and
   the direct-read gate (§4 checklist) is not green.

### P2 — recursion, mapping, and operational ceilings

8. **Recursive path edge cases**: `_NODES` intermediates regression (P1 #4);
   Gremlin path materialization over recursive varlen declined
   (`src/ir/rel/varlen.rs:94-98`); unbounded Gremlin path rendering.
9. **BYOS/Postgres gates** (`docs/graph_relational_backend.md:128-149`):
   integer-only ids, single mapping per backend, mixed-type mapped properties
   rejected; Postgres path behind a feature and largely untested against the
   full denominator.
10. **Operational ceilings**: `MAX_EXECUTABLE_PLAN_NODES=200` /
    `MAX_EXECUTABLE_PLAN_DEPTH=64` (`rel/mod.rs:75-76`) and
    `MAX_ISLAND_PLAN_NODES=200` (`exec.rs:164`); per-island graph
    re-materialization (`docs/handoff_sql_islands.md:198-202`) — the
    `applied_setup` cache mitigates but the DDL is still `CREATE OR REPLACE`
    per fixture (`sql/mod.rs:110-119`).
11. **Gremlin unsafe-island heuristic** (`rel/mod.rs:192-204`): plans with
    ≥2 both-direction expands + select-history projects + depth >28 are
    refused wholesale as "not a safe SQL island" — a hand-tuned cut, not a
    semantic boundary.

## 6. End-to-end release gates (proposed)

Ordered; each is a hard denominator, not an aspirational target.

1. **Parameter gate**: parameterized read queries run on DuckDB with bound
   parameters and agree with the interpreter for every type; a missing
   parameter is an error, never `Null`. (Blocks P0-3.)
2. **Read-pushdown gate**: 100% SQL/interpreter agreement on the eligible
   Cypher corpus (the 7 divergences closed), then on Gremlin.
   (`docs/sql_islands_completion.md:63-64`.)
3. **Read-coverage gate**: 100% pushdown — every supported read plan lowers as
   one complete SQL query; zero residual `RelError::Unsupported` that is not
   an intentional public boundary. (`sql_islands_completion.md:134`.)
4. **Interpreter-free read gate**: direct-read is the default executor; the
   hybrid partitioner is retained only as a migration/debug facility.
   (`sql_islands_completion.md:149`.) **Freeze golden results first** — the
   interpreter is the only correctness reference (`sql_islands_completion.md:192-199`).
5. **Durability gate**: a persistent DuckDB database survives process
   restart; the graph catalog is loaded once and reused across queries.
   (`sql_islands_completion.md:167-168`.)
6. **Mutation gate**: `CREATE`/`MERGE`/`SET`/`DELETE` (incl. detach) execute
   in DuckDB transactionally with rollback/isolation tests; mixed read/write
   visibility correct. (`sql_islands_completion.md:179-180`.) Until green, the
   `contains_mutation` guard (`exec.rs:655`) stays and `mutations_are_never_islanded`
   (`tests/exec_islands.rs:148`) must keep passing.
7. **Portability gate**: BYOS and Postgres match the DuckDB denominator where
   advertised; the W3C SPARQL query-file benchmark is reported as parser/
   planner coverage only, never execution conformance
   (`docs/HANDOFF.md:151-154`).
8. **Interpreter retirement gate**: `ExecStats::fully_pushed_down`
   (`exec.rs:218`) is true for every corpus case, then (and only then) remove
   or quarantine the interpreter production entry points.
   (`docs/sql_islands_completion.md:189-195`.)

## 7. Standing invariants that gate all of the above

Do not trade these for pushdown (from `docs/sql_islands_completion.md:232-251`
and `docs/handoff_sql_islands.md:174-202`):

- **Decline, never fabricate** (`decode_value` returns `Option`,
  `exec.rs:583`; `array_value` null-substitution is property-read-only).
- **Unexpressible SQL is an error, not a guess** (`SqlError::Unsupported`,
  `sql/mod.rs:67`).
- **No silent approximation** (unbounded varlen must not become a capped
  unroll; `src/ir/rel/varlen.rs:74-76`, `varlen.rs:13`).
- **Both paths agree on element identity** (`interpreter::element_id` vs
  `rel_index_case` numbering).

## 8. Quick triage map (which layer owns a bug)

From `docs/handoff_sql_islands.md:35-43`: run one case under
`GRAPH_REL_EXEC=datafusion|duckdb|islands`. All three wrong → lowering
(`src/ir/rel/mod.rs`); datafusion right, duckdb wrong → SQL generation
(`src/ir/rel/sql/mod.rs`); only islands wrong → partitioner
(`src/ir/exec.rs`).
