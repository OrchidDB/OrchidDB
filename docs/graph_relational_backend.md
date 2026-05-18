# Graph IR Relational Backend

This is the placement guide for future Cypher, Gremlin, and SQL execution work.

## Layers

- `src/ir/plan.rs` is the durable graph algebra. Language planners should keep producing Graph IR, including operators that are not relationally executable yet.
- `src/ir/df.rs` is the DataFusion extension-plan bridge. It preserves Graph IR operators as DataFusion `Extension` nodes so HEP and rule passes can inspect, rewrite, and round-trip graph plans.
- `src/ir/rel/` is the executable relational backend. It decomposes Graph IR into base relational DataFusion `LogicalPlan`s and runs those plans with `SessionContext::execute_logical_plan`.
- SQL generation belongs after relational decomposition. SQL dialect rules should consume DataFusion logical plans or SQL islands, not Graph IR directly.

## SQL Islands

The backend should lower only regions with relational semantics that are known to be equivalent. Unsupported graph regions must remain classified with a reason; they should not be silently approximated.

An island boundary is a semantic boundary, not just a syntax boundary. Good island candidates include label/type scans, property filters, one-hop expands, projections, sort/slice, row distinct, joins, unions, and simple aggregates. Poor candidates include path materialization, graph mutations, side effects, provider-specific procedures, and graph algorithms unless a rule proves an equivalent relational form.

Future dialect support should be rule driven:

- capability rules decide whether a DataFusion plan region can be emitted for a dialect;
- rewrite rules normalize DataFusion plan shapes into dialect-friendly forms;
- unparsing rules turn an accepted island into SQL;
- non-SQL regions keep their Graph IR identity and execute through a graph-aware path.

## Recursion

Variable-length expansion and repeat-style traversal should target DataFusion recursive queries when the semantics can be expressed as a recursive relation. The recursive lowering should live in `src/ir/rel/recursive.rs`, build `LogicalPlan::RecursiveQuery` through DataFusion APIs, and keep path/history columns explicit when uniqueness or path output is part of the contract.

Do not add a lower execution layer below DataFusion for this. The next executable representation after Graph IR decomposition is the DataFusion logical plan.

## Ownership

- Language-specific planning changes stay under `src/language/<language>/planner`.
- Shared semantic analysis and normalization stay before backend lowering.
- DataFusion relational lowering stays under `src/ir/rel`.
- DataFusion extension bridge and HEP integration stay under `src/ir/df.rs`.
- Conformance progress harnesses stay under `tests/` and should reuse existing Cypher and Gremlin case infrastructure.
