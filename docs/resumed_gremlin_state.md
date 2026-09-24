# Gremlin state resumption handoff

Resumed the interrupted G2 work on 2026-09-23. This completes a focused slice,
not the full G2 completion plan or a Gremlin conformance claim.

## Verified scope

- Recursive repeat beyond the former eight-round unroll limit (`times(10)`).
- Empty seeds; prefix/postfix emit and until examples; traversal-form probes;
  `loops()` termination; seeds that exit a prefix until before running the body.
- Numeric sack sum and assignment, including sacks carried through a two-round
  repeat. Repeat correlation now follows the body input before nested probes,
  preserving the sack binding.
- Seeded side-effect sum/min/max reductions, including empty input retaining its
  seed; aggregate/cap bag output.
- Cross-iteration dedup is explicitly rejected during relational lowering rather
  than incorrectly evaluating an independent DISTINCT each round.
- Hoisted recursive and ordinary CTEs are emitted in dependency order. Null-safe
  apply joins use equality OR both-null because the SQL unparser rejects the
  IsNotDistinctFrom operator.

Validation command (debug build, DuckDB):

```sh
RUST_MIN_STACK=16777216 cargo test --test gremlin_state_duckdb --test repeat_semantics --test recursion_limits --test recursive_cte_support
```

Results: 13 state tests, 3 repeat tests, 3 recursion-limit tests and 1 recursive
CTE test passed; 2 diagnostic probe tests remain intentionally ignored.
`git diff --check` passed. No full corpus sweep was performed.

## Remaining gaps

- Complete nested/named-loop scope and all emit/until placement combinations
  still need independent expected-result coverage. Existing examples do not
  establish full TinkerPop repeat conformance.
- Stateful dedup and barriers across iterations, sack split/merge, and general
  query-local side-effect scheduling remain incomplete.
- Recursive bodies with unsupported barriers still use the existing bounded
  fallback for small fixed counts and otherwise fail explicitly.
- Traversal-form recursive probes support element-local correlation; probes
  depending on labels, sack, path or loop state are not generally supported.
- New seeded fold lowering supports sum/min/max. Other fold reducers and sack
  division/addAll are explicitly unsupported by this helper. Additional scalar
  sack operators have lowering but are not comprehensively tested here.
- Numeric list sum uses a DuckDB-native SQL function and explicitly declines
  direct DataFusion evaluation. Mixed numeric types, overflow, null-containing
  folds and broader numeric edge cases need additional conformance coverage.

Owned changes: `src/ir/rel/gremlin_state.rs`, `src/ir/rel/repeat.rs`,
`src/ir/rel/sql/recursive.rs`, Gremlin planner `lowering/repeat.rs`, and
`tests/gremlin_state_duckdb.rs`; small module/dispatch and join integration changes
in `src/ir/rel/mod.rs`. Existing unrelated working-tree changes were preserved.
