# DuckDB extension completion plan

Status: implementation complete. The integration gate passes 53 extension tests (including real Iceberg/Lance), 5 compiler tests, 305 core tests, and 9 mapped storage/key tests. Pinned conformance passes 3,897 Cypher and 1,511 Gremlin cases. Existing SPARQL scope passes 974 cases, with 77 skipped and 74 not applicable. Final isolated-source acceptance is complete; committed reports identify the same immutable artifact. README, documentation, and local extension packaging are updated; public CLI and client release wiring are removed. This is the single implementation checklist for the extension.

## Objective and constraints

Complete Cypher, Gremlin and SPARQL conformance through the DuckDB extension, using the existing language implementations. DuckDB owns the connection, transaction, source scans, relational operators and execution scheduling. Preserve native query syntax, property-graph declarations, and real Iceberg and Lance integration on DuckDB 1.5.6; verify the supported stable DuckDB release again before the final build.

- Work only in the existing `codex/duckdb-cypher-extension` branch and checkout. **No worktrees.**
- Preserve the user's unrelated release, dependent-execution and benchmark changes.
- Reuse parsers, planners, scalar functions, graph kernels, storage codecs, mapped source access and persistence. Extract interfaces where the existing implementation is coupled to an executor.
- Do not introduce another GraphIR interpreter, replacement language implementation, whole-query GraphEngine/DataFusion fallback, or independent database connection.
- Existing JVM algorithm/application-callback kernels may remain compiled operators under the DuckDB host. They must not become whole-query delegation to the Java assertion runner.
- No effects during bind, EXPLAIN or PREPARE. Writes execute once within the caller's transaction.
- Local builds and tests only; do not dispatch GitHub Actions.

## Complete scope

The pinned catalogs contain 3,897 Cypher, 1,511 Gremlin and 1,125 RDF cases. Final evidence must describe one immutable extension artifact, the exact catalogs and unchanged upstream assertions. Historical passing results cannot be combined. Skips, unsupported placeholders, adapter errors and unexecuted cases are not passes.

SPARQL scope is the existing embedded syntax/query/update functionality, per the user’s explicit scope correction. Port and fix that behavior on DuckDB; do not add federation, entailment providers, result serialization APIs, or HTTP protocol functionality to satisfy additional catalog profiles. Keep the existing applicability boundary explicit when reporting conformance; out-of-scope profiles are not passes.

Existing full reports under `target/conformance/extension/paths-full-{cypher,gremlin,rdf}.json` are the frozen planning input. Subsequent focused path and integration checks validate later edits; they are not replacement full-suite results. Do not regenerate broad discovery reports while architectural prerequisites are still missing.

## Architecture to implement

```text
Existing parsers and language planners
                 |
       Existing GraphIR compiler
                 |
    Compiled relational/kernel graph
        /                       \
 DuckDB relational plans    Shared kernel descriptors
        \                       /
       DuckDB execution and subplan runner
                 |
       One statement execution context
                 |
    Shared mapped/managed source and persistence
                 |
      Caller-owned DuckDB transaction
```

Keep SQL islands and nonmaterialized source CTEs for pushdown. Residual kernels fetch only required properties/adjacency through the existing mapped source implementation. Managed graph data lives in DuckDB relations; do not make a whole-graph serialized blob the query engine.

### Shared contracts

1. **Compiled kernel descriptor:** immutable callable operation, compiled inputs, required bindings, streaming/global/stateful contract and slice/order metadata. Reuse `runtime::Compiler`; separate its descriptors from the DataFusion physical wrapper.
2. **Statement state:** graph overlay, `ExecutionContext`, frontier, correlation/batch key, bulk, path/select/loop/sack state, clocks, RNG, cancellation and resource ownership. Fresh per execution, shared by its nested operators.
3. **Compiled subplan runner:** executes an already compiled body against a frontier using the same state and transaction. Existing apply/choose/coalesce/repeat/merge/group code calls this interface. The legacy adapter may use DataFusion; the extension adapter uses DuckDB.
4. **Host relational access:** typed scalar parameters and batch-backed input relations, Arrow result batches, SQL execution and metadata discovery. The existing mapped source and persistence code depend on this interface rather than a concrete `DuckDbExecutor`.
5. **Typed values:** reuse `Value`, scalar/identity codecs and row transport. Preserve graph identity, property records, numeric widths, null versus absence, bulk and ordering. Plain SQL primitives remain plain SQL results where possible.

Callbacks borrowing `ClientContext` must obey its thread affinity. Compiled handles may outlive one execution; borrowed host pointers and query-local state may not. DuckDB tasks must finish before resources are destroyed.

## Work packages

### P1 — Extract the existing kernel and subplan execution boundary

Owner: host execution agent. Dependencies: none.

- [x] Extract shared kernel descriptors/state from `src/ir/rel/runtime.rs` without changing GraphIR compilation semantics.
- [x] Move DataFusion prepare/collect/cache ownership out of `runtime/control.rs::Subplan::run_internal` into the legacy runner.
- [x] Make the existing control and group kernels call the shared subplan runner; retain group prefix/barrier/finalizer and correlated frontier semantics.
- [x] Expose the existing source cursor/invocation contracts to the DuckDB adapter without copying their execution semantics.
- [x] Preserve the legacy backend through focused existing kernel/control tests.

Acceptance: nested compiled bodies can be executed by an injected host runner; the extension path cannot reach DataFusion physical planning/collect. Empty frontier and one empty row remain distinct. No new GraphIR operator dispatcher.

### P2 — Extract typed host storage access and mapped persistence

Owner: storage agent. Dependencies: none; coordinate interfaces with P1/P3.

- [x] Introduce the typed host relational contract, with SQL, scalar parameters, batch relations, results and execution effects.
- [x] Extract `src/engine/mapped_source.rs` behind it, retaining batched key/property/adjacency access, composite-key normalization, caching and access accounting.
- [x] Extract `src/engine/mapped_storage.rs`, including metadata binding and `persist`, behind it. Reuse row coalescing, defaults, FK nullability, dependency ordering and typed write batches.
- [x] Keep legacy engine adapters as facades; do not instantiate them in the extension.
- [x] Define cache invalidation/read-own-writes fences and explicit source write capabilities.

Acceptance: selective residual reads, composite keys, shared physical rows, FK updates/deletes and defaults work through an injected host. No duplicate persistence implementation using RDF-specific mutation rules.

### P3 — Bind and schedule compiled programs in DuckDB

Owner: primary agent. Dependencies: P1 and P2 contracts.

- [x] Add typed Arrow/batch transport to the C++/Rust boundary; the current SPARQL VARCHAR parameter callback is insufficient for general graph writes.
- [x] Bind compiled kernel descriptors and relational inputs into DuckDB operators with streaming/global/stateful scheduling contracts.
- [x] Implement the DuckDB compiled-subplan runner with frontier/batch relations and shared statement state.
- [x] Generalize the existing ordered update-program lifecycle for mutation/control programs; keep side effects out of scalar UDFs.
- [x] Resolve schemas and declare affected databases during bind; acquire fresh execution state on each EXECUTE.
- [x] Integrate cancellation, resource cleanup, profiler ownership and typed diagnostics. Preserve the existing task-draining guard.

Acceptance: same-transaction reads and writes, atomic statement failure, explicit ROLLBACK, prepared reexecution, effect-free EXPLAIN/PREPARE, cancellation/recovery, and SQL composition. Execution evidence identifies DuckDB operators and shared kernels, never an alternate whole-query executor.

### P4 — Consolidate typed values, projections and relational boundaries

Owner: value-boundary agent. Dependencies: none for scalar work; P1/P3 for stateful boundaries.

- [x] Finish one value/row contract across expressions, predicates, branch unions, aggregates, sort/distinct and graph outputs.
- [x] Reuse `current_project_op` for productive null versus absent element properties; preserve other bindings during current projection.
- [x] Preserve repeated select history, path labels, scalar/element mixtures and property records. Native boolean results must be unwrapped at every SQL predicate consumer.
- [x] Use existing comparison/aggregation/group/path kernels for mixed types and equality. Remove obsolete string-display representations from executable value boundaries.
- [x] Keep detached property context bounded to referenced elements; preserve composite and typed public identities.
- [x] Preserve ordinary SQL results and source pushdown while using the value carrier only where required.

Acceptance: graph/list/map/property values round-trip; map missing values remain productive where specified; heterogeneous branches do not coerce values; comparison/orderability/null/NaN cases use existing semantics. No duplicate scalar or reducer implementation.

### P5 — Managed graph representation and complete fixtures

Owner: storage agent after P2; primary agent owns extension API wiring. Dependencies: P2/P3/P4.

- [x] Add explicit extension-owned managed graph relations for dynamic labels/properties, reusing storage/incremental/value codecs and catalog identity semantics.
- [x] Support unlabeled and multilabeled vertices, stable identity across label changes, heterogeneous properties and persistence/reopen.
- [x] Represent vertex-property IDs, single/list/set cardinality, meta-properties and null presence using existing property-record code.
- [x] Support one public relationship label across multiple endpoint-label pairs without artificial public labels or identity collisions.
- [x] Expose managed scans/joins as DuckDB relations; retain declared-schema restrictions for externally mapped graphs.
- [x] Replace fixture adapter rejection/workarounds with production graph APIs. Reused fixtures must reset correctly after mutations.

Acceptance: all pinned fixtures load losslessly, including CREW; source state survives reopen and rollback. Iceberg/Lance schemas and write capabilities are respected.

### P6 — Enable existing Cypher mutation and graph semantics

Owner: storage agent with host/value owners. Dependencies: P1–P5.

- [x] Connect `create_op`, `set_property_op`, `delete_op`, catalog mutation methods and existing MERGE control semantics to ordered DuckDB programs.
- [x] Preserve correlated MERGE, ON CREATE/ON MATCH, sequential effects, read-own-writes, returned bindings, labels and path deletion.
- [x] Extract shared observations from `src/engine/snapshot.rs`; expose actual DuckDB-backed state to the unchanged upstream side-effect assertions.
- [x] Fix nullable OPTIONAL MATCH path binding so `nodes(null)` and `relationships(null)` remain null.
- [x] Integrate managed graph reads across MATCH/WHERE, WITH/RETURN, OPTIONAL MATCH, grouping/order/slice/UNION, procedures, paths, pattern comprehensions and existential/counting subqueries.
- [x] Propagate authoritative runtime error type/detail/phase, including mutation errors.

Acceptance: upstream GIVEN statements execute through the production extension, exact five-part state snapshots reflect effects, and failed statements leave no changes. Cover read families currently hidden behind mutation-dependent setup; enabling CREATE alone is not completion.

### P7 — Enable existing Gremlin control, state and mutation kernels

Owner: host execution agent, then value-boundary agent for integration. Dependencies: P1–P5.

- [x] Reuse apply/choose/coalesce/repeat kernels for nested traversal bodies, heterogeneous branches, loops, emit/until and branch-local barriers.
- [x] Reuse `ExecutionContext`, group accumulators, barriers, sample/slice/distinct and sack/side-effect kernels with one statement state and RNG.
- [x] Preserve exact bulk, lazy/eager timing, empty aggregate identities, group finalizers and multiple/nested caps. Unchosen branches must not execute effects or emit empty reducer rows.
- [x] Connect add/merge/drop and property/cardinality mutations to shared graph persistence.
- [x] Supply graph-aware degree/search/property services through the extracted source; do not substitute detached scalar-only catalogs for storage access.
- [x] Connect existing GraphJvm algorithm and application-callback kernels as compiled host operators with source callbacks, cancellation and scope cleanup.

Acceptance: Branch/Choose/Coalesce/Optional/Local/Union/Repeat/Loops, Aggregate/Store/Group/Cap/Sack, strategies, graph algorithms and mutations use their original kernels. No arbitrary unrolling limit becomes the general repeat implementation.

### P8 — Complete original assertion providers and state transport

Owner: value-boundary agent after P5–P7. Dependencies: P5/P6/P7.

- [x] Reuse `NativeJUnitAssertions.java`, `placeholders.json` and the original pinned JUnit bodies with an extension-backed provider/remote source.
- [x] Implement property-reference and state transport without changing assertions or executing asserted traversals in TinkerGraph.
- [x] Remove the 15-placeholder interception only after all originals can execute against the loaded extension.
- [x] Keep all asserted queries attributable to the same extension process/artifact and expose real state snapshots.

Acceptance: all original provider cases run exactly once; no ignored assumptions, manufactured results, unsupported placeholders or adapter errors.

### P9 — Preserve existing SPARQL behavior on DuckDB

Owner: primary agent. Dependencies: P3; existing query/update implementation remains the reference.

- [x] Preserve current embedded syntax/query/update behavior, typed RDF identity, datasets and transaction semantics.
- [x] Reuse the existing SPARQL planner, relational lowering, scalar functions and update implementation.
- [x] Validate the same applicable embedded cases against the extension with unchanged expected results and assertions.
- [x] Keep broader federation, reasoning, serialization and HTTP profiles outside this implementation scope.

Acceptance: all cases covering the preexisting embedded SPARQL functionality pass on the extension. Report the precise applicable scope; do not claim full RDF catalog coverage or count out-of-scope profiles as passes.

### P10 — Product integration and final verification

Owner: primary agent. Dependencies: all packages.

- [x] Preserve one-statement graph declarations, host schema discovery, native Cypher/Gremlin syntax, SQL composition and parameter binding.
- [x] Exercise real Iceberg snapshots and Lance datasets/indexed scans, including cross-source joins, selective filters and property projection.
- [x] Verify extension load/persistence/rebind, actual writable sources, transaction visibility and stable DuckDB compatibility.
- [x] Audit the extension call graph for prohibited alternate executors and whole-query fallbacks.
- [x] Run all focused integration checks on the final artifact, then each complete pinned suite once as the release acceptance gate.
- [x] Record artifact/catalog/assertion identities and ensure every catalog case has an actual outcome. Claim 100% only when the requested scope is fully passing.

## Execution batches and coordination

| Batch | Work | Integration gate |
| --- | --- | --- |
| A — shared interfaces | P1, P2 and P4 in parallel; primary defines P3 host transport | Legacy kernel/storage tests and focused host/typed-value contracts |
| B — DuckDB execution/storage | P3 and P5; P6/P7 attach existing kernels as interfaces land | Transactional mutation, nested subplan, source access and full fixture contracts |
| C — complete language/provider integration | Finish P6/P7/P8/P9; remove old temporary boundaries | Package acceptance cases and all extension integration tests |
| D — final conformance | P10 | One immutable build; full catalogs; no historical result union |

The primary agent owns this document, integration and C++ host code. Subagents receive explicit file ownership before editing. Shared interface changes are announced before consumers change. The user subsequently requested no more subagents; remaining implementation and verification are performed directly by the primary agent. No branch switching or worktrees. A completed dependency unlocks its next package immediately; this document is not an approval pause.

## Validation discipline

Use existing reports to understand scope. During implementation, run focused tests for the boundary being changed and necessary compilation checks. Do not run entire catalogs after every fix or use their pass count to choose the next isolated patch. Full-suite runs occur at completed integration milestones and the final acceptance gate. If a gate exposes a defect, add a focused regression, fix the shared component and rerun the affected checks before repeating that gate.
