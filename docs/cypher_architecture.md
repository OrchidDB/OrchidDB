# Cypher Architecture Guide

This document describes where future Cypher work should live. It is intentionally general: use it for parser features, semantic rules, planner changes, runtime functions, and corpus coverage work.

## Principles

Cypher should move through clear layers:

1. Parse surface syntax into a typed AST.
2. Validate names, scopes, binding kinds, and query-shape rules in semantic analysis.
3. Lower validated AST into Graph IR using the Cypher planner context APIs.
4. Execute reusable value behavior in IR/runtime helpers, not planner-specific rewrites.
5. Measure language progress with the Ladybug corpus and focused regression tests.

Keep feature ownership in one layer when possible. Do not add semantic checks in runtime code, do not encode runtime behavior in the parser, and do not bypass planner context helpers to manage Cypher traversal scope manually.

## Parser And AST

Parser work belongs under `src/language/cypher/parser/` and AST shape under `src/language/cypher/ast/`.

Use parser lowering only to normalize syntax into AST concepts. Examples:

- Kuzu lambda syntax should become `Expr::ListTransform`, `Expr::ListFilter`, or `Expr::ListReduce`.
- Cypher list comprehensions should remain `Expr::ListComprehension`.
- Cast syntax should become a normal function/cast-shaped AST expression, not a runtime-specific special case.

Do not validate visible variables, binding kinds, aggregate scope, or procedure yield collisions in the parser. Those belong in semantic analysis.

## Semantic Analysis

Semantic checks belong in `src/language/cypher/semantics.rs`.

This pass should answer questions that are true before IR exists:

- Is a referenced variable visible?
- Is a variable being rebound illegally?
- Is a binding used as a node, relationship, path, or scalar consistently?
- Do `WITH`, `RETURN`, `ORDER BY`, `UNION`, subquery, and procedure-yield scopes line up?
- Is a construct valid Cypher even before considering a graph dataset?

The semantic analyzer should become the source of truth for user-facing Cypher validity. Planner checks may remain as defensive assertions, but new semantic rules should be added to `semantics.rs` first and covered by semantic unit tests.

When a new construct introduces local variables, model that as a nested semantic scope rather than filtering names after collecting free variables. This is especially important for `EXISTS`, pattern comprehensions, list lambdas, quantified predicates, and subqueries.

## Planner Lowering

Planner lowering belongs under `src/language/cypher/planner/lowering/`.

Use modules by responsibility:

- `dispatch.rs`: clause-level routing and clause composition.
- `pattern.rs`: `MATCH`, `OPTIONAL MATCH`, path and relationship expansion.
- `predicate.rs`: `WHERE`, `EXISTS`, and pattern predicates.
- `project.rs`: `WITH`, `RETURN`, aggregation, scalar expression materialization, list/pattern comprehensions.
- `context.rs`: traversal contracts, visible scope, binding kinds, and nullable state.
- `sources.rs`: graph source selection and label/type scans.

All child traversal work should go through `Lowerer::with_child_traversal`. Do not manually call push/pop traversal helpers from feature code. This keeps lexical scope restoration, traversal metadata, and future traversal-contract checks centralized.

Use the correct child traversal kind for the construct being lowered. The kind is documentation and contract data, not just bookkeeping. If a new construct needs distinct scope behavior, add or adjust a `CypherTraversalKind` in `context.rs` before lowering it.

## IR And Runtime

Reusable value semantics belong in IR expressions and interpreter/runtime modules.

Prefer lazy IR expressions when a Cypher expression evaluates per element while capturing outer row bindings. Current examples are:

- `IrExpr::ListReduce`
- `IrExpr::ListTransform`
- `IrExpr::ListFilter`

Do not lower these through plan-level `GraphApply` or `GraphUnwind` unless the construct genuinely needs relational rows. Lazy expression evaluation should use row-local binding overlays in `src/ir/interpreter/expr/`.

Runtime function behavior belongs under `src/ir/interpreter/runtime/` and should eventually be split by domain:

- casts
- lists
- strings
- temporal values
- nested values
- graph element helpers

Function aliases should normalize before dispatch. Null propagation, strict conversion, lenient conversion, and overflow behavior should be explicit policy decisions rather than incidental match-arm order.

## Casts

Cast behavior should be centralized behind a small set of modes:

- explicit strict casts that report conversion or overflow errors
- lenient/try-style casts that return null where Cypher or Kuzu expects null
- nested element casts that recurse through lists, structs, maps, and unions without turning every inner null into a top-level failure

The planner should only lower a cast request. Runtime code should decide conversion behavior based on the cast mode and target type.

## Corpus Coverage

The Ladybug runner is the language coverage signal:

```sh
CYPHER_TIMEOUT_MS=1000 cargo test --test cypher_ladybug_cases -- --nocapture
```

Focused slices are the right iteration loop:

```sh
CYPHER_SUITE=function/list CYPHER_TIMEOUT_MS=1000 cargo test --test cypher_ladybug_cases -- --nocapture
CYPHER_SUITE=cast/cast_error CYPHER_TIMEOUT_MS=1000 cargo test --test cypher_ladybug_cases -- --nocapture
CYPHER_SUITE=tck/match CYPHER_TIMEOUT_MS=1000 cargo test --test cypher_ladybug_cases -- --nocapture
```

Use `target/cypher_case_failures` to choose work. A fix should usually include:

- one or more semantic/parser/planner/runtime unit tests for the exact behavior
- one focused corpus slice run
- the headline corpus command when the change could affect broad behavior

Treat `Correct / total` as the coverage number. `Skipped` is harness or fixture coverage. `Incorrect`, `ParseError`, `PlanError`, and `RunError` are implementation gaps unless a case explicitly expects an error and the harness classifies it as correct.

## TCK And Fixture Work

Many TCK-style cases need scenario graphs. Do not fake these in planner or runtime code.

Fixture support belongs in the case runner and dataset layer:

- `tests/cypher_ladybug_cases.rs`
- `tests/cypher_case_runner/`
- `tests/data/ladybug/dataset/`

If a case depends on setup data, add structured fixture support. Do not infer graph state from expected output or add feature-specific special cases in the engine.

## Review Checklist

Before landing Cypher work, check:

- Parser changes only shape AST.
- Semantic validity lives in `semantics.rs`.
- Planner code uses `with_child_traversal` for nested traversal scope.
- Runtime behavior is reusable IR/runtime logic, not planner-only behavior.
- Existing user changes were not reverted.
- Focused tests pass.
- Relevant Ladybug slice was measured and the coverage effect is known.
