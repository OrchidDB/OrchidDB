# Cypher: plan for full openCypher conformance

## Architecture decisions to make first

Keep the execution model: **Cypher → Graph IR → SQL IR DAG → DuckDB regions + DataFusion residual operators**. Extend semantic contracts in that pipeline; do not introduce a Cypher execution engine or execute Graph IR as a fallback.

### 1. Make language semantics explicit before SQL lowering

Extend the existing `GraphPlanPolicy` and use explicit expression/operator modes where behavior differs. The current policy already distinguishes missing properties, multiplicity, paths and match uniqueness. It does not fully specify scalar type errors, comparison versus ordering, collection operations, aggregate null handling, or temporal behavior.

- Resolve openCypher versus existing Kuzu extensions in the frontend. For example, current lowering intentionally makes `labels()` return a string; openCypher needs a list. Preserve any retained Kuzu behavior through an explicit dialect option, never through test detection.
- Encode the resolved behavior in Graph IR expressions/operators. SQL IR lowering must carry it into SQL expressions, UDFs or DataFusion residual kernels. Generic DuckDB operators are valid only when they implement that contract.
- Keep syntax, scope and statically provable type errors in Cypher semantic analysis. Carry structured dynamic errors through execution. Do not move frontend validation wholesale into Graph IR.
- Keep Gremlin's current defaults: traverser bulk, missing/unproductive values, label strings, numeric identity, null handling and path history. Do not globally change shared `Value` equality, casts or function aliases to satisfy Cypher.

**Implementation seam:** `src/ir/policy.rs`, `src/ir/expr.rs`, `src/ir/plan.rs`, Cypher expression lowering, and `src/ir/rel/{expression,aggregates,collection_operators,runtime}.rs`. Add semantic modes only where behavior actually differs; avoid a giant collection of compatibility flags.

### 2. Separate element identity, storage mapping and Cypher labels

Today `CreateNode.label` is one string; `Value::Node` identity includes a label and ID. Cypher needs zero or more labels, with identity preserved when labels change. Removing the planner's one-label restriction alone cannot implement that.

Introduce a stable storage/source identity plus a separate Cypher label set. Add label-set creation and label mutation to Graph IR. Keep Gremlin's single provider label contract explicit. Update scans, endpoints, equality, serialization and deduplication together so one element never becomes several because it has several labels.

Mapped writes must still resolve to declared tables, keys and property columns. Define an explicit writable source for unlabelled creation and declared storage for mutable labels (for example, configured membership columns or a mapped membership table). A label must not silently select a new table or cause a generic property-bag fallback. Reject ambiguous or unwritable mappings before applying effects. The TCK fixture schema can declare these capabilities once; it must not route individual scenarios to special implementations.

**Decision required in implementation:** a fixed mapping without an unlabelled creation target or mutable label storage cannot represent all openCypher writes. Full language conformance requires a capable declared schema; it does not make every user mapping writable.

### 3. Preserve typed values across every execution boundary

The current `Value::DateTime(String)` and string-valued durations cannot distinguish all Cypher temporal types. Introduce date, local time, offset time, local datetime, zoned datetime and duration values; preserve nanoseconds, zone IDs and calendar components. Durations need months/days/seconds/nanoseconds, not a single elapsed-millisecond number.

Use a lossless tagged representation for heterogeneous lists/maps and these temporal types through Arrow, SQL IR, DuckDB materialization and native result encoding. Keep Gremlin's existing datetime and native numeric/container contracts compatible. Separate three-valued predicate comparison from total ordering used by sorting and min/max. A single shared comparator cannot express both.

### 4. Complete effects and diagnostics contracts

Extend existing `GraphCreate`, `GraphMerge`, `GraphSetProperty` and `GraphDelete`; add explicit property/label removal and label updates. Carry write dependencies into the SQL IR DAG so optimizers cannot reorder, duplicate or cache effects across reads. Use the existing mapped write implementation as a foundation, including read-after-write visibility and atomic failure behavior.

Extend structured errors beyond the five current `CypherSemanticError` categories. Record type, detail and phase at the originating validator/operator, and preserve them through SQL/DataFusion failures. The adapter should translate that information, not infer expected errors from message text or the scenario.

## Evidence and grouped scope

Assessment date: 2026-09-24. Source: [recorded openCypher results](../conformance/upstream-results/crabgraph-opencypher.json), openCypher **2024.3**, pinned revision `677cbafabb8c3c5eed458fd3b1ec0daec8d67d23`.

The recorded run has **1,908 / 3,897 passing (49.0%)** and **1,989 non-passing**: 1,639 failures, 300 adapter errors, 50 skipped procedure scenarios. Its build metadata identifies `d2509ff` with a modified working tree. This is the checked-in evidence, not a fresh measurement of current `ae16e5f`. No suite was run for this assessment. The older Ladybug/Kuzu counts in `cypher_remaining_work.md` describe a different corpus.

The following partition counts each non-passing scenario once, in the order shown. It groups the first observed blocker; fixing setup can expose another failure.

| Observed blocker | Scenarios | Scope |
| --- | ---: | --- |
| Exactly-one-label creation restriction | 444 | 365 fail during GIVEN setup; 79 during the tested query |
| Remaining temporal scenarios | 923 | Temporal feature directory, after removing 18 label-blocked scenarios |
| Remaining unclassified expected errors | 263 | Excludes temporal scenarios already counted |
| Procedure registration unavailable | 50 | Upstream GIVEN procedure fixtures skipped |
| Remaining adapter errors | 23 | Native temporal/path representation, outside preceding groups |
| Remaining semantic/result/effect failures | 286 | Types, collections, ordering, updates, paths, projection and aggregation |
| **Total** | **1,989** | |

Useful overlapping views: all temporal features have **941** non-passes; **410** scenarios fail during fixture queries; **274** errors lack authoritative classification; **23** native datetime conversions and **3** native path conversions fail; **11** failures reach side-effect assertions. These numbers are not additive and are not promised pass gains.

## Ten highest-impact changes

Priority balances affected scenarios with dependencies. Counts below are observed reach, not forecasts; several changes contribute to the same scenario.

### 1. Support unlabelled and multi-label nodes end to end

**Reach:** 444 scenarios directly blocked by the creation restriction, with additional `labels()` mismatches and label mutation cases.

Implement the identity/label separation above, create zero/multiple labels, label-set scans and list-valued `labels()`. Update native vertex encoding, which currently emits only `[v['label']]`. Preserve node identity through label changes and preserve mapped table targeting.

**Owners:** Cypher `planner/lowering/dispatch.rs` and `project/expression.rs`; `CreateNode`, catalog/value identity, mapped writes, scan lowering and native adapter. **Scope: large, foundational.** Done when unchanged GIVEN queries execute and label reads/changes round-trip through the normal engine.

### 2. Implement typed temporal constructors and projections

**Reach:** 195 failures in Temporal1 (map construction), 50 in Temporal2 (string construction), 182 in Temporal3 (projection); additional temporal ordering/setup failures outside that directory.

Implement `date`, `time`, `localtime`, `datetime`, `localdatetime`, calendar/week/ordinal/quarter forms, timezone offsets and named zones, component overrides and field access. Include typed storage, native encoding and canonical rendering from the start. Current recorded errors include 303 `localdatetime/1`, 119 `localtime/1` and 116 `time/1` rejections across the corpus.

**Owners:** typed values, temporal runtime helpers, function resolution, SQL IR value transport and `conformance/upstream/cypher.py`. **Scope: large; requires architecture 3.** Do not collapse local and zoned values or nanosecond precision into DuckDB's default timestamp representation.

### 3. Implement temporal truncation as one typed operation family

**Reach:** Temporal9 has **322** non-passes, the largest individual feature file.

Support truncation units and component overrides across all temporal types, including calendar/week boundaries and zone preservation. Represent operation/type explicitly; share calendar logic with constructors. Use SQL pushdown only where units, precision and timezone semantics agree; execute the rest as DataFusion residual computation.

**Owners:** temporal expressions/functions and SQL IR lowering. **Scope: medium after #2.** Validate boundary values and overrides, not just the simplest truncation for each type.

### 4. Complete durations, temporal arithmetic and comparisons

**Reach:** Temporal10 has **127** non-passes (durations between values), Temporal8 has **27** (arithmetic), Temporal7 has **9** (comparison).

Implement duration construction, normalization, addition/subtraction and `between`/unit-specific differences with calendar and elapsed-time distinctions. Preserve negative/fractional components and daylight-saving behavior. Finish temporal storage, components and rendering (Temporal4–6: another 29 non-passes, also dependent on #2).

**Owners:** typed duration representation and temporal kernels, comparison modes and mapped property encoding. **Scope: large.** Calendar months must not become a fixed number of seconds.

### 5. Complete semantic type checking and structured error reporting

**Reach:** 274 adapter errors awaiting type/detail/phase classification; additional successful executions that should reject invalid input, including the 56 failing quantifier scenarios.

Extend existing validators for operand/function signatures, lambda item types, collection indexing, parameters, numeric literals, aggregation scopes and SKIP/LIMIT. Preserve dynamic checks for unknown types. Of the unclassified errors, 129 expect compile-time `SyntaxError/InvalidArgumentType`; 26 expect `InvalidAggregation`. These are high-leverage families, not a reason to classify every binder error alike.

**Owners:** `semantics/{expression_types,validation,aggregates}.rs`, `planner/error.rs`, runtime errors and bridge serialization. **Scope: medium/large.** An error only qualifies when its actual category and phase satisfy the original assertion; classification alone does not prove an unsupported operation correct.

### 6. Fix heterogeneous values, collection semantics and Cypher comparison

**Reach:** list 66, boolean 119, quantifier 56, comparison 24 and with-orderBy 137 non-passes, overlapping #1, #2 and #5.

Preserve mixed list element types through SQL/Arrow conversion. Recorded results expose encoded strings such as `d[1].i` as user values. Implement distinct contracts for null-aware predicate comparison and heterogeneous total ordering, including NaN. Finish index/slice/range errors, three-valued booleans and quantifiers; distinguish invalid types from legitimate null results.

**Owners:** explicit Graph IR expression modes, collection lowering, tagged value codecs and ordering/aggregate kernels. **Scope: large and broadly shared.** Representative counterexamples: `[1,2] >= [1,null]` currently returns false instead of null; mixed-type `min`/`max` compare representations rather than Cypher values. Gremlin native equality and container identity must remain unchanged.

### 7. Complete Cypher write and effect semantics on mapped storage

**Reach:** CREATE 66, MERGE 52, REMOVE 33, DELETE 31 and SET 27 non-passes; many are initially blocked by #1.

Implement currently rejected `REMOVE`, label SET/REMOVE, null-as-property-removal, map replacement/merge, MERGE branch behavior, relationship deletion constraints and DETACH DELETE. Validate same-clause binding visibility and exact side effects. Carry mutation sequencing and statement atomicity through the SQL IR DAG and declared table writes.

**Owners:** parser updating AST, Cypher mutation lowering, Graph IR effect nodes, DataFusion mutation kernels and `src/mapped_engine/write.rs`. **Scope: large.** Test persisted tables and a subsequent read, not only returned rows. Keep Gremlin property cardinality, null and removal behavior explicit.

### 8. Finish query-part scope, aggregation and ordered row semantics

**Reach:** WITH/RETURN, their ORDER BY and SKIP/LIMIT families, UNION, and aggregation. Many apparent read failures currently stop at setup, so their independent count is not yet measurable.

Complete alias visibility and aggregate validation across query parts; preserve ordering through projection, DISTINCT, pagination and correlation. Specify empty-input aggregates and null-eliding `collect`, including DISTINCT. The recorded `collect(DISTINCT n.num)` over an unmatched OPTIONAL returns `[null]` where the TCK expects `[]`. Preserve bag multiplicity and exact result column names.

**Owners:** Cypher scope/project lowering; Graph IR aggregate/null modes; SQL IR aggregate, join and ordering lowering. **Scope: medium/large.** Cypher `collect` must not change Gremlin fold or traverser bulk behavior.

### 9. Complete MATCH relationship uniqueness, optional patterns and paths

**Reach:** MATCH 73, graph expressions 54, pattern expressions 15, path expressions 4 and existential subqueries 3 non-passes, overlapping setup blockers.

Enforce relationship uniqueness at the correct MATCH scope, including undirected self-loops and multiple pattern parts; preserve legal reuse across separate clauses. Finish zero/variable-length paths, bound relationship-list reuse, path construction/direction and OPTIONAL null extension. Current failures include rejecting a bound list of relationships used in a variable-length pattern and three invalid native path encodings.

**Owners:** Cypher pattern lowering; existing `MatchMode`, `PathMode`, expansion/correlation and SQL IR path operators. **Scope: medium/large.** A Cypher path alternates nodes and relationships; Gremlin traverser history can contain arbitrary objects. Keep these contracts separate through serialization.

### 10. Add procedure catalog/signatures and upstream fixture registration

**Reach:** all **52** CALL scenarios: 50 skipped registration fixtures and two failures for missing procedures.

Add a procedure registry with signatures, argument checks, output columns, yield scope and effect metadata. Lower procedure invocation through the existing Graph IR procedure mechanism and SQL IR residual execution. Let the adapter register the exact GIVEN signatures and rows supplied by upstream scenarios through a provider API. Missing procedures must produce a classified compile-time error, not a null result.

**Owners:** procedure semantic analysis, `ProcedureMode`/procedure IR, registry/runtime and TCK registration adapter. **Scope: medium.** Fixture rows are supplied by upstream GIVEN steps; never manufacture them from expected results.

## Delivery and conformance gates

1. Implement the policy/value/identity contracts incrementally with #1, #2 and #5. Each must reach SQL IR and execution; unused IR fields are not completion.
2. Batch coherent work: labels + writes; temporal constructors + truncation + arithmetic; scalar/collection + query-part + path semantics; procedure support. Use focused upstream scenarios and boundary tests during each batch, then full local suites at integration points.
3. Add paired Cypher/Gremlin checks where semantics differ: labels, missing/null properties, comparison, collections, temporal round-trips, aggregate nulls, path history and mutation effects. Check DuckDB versus DataFusion execution of the same IR contract where both are eligible.
4. Acceptance is **3,897/3,897 unchanged pinned openCypher scenarios passing**, with no skipped/unclassified assertions, plus preservation of the full **1,511/1,511 Gremlin** result. A new clean-build run must establish both; the historical Cypher file is only triage evidence.
5. Run locally through one normal Crabgraph configuration and execution pipeline. Include upstream setup, results, errors and effects. GitHub Actions only publishes the existing production comparison after results are committed; no tests run there and no alternate Crabgraph leaderboard entry is added.

These ten work packages address the observed families. They do not promise every currently hidden assertion will pass immediately after its first blocker is removed. Re-group the remaining failures after each integrated batch, keeping the original denominator and assertions.
