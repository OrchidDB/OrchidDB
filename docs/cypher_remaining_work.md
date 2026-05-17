# Cypher Remaining Work

This document captures the remaining work after the May 2026 Cypher implementation pass. It is written for the next engineer or agent picking up the Ladybug corpus work.

## Current State

The Cypher stack now has useful coverage across parser, planner, runtime, and harness layers, but it is still not a complete Cypher or Kuzu-compatible implementation.

Recent wins:

- Parser and lowering accept node label disjunction, `CAST(expr AS type)`, `EXISTS { MATCH ... }`, recursive relationship filter syntax, Kuzu lambda list syntax, and colon slice syntax.
- Runtime has broader scalar/list/string/date/struct/union helper coverage, stricter Kuzu-style cast errors, nested cast support, and arithmetic overflow protection.
- Planner has better `WITH ... WHERE` placement, float `SKIP`/`LIMIT` rejection, and binding-kind tracking for some invalid pattern reuse.
- The Ladybug harness now classifies expected parse/plan/run errors as accurate when case output expects database errors.

Verified focused tests:

- `cargo check` passes with warnings.
- `cargo test --test cypher_parser_lowering` passes 8 parser/lowering tests.
- `cargo test --test planner_integration cypher_ -- --nocapture` passes 5 Cypher planner integration tests.
- `cargo test --test cypher_ladybug_cases expected_error -- --nocapture` passes expected-error helper tests.

Recent corpus slices:

- `cast/cast_error`: improved from `0/224` accurate to `160/224` accurate.
- `function/list`: currently about `176/448` accurate; parser failures dropped, but runtime failures increased around transformed/list-comprehension evaluation.
- `cast/cast_to_nested_types`: still unstable under strict casting; valid nested casts need to be separated from expected invalid-cast failures.

## How To Measure Progress

Use the Ladybug runner with a short timeout while iterating:

```sh
CYPHER_TIMEOUT_MS=1000 cargo test --test cypher_ladybug_cases -- --nocapture
CYPHER_SUITE=function/list CYPHER_TIMEOUT_MS=1000 cargo test --test cypher_ladybug_cases -- --nocapture
CYPHER_SUITE=cast/cast_error CYPHER_TIMEOUT_MS=1000 cargo test --test cypher_ladybug_cases -- --nocapture
CYPHER_SUITE=cast/cast_to_nested_types CYPHER_TIMEOUT_MS=1000 cargo test --test cypher_ladybug_cases -- --nocapture
```

Failure dumps are written to `target/cypher_case_failures`. They are the best source for choosing the next high-impact slice.

## Highest Priority Work

### 1. Lazy Collection Expression Architecture

The largest near-term win is to stop lowering Kuzu lambda list functions through eager list-comprehension paths that cannot preserve nested row/local bindings correctly.

Current state:

- `LIST_REDUCE(list, (acc, item) -> expr)` has a dedicated lazy IR expression path.
- `LIST_TRANSFORM(list, x -> expr)` and `LIST_FILTER(list, x -> predicate)` parse and lower, but still depend on existing list-comprehension materialization.
- `function/list` parser failures improved, but runtime failures remain high.

Needed architecture:

- Add explicit IR expression variants for list transform and list filter, similar to the new reduce form:
  - `IrExpr::ListTransform { list, item, map }`
  - `IrExpr::ListFilter { list, item, predicate }`
- Evaluate these in `src/ir/interpreter/expr/mod.rs` using row-local binding overlays instead of plan-level materialization.
- Add DataFusion round-trip support in `src/ir/df.rs` for the new variants so explain/serialization paths do not lose them.
- Keep parser AST-level constructs in `src/language/cypher/ast/expression.rs`, but lower Kuzu lambda functions directly to the lazy IR variants where possible.
- Add tests for nested lambdas, null input, null predicate, capture of outer bindings, and nested lists.

Target corpus:

- `function/list`
- `function/lambda`
- TCK list expression suites under `tck/expressions/list`

### 2. Cast Semantics Split: Strict vs Lenient

Strict casting was a big win for expected error cases, but it can regress valid nested casts by throwing conversion errors too broadly.

Current state:

- `cast/cast_error` is now `160/224` accurate.
- `cast/cast_to_nested_types` has many run errors after strict casting.
- Runtime still contains both older lenient helpers and newer strict helpers, which creates duplicate/unreachable match arms.

Needed architecture:

- Define a single cast engine with an explicit mode:
  - `CastMode::ExplicitStrict` for `CAST(...)` and Kuzu `TO_*` functions that should error on invalid conversion.
  - `CastMode::TryOrLenient` only for functions that are specified to return null on invalid conversion.
  - `CastMode::NestedElement` for list/struct/union element conversion, where null elements and compatible nested values should not become top-level errors.
- Return a structured cast result:
  - success value
  - null result
  - conversion error
  - overflow error
  - unsupported target type
- Normalize error text at the runtime boundary so the harness does not need case-specific special handling.
- Remove duplicate function match arms in `runtime/mod.rs` once the unified cast engine is in place.

Target corpus:

- `cast/cast_error`
- `cast/cast_to_nested_types`
- `function/cast`
- user-defined type cast error suites, once UDT architecture exists.

### 3. Function Registry And Runtime Dispatch Cleanup

`src/ir/interpreter/runtime/mod.rs` has become a large match statement with overlapping aliases and unreachable arms. This makes it hard to add functions without accidental behavior changes.

Needed architecture:

- Introduce a small function registry layer:
  - canonical function name
  - aliases
  - arity
  - null propagation policy
  - implementation function
  - strict/lenient error policy
- Keep simple scalar helpers in focused modules:
  - `runtime/casts.rs`
  - `runtime/lists.rs`
  - `runtime/strings.rs`
  - `runtime/temporal.rs`
  - `runtime/nested.rs`
- Make alias resolution deterministic before dispatch, so `tofloat`, `to_float`, and `float` cannot be split across old and new code paths.
- Add table-driven tests for alias equivalence and null behavior.

Target corpus:

- `function/list`
- `function/string`
- `function/cast`
- temporal and arithmetic function suites

### 4. Match/TCK Fixture Model

Many TCK match cases reference `CSV tck`, but `tests/data/ladybug/dataset/tck/schema.cypher` is empty and there is no `copy.cypher`. The expected rows clearly assume scenario-specific graphs. A single global synthetic fixture would not honestly satisfy all match cases.

Needed architecture:

- Extend Cypher case files or the runner to support scenario graph initializers, similar to Gremlin's `graph_initializer`.
- Import or reconstruct TCK scenario setup steps instead of treating `CSV tck` as one shared dataset.
- Keep the current dataset loader for schema/copy-backed Ladybug fixtures, but add a separate path for inline scenario graphs.
- Represent scenario setup in a structured format rather than inferring nodes from expected output.

Target corpus:

- `tck/match/*`
- `tck/with/*`
- `tck/with_where/*`
- TCK boolean/list expression cases that assume a non-empty scenario graph

## Major Feature Areas Still Missing

### Recursive Relationships And Variable-Length Paths

Parser support exists for more recursive relationship syntax, but semantics are incomplete.

Needed:

- Path expansion operator that can emit path values with both node and relationship sequences.
- Correct min/max range handling, including lower greater than upper validation.
- Zero-hop behavior.
- Relationship uniqueness rules for variable-length paths.
- Filter predicates inside recursive relationship patterns.
- Path projection formatting consistent with expected `_NODES` and `_RELS`.

Target corpus:

- `recursive_join/*`
- `tck/match/match6`
- path return suites in demo DB cases

### Pattern Binding Semantics

Planner binding-kind tracking exists, but it is intentionally conservative and incomplete.

Needed:

- Full visible-symbol table per query part.
- Distinction between node, relationship, recursive relationship, path, scalar, aggregate, and imported variables.
- Correct rebinding behavior across `MATCH`, `WITH`, `OPTIONAL MATCH`, subqueries, and union branches.
- Kuzu-compatible error messages for invalid reuse.

Target corpus:

- `tck/match/match1`
- `tck/match/match2`
- `tck/match/match6`
- `with` and `with_where` TCK suites

### Aggregation And Grouping

Some aggregate behavior exists, but Kuzu/Cypher edge cases remain.

Needed:

- Complete aggregate alias visibility rules.
- Stable treatment of nulls in aggregate functions.
- `DISTINCT` inside aggregate arguments.
- Mixed aggregate/non-aggregate projection validation.
- Better decimal/numeric aggregate output formatting.

Target corpus:

- projection aggregate suites
- demo DB group-by cases
- arithmetic and numeric function suites

### Sorting, List Ordering, And Null Ordering

List sort helpers exist but are not complete.

Needed:

- Typed comparison for all supported scalar values.
- Kuzu null ordering options for `list_sort` and query `ORDER BY`.
- Stable sort behavior for nested values where expected.
- Cross-type comparison errors where Kuzu rejects input.

Target corpus:

- `function/list` sort/reverse-sort cases
- TCK order-by suites

### Nested Values: Struct, Union, Map, Lists

Struct/union support is currently represented with `Value::Map`, which is useful but not enough for full fidelity.

Needed:

- Typed nested value representation, or metadata attached to `Value::Map`, so struct and union output can preserve field order and type tags.
- Proper `union_tag`, `union_value`, `union_extract`, `struct_pack`, and `struct_extract` semantics across nulls and invalid tags.
- Parser/lowering for named arguments and map/struct literals without broad source rewriting where possible.
- Recursive casts for nested list/struct/union types without turning valid null elements into top-level conversion errors.

Target corpus:

- `cast/cast_to_nested_types`
- `function/struct`
- `function/union`
- user-defined type suites

### Temporal, UUID, Interval, And Decimal Types

The loader normalizes some values as strings, and runtime helpers provide partial behavior. This is enough for formatting smoke tests but not for full semantics.

Needed:

- Native internal representations or disciplined wrappers for date, timestamp, interval, UUID, and decimal.
- Arithmetic and comparison semantics for temporal and interval values.
- Cast validation and canonical formatting.
- Overflow handling for fixed-width integer and decimal operations.

Target corpus:

- `function/date`, `function/timestamp`, `function/interval`
- arithmetic decimal suites
- cast suites

### Subqueries And Exists

`EXISTS { MATCH ... }` parses, but subquery semantics are partial.

Needed:

- Correlated subquery planning with explicit imports.
- Proper scalar vs existential subquery result contracts.
- Scope isolation and variable shadowing rules.
- Runtime execution that can evaluate subqueries per input row without global leakage.

Target corpus:

- `WHERE EXISTS` demo DB cases
- TCK subquery predicate cases

### Updating, DDL, And Transaction Features

The current execution model is mostly read-only over an in-memory `PropertyGraph`.

Needed:

- Mutation-capable graph representation or write-log overlay.
- `CREATE`, `MERGE`, `SET`, `DELETE`, `REMOVE` planning and execution.
- Schema operations for `CREATE NODE TABLE`, `CREATE REL TABLE`, type aliases, and user-defined types.
- Transaction/checkpoint behavior only if the corpus requires it; otherwise mark unsupported explicitly.

Target corpus:

- transaction suites
- rel group suites
- user-defined type suites

## Harness And Corpus Work

### Expected Error Handling

Expected error classification is now generic, but it only applies when the implementation actually errors. Remaining `cast/cast_error` incorrect cases are successful executions that need runtime semantic fixes.

Needed:

- Keep expected-error matching conservative.
- Add phase-specific expected-error counts to full-run reports.
- Avoid changing row mismatches into expected errors unless the query actually fails.

### Failure Triage

The best workflow is:

1. Run a filtered suite.
2. Inspect `target/cypher_case_failures`.
3. Group failures by parse/plan/run/incorrect.
4. Fix one semantic family.
5. Add focused unit tests.
6. Re-run the filtered suite and at least `cargo check`.

Avoid chasing individual case files unless they represent a repeated pattern.

## Suggested Implementation Order

1. Add lazy IR forms for `LIST_TRANSFORM` and `LIST_FILTER`.
2. Refactor casts into strict/lenient/nested modes and recover `cast_to_nested_types`.
3. Clean runtime function dispatch into alias-aware modules.
4. Add scenario graph initializer support for Cypher TCK cases.
5. Implement recursive path semantics and path formatting.
6. Complete binding-kind/scope validation across query parts.
7. Expand nested value representation beyond plain maps.
8. Fill temporal/decimal/UUID semantics.
9. Add mutation and DDL support only after read-query coverage is materially higher.

## Risks

- Broad runtime changes can convert row mismatches into run errors. That is useful only when the corpus expects errors.
- Source-text parser normalization is effective for Kuzu extensions but can accidentally rewrite valid Cypher. Prefer grammar/lowering support when touching a stable syntax family.
- The `CSV tck` fixture is empty. Do not seed it from expected output; build proper scenario initializer support.
- Duplicate runtime aliases create unreachable match arms and can hide the new behavior behind an older arm.
- Full-corpus percentages can move down even when a real feature is added, because parse errors become runtime or incorrect-output failures. Track both total accuracy and failure type movement.

## Good Next Slices For Parallel Work

- Worker A: lazy `LIST_TRANSFORM` and `LIST_FILTER` IR/evaluator/DataFusion round-trip.
- Worker B: strict/lenient/nested cast refactor, preserving `cast/cast_error` gains while recovering `cast_to_nested_types`.
- Worker C: runtime function registry cleanup for list/string/cast aliases.
- Worker D: Cypher scenario graph initializer support for TCK match/with suites.

