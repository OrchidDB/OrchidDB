# Cypher SQL lowering audit

This is the consolidated implementation map for lowering Cypher to PostgreSQL,
DuckDB, and Trino SQL, including Arrow execution boundaries and extension
rewrites. It describes the working tree based on `94966a7d`, with the ongoing
Cypher-only library changes, reviewed on 2026-10-28. It is an internal audit,
not a public compiler API or a claim of complete source conformance.

The implementation has several SQL routes with different capabilities. A query
can become ordinary source SQL, SQL containing registered scalar UDFs, several
SQL regions connected by Arrow, or an explicit non-relational boundary. Some
queries are rejected. Passing the DuckDB conformance suite does not establish
that the same query fully lowers to PostgreSQL or Trino source SQL. DuckDB is
the sole completion engine; source pushdown can optimize away all of its work.

The remediation below adds semantic validation before SQL coercion, shared
target preparation, explicit emission scopes, narrower Arrow boundaries, and
PostgreSQL/Trino list implementations. G01–G13 retain the original findings and
record their disposition. Remaining source restrictions and performance limits
are distinguished from repaired correctness bugs.

## Reading the inventory

Strategy IDs `L01`–`L34` also appear in code comments as `SQL audit Lxx`.
Gap IDs `G01`–`G13` distinguish observed failures from risks found by inspection.
Symbols are named beside file links so references survive line-number changes.
Examples describe SQL shape unless explicitly identified as captured output.

| Route | What executes | What success establishes |
| --- | --- | --- |
| Built-in SQL | Source scans, relational operators, engine scalar functions | That particular typed query has an SQL implementation on its owner |
| SQL with UDFs | Engine relational operators plus registered `orchid_cypher_*` or compatibility scalar helpers | SQL owns relational work; scalar work still depends on Orchid kernels |
| Arrow boundary | Typed batches move between source, local projection, executable boundary, and DuckDB | Transport/composition works; this alone is not full source lowering |
| Explicit boundary | Mutation, procedure, control flow, literal input, graph source, or result transport | An admitted host operation, not necessarily one SQL statement |
| Rejection | Planning reports unsupported semantics, shape, representation, or ownership | A gap or intentional capability restriction; no execution fallback is implied |

`native_values` selects language-value representations and scalar implementations.
It does **not** mean all work runs outside SQL, or prove that every mixed-type
operation is lossless (see G01).
`language_functions` describes helper availability on a SQL session. It is
separate from both `native_values` and the completion engine.

## Pipeline and execution contracts

```text
Cypher text + separately supplied parameters and schema
  -> parser validation and normalization
  -> scoped Cypher AST and GraphPlan
  -> graph normalization, procedure binding, path/map demand
  -> bound Graph IR to typed relational plan and scalar expressions
  -> constraint proofs, required normalization, optional optimization
  -> source representations and placement
       source compiler: largest single-owner SQL subtree first
       host compiler: SQL islands around explicit executable boundaries
  -> dialect preparation and SQL AST emission
  -> source SQL / DuckDB SQL over Arrow inputs
  -> typed result transport and graph-value decoding
```

This is a conceptual order. The actual entry points do not run identical pass
lists. [compiler.rs](../../src/compiler.rs) `compile_parsed` lowers mapped reads,
runs constraint/representation/statistics preparation, and delegates source
placement and DuckDB completion to [execution/plan.rs](../../src/execution/plan.rs).
[runtime/host.rs](../../src/ir/rel/runtime/host.rs) `compile_for_host` and
[dag.rs](../../src/ir/rel/dag.rs) `prepare_with_extensions` additionally construct
host boundaries, run required relational normalization, and partition SQL
regions. [pipeline.rs](../../src/ir/rel/pipeline.rs) supplies shared rule entry
points; it does not eliminate the two orchestration paths.

| Contract | Actual behavior and audit implication |
| --- | --- |
| Source pushdown | First attempts the whole single-owner query. Source-free expressions can use the sole registered SQL source. Full pushdown avoids initializing or querying DuckDB. |
| DuckDB completion | Supported residual joins, filters, aggregates, and sorts run in DuckDB over named Arrow inputs. This is automatic, with no completion-engine choice. Missing runtime bindings fail preflight before source or local-function effects. |
| Narrow Arrow operations | `Input` and `LocalProject` can supply literals, select columns, cast declared arguments, call registered local functions, and decode graph descriptors without relational completion. |
| Host `SqlRequired` | Rejects native relational kernels, while allowing enumerated execution boundaries. A nonzero kernel count can be transport/control work rather than a missed SQL filter. This policy validates implementation coverage; it does not select a different completion engine. |
| Host `Hybrid` | Internal policy remains, but the current executable compiler still requires an implementation for every residual operator. It is not a promise of an independent general DataFusion executor. |
| SQL generation failure | Can select a smaller supported region or another legal compile-time representation. A failed database execution is not retried on another engine. |
| Optimization disabled | Required implementation stages, semantic ownership, and capability validation still apply. Optional rewrites can stop without changing semantics; required-budget exhaustion is an error. |

DataFusion supplies logical plans, types, analysis, optimizer rules, scalar
definitions, and SQL unparsing. The production execution path does not use a
general DataFusion physical query executor. Historical comments mentioning
in-process DataFusion execution must not be treated as the current architecture.

## Strategy inventory

### L01 Frontend normalization and scoped Graph IR

**What:** MATCH, OPTIONAL MATCH, WITH/RETURN, UNWIND, aggregation, UNION,
subqueries, procedures, and writes.

**How:** Validate syntax and lexical rules, normalize supported syntax extensions
before subquery extraction, bind parameter values, resolve procedures, and build
Graph IR operators. Projection planning extracts aggregates and collection or
subquery computations into operators with explicit bindings. Ordinary MATCH
becomes scans/expands/filters; OPTIONAL MATCH becomes optional apply; CALL
subqueries import correlations and export only declared outputs. MERGE builds
match/create control branches rather than a portable SQL MERGE statement.

**Guards:** Scope, arity, alias, and parameter errors belong to preparation.
Later optimization cannot justify accepting invalid source syntax. Collection
iterators and nested subqueries introduce scopes that must survive substitution.

**Code:** [preparation.rs](../../src/language/cypher/preparation.rs) `prepare`;
[parser/mod.rs](../../src/language/cypher/parser/mod.rs) `parse_query_inner`;
[dispatch.rs](../../src/language/cypher/planner/lowering/dispatch.rs) `lower_clause`;
[project.rs](../../src/language/cypher/planner/lowering/project.rs).
**Tests:** `cypher_parser_lowering`, `cypher_subquery_sql`.

### L02 Graph scans and physical identity

**What:** Node labels, relationship types, properties, IDs, and endpoints.

**How:** Mapped tables become scans with binding-prefixed columns such as
`n__id`, `n__label`, and `n__prop__name`. Query mappings contribute parsed
relational subplans. Candidate label/type partitions are combined with bag
unions and compatible schemas. Managed/local graphs expose Arrow batches or
live overlay sources. Property types, nullability, composite key components,
and identity domains remain typed; graph display text is a separate concern.

**Guards:** Physical storage label and public logical label membership are
different. `requires_logical_node_labels` requests membership payload only when
consumers need it. Local graph scans preserve deletion gaps and overlay changes;
unsupported overlay conversions decline rather than truncate values.

**Code:** [mapping.rs](../../src/ir/rel/mapping.rs)
`lower_mapped_node_scan_restricted`, `lower_mapped_rel_scan_restricted`;
[scans.rs](../../src/ir/rel/scans.rs) `lower_node_scan`, `with_public_ids`;
[columns.rs](../../src/ir/rel/columns.rs) `requires_logical_node_labels`;
[managed_scan.rs](../../src/ir/rel/managed_scan.rs).
**Tests:** `sql_compiler_identities`, `sql_compiler_composite_keys`,
`cypher_label_sql`, `scalar_primary_keys`. See G07 for identity fixes and Arrow transport limits.

### L03 Logical sources and access representations

**What:** Partitioned sources, equivalent source representations, collection
sources, computed relationships, and source access choices.

**How:** Keep alternative source definitions as typed plans until predicates
reach the relevant occurrence. Select an eligible representation, expand only
the selected alternative, restrict partitions/collection reads, and then place
the result on its owner. Computed relationship definitions become relational
joins/search operators or nested Cypher plans. Statistics can choose among legal
access and join alternatives and order total predicates by estimated work.

**Guards:** Estimates are not integrity proofs. Representation equivalence,
schema compatibility, source ownership, and cycle checks are independent of
estimated cardinality. Unknown or effectful predicates cannot be reordered just
because another evaluation order looks cheaper.

**Code:** [representation.rs](../../src/ir/rel/representation.rs) `select`;
[layout.rs](../../src/ir/rel/layout.rs);
[collection_source.rs](../../src/ir/rel/collection_source.rs);
[mapping/computed.rs](../../src/ir/rel/mapping/computed.rs);
[statistics/optimize.rs](../../src/ir/rel/statistics/optimize.rs) `optimize`;
[statistics/access.rs](../../src/ir/rel/statistics/access.rs).
**Tests:** `representation_sources`, `partition_layouts`, `collection_sources`,
`computed_relationships`, `statistics_access`.

### L04 Fixed traversals and redundant label or cast removal

**What:** Fixed-hop patterns, bound endpoints, and undirected edges.

**How:** Join node identity to relationship endpoint identity and then to the
target node. Endpoint metadata restricts candidate node tables before building
the union. Existing target bindings are checked against pattern labels.
Undirected expansion combines directions while accounting for self-loops.
Relationship history prevents reuse within the relevant pattern.

Equal identity types compare directly. `common_identity_type` reconciles
different identity representations; `cast_identity_expr` avoids a cast when the
type already matches. Later constant-join cleanup removes label equalities
proved true by literal source projections. Further endpoint-join elimination
requires the integrity proofs in L26.

**Guards:** Labels still distinguish identities from different graph partitions.
Outer-join null extension does not prove a constant label. Trail membership is
not redundant merely because the path value is never returned.

**Code:** [expansion.rs](../../src/ir/rel/expansion.rs) `lower_expand_direction`,
`mapped_endpoint_labels`, `track_relationship`;
[columns.rs](../../src/ir/rel/columns.rs) `identity_compare`;
[constant_join.rs](../../src/ir/rel/sql/constant_join.rs) `rewrite`.
**Tests:** `trino_pushdown`, `sql_compiler_traversals`, `cypher_path_demand_sql`.

### L05 Variable paths and payload demand

**What:** Variable-length expansion, path length, path values, and trail state.

**How:** Prefer a recursive CTE carrying the input columns, current identity and
label, depth, and relationship history. Add node/relationship payload only when
observed. If recursive construction fails, explicitly bounded paths with maximum
length at most `VARLEN_UNROLL_CAP` (currently 6) may use a union of fixed-hop
branches. Unbounded traversal is never silently capped. Path-demand rewriting
folds known lengths and non-null path counts and drops dead payload construction.

**Guards:** Pruning payload must preserve traversal, trail uniqueness, and row
multiplicity. Recursive work-table schemas must stay aligned with both terms.
Trino source execution requires an explicit finite traversal bound and casts the
seed to the declared recursive schema. A final LIMIT is not such a bound.
Unbounded recursion now completes as one DuckDB region over Arrow source scans;
the recursive work table stays inside that region.
Shortest paths now use an edge-unique recursive relation, stop expansion at
a bound target, and rank depth per input occurrence and destination. One-path
selection uses row_number; all shortest ties use rank. Zero-hop and node-only
outputs are preserved. This enumerates candidate trails before ranking, so it
is not a breadth-first frontier algorithm; dense unbounded graphs can be costly.

**Code:** [varlen.rs](../../src/ir/rel/varlen.rs) `lower_expand_varlen`;
[path_demand.rs](../../src/ir/rel/path_demand.rs) `specialize_plan`;
[recursive.rs](../../src/ir/rel/sql/recursive.rs) `unparse_plan`;
[recursive_projection.rs](../../src/ir/rel/optimizer/recursive_projection.rs).
**Tests:** `recursive_sql_islands`, `recursion_limits`, `cypher_path_demand_sql`,
`trino_pushdown`. See G06.

### L06 Joins and ordinary MATCH composition

**What:** Independent patterns, inner/outer joins, and successive MATCH clauses.

**How:** Build relational cross/inner/outer joins with explicit output bindings.
For ordinary streaming inner apply, absorb the left plan into the right without
an occurrence-numbering/rejoin cycle. Predicates over existing bindings can move
before unrelated fixed expansions. Aggregate inputs crossing a Cartesian join
receive explicit scope boundaries so aggregation completes before multiplication.

**Guards:** Retain bindings from both sides. Do not move predicates across
OPTIONAL, LIMIT, windows, or computed sources without the corresponding proof.
An outer join without keys still needs SQL `ON TRUE`.

**Code:** [joins.rs](../../src/ir/rel/joins.rs) `lower_join`, `lower_apply`;
[projection.rs](../../src/ir/rel/projection.rs) `push_expand_equality`;
[predicate_scope.rs](../../src/ir/rel/sql/predicate_scope.rs) `rewrite`.
**Tests:** `relational_lowering`, `cypher_optional_cardinality_sql`, `trino_pushdown`.

### L07 Correlated apply and existence shortcuts

**What:** OPTIONAL, EXISTS/NOT EXISTS, scalar/count subqueries, pattern
comprehensions, and returning CALL bodies.

**How:** Try three compact forms before general apply: group a one-hop pattern
collection by anchor and join it back; emit correlated EXISTS/COUNT without
building a list; or join a simple expansion directly on endpoints. Semi/anti
apply ignores right-side duplicates. General per-input barriers use correlation
keys and, when needed, an occurrence identity made with `row_number()`, then
join results back. Scalar apply guards against more than one right-side result.

**Guards:** Equal binding values do not identify duplicate outer occurrences.
CALL exports, per-row LIMIT/aggregation, empty right inputs, and optional null
extension constrain shortcut selection. Scalar-cardinality windows can disappear
only with a structural or valid uniqueness proof. `optional_filter` can replace
presence-only optional work with semi/anti joins when a non-null match identity
proves the meaning of the filter.

**Code:** [apply.rs](../../src/ir/rel/apply.rs)
`try_grouped_pattern_collect`, `try_simple_expand_cardinality`,
`try_simple_expand_apply`, `with_row_identity`, `guard_scalar_cardinality`;
[joins.rs](../../src/ir/rel/joins.rs) `lower_apply`;
[optional_filter.rs](../../src/ir/rel/sql/optional_filter.rs).
**Tests:** `cypher_pattern_existence_sql`, `cypher_optional_cardinality_sql`,
`correlated_sql_islands`, `cypher_subquery_sql`. See G04.

### L08 Scalar dispatch and typed kernel fallback

**What:** Arithmetic, predicates, CASE, string/math functions, dynamic values,
graph values, and expressions whose semantics differ from SQL coercion.

**How:** `lower_expr` first respects engine-owned conversion semantics and tries
structural graph scalar lowering. With native values enabled it can select list
consumers/lambdas, map construction, and opaque-value kernels before general SQL.
Otherwise it tries typed SQL, then a supported native scalar implementation when
enabled. Typed `orchid_cypher_*` calls receive Arrow arguments and implement one
semantic operation. Compatibility value/evaluator helpers also remain.

**Guards:** A fallback expression is useful only if the chosen SQL owner supports
it. DuckDB registration does not make the helper available in PostgreSQL/Trino.
Do not label the entire expression path portable because a simple primitive
example selected ordinary SQL. An unsupported helper remains a planning gap.

**Code:** [expression.rs](../../src/ir/rel/expression.rs) `lower_expr`;
[scalar_functions.rs](../../src/ir/rel/scalar_functions.rs);
[native_values.rs](../../src/ir/rel/native_values.rs);
[native_values/typed.rs](../../src/ir/rel/native_values/typed.rs) `CALLS`,
`INTERNAL`, `AGGREGATES`, `sql_name`;
[extension/functions](../../extension/functions/src/lib.rs).
**Tests:** `cypher_typed_scalar_sql`, `cypher_scalar_semantics_sql`,
`language_function_fallback`, `extension/tests/test_udfs.py`.

### L09 Exact comparisons and heterogeneous primitive properties

**What:** Cypher equality/order, NULL, NaN, mixed numeric types, and tagged
primitive property columns.

**How:** Typed comparison rules retain null/NaN semantics and permit numeric
promotion only where its domain is safe. Proven finite results can omit redundant
NaN guards. Heterogeneous primitive columns use a tag plus typed payload fields;
CASE selects compatible comparisons instead of coercing everything to text.
Sort keys use Cypher type ranks plus payload, not transport-tag ordering.

**Guards:** Finite-domain and arithmetic proofs are separate from purity.
Mixed floating/integer tagged comparisons can be rejected when exactness is not
proved. A scalar comparison proof does not automatically apply to UNION or SQL
grouping. `lower_case_condition` currently lacks a general Boolean result check
for ordinary typed expressions; see G01 and G02.

**Code:** [scalar_rule_program.rs](../../src/ir/rel/scalar_rule_program.rs)
`run_collection_comparison_rules`, `excludes_nan`;
[scalar_union.rs](../../src/ir/rel/scalar_union.rs) `comparison`, `order_keys`;
[expression.rs](../../src/ir/rel/expression.rs) `lower_case_condition`.
**Tests:** `cypher_integer_domain_sql`, `cypher_scalar_semantics_sql`,
`distinct_semantics`, `mapped_cypher_order_sql`.

### L10 Conversions and semantic ownership

**What:** `toFloat`, integer/Boolean/string conversions, and declared argument
casts for source functions.

**How:** Select the conversion contract before selecting an implementation.
Conformant string-to-float uses a verified DuckDB helper when available, otherwise
guarded SQL with trimming, grammar checks, cast, and finite-result checks.
Explicit DuckDB-native policy uses the owning engine's parser. Other primitive
conversions use typed rules, finite input-domain proofs, or scalar kernels.
Declared source argument types are converted to dialect casts only after the
source dialect is known.

**Guards:** Guarded SQL may repeat its operand, so it requires a stable operand.
Constant folding and residual evaluation cannot steal an engine-owned operation.
Helper name alone is not a verified capability. Invalid conversion and invalid
input-type behavior must be tested separately from valid input results.

**Code:** [conversion_rules.rs](../../src/ir/rel/conversion_rules.rs)
`select_string_to_float`, `can_fold_plan`;
[scalar_semantics.rs](../../src/ir/rel/scalar_semantics.rs)
`conformant_string_to_float`, `lower_cypher_conversion`;
[unparse.rs](../../src/ir/rel/sql/unparse.rs) `resolve_declared_casts`;
[pipeline.rs](../../src/ir/rel/pipeline.rs) `validate_sql_contract`.
**Tests:** conversion-rule unit tests, `cypher_scalar_semantics_sql`,
`source_scope`, scalar-UDF integration tests. See G09.

### L11 Temporal values

**What:** Dates, times, datetimes, durations, components, arithmetic, comparison,
and ordering.

**How:** Try native DATE lowering first, typed temporal SQL second, and complete
small finite-domain specialization third. Nominal temporal types distinguish
serialized transport from ordinary strings. Typed SQL derives calendar/clock,
offset, and nanosecond fields, constructs comparison/order keys, and implements
supported duration/calendar operations. Finite specialization evaluates known
values with the reference semantics and emits exact lookups/ranks. Scalar UDFs
cover additional supported operations and receive a shared query clock.

**Guards:** Serialized text is not a temporal comparison key. An unrelated
temporal column must not fence ordinary numeric work. Mixed temporal kinds,
unproved constructors, and temporal DISTINCT retain restrictions. Large SQL
expressions from component extraction are a separate cost concern from whether
the query successfully lowers.

**Code:** [temporal_semantics.rs](../../src/ir/rel/temporal_semantics.rs)
`lower_finite`, `finite_order_key`;
[runtime_date.rs](../../src/ir/rel/temporal_semantics/runtime_date.rs);
[runtime_typed.rs](../../src/ir/rel/temporal_semantics/runtime_typed.rs);
[scalar_rule_program.rs](../../src/ir/rel/scalar_rule_program.rs) `run_temporal_rules`.
**Tests:** `cypher_temporal_sql`, `cypher_temporal_operators_sql`,
`cypher_runtime_date_sql`, `cypher_temporal_finite_sql`, `mapped_temporal_runtime_sql`.

### L12 Map elimination and structural graph scalars

**What:** Closed map/list constructors, selected map fields, first fields of
collected maps, graph equality/grouping, labels, keys, and graph coalesce.

**How:** Substitute evaluation-free constructors through closed projection scopes
when consumers observe only selected fields. A first-field collect rewrite uses
a singleton list to retain NULL that scalar COLLECT would discard. Structural
graph operations read identity/presence/property columns directly. Graph
coalesce chooses all components from one selected binding using its presence;
independent COALESCE on every property would mix different nodes.

**Guards:** Stop substitution at joins, aggregation, optional null extension,
DISTINCT, and correlation. Escaping maps still need a typed value carrier.
Mapped/provider graph identity has additional fences; local snapshot proofs
must not be applied to every provider.

**Code:** [cypher_maps.rs](../../src/ir/rel/cypher_maps.rs)
`scalarize_projection`, `scalarize_collected_map_projection`;
[graph_scalars.rs](../../src/ir/rel/graph_scalars.rs)
`lower_graph_scalar`, `graph_coalesce_projection`, `graph_group_keys`;
[projection.rs](../../src/ir/rel/projection.rs) `project_item_exprs`.
**Tests:** `cypher_map_sql`, `cypher_collected_map_sql`, `cypher_graph_identity_sql`,
`cypher_graph_coalesce_sql`, `cypher_graph_group_sql`.

### L13 List carriers and UNWIND

**What:** Literal/dynamic lists, typed list properties, ranges, list addition,
indexing, slicing, and UNWIND.

**How:** Homogeneous literals become native SQL lists with concrete types for
empty/null children. List properties can use typed shadow columns instead of
their display encoding. UNWIND becomes VALUES or UNNEST; a safe singleton can
become a projection. A terminal literal UNWIND has a separate compact emitter.
Inclusive ranges use an integer generator rather than overflowing `stop + 1`.
Typed list concat/append/prepend retains Cypher null rules.

**Guards:** Ordinary UNWIND drops null/empty lists; optional collection machinery
may inject a typed null element to retain its parent. Distinguish an empty list,
null list, null element, and a synthesized presence row. Dynamic zero-step range
validation and heterogeneous lists can require kernels or rejection.

**Code:** [cypher_lists.rs](../../src/ir/rel/cypher_lists.rs) `literal`;
[collections.rs](../../src/ir/rel/collections.rs) `outer_list`;
[collection_operators.rs](../../src/ir/rel/collection_operators.rs) `lower_unwind`;
[collection_evaluation.rs](../../src/ir/rel/collection_evaluation.rs)
`lower_dynamic_integer_range`; [range.rs](../../src/ir/rel/range.rs);
[compact_unwind.rs](../../src/ir/rel/sql/compact_unwind.rs) `emit`.
**Tests:** `cypher_compact_unwind_sql`, `cypher_nested_collection_sql`,
`cypher_list_order_sql`, `cypher_collection_evaluation_sql`.

### L14 Native SQL list lambdas

**What:** List filters/transforms, quantifiers, and selected cardinality consumers
on the native-values route.

**How:** Keep a typed lambda body in compiler metadata and expose captured
columns as function arguments so demand analysis sees them. Emit DuckDB
`list_filter`/`list_transform` with explicit three-valued reductions, or Trino
`filter`/`transform`/`any_match`/`all_match`/`none_match`. DuckDB scalar subqueries
inside lambda bodies are replaced by supported focused scalar kernels.

**Guards:** Lexical capture and shadowing must survive nested lambdas. Eligibility
and emission share one typed capability check. Trino single/count use ROW
reduction state. PostgreSQL flat arrays use ordered UNNEST with MATERIALIZED
source/predicate evaluation and explicit three-valued reductions. Nested/ragged
arrays retain a fence because PostgreSQL UNNEST flattens dimensions. See G05/G06.

**Code:** [collection_lambda.rs](../../src/ir/rel/collection_lambda.rs)
`lower_sql_lambda`, `lower_list_consumer`;
[sql/lambdas.rs](../../src/ir/rel/sql/lambdas.rs) `adapt`.
**Tests:** `trino_pushdown`, `cypher_constant_nested_quantifier_sql`,
`cypher_list_comprehension_lowering`.

### L15 Quantifiers and cardinality without list construction

**What:** ALL/ANY/NONE/SINGLE, `size([x IN xs WHERE p])`, Boolean COLLECT consumers,
and iterator-independent predicates.

**How:** On the non-native-values route, a total primitive predicate becomes a
correlated scalar UNNEST with counts of true, false, and unknown. Reconstruct
the language truth table, including null versus empty input. If the predicate
does not use the item, combine list cardinality with its truth value and avoid
UNNEST. Boolean quantification over eligible COLLECT can fuse into aggregate
counters. General collection evaluation numbers input occurrences, expands,
reduces, and rejoins rather than conflating equal outer values.

**Guards:** Require repeatable list producers, immutable source plans, valid
element domains, and total intermediate arithmetic before duplicating work.
Nullability of the list differs from nullability of its elements. Effectful
fallback shapes are not automatically valid on every engine (G02/G04).

**Code:** [quantifier_scalar.rs](../../src/ir/rel/quantifier_scalar.rs)
`lower_scalar_collection_reduce`, `fuse_boolean_collect`;
[collection_evaluation.rs](../../src/ir/rel/collection_evaluation.rs)
`prepare_collection_projection`;
[collection_operators.rs](../../src/ir/rel/collection_operators.rs) `lower_quantifier`.
**Tests:** `cypher_scalar_quantifier_sql`, `cypher_quantifier_length_sql`,
`cypher_quantifier_cost_sql`, `cypher_collection_evaluation_sql`.

### L16 Ordered primitive list reconstruction

**What:** Eligible non-native-values list comprehensions returning a list.

**How:** Build a correlated scalar relation over generated element indices,
extract each item, apply filter/map, and aggregate the result ordered by the
original index. Restore typed empty-list and null-list results. This avoids a
global outer-row ordinal while preserving element order.

**Guards:** The input must be a repeatable primitive list from an immutable
source; map/filter syntax must satisfy a deliberately narrow totality proof.
Nested comprehension sources do not use this shortcut. Do not confuse it with
the native lambda route in L14 or cardinality-only reduction in L15.

**Code:** [primitive_list_comprehension.rs](../../src/ir/rel/primitive_list_comprehension.rs)
`lower_primitive_list_comprehension`.
**Tests:** `cypher_primitive_list_comprehension_sql`, `cypher_list_order_sql`.

### L17 List membership and equality

**What:** IN, list equality/inequality, and supported list ordering.

**How:** Choose literal equality expansion or typed native-list membership with
explicit null behavior. Primitive list equality compares cardinality and aligned
elements under three-valued semantics rather than assuming backend array
equality matches Cypher. Homogeneous integer/Boolean list ordering has a guarded
native route; other domains can need language keys or kernels.

**Guards:** Membership and scalar totality use different proof whitelists.
NaN, mixed numeric lists, nested nulls, volatile operands, and empty domains must
not be generalized from a successful simple integer case.

**Code:** [scalar_rule_program.rs](../../src/ir/rel/scalar_rule_program.rs)
membership rule; [list_comparison.rs](../../src/ir/rel/list_comparison.rs)
`lower_primitive_list_equality`, `arithmetic_is_total`, `repeatable`;
[scalar_semantics.rs](../../src/ir/rel/scalar_semantics.rs) `cypher_list_order_type`.
**Tests:** `cypher_membership_sql`, `cypher_list_comparison_range_sql`.

### L18 Aggregation and collection demand

**What:** COUNT, SUM/AVG, MIN/MAX, standard deviation, percentiles, grouping,
COLLECT, and DISTINCT aggregates.

**How:** Build SQL aggregate calls with explicit distinctness, empty-input
defaults, null-elision, and type handling. `count(node)` counts its presence ID;
distinct graph counts use identity rather than rendered payload. Group graph
values by identity and retain representative payload. Semantic scalar keys can
be paired with representative values. COLLECT uses native arrays where lossless,
or typed aggregate finishing helpers. Ordered/distinct collection paths can
carry input ordinals and first-occurrence ranks. Closed `size(collect(x))`
consumers rewrite to COUNT; no collection allocation is needed.
SUM/AVG give untyped NULL operands an accumulator type without casting already
typed numeric inputs. Temporal capability checks inspect consumed arguments,
so an unrelated temporal sort column does not block integer aggregation; graph
grouping and counts validate their structural identity operands separately.

**Guards:** Current collection implementations preserve their established input
order; an order-insensitive consumer proof can remove that requirement. Preserve
sibling aggregates and all-null groups when ranking distinct collection inputs.
The explicit `SqlNative` aggregate profile changes selected semantics; audit it
separately from the language-compatible profile.

**Code:** [rules/aggregate.rs](../../src/ir/rules/aggregate.rs);
[aggregates.rs](../../src/ir/rel/aggregates.rs) `lower_collect_non_null`,
`lower_first_distinct_collect`, `count_input_rows`, `normalize_outputs`;
[operators.rs](../../src/ir/rel/operators.rs) `GraphAggregate` arm.
**Tests:** `cypher_simple_count_sql`, `cypher_graph_group_sql`,
`cypher_window_demand_sql`, `cypher_sql_cost`.

### L19 DISTINCT and representative selection

**What:** Duplicate elimination, cardinality consumers, and terminal DISTINCT.

**How:** Use valid uniqueness facts to eliminate redundant DISTINCT. Cardinality
consumers group only by semantic keys. A terminal result bag without an
order-sensitive consumer can avoid first-seen representative windows. General
distinct processing retains language equality keys and representative/order
machinery where required. Required relational normalization implements remaining
DISTINCT as aggregation (or LIMIT 1 for zero-column bags).

**Guards:** A fan-out functional dependency is not row uniqueness. A nullable
unique SQL key does not always prove Cypher NULL-equal uniqueness. An unsorted
slice or collection above DISTINCT can observe occurrence order and blocks the
terminal shortcut.

**Code:** [operators.rs](../../src/ir/rel/operators.rs) `terminal_distinct`,
`GraphDistinct` arm; [rules.rs](../../src/ir/rel/rules.rs)
`lower_cardinality_input`, `eliminate_redundant_distinct`;
[optimizer.rs](../../src/ir/rel/optimizer.rs) `ConstraintDistinctGuard`;
[lowering_stage.rs](../../src/ir/rel/lowering_stage.rs).
**Tests:** `distinct_semantics`, `cypher_window_demand_sql`, `trino_pushdown`.

### L20 UNION and branch alignment

**What:** UNION ALL, UNION DISTINCT, and graph-valued union outputs.

**How:** Align branch outputs by position/name. Mismatched Cypher scalar types
are packed on both branches before UNION can choose a lossy common SQL type.
DISTINCT unions concatenate first, group by Cypher semantic keys, and retain an
original representative payload: 1 and 1.0 compare equal, while adjacent large
integers and doubles remain distinct. Structural graph unions align physical
columns and retain identity. Boundary reentry applies the same semantic grouping.

**Guards:** Exact mixed-value behavior requires the language carrier/key
implementation and may therefore require DuckDB completion. It must never
be replaced by an implicit DOUBLE cast. Graph unions still require compatible
layouts; missing fields are not silently invented.

**Code:** [branches.rs](../../src/ir/rel/branches.rs) `lower_union`;
[graph_scalars.rs](../../src/ir/rel/graph_scalars.rs) `lower_graph_union`;
[runtime/reentry.rs](../../src/ir/rel/runtime/reentry.rs) `lower_union_after_boundary`.
**Tests:** `cypher_sql_cost`, `distinct_semantics`, `cypher_graph_identity_sql`.

### L21 Sorting and slicing

**What:** ORDER BY, static/dynamic SKIP/LIMIT, and per-input slices.

**How:** Build typed sort keys, including graph/heterogeneous/temporal keys where
needed. Plain static slicing uses LIMIT/OFFSET. Proven valid independent dynamic
bounds remain SQL limit expressions evaluated once, rather than per-row random
filters. Correlated barriers can use partitioned row numbers. PostgreSQL's exact
ordered top-one-per-partition pattern can become DISTINCT ON. Final projection
rewriting keeps observable ORDER BY at the outer query level, especially on Trino.

**Guards:** Slices block unsafe filter/projection movement. Per-invocation dynamic
bounds need separate lowering. Deterministic row-independent bounds are
validated during graph preparation, before input cardinality can suppress the
error. A Filter above Limit receives a separate SQL scope so WHERE cannot
migrate before LIMIT during unparsing (G03/G04).

**Code:** [operators.rs](../../src/ir/rel/operators.rs) `GraphSort`, `GraphSliceExpr`;
[dynamic_slice.rs](../../src/ir/rel/dynamic_slice.rs) `bound`;
[postgres_top_one.rs](../../src/ir/rel/sql/postgres_top_one.rs) `rewrite`;
[unparse.rs](../../src/ir/rel/sql/unparse.rs) `preserve_limit_output`.
**Tests:** `cypher_dynamic_slice_sql`, `cypher_sql_cost`, `mapped_cypher_order_sql`.

### L22 Procedures and dependent table functions

**What:** Registered read procedures, table-function sources, and arguments that
must be bound before preparing source SQL.

**How:** Immutable registered table snapshots with successful primitive constant
arguments can specialize into relational values before capability validation.
Other supported table functions lower to direct/lateral SQL, a replacement
logical plan, or a dependent SQL template executed with source-row bindings.
Outer functions use LEFT LATERAL semantics. Procedures outside those contracts
remain explicit executable boundaries.

**Guards:** Procedure specialization requires total input and cannot erase errors
just because a procedure returns no rows. Dependent templates must match output
schema, source parameter count, and owning dialect. Binding uses typed AST
literals. A dependent execution loop is not a single set-oriented source query.

**Code:** [table_procedures.rs](../../src/ir/rel/table_procedures.rs) `specialize_call`;
[sql/lowering.rs](../../src/ir/rel/sql/lowering.rs) `lower_relation`,
`lower_table_function_inner`, `RelationLowering`;
[dependent.rs](../../src/ir/rel/dependent.rs),
[dependent/exec.rs](../../src/ir/rel/dependent/exec.rs).
**Tests:** `cypher_table_procedure_sql`, `collection_sources`, `engine_adapter`.
See G04 for composed procedure scope failures.

### L23 Portable functions and extension rewrite hooks

**What:** `fn.*` functions, declared source functions, pgvector/engine extensions,
and custom typed relational rewrites.

**How:** A logical function carries native identity, signature, volatility,
per-dialect SQL templates/overloads, and optional ordering mapping. Adapt syntax
as ASTs, substituting argument nodes into trusted templates. Typed preparation
handles calls that need more than function renaming. `DialectAdapter` can
rewrite expressions/queries/input CTEs, provide codecs, lower complete function
calls, describe relational ownership, or return SQL/replacement/dependent plans.
Built-in Trino is implemented through this adapter interface.

**Guards:** Require explicit mappings and argument/domain validation; backend
name similarity is insufficient. Retain aggregate FILTER, DISTINCT, ordering,
and windows. A scoring order mapping applies only to the recognized bare score,
not arbitrary arithmetic. Custom dialect registration cannot replace reserved
built-in names. Extension availability is not established merely by emitting its
function name. The DuckDB extension module registers scalar UDFs only.

**Code:** [functions/logical.rs](../../src/ir/functions/logical.rs);
[functions/portable.rs](../../src/ir/functions/portable.rs) `ScalarCapability`;
[portable/support.rs](../../src/ir/functions/portable/support.rs);
[sql/functions.rs](../../src/ir/rel/sql/functions.rs) `prepare_scoped_ast`, `template`;
[sql/logical_functions.rs](../../src/ir/rel/sql/logical_functions.rs);
[dialect.rs](../../src/ir/rel/sql/dialect.rs) `DialectAdapter`.
**Tests:** `portable_functions`, `engine_adapter`, `source_scope`,
`language_functions`. See G10.

### L24 JSON scalar and relational transformations

**What:** JSON access, paths, canonicalization, typed conversion, and row expansion.

**How:** Typed JSON functions can lower to complete scalar subqueries or recursive
SQL transformations, not just renamed functions. Canonicalization accounts for
duplicate members using their final occurrence. Row-producing operations use
engine-owned JSON table functions; native-list alternatives can become ordinary
relational plans. PostgreSQL uses JSONB where its representation is appropriate.

**Guards:** Keep JSON null separate from SQL null and retain nominal JSON type
metadata. Paths, duplicate keys, conversions, and structured property transport
need independent checks. These implementations do not establish that every
stored-JSON Cypher projection is supported (G08).

**Code:** [functions/json.rs](../../src/ir/functions/json.rs);
[sql/json.rs](../../src/ir/rel/sql/json.rs) `canonical_sql`, `lower_function`;
[json_transform.rs](../../src/ir/rel/sql/json_transform.rs) `lower`;
[json_rows.rs](../../src/ir/rel/sql/json_rows.rs) `lower`.
**Tests:** `json_integration`, `json_collections`, `cypher_property_transport_sql`.

### L25 Ranked search and connector rewrites

**What:** Computed vector/full-text relationships and extension-owned sources.

**How:** A `RankedJoin` retains source/target, metric, filter, ordering, and limit.
Metadata selects ordinary relational scoring or indexed source SQL. PostgreSQL
pgvector can use a correlated source query or a bound dependent template.
DuckDB Lance access is an existing dependent extension rewrite. DuckDB Iceberg
and observability rewrites and Trino connector providers also remain in code.

**Guards:** Format/index facts and ownership choose the implementation. An index
score consumed by a complete relation rewrite need not also have a standalone
scalar mapping. These are SQL-owner capabilities, not extra bundled Weaviate or
Elasticsearch clients. They are retained scope surface and need an explicit
keep/remove decision if the library is to shrink further.

**Code:** [search.rs](../../src/ir/rel/search.rs);
[sql/search.rs](../../src/ir/rel/sql/search.rs) `lower_relation`, `postgres_template`,
`lance_template`; [duckdb_iceberg.rs](../../src/ir/rel/sql/duckdb_iceberg.rs);
[duckdb_observability.rs](../../src/ir/rel/sql/duckdb_observability.rs);
[trino/connectors.rs](../../src/ir/rel/sql/trino/connectors.rs).
**Tests:** `computed_relationships`, `sql_compiler_lake_queries`,
`duckdb_observability`, `trino`. See G12.

### L26 Constraint-backed simplification

**What:** Redundant DISTINCT, joins/endpoints/self-joins, grouping keys,
contained union branches, and guaranteed existence checks.

**How:** Analyze uniqueness, non-nullness, functional dependencies, source
lineage/completeness, and foreign keys. Eliminate an unused outer-join target
with at-most-one proof; eliminate an inner endpoint only with exactly-one proof.
Reduce functionally determined grouping keys while retaining their values as
aggregated representatives. Remove proven contained set-union subsets and
guaranteed semi-join membership. Record `RewriteProof` evidence.

**Guards:** Only enforced facts or facts validated in the matching snapshot
scope authorize these changes. Declared facts and estimates do not. Filtered
targets, nullable foreign keys, non-injective casts, null-aware joins, and
observable expressions restrict removability. Run again after demand pruning,
when unused endpoint columns become visible.

**Code:** [constraints/mod.rs](../../src/ir/rel/constraints/mod.rs) `Evidence::usable`;
[constraints/properties.rs](../../src/ir/rel/constraints/properties.rs);
[constraints/rewrite.rs](../../src/ir/rel/constraints/rewrite.rs) `rewrite_node`;
[pipeline.rs](../../src/ir/rel/pipeline.rs) `relational`.
**Tests:** `advanced_sql_rules`, `foreign_key_relationships`, constraint unit tests.

### L27 Required relational normalization and optimizer guards

**What:** Remaining DISTINCT, structural empty inputs, extension relation
rewrites, projection pruning, and residual subqueries.

**How:** Before host SQL partitioning, run required normalization to a bounded
structural fixed point. Use the existing DataFusion logical optimizer with
guards for constant folding, recursive projection layouts, DISTINCT uniqueness,
and type-changing UNNEST boundaries. After SQL gets first refusal, residual
subqueries may use decorrelation/scalar-subquery-to-join rules; validate that no
unimplemented logical forms remain.

**Guards:** Optimization flags cannot skip required implementations. Do not push
field extraction below UNNEST merely because a column name survives: its type
has changed. SQL regions expose their own implementation, rather than handing
their inner expressions to an unrelated physical planner.

**Code:** [lowering_stage.rs](../../src/ir/rel/lowering_stage.rs)
`normalize`, `normalize_residual`, `validate_residual`;
[optimizer.rs](../../src/ir/rel/optimizer.rs) `session`, `prune_sql_outputs`;
[empty_relations.rs](../../src/ir/rel/empty_relations.rs).
**Tests:** `pipeline_tests` unit module, `sql_required`, `sql_execution_boundaries`.
G03 and G09 record the required-validation and statement-context regressions.

### L28 SQL preparation and cleanup sequence

The main sequence in [unparse.rs](../../src/ir/rel/sql/unparse.rs)
`unparse_plan_inner` delegates to `prepare_region` and `emit_prepared`.
Preparation owns typed validation, implementation selection and relational
cleanup; emission owns the remaining scope fences and AST rendering. Both
source and completion calls pass the same statement RuleContext. The passes
within each stage are ordered and are not interchangeable.

| Order | Transformation | Reason and guard |
| --- | --- | --- |
| 1 | Resolve declared casts; Trino typed specialization; validate semantic ownership; preserve correlation | Know types/owner before emitting any implementation |
| 2 | Try terminal compact UNWIND | Narrow alternate emitter, not general query flattening |
| 3 | Projection cleanup; adapter relation transforms; representation selection; portable/subquery validation and specialization | Expose legal typed SQL expressions and reject unsupported calls |
| 4 | Constant join keys; output demand; predicate scopes; optional presence; literal CASE | Remove proven label comparisons, dead payload, and avoidable optional work |
| 5 | Repeated cleanup/demand; aggregate-scan simplification; late constraint proofs | Earlier rewrites expose more removable aliases/endpoint joins |
| 6 | Conditional COUNT to FILTER; PostgreSQL nested-list encoding | Preserve aggregate/null and collection representation contracts |
| 7 | Clear inexact statistics scan hints; share pure partition scans; PostgreSQL source scalar placement | Avoid duplicate hidden filters/scans and repeated eligible scalar work after fan-out |
| 8 | Expand sort fetch; strip constant sorts; PostgreSQL top-one rewrite | Expose legal LIMIT and exact DISTINCT ON opportunities |
| 9 | Zero-column carriers; final demand/constraints; identity projection removal; collation/numeric width preservation | SQL needs a column; declared output types and text ordering still matter |
| 10 | Encode empty relations/literals; normalize output names; lift observable sort; preserve limit output; qualifier repair | Preserve cardinality, names, ordering, and lexical scope |
| 11 | Explicit outer-join predicate; correlation repair; CTE unparse; AST adaptation; dialect finalization | Produce executable dialect syntax with validated scopes |

Cleanup merges only safe projections: moving a computation once through a pure
rename is different from duplicating or discarding it. `literal_case` removes
structurally unreachable arms without executing them. `aggregate_scan` bypasses
column/literal naming projections. `count_filter` uses SQL FILTER where the
dialect supports its selected form. `source_scalar` deliberately materializes
eligible PostgreSQL source-owned work before join fan-out. Final AST block
compaction only removes its narrow pure-rename wrapper.

**Code:** [cleanup.rs](../../src/ir/rel/sql/cleanup.rs),
[aggregate_scan.rs](../../src/ir/rel/sql/aggregate_scan.rs),
[count_filter.rs](../../src/ir/rel/sql/count_filter.rs),
[shared_scans.rs](../../src/ir/rel/sql/shared_scans.rs),
[source_scalar.rs](../../src/ir/rel/sql/source_scalar.rs),
[literal_case.rs](../../src/ir/rel/sql/literal_case.rs),
[compact_blocks.rs](../../src/ir/rel/sql/compact_blocks.rs).
**Tests:** `cypher_sql_cost`, `cypher_simple_count_sql`, `advanced_sql_rules`,
`trino_pushdown`, SQL preparation unit tests.

### L29 CTEs and scope preservation

**What:** Recursive work tables, aggregate/UNWIND/join barriers, shared
occurrences, and correlated scalar subqueries.

**How:** Replace recursive nodes and explicitly marked scope barriers with CTE
references, emit terms separately, and order definitions by dependency.
Selected apply/quantifier/source-scalar boundaries use MATERIALIZED on
DuckDB/PostgreSQL to prevent repeated evaluation. Keep genuinely correlated
scalar scopes inline. Rebind both outer-reference metadata and direct projection
columns when aliases change.

**Guards:** A CTE alias is not inherently an evaluation barrier on every backend.
Hoisting a correlated body to a global CTE can detach it from its row. Trino
has different recursion/materialization constraints. CTE dependency cycles are
rejected. The existing nested window/LATERAL failure means scope machinery is
not complete (G04).

**Code:** [recursive.rs](../../src/ir/rel/sql/recursive.rs) `extract_ctes`,
`unparse_plan`; [correlation.rs](../../src/ir/rel/sql/correlation.rs) `preserve_scopes`;
[collections.rs](../../src/ir/rel/collections.rs) `unnest_scope`.
**Tests:** `correlated_sql_islands`, `recursive_sql_islands`, `cypher_subquery_sql`.

### L30 Dialect types and lossless result transport

**What:** Scalar widths, composite identities, structs/maps/lists, binary,
temporal domains, graph values, and SQL literals.

**How:** Preserve logical Arrow types across dialect encodings. PostgreSQL uses
schema-directed JSONB structured transport with textual leaves; nested list
children use JSONB inside a one-dimensional array to preserve ragged/empty/null
children. DuckDB uses supported native structured types. Trino uses typed
ROW/ARRAY/MAP and selected graph/path descriptors, then local descriptor decoding.
Literal emitters quote/encode values as data, including apostrophes and control
characters. Output casts can reconcile engine result widths before consumers.

**Guards:** A transport encoding is not an equality/order implementation.
Recursive Trino seed casts, typed NULL/empty-list casts, function result casts,
and argument casts have different reasons. No blanket cast removal is safe.
Schema-directed round trips must test full-width integers and nested nulls.

**Code:** [sql/mod.rs](../../src/ir/rel/sql/mod.rs) `exchange_literal`, type mapping;
[structured.rs](../../src/ir/rel/sql/structured.rs);
[postgres_lists.rs](../../src/ir/rel/sql/postgres_lists.rs);
[trino/typed.rs](../../src/ir/rel/sql/trino/typed.rs) `specialize`;
[execution/graph_values.rs](../../src/execution/graph_values.rs);
[sql/rows.rs](../../src/ir/rel/sql/rows.rs), [sql/literals.rs](../../src/ir/rel/sql/literals.rs).
**Tests:** `typed_island_results`, `cypher_property_transport_sql`,
`trino_pushdown`, `scalar_primary_keys`. See G07/G08/G10.

### L31 Source ownership and SQL region placement

**What:** Whole-source pushdown, completion, unsupported expressions, and multiple
source queries.

**How:** The source compiler identifies owners of physical scans (excluding
internal recursive work tables) and first attempts the whole single-owner plan.
If necessary, it retains allowed local projections or recursively supplies
DuckDB inputs. A source capability miss automatically selects DuckDB completion;
if completion also cannot lower the plan, the error retains the source failure
alongside the completion failure. Host SQL eligibility checks functions,
types, extension nodes, and shape-specific subquery requirements before trying
the largest region. Memoized graph lowering reuses successful immutable children.

**Guards:** Ownership and dialect are distinct from completion. Free correlated
bodies are not standalone cacheable islands. A full relation rewrite can consume
otherwise unsupported inner scalar placeholders. Trino rejects correlated
UNNEST/windows and outer-reference LIMIT shapes in `check_subquery`.

**Code:** [execution/plan.rs](../../src/execution/plan.rs) `Builder::lower`;
[island_planner.rs](../../src/ir/rel/island_planner.rs) `IslandMemo`;
[eligibility.rs](../../src/ir/rel/sql/eligibility.rs) `SqlEligibility::visit`;
[capabilities.rs](../../src/ir/rel/sql/capabilities.rs) `SubqueryRequirements`;
[dag.rs](../../src/ir/rel/dag.rs) `partition`.
**Tests:** `trino_pushdown`, `source_completion`, `source_scope`, `exec_islands`,
`engine_adapter`. Completion tests include a factory tripwire for pushdown, missing
binding preflight, and an actual DuckDB residual filter over Arrow inputs.
See G09 for orchestration differences and G13 for extra source columns at local
projection boundaries.

### L32 Arrow execution and reentry after boundaries

**What:** Continuing SQL after mutation/procedure/control work and completing
queries over source Arrow batches.

**How:** Encode boundary output into a typed relational schema; compile supported
operators above it back into SQL. DAG input cutting substitutes typed runtime
tables and schedules each producer dependency. Inputs are obtained before taking
the SQL session lock. Result decoding restores graph bindings and logical values.
DuckDB executes the relational work over registered Arrow inputs; source sessions
can return batch streams.

**Guards:** Ordered inputs, ambiguous names, and zero-column interfaces restrict
cutting. Mutation and potentially throwing projections can require eager
boundaries. Batch APIs do not imply end-to-end streaming or zero copying:
some boundaries collect/materialize, and compatibility carriers encode payloads.
Measure bytes, batches, and dependency invocations separately from SQL count.

**Code:** [dag/exchange.rs](../../src/ir/rel/dag/exchange.rs) `cut_inputs`;
[runtime/reentry.rs](../../src/ir/rel/runtime/reentry.rs) `lower_after_boundary`;
[execution/exchange.rs](../../src/execution/exchange.rs);
[sql/duckdb_stream.rs](../../src/ir/rel/sql/duckdb_stream.rs);
[engine_exec.rs](../../src/ir/rel/engine_exec.rs).
**Tests:** `sql_execution_boundaries`, `typed_island_results`, `streaming`,
`duckdb_streaming_session`, `unified_mapped_engine`.

### L33 Mutation and read-after-write boundaries

**What:** CREATE, MERGE, SET/REMOVE, DELETE, mutable graph reads, and mapped writes.

**How:** Cypher plans writes as effectful Graph IR operations executed by compiled
host kernels/storage adapters. Mapped storage stages changes as relations and
issues INSERT/UPDATE/DELETE against mapped physical tables. Subsequent reads use
the current overlay or reenter SQL. The compatibility `cypher_update` API accepts
a narrower MATCH plus single property assignment.

**Guards:** These writes are not general source-only DML lowering across all
three dialects. Transactions, duplicate matches, identifier/endpoint changes,
label visibility, and error recovery are storage contracts as well as compiler
contracts. `sql/mutation.rs` contains retained RDF-oriented helpers and is not
the main Cypher write lowering path.

**Code:** [runtime/policy.rs](../../src/ir/rel/runtime/policy.rs) `boundary`;
[runtime/program.rs](../../src/ir/rel/runtime/program.rs);
[host/mapped_storage.rs](../../src/ir/rel/host/mapped_storage.rs);
[mapped_engine/update.rs](../../src/mapped_engine/update.rs).
**Tests:** `mapped_writes`, `mapped_updates`, `mapped_update_edges`,
`mapped_cypher_labels`, `cypher_state_snapshot`. See G07/G08.

### L34 Finite domains and effect-aware partial evaluation

**What:** Constant quantifiers, known label sets, immutable procedure rows,
finite temporal values, map projections, and dead conditional branches.

**How:** Use complete literal/source domains to specialize expressions or select
only reachable branches; use scoped effect proofs to decide whether evaluation
can be duplicated or discarded. Query-local rule sessions track visits,
revisions, bounded traces, and required versus optional outcomes. Some domain
specialization remains part of mandatory implementation selection even when
optional optimization is off.

**Guards:** Knowing an output domain does not prove the producer cannot throw.
Immutability, repeatability, totality, and external effects are separate facts.
Never use observed fixture values or a function's name as a universal proof.
Internal scoped aliases must not capture user iterator names.

**Code:** [rules/mod.rs](../../src/ir/rules/mod.rs) `RuleContext`, `RuleSession`;
[rules/effects.rs](../../src/ir/rules/effects.rs);
[query_domains.rs](../../src/ir/rel/query_domains.rs);
[cypher_lists.rs](../../src/ir/rel/cypher_lists.rs) `closed_quantifier`;
[scalar_semantics.rs](../../src/ir/rel/scalar_semantics.rs) `total_domain_column`;
[scalar_rule_program.rs](../../src/ir/rel/scalar_rule_program.rs).
**Tests:** `cypher_diagnostic_folding_sql`, `cypher_effect_regions_sql`,
`cypher_constant_nested_quantifier_sql`, rule-pipeline unit tests.

## What generated SQL should and should not contain

| SQL artifact | Existing elimination strategy | Legitimate remaining reason | Audit probe |
| --- | --- | --- | --- |
| Fixed-label equality | L04 constant join keys | Multiple label identity domains or nullable optional side | Fixed one-label traversal versus multi-label and OPTIONAL variants |
| Logical label arrays | L02 demand analysis | `labels`, label predicates, or observable graph results | Same scan returning property, labels, then whole node |
| Endpoint casts | L04 exact typed comparisons | Different identity domains or composite reconciliation | Equal-width IDs, mixed widths, composite IDs, maximum integers |
| Other casts | L10/L30 typed emission and duplicate-cast cleanup | Declared function signature, recursive seed, output width, typed null/empty list | Inspect cast purpose and input/output type, not raw count alone |
| `row_number()` | L07 compact apply, L15 scalar reduction, L19 terminal DISTINCT | Duplicate outer occurrences, per-input slices, scalar cardinality, ordered distinct collection | Duplicate anchors, empty optional side, DISTINCT followed by slice/collect |
| `array_agg` | L15 Boolean fusion; L18 cardinality rewrite | A returned list, ordered collection, or typed aggregate implementation | `size(collect(x))`, ALL over collect, returning both list and count |
| Graph serialization | L12 structural scalars and L18 presence counts | Escaping graph/map/path values or unsupported heterogeneous semantics | `count(n)`, `id(n)`, `n.name`, then `n` |
| Joins to endpoints | L26 integrity proof after demand pruning | Endpoint might be missing, filtered, duplicated, or observed | Compare enforced FK/unique facts with declared-only and nullable facts |
| CTE/derived tables | L28 safe cleanup and block compaction | Correlation scope, aggregate-before-join, evaluation count, recursive dependency | Duplicate/volatile producers and LIMIT before versus after projection |
| Repeated scans | L03 representation selection and L28 sharing | Distinct occurrences with different filters/ownership/effects | Same partition union consumed twice, then differently filtered consumers |
| CASE/null/NaN guards | L09 finite proof and L28 literal CASE | Observable null/invalid/NaN behavior | NaN, infinity, all-null, mixed types, invalid predicate |
| Hidden constant column | L27/L28 zero-column carrier | SQL cannot represent a zero-column row | Nonempty and empty zero-column bags with DISTINCT/COUNT |

Captured source probes in the prior audit produced these simplified bodies
(the fixture substitution is omitted here):

```sql
-- MATCH (p:Person) RETURN count(p) AS n
SELECT count(1) AS "n" FROM "people";

-- Trino: RETURN [x IN range(1,5) WHERE x % 2=0 | x*2] AS xs
SELECT transform(filter(ARRAY[1,2,3,4,5], x -> x % 2 = 0), x -> x * 2) AS "xs";
```

PostgreSQL now lowers this flat-list comprehension through ordered UNNEST
(see L14 and the live lambda tests). Ordered property queries can still require
a sort-input scope; each wrapper must be justified by scope, effects, or ordering.

## Gap remediation and retained limits

The initial baseline reproduced 52 failures in 29 integration targets. The
ledger retains original observations; validation below distinguishes executable
regressions from updated obsolete shape expectations.

| ID | Original failure | Implementation and evidence |
| --- | --- | --- |
| G01 | Mixed-number UNION loses precision | Both arms receive lossless carriers before SQL coercion; DISTINCT groups by Cypher equality keys. `sql_gap_regressions`, `distinct_semantics`, and union result tests cover exact numbers, nulls, and multiplicity. |
| G02 | Non-Boolean predicates become truthy | Typed Boolean/null predicates emit directly; other predicates require a strict validator evaluated only in their actual scope, preserving empty-list/dead-branch semantics. Structured semantic errors propagate through island memoization and reentry instead of becoming capability misses. Runtime CASE/quantifiers use the same rejection. `sql_gap_regressions`, `cypher_collection_evaluation_sql`, `cypher_window_demand_sql`. |
| G03 | Empty inputs suppress invalid bounds | Graph preparation evaluates deterministic independent bounds once, before scanning. The removable row-level guard is gone. `sql_gap_regressions`, `cypher_dynamic_slice_sql`. |
| G04 | Lost outer bindings, nested windows, filter/LIMIT inversion | Emission fences Sort(Join) and Filter(Limit); recursive CTE names reserve existing scopes. Independent nested quantifiers no longer fall into the legacy LATERAL/window route merely because a sibling is volatile. `cypher_table_procedure_sql`, `cypher_subquery_sql`, `cypher_quantifier_cost_sql`, `cypher_upstream_parity`. |
| G05 | PostgreSQL rejects native lambdas | Shared typed capability selects ordered UNNEST with materialized source/body evaluation for flat scalar arrays. `sql_lambda_dialects` executes empty, nullable, captured, filter/transform, and reduction cases on real PostgreSQL and Trino. Nested arrays retain an explicit restriction. |
| G06 | Missing Trino reductions, recursive completion, shortest paths | Trino single/count use typed reduction state; unbounded recursive regions complete in DuckDB without splitting their work tables. Recursive inputs spool once to bounded-memory Arrow IPC and replay without refetching the source. Shortest-path Graph IR uses recursive trails plus depth ranking. `trino_pushdown`, `sql_gap_regressions`, `mapped_sql_execution`, and live `trino_opa` coverage. Source correlated UNNEST/window restrictions remain; shortest-path enumeration has the performance limit described in L05. |
| G07 | Identity/label mismatches across SQL, snapshot, writes | Snapshot property owners and elements use the same typed Cypher identities, without a separate display-string scan; keyed catalogs retain declared key types. Cypher/public/source identity are distinct operations, mapped reads preserve full-width source keys, and dynamic labels use row membership. `cypher_state_snapshot`, `mapped_cypher_labels`, `unified_mapped_engine`, `scalar_primary_keys`, and mapped update tests. |
| G08 | Registration lifetime, dirty rows, JSON, streaming failures | Temporary helper registration survives reopen/rollback correctly; existing stream functions are checked before registration. Cypher result capture avoids allocating Gremlin property handles that dirtied every scanned row; unchanged null-property settings are inert. JSON documents retain their domain. Streaming batches compile through SQL-required execution; DuckDB scan callbacks enter the caller's Tokio runtime when polling network Arrow inputs. `duckdb_session`, `engine_incremental`, `json_integration`, `streaming`, and mapped write/error-recovery tests. |
| G09 | Source emission loses query context; capability drift | Source and completion emission share the statement RuleContext and the prepare_region/emit_prepared boundary. Lambda eligibility shares the emitter contract. Adjacent result decode/encode boundaries fuse when no state is needed. `source_completion` checks tracing and required-budget failure on all three dialects, including optional optimization disabled. |
| G10 | Trino absent from capability catalog; qualified call typing fails | ScalarCapability includes Trino availability derived from its actual adapter. Typed validation precedes mapping dispatch. Logical function result types are frozen before literal-name encoding; portable array overloads have explicit Trino guards. `portable_functions`, catalog unit tests, live lambda/array cases. Missing extension functions still require declared mappings or completion. |
| G11 | Obsolete spelling/count/rejection assertions hide behavior | Cost separates native relational kernels from admitted transport/control boundaries. Tests retain result checks and assert relational ownership, projection width, or justified SQL shape. Region diagnostics remain available for fully lowered queries. Original failures are classified in the retained validation ledger. |
| G12 | Disconnected prototypes and misleading legacy reachability | Removed 15 disconnected files under src/syntax and src/transform; neither was declared by lib.rs. Public language entry points reject Gremlin/SPARQL (`source_scope`). Compiled shared IR, equality, identity, cancellation and control helpers remain reachable and are not counted as supported language frontends. See the reachability table below. |
| G13 | Local UDF transfers unused IDs/labels/properties | Local expressions recursively report input-slot demand. Source children project only those slots and argument indices are remapped. Zero-argument calls carry one constant to preserve row counts. `source_completion` checks narrow SQL, reordered multi-column arguments, zero-argument multiplicity, and actual results. |

### Reachability and packaging disposition

| Component | Reachability and disposition |
| --- | --- |
| src/syntax, src/transform | No module declarations or callers; removed the disconnected prototype. |
| Cypher frontend | The accepted public query language; Rust and Go clients use the same compiler. |
| Gremlin/RDF branches in shared IR | Still compiled through exhaustive Graph IR dispatch and shared value/equality code. No public parser/client support; retained shared code is not a source-conformance promise. |
| JVM/control structures | Retained host cancellation, program state, and explicit control boundaries reference these types. Removing them requires a separate internal-IR migration, not deleting files based on names. |
| Legacy source protocol and catalog/streaming hooks | Reachable through host sources, procedures, managed storage, and batch execution. Transport/host boundaries remain explicit. |
| Extension mappings | Reachable through declared operator tables and source adapters. Preserve pgvector and DuckDB/Trino rewrite hooks. The separate DuckDB extension exports scalar functions only. |

### Dialect coverage boundaries

| Family | DuckDB | PostgreSQL | Trino |
| --- | --- | --- | --- |
| Primitive mapped reads/joins/count | Native SQL routes | Native SQL routes | Native SQL routes |
| Dynamic language scalars | Built-ins or verified Orchid UDFs | Explicit mappings/relational rewrites; no blanket Orchid helper support | Explicit adapter mappings and selected typed specializations |
| Native list lambdas | Filter/transform and language truth-table reductions | Ordered UNNEST for flat scalar arrays | Filter/transform/any/all/none/single/count |
| Correlated collection subqueries | Scoped relational implementations; nested regression matrix | Supported shapes subject to type/function checks | Correlated UNNEST/windows fenced; residual completion |
| Recursive variable paths | Recursive SQL; bounded fallback in shared lowering | Recursive SQL; bounded fallback in shared lowering | Finite source traversal; unbounded DuckDB completion |
| Nested structures | Native typed values plus language carriers | JSONB structured/ragged-list transport | ROW/ARRAY/MAP and selected graph descriptors |
| Extension access | Registered functions and DuckDB relation rewrites | pgvector and declared function mappings | Adapter/connector and declared function mappings |
| Cypher writes | Host mutation/storage boundary | No blanket portable source-DML contract | No blanket portable source-DML contract |

These are implementation families, not all-input support promises. Type, effect,
schema, correlation, profile, and extension-registration guards still apply.
DuckDB Arrow transport widens Float16 exactly, preserves unsigned widths,
supports Decimal32/64/128 and Decimal256 up to precision 38, and retains TIME_NS
precision. Decimal precision above 38 and nanosecond INTERVAL/Duration transport
are explicitly rejected: DuckDB cannot represent them exactly. PostgreSQL and
Trino native type capabilities are separate. Streaming currently recompiles the
SQL-required batch program for each batch; compilation amortization remains an
optimization opportunity.

## Validation and remaining coverage

DuckDB is the sole completion engine. Registered sources select source planning
automatically; full pushdown and narrow Arrow operations can eliminate DuckDB
work. There is no public completion-engine selector.

The remediation began with 52 reproduced failures across 29 integration targets.
The [failure ledger](../../target/gap-remediation/failures.json) retains each
original assertion, its classification, and its replacement verification case.
All 52 now pass. The [final validation summary](../../target/gap-remediation/current-validation.json)
records commands, counts, environment, and the final conformance binary identity.
Some failures were wrong results, invalid SQL, missing implementations, or lost
errors; others asserted obsolete helper names, transport-kernel counts, or
rejection of functionality that now lowers correctly. Updated assertions retain
result checks and inspect actual relational ownership or SQL work.

| Current check | Result and scope |
| --- | --- |
| [Full Rust workspace](../../target/gap-remediation/workspace-complete.log) | 1,386 passed, zero failed, 22 ignored with DuckDB/Trino features. Fourteen ignored live tests passed separately below; eight legacy JVM tests remain skipped. Includes core, Rust client, native binding, and scalar-function crate tests. |
| [Source planning without embedded features](../../target/gap-remediation/source-only.log) | 65 passed across source completion, dialect selection, extension rewrites, SQL compilation, federation, and adapter hooks. No default features enabled. |
| [Cypher conformance](../../target/gap-remediation/cypher-complete.json) | 3,897/3,897 passed on the final binary, including the predicate/snapshot corrections. This establishes DuckDB completion conformance, not full PostgreSQL/Trino source pushdown. |
| [Live Trino/OPA](../../target/gap-remediation/live-trino-verified.log) | Six passed: policy filters/masks, connector joins, scalar/local functions, Arrow streaming/errors, and unbounded recursive completion. The recursive test asserts every source operation executes once. |
| [Live list/array implementations](../../target/gap-remediation/live-lambdas-verified.log) | PostgreSQL and Trino each pass 14 query cases, including empty/null lists, captured bindings, three-valued reductions, transforms, filters, membership, concatenation, sorting, and indexing. |
| [Live JSON](../../target/gap-remediation/live-json.log) and [portable functions](../../target/gap-remediation/live-portable.log) | Six tests passed against PostgreSQL 16 and the DuckDB 1.5.6 CLI. Core embedded-engine tests use DuckDB 1.5.2. |
| [Native/UDF build](../../target/gap-remediation/native-udf-build.log), [Go race tests](../../target/gap-remediation/go.jsonl), and [extension tests](../../target/gap-remediation/udf-tests.log) | Native and scalar-function libraries build without embedded DuckDB features. Go: 20 passed, one live Kafka/SpiceDB test skipped. The rebuilt scalar extension loads in DuckDB 1.5.6; four tests pass for scalar-only registration, 10,000-row Arrow batches, nested nulls, and error recovery. Validated locally on macOS ARM64. |
| Audit integrity | All 34 strategy IDs have code annotations; relative links are checked against retained local artifacts and source files. |

Eight JVM-specific compatibility tests remain outside the current public Cypher
library scope. Live tests are ignored by the default workspace command and run
explicitly above. Generated logs/reports live under ignored `target/` paths and
are not expected in a fresh checkout; linked test sources are the portable
regression evidence. Older pre-remediation results remain under
`target/due-diligence/` and `target/lowering-audit/`, separate from this run.

The next audit pass should use one case record per query/schema/parameter/profile
combination and retain these independent outcomes:

1. Frontend acceptance or classified language error.
2. Selected lowering IDs and proof/capability conditions.
3. Per-source SQL and execution-plan operations, including every local call.
4. Actual backend execution result, error, types, nulls, order, and multiplicity.
5. SQL shape and execution cost: scans, joins, windows, aggregate/list allocation,
   graph encoding, bytes transferred, rows per boundary, and repeated dependencies.

Run each relevant case with native values on/off, optional optimization on/off,
constant folding on/off, empty/nonempty input, duplicate outer rows, nullable
keys, mixed numeric boundaries, shadowed iterators, and effectful/throwing
expressions. Cover DuckDB source-only, DuckDB completion, PostgreSQL source-only,
Trino source-only, and verified helper availability separately. These dimensions
are an audit plan, not a claim that the cross product is already tested.

Extend source-only coverage independently of the passing completion suite.
Require result/error parity before accepting removal of a
label, cast, row number, CTE, or semantic guard. Use existing internal probes
[trino_sql_plans.rs](trino_sql_plans.rs),
[cypher_sql_conformance.py](cypher_sql_conformance.py), and
[compare_cypher_cost.py](compare_cypher_cost.py) as starting points; inspect their
route/profile defaults before interpreting their output.

## Proposed architecture for closing the SQL gaps

Status: partially implemented architecture. Shared typed preparation/emission,
semantic-error propagation, statement context, capability checks, and boundary
demand are implemented. The fully explicit scoped-block IR and generalized
candidate-selection interface below remain migration targets; existing SQL AST
scoping passes still perform some repairs. Do not confuse those targets with
completed implementation or require a new IR merely to close the regressions.
This extends the existing [rule-stage design](rule-pipeline-rearchitecture.md)
for the current Cypher/Rust/Go scope and sole DuckDB completion engine.

### Responsibility split

Use one shared compiler pipeline over the existing Graph IR, DataFusion logical
plans, and SQL AST. Preserve the established rule scheduler, semantic kernels,
constraint proofs, and dialect adapters. The architectural change is to give
semantic validation, implementation selection, region formation, and emission
separate owners and enforce their contracts at every entry point.

`prepare_region` now owns representation selection, typed optimization,
capability validation, and relational correlation preparation; `emit_prepared`
owns final scope fences and rendering. Source planning still attempts preparation
to discover support, and host planning retains broader eligibility analysis.
Lambda eligibility already shares the emitter contract. A generalized selection
result would let both routes consume every prepared implementation and its
diagnostic reasons without repeating capability discovery.

```text
Cypher + schema + parameters
  -> bound Graph IR with validated language contracts
  -> typed logical plan + demand, scope, identity, and effect facts
  -> legal implementations for each candidate source / DuckDB target
  -> source and DuckDB regions with exact Arrow boundary schemas
  -> scoped SQL query blocks and dialect AST
  -> SQL rendering + execution through caller-owned bindings
```

Preparation may revisit a bounded group after placement or binding exposes new
facts. A shared pipeline does not require every path to have identical inputs or
run every pass exactly once. Required validation remains active with optional
optimization disabled. Scope changes invalidate dependent analyses.

| Responsibility | Required contract | Gaps addressed |
| --- | --- | --- |
| Language validation | Boolean predicate domains, exact mixed-value semantics, evaluation scope, and error behavior are settled before SQL coercion can choose them | G01–G03 |
| Demand and relational proofs | Every output, correlated reference, key, ordering requirement, label payload, and occurrence identifier has a consumer or semantic reason | G13; label/cast/window costs in L04/L19 |
| Target implementation selection | A typed operation has an implementation compatible with the actual dialect, session capabilities, correlation shape, and language contract | G05/G06/G09/G10 |
| Region formation | Fuse compatible adjacent operators on one owner; carry exactly required columns across real execution boundaries | G09/G13; unnecessary completion statements |
| Scoped SQL construction | Resolve column references through explicit input/output bindings; preserve scope, order, multiplicity, and selected types | G04; unnecessary nesting and qualifier repairs |
| Rendering and execution | Print the selected dialect AST; validate required bindings/helpers before effects; preserve execution errors without retry | Shared source/completion behavior |

### Keep language operations recognizable until target selection

Reuse typed logical-function definitions and logical extension nodes for operations
such as list transforms, quantifiers, graph values, and exact comparisons. Retain
the operation's contract until a legal implementation is selected. Backend SQL
spelling or an opaque helper name must not become its only identity.

For a list comprehension, candidate implementations can be a native SQL lambda,
a relational UNNEST/filter/ordered reconstruction, or DuckDB completion. Candidates
must prove the same null, order, duplicate, capture, and evaluation behavior.
PostgreSQL can select the relational implementation before a native lambda is
committed. Trino can select only alternatives whose full correlated shape is
supported. An unavailable alternative yields a structured reason; invalid Cypher
or an internal invariant failure is not treated as a capability miss.

Extend `DialectAdapter`, `RelationLowering`, `SubqueryRequirements`, and existing
function metadata into one target capability contract used by both preparation
and eligibility. Include argument/result types, nested shape restrictions, and
verified helper/extension registration. pgvector, DuckDB extensions, and Trino
function mappings remain typed implementation providers. The separate DuckDB
extension continues to supply scalar UDFs only.

Use a finite, deterministically ordered set of alternatives initially. Reuse
existing statistics only among legal alternatives; unknown cost is not evidence
that moving work across a source boundary is faster. No new optimizer framework
or general search engine is required for this migration.

### Make SQL overhead accountable before emission

Carry column identity, graph identity domains, nullability, keys, explicit order,
correlation, and effects in existing analyses and revision-scoped metadata.
Materialize hidden fields only when required by the selected implementation:

- A fixed label can remain a compile-time fact until a consumer observes it or
  identity discrimination/null extension requires a runtime value.
- A cast needs a type, semantic, or transport requirement. Equal physical types
  alone do not prove that all surrounding language conversion logic is removable.
- Row numbering needs an occurrence/order/cardinality requirement. Reuse a proven
  sufficient key where valid; otherwise retain the necessary window.
- A local UDF needs its arguments and any live downstream columns. Propagate that
  demand through the boundary and remap positional Arrow indices together.

For G13, the target source query is a single projection of the needed `id`, followed
by the existing local function. It should not transfer a duplicate identity and a
constant label solely because the original graph scan exposed them.

Perform demand analysis before region selection and again after implementations
introduce required hidden columns. Merge adjacent compatible DuckDB operators
into one completion region instead of manufacturing an Arrow boundary per
operator. Effects, dependencies, transaction boundaries, and incompatible session
requirements can still require separate regions.

### Construct query scopes explicitly

Introduce a narrow internal query-block construction layer over the existing SQL
AST/unparser. Bind output slots and outer references to stable scoped identities;
assign SQL aliases when constructing each block. Keep those identities local to
SQL preparation initially so this does not require replacing every logical plan.

Projection/filter/join operators can share a SELECT block when their dependencies
and evaluation contracts permit it. Aggregation, DISTINCT, windows, LIMIT,
correlation, reuse, and effects determine where a derived table or CTE is needed.
A block boundary records its reason. Flattening is permitted only after those
requirements are checked; shorter SQL is not itself a semantic proof.

After scoped construction is authoritative for an operator family, retire its
corresponding qualifier/name repair path. Keep existing AST-based dialect spelling
and literal encoding. The final renderer performs no semantic selection, placement,
or speculative fallback.

### One preparation result, used by every entry point

A proposed internal `prepare_region(plan, demand, target, query_context)` entry
point should return the selected typed plan/SQL block, exact input and output
schemas, required helpers, and selection evidence. These are proposed interfaces,
not new customer APIs. Reuse existing `PreparedSql` and execution-plan transport
where their contracts fit rather than introducing another public protocol.

Source planning, host DAG preparation, and DuckDB completion must use that result.
Eligibility consumes the same selection decision rather than maintaining a second
function whitelist. Cache decisions against plan revision, demand, lexical scope,
query semantic context, target/session capability identity, and existing schema
and parameter-sensitive keys. Preserve the shared query budget through nested
preparation; a region must not silently receive a fresh default context.

Query-independent language errors are rejected during validation. Row-independent
LIMIT/SKIP bounds need validation at query scope so empty inputs cannot suppress
it. Correlated bounds retain their specified evaluation scope. Do not eagerly
run arbitrary expressions from unreachable branches to obtain this guarantee.
This is an explicit validation contract, not a blanket constant-folding rule.

### Migration and acceptance gates

Focused fixes for the observed G01–G04 bugs can land with regressions before the
full migration. The sequence below governs architectural replacement, not a
requirement to leave known correctness bugs unfixed until the refactor finishes.

| Step | Concrete delivery | Gate before advancing |
| --- | --- | --- |
| 1 | Extract shared preparation from `unparse.rs`; carry the existing query context through source and completion paths; retain current emitters behind it | Same query, target, capabilities, and context yield matching required decisions across entry points, including compact emitters; existing result/type/error checks pass |
| 2 | Add boundary-aware demand and input-slot remapping; merge compatible completion regions using existing partitioning infrastructure | G13 transfers only required columns; full pushdown never initializes DuckDB; residual chains do not add avoidable engine queries; duplicates/nulls/UDF evaluation counts are preserved |
| 3 | Migrate predicate validation, mixed-numeric UNION alignment, and LIMIT/SKIP validation into explicit semantic contracts | G01–G03 regressions pass with optimization and native-values settings varied, including empty inputs and numeric boundaries |
| 4 | Route list/quantifier alternatives through the shared target capability decision | PostgreSQL uses its legal relational alternative; Trino capability limits are explicit; eligibility and emission agree; missing helpers fail before callbacks |
| 5 | Migrate nested/correlated operator families to scoped SQL blocks and remove displaced repair passes | G04 examples execute on actual engines; output ordering, multiplicity, shadowing, and outer references survive composition |

Each step needs three independent checks: language results/errors/types/effects,
actual backend SQL execution, and structural cost assertions such as transferred
column count, source/completion statement count, unnecessary windows, and helper
requirements. Result equivalence alone does not establish efficient SQL; SQL size
or parse success alone does not establish correctness. Classify stale assertions
separately as G11 requires. Record selection/block-boundary reasons in the existing
bounded diagnostic trace so a remaining cast, label, or window can be explained.

The implemented stages close the concrete regressions listed above. A complete
scoped-block IR migration, broader source-only conformance coverage, and removal
of additional shared legacy IR remain separate work. Retained capability fences
and shortest-path costs must stay visible during further optimization.

## Implementation surface outside the main routes

[src/ir/rules/PIPELINE.md](../../src/ir/rules/PIPELINE.md) records rule-framework
details; the consolidated strategy and gap inventory is this document.
`src/transform` and `src/syntax` were disconnected prototypes and have been removed. `src/ir/rel/gremlin*.rs`, `repeat.rs`, `rdf.rs`, and `rdf_mapping.rs`
contain retained internals; some helpers are reused by Cypher, so file names do
not establish dead code. Parser/client removal does not prove all those branches
are unreachable from internal construction APIs.

The old `engines`/`execution_engine` federation protocol and its routing helpers
also remain alongside the current source-plan route. Requests with `sources`
select source planning automatically unless they use the legacy federation
fields. The internal `engine` protocol tag accepts only DuckDB and selects the
source-plan route for source-free requests; Rust/Go source APIs expose no engine
choice. Source-plan validation rejects mixing that route with old fields. Likewise `sql/program.rs` and host procedure
machinery do not belong to the scalar-only DuckDB extension registration surface.
Audit these separately for deletion rather than counting them as additional
supported engines or public compiler APIs.
