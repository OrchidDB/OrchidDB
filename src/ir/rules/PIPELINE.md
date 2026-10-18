# Active rule pipeline

This inventory describes the active implementation, not the disconnected
`src/transform` prototype. Query context is explicit; clones share revision,
visit budget and bounded trace state. `new_query` starts independent state.

Public backend lowering and compiler preparation create fresh query state.
The lower-level host composition API takes a query-owned `CompileOptions`
context: callers composing graph compilation, `CompiledProgram`, and
`KernelState` pass the same context to all three, and use `new_query` before
starting a separate statement. This intentionally keeps dependent compilation
inside the statement's shared budget. Existing memo caches are query-local;
extension prepared calls force rebind and clear native plans at query end.

| Boundary | Facts available and work |
| --- | --- |
| Frontend | Syntax, arity and lexical validation; GraphPlan policy and aggregate kinds retain language intent. No placeholder bridge type is used as a scalar-domain proof. |
| Graph normalization | Scoped bottom-up traversal, including control bodies, correlated branches, repeat probes and mutation arguments. Literal-true filter removal preserves all input effects and metadata. |
| Typed construction | Actual mapped schema and expression types. Scalar implementation selection chooses the requested contract before guards are expanded; compatible helpers require an adapter-verified registration. |
| Constraint preparation | Existing constraints/proof machinery analyzes the bound plan. Reanalysis after seed binding is intentional: correlated bindings can expose new facts. |
| Optimization | Existing DataFusion optimizer policy retains folding and recursive-projection guards. Optional passes may exhaust their budget at an equivalent plan. Required semantics cannot be skipped. |
| Placement | Representation, statistics and region choice follow binding. Engine-owned operations cannot acquire a different meaning by moving to residual execution. |
| Adapter preparation | Typed plan cleanup, conditional-count normalization and SQL lowering precede rendering. Compact emitters must obey required context validation too. |
| Execution | Mapped/direct/DAG paths carry the same context. Runtime kernels implement the selected contract or reject unsupported ownership. |

The canonical child traversal is `ir::analysis::children_mut`; it is exhaustive
across Node variants. `rules::graph` adds exhaustive expression fields and
lexical binder stacks. Region paths and revision tokens distinguish identical
expressions in different scopes. Every successful common-rule rewrite advances
the query revision; callers that construct/optimize plans outside a common rule
must invalidate before reusing analysis. No debug-string identity is used.

Graph normalization mutates cloned nodes in place and retains graph policy,
result form, hidden bulk/path fields and all unmodified metadata. The structural
DataFusion bridge is not run through the executable optimizer.

## Migrated aggregate and effect decisions

`graph.aggregate.collect_size` runs after normal frontend validation and removes
the old frontend shortcut. It requires a closed projection with only cardinality
consumers of a null-eliding aggregate. Distinctness, argument evaluation, grouping
and output aliases survive. Returning the list as well, an open projection, or a
shadowed local binding prevents the rewrite. Collection SQL carries explicit
null-elision, typed-empty-list, distinctness and input-order requirements.

`effects_for_bound_scalar` and `effects_for_bound_membership` adapt the two prior
proof domains without merging their whitelists. Throwing, volatility, external
effects and scoped repeatability remain separate facts. Their precondition is
successful SQL binding/domain validation; they are not a Graph IR property-type
inference facility. Unknown facts cannot authorize duplication or discarding.

## Additional implementation stages and retained boundaries

Catalog procedure binding and path representation normalization now enter named
required GraphNormalize stages, in their original order before capability
validation. Path normalization preserves traversal and multiplicity while
selecting the payload representation; it remains available with optimization
disabled, as before. Procedure binding revisits only actual calls in constructed
or island subtrees. Map projection carrier selection likewise uses named,
bounded implementation rules, including recursive projection lowering.

Cypher membership selects literal equality expansion or typed native-list lookup
through `scalar.collection.membership`, retaining its separate repeatability,
NULL, element-type and NaN proofs. Collection aggregate construction reports
success only after its null-elision, distinctness, ordering and carrier checks
succeed. Singleton UNWIND folding is an optional equivalent rule; exhausted
required rules below UNWIND remain fatal rather than becoming fallback misses.

Finite-result proofs can follow a searched CASE branch guarded by the exact
bound expression's finite interval. This requires repeatable operands and never
infers a finite result from an OR condition, simple CASE, or different value.
It removes redundant comparison NaN wrappers around conformant SQL conversions
without changing the conversion's regex, finite checks, or native policy.

Procedural work deliberately retained: syntax/arity and schema validation,
literal/value encoding, relational operator construction, aggregate state
construction, residual kernel evaluation, and adapter syntax rendering. These
are not competing rewrite schedules. Existing runtime and constraint analyses
retain their distinct proof domains; sharing vocabulary does not authorize
merging their effect assumptions. The disconnected transform prototype remains
untouched. Island memoization is query-local and excludes correlated/temporary
nodes; revision tokens do not replace its immutable-node identity contract.
