# Handoff: SQL islands, correctness crosscheck, and the `WITH RECURSIVE` plan

State at commit `924e461`. Everything described here is committed and pushed;
the tree is clean.

This supersedes the "how do we measure the relational backend" parts of
[`handoff_rel_lowering.md`](handoff_rel_lowering.md) and
[`handoff_corpus.md`](handoff_corpus.md), which predate the crosscheck.

## 1. The one tool that matters

```
GRAPH_REL_CROSSCHECK=1 GRAPH_REL_EXEC=islands GRAPH_REL_LANG=cypher \
  cargo test --release --test graph_rel_backend_cases -- --ignored --nocapture
```

This runs every case through **both** the SQL path and the interpreter and
reports where they disagree. The divergence list, with the query and both
answers, lands in `target/graph_rel_backend_cases/summary.txt` under
`--- divergences ---`.

**Use this rather than the expected-output pass rate to find bugs.** The pass
rate conflates two unrelated things: our lowering being wrong, and the corpus
fixture being wrong. The interpreter is the reference at ~87.6%, so a case
where the interpreter agrees with us but both miss the expected output is
*not ours to fix*. The first bug chased by hand this session —
`MATCH (a:account)-[]->(b:account) ... RETURN COUNT(b)` returning 0 — turned
out to be a `snap/twitter` fixture that never loaded its 2.4M edges. The
interpreter returns 0 there too. Hours saved by checking.

Useful env vars: `GRAPH_REL_LIMIT=1600` (a good speed/signal tradeoff; the
full corpus takes 20+ minutes), `GRAPH_REL_SUITE=<path substring>` to isolate
one case, `GRAPH_REL_EXEC` ∈ `datafusion` | `duckdb` | `islands`.

**Isolating a bug: run one case in all three modes.** If `datafusion` and
`duckdb` (whole-plan) agree and only `islands` is wrong, the bug is in the
partitioner (`src/ir/exec.rs`). If all three are wrong, it is the lowering
(`src/ir/rel/mod.rs`). If `datafusion` is right and `duckdb` is wrong, it is
SQL generation (`src/ir/rel/sql/mod.rs`) — that split is what found the
unparser dropping aggregate `ORDER BY`.

Only `duckdb` mode writes the generated SQL into the per-case dump, and dumps
are only written for *failing* cases.

## 2. Where things stand

Cypher, first 1600 cases, islands mode on DuckDB:

| Metric | Start of session | Now |
| --- | --- | --- |
| Agreement with interpreter | 89.0% | **98.3%** (16 divergences) |
| Matched expected output | 50.6% | **55.0%** |
| Cases fully pushed down | — | **94.9%** |

"Fully pushed down" (`ExecStats::fully_pushed_down`) means no interpreted
operators remain — only the `GraphReturn` shaping boundary over
already-computed rows. **That is the number that gates deleting the
interpreter**, and it has to be 100% with agreement at 100%.

## 3. Architecture as it now stands

- `src/ir/exec.rs` — the partitioner. Walks top-down, finds maximal
  lowerable subtrees, executes each on an `IslandTarget`, splices results
  back as `Node::GraphValues`. Residual stays an ordinary Graph IR plan.
- `IslandTarget` — the engine is swappable. `SqlTarget` wraps any
  `SqlExecutor` (DuckDB default, Postgres behind its feature);
  `DataFusionTarget` runs in-process. `GRAPH_ISLAND_TARGET` selects at
  runtime. `tests/exec_islands.rs` asserts the targets agree.
- Never islanded: mutations (a relational run computes rows without writing
  to the catalog, so the write would vanish) and correlated subtrees (they
  fail to lower standalone on the unresolved binding).

### The rule that keeps islands honest

`decode_value` in `exec.rs` returns `Option<Value>`, and `None` makes the
whole island **decline** so the subtree is evaluated directly. Never
substitute `Null` for a type you cannot represent. `array_value` in the
catalog does exactly that by design for property reads, and reusing it here
silently emptied every `collect()` result in the corpus. An unsupported type
must be a fallback, never a wrong row.

## 4. Next up: `WITH RECURSIVE`

Variable-length expansion (`lower_expand_varlen`) is **bounded unrolling**:
the union of k-hop join chains, `VARLEN_CAP: u32 = 6`. Bounded ranges above
the cap error loudly, but

```rust
// Unbounded expands are approximated by the unroll cap.
None => VARLEN_CAP,
```

means `-[e:knows*]->` silently becomes `*1..6`. Invisible on the fixtures,
**wrong on any real graph**.

### Why this is tractable

DataFusion's unparser refuses to emit a `LogicalPlan::RecursiveQuery`
(`not_impl_err!` in `datafusion-sql/src/unparser/plan.rs` — that is the
crate's code, not ours). But it does emit each *term* on its own, including
the recursive term's self-reference to the CTE, so only the wrapper text is
missing. DataFusion can also *execute* a hand-built `RecursiveQuery`
(`RecursiveQueryExec` + `WorkTable`; `CteWorkTable::new(name, schema)` is
public via `datafusion::datasource::cte_worktable`).

Both behaviours are pinned by `tests/recursive_cte_support.rs`. **Run that
first** — if it fails, this plan needs rethinking.

### The plan

1. **`lower_expand_varlen`** builds a `RecursiveQuery`. CTE schema: carried
   input columns + `__w_cur_id`, `__w_cur_label`, `__w_depth`, `__w_trail`,
   `__w_rels`. Seed = input at depth 0, cursor on the source, `trail = ','`.
   Recursive term = `CteWorkTable` scan ⋈ edge scan on
   `cur_id = src_id AND cur_label = src_label` (reversed for `In`, a `CASE`
   picking the far endpoint for `Both`), filtered by
   `strpos(trail, key) = 0`, re-projecting the same schema with the cursor
   advanced, `depth + 1`, `trail || key`, `rels || <edge display>`.
2. **Consume it**: filter `depth BETWEEN lo AND hi`, project the target
   binding from the cursor, join the target node table for properties and
   label filtering. With a rel binding, also emit the path text and
   `e__pathlen` (the `path_len_col` plumbing already exists).
3. **`sql::unparse`**: rewrite each `RecursiveQuery` node to a `TableScan` of
   the CTE name, collect the terms, unparse main plan and terms separately,
   emit `WITH RECURSIVE a AS (… UNION ALL …), b AS (…) <main>`.
4. **`plan_tables_excluding`** must skip CTE scans or it will try to
   `CREATE TABLE`/`INSERT` the working table as catalog data. The exclusion
   mechanism already exists for BYOS mappings.
5. **Keep the unrolling as fallback** when any part of the recursive path is
   unsupported.

### Decisions already made, with reasons

- **The trail is a delimited string, not an array.** Append with `||`, test
  membership with `strpos(trail, key) = 0`. DataFusion's `array_has`
  unparses to something DuckDB does not accept, and fixing that would need a
  custom `Dialect` impl. Strings use only operators both engines share. The
  same accumulation gives `_RELS` for `RETURN e`.
- **No depth cap.** Trail semantics forbids repeating a relationship, so a
  walk is bounded by the edge count on any finite graph. A cap would
  reintroduce exactly the silent truncation being removed. Add one only as
  an opt-in performance guard.
- **`_NODES` comes second.** Rendering intermediate nodes needs the node
  table joined *inside* the recursive step. Land depth/trail/`length()` and
  unbounded traversal first.

### Risks, in order

1. The capability test has `RecursiveQuery` as the plan **root**; in real
   use it sits mid-plan under joins and projections. DataFusion's physical
   planner may not accept that position — if so, the DataFusion target needs
   the same rewrite-to-scan treatment as SQL.
2. DataFusion requires the two terms' schemas to match **exactly**; fiddly
   with carried columns.
3. Postgres requires the recursive term to reference the CTE exactly once
   and not inside a subquery. Our shape complies, but the unparser could
   nest it in a derived table.

## 5. The remaining 16 divergences

- **Path printing** (`RETURN e`) and `count(DISTINCT e)` over a
  variable-length binding — 4. The path column is materialized per branch
  but is not reaching the projection in every shape. Subsumed by the
  `WITH RECURSIVE` work.
- **Undirected `-[e*2..2]-`** `length(e)` — 1. Directed ranges work.
- **`size(collect(node))`** returns a string length — 2. Node values render
  to text in the lowering, so the collected list is text.
- **`MIN`/`MAX` on strings with non-ASCII bytes** — 3. Collation difference.
- **`collect(DISTINCT)` ordering** — 1. First-appearance order is not
  expressible as a SQL ordering; deliberately left to the engine.
- **int128 overflow comparison** — 2.

## 6. Traps worth not rediscovering

- **The unparser silently drops aggregate `ORDER BY`.** It emits ordering
  only as `WITHIN GROUP`, and only for functions accepting that clause; for
  `array_agg` it vanishes and the SQL means something different from the
  plan. `restore_aggregate_ordering` regenerates the call with the same
  unparser and splices the clause in, refusing when the call text is not
  unique. Do not assume an expression survives unparsing — diff the two
  modes.
- **`RETURN n.*` and `n.id`.** Cypher treats a property literally named `id`
  as ordinary (`node_property_keys_with_id`); Gremlin hides it. Excluding it
  for both made `WHERE n.id = 49992` lower to `WHERE NULL = 49992` under the
  NullOnMissing policy — a wrong answer, not an error.
- **Star projections are one binding, not many.** Direct evaluation makes
  `x.*` a single binding holding a map that `finalize_return` expands; the
  relational plan fans it into `__star__` columns up front. The island
  collapses them back, recording projection order separately because the
  backing map is sorted.
- **Element ids must agree between paths.** `interpreter::element_id` and
  `rel_index_case` both encode Kuzu's numbering (rel tables share the node
  namespace and reserve a reverse-adjacency slot per type). Change one and
  the same edge prints differently depending on which path ran it.
- **Do not trust an all-green suite's silence.** `cargo test` captures
  stdout unless a test fails, so `--nocapture` is required for any harness
  that reports by printing.
- **Islands re-materialize the whole graph per island** as `CREATE TABLE` +
  `INSERT`. On large fixtures a full corpus run exceeds 20 minutes. The fix
  is to load the graph into the engine once and lower through the existing
  `GraphMapping` so plans reference stable `node_*`/`edge_*` tables — the
  same direction as "the data lives in the database".
