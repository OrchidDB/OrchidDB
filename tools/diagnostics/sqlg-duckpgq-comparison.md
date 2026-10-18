# OrchidDB vs SQLg vs DuckPGQ: twelve actionable issues

Reviewed 2026-10-17. **Stopped at twelve findings.** This is a compatibility and implementation audit, not a performance benchmark or a claim that the products have identical goals.

## Scope and evidence

OrchidDB source baseline: `22262a0c0e447f539ab83949524390899cdf0b09`, **with existing uncommitted changes included in inspection**. Runtime probes used the existing `extension/build/orchid.duckdb_extension` with DuckDB 1.5.6. That binary predates the current working-tree edits; runtime observations below are corroborated by current source, but are not a fresh-build certification. No production code was changed.

Eleven findings have executable probes; finding 12 is source-confirmed only. Three type probes count as **one** issue. Full SQL, setup, and observed output are in [results.json](sqlg-duckpgq-audit/results.json); rerun [probe.py](sqlg-duckpgq-audit/probe.py) from the repository root:

```sh
extension/vendor/test-env/bin/python tools/diagnostics/sqlg-duckpgq-audit/probe.py
```

Competitor behavior comes from primary documentation, not locally executed competitor tests. SQLg is compared against its [3.1.3 documentation](https://sqlg.org/docs/3.1.3/). DuckPGQ documentation is current as retrieved; [DuckDB's guide](https://duckdb.org/docs/current/guides/sql_features/graph_queries) currently directs DuckPGQ users to DuckDB 1.4.4 and warns about 1.5.x compatibility. Do not install the two extensions into this audit's 1.5.6 environment and assume a valid benchmark.

Priority: **P1** correctness or ordinary write compatibility; **P2** interoperability, modeling, or performance work. Feature gaps are explicitly identified; they are not presented as regressions or broken advertised guarantees. “Unverified” means this audit makes no competitor capability claim.

## Side-by-side comparison

| # | Area | OrchidDB observed / inspected | SQLg | DuckPGQ |
|---|---|---|---|---|
| 1 | Null vertex identity | Required MATCH can yield a null node | Relational vertex identity [S] | Exact invalid-key behavior unverified |
| 2 | Conditional failure | `fail()` rejected during planning | TinkerPop execution; runtime step contract [S, T] | Different language |
| 3 | Shortest-path query | Cypher `shortestPath` rejected | pgRouting Dijkstra integration [S] | `ANY SHORTEST` [Q] |
| 4 | SQL graph-query syntax | No `GRAPH_TABLE` rewrite | Gremlin API [S] | SQL/PGQ `GRAPH_TABLE` [Q] |
| 5 | Source types | UUID, TIMESTAMPTZ, MAP rejected by extension | Backend-specific types [S]; exact parity unverified | Exact type parity unverified |
| 6 | Existing key metadata | Explicit KEY required | Own topology model [S] | Constraint-based inference [D] |
| 7 | One label across sources | Duplicate mapped label rejected | Namespaced topology [S]; same semantics unverified | Unverified |
| 8 | Property exclusions | ALL COLUMNS EXCEPT rejected | Different topology API | Supported [D] |
| 9 | Discriminator labels | IN discriminator syntax rejected | Different topology API | Supported [D] |
| 10 | Generated primary keys | Mapped CREATE requires supplied key | IDs assigned on flush/commit [S] | Graph reads; use SQL writes |
| 11 | Nonliteral defaults | Function default requires explicit value | Topology defaults [S]; expression parity unverified | Graph reads; use SQL writes |
| 12 | Write dependency reads | One old-row query per affected row | Batch mutation modes [S] | Not a graph-mutation comparison |

Sources: [S: SQLg](https://sqlg.org/docs/3.1.3/), [T: TinkerPop 3.7.3 `fail` contract](https://tinkerpop.apache.org/javadocs/3.7.3/full/org/apache/tinkerpop/gremlin/process/traversal/dsl/graph/GraphTraversal.html), [Q: DuckPGQ queries](https://duckpgq.org/documentation/sql_pgq/), [D: DuckPGQ graph definitions](https://duckpgq.org/documentation/property_graph/).

## Findings and fixes

### 1. P1 — Reject invalid null vertex keys before returning graph results

**Reproduced correctness problem:** map `p(id BIGINT, name VARCHAR)` after inserting `(NULL,'a')`. A required `MATCH(n:P)` followed by `RETURN count(n), count(*), n IS NULL` returns `(0,1,true)`. A matched vertex should not simultaneously be represented as a missing node.

**Source:** [mapped node scan](../../src/ir/rel/mapping.rs#L1216) projects source keys without a null check. [Graph scalar lowering](../../src/ir/rel/graph_scalars.rs#L129) uses null identity to represent missing elements. The README explicitly disclaims uniqueness certification, but silent null-element results still need a defined policy.

**Fix:** reject invalid identities through a validation path, or consistently exclude invalid rows under an explicitly documented policy. Validate every component of composite keys. Do not merely fix the displayed count.

**Acceptance:** single-column and composite null keys cannot produce a null node from a required MATCH; SQL and native execution agree. Valid optional matches still produce nulls normally.

### 2. P1 — Implement `fail()` as a runtime effect

**Reproduced defect:** `g.V().has('id',999).fail('should not run')` fails at bind time on a graph containing IDs 1–3. No traverser reaches the step. The [TinkerPop contract](https://tinkerpop.apache.org/javadocs/3.7.3/full/org/apache/tinkerpop/gremlin/process/traversal/dsl/graph/GraphTraversal.html) triggers failure when execution reaches it.

**Source:** [lower_fail](../../src/language/gremlin/planner/lowering/procedures.rs#L313) unconditionally returns Unsupported.

**Fix:** lower an explicit runtime failure operator that preserves message and error classification. Keep it inside branch and effect execution boundaries.

**Acceptance:** empty input and untaken branches succeed; reached branches fail; preceding writes obey statement rollback semantics.

### 3. P2 — Expose shortest-path execution through Cypher

**Reproduced compatibility gap:** `MATCH p=shortestPath((a:P {id:1})-[:R*]->(b:P {id:3})) RETURN length(p)` is rejected by the parser on a three-node chain. Expected result is 2. DuckPGQ supplies a comparable shortest-path query operation [Q].

**Source:** [pattern lowering](../../src/language/cypher/parser/lowering/patterns.rs#L47) handles anonymous pattern elements, without this selector syntax. Orchid already has [path search kernels](../../src/ir/runtime/ops/path_pattern.rs) and [shortest-path execution](../../src/ir/runtime/scalar/graph.rs#L270).

**Fix:** connect selector-aware frontend syntax to shared path IR; define single-path versus all-ties behavior and endpoint grouping. Avoid enumeration of all trails followed by sorting.

**Acceptance:** chain, diamond ties, cycles, disconnected endpoints, and bounded lengths. This is a frontend gap, not a claim that Orchid lacks all shortest-path machinery or JVM algorithms.

### 4. P2 — Add SQL/PGQ `GRAPH_TABLE` interoperability

**Reproduced feature gap:** `SELECT * FROM GRAPH_TABLE(g MATCH (n:P) COLUMNS(n.name))` fails at `MATCH`. Having `CREATE PROPERTY GRAPH` does not make the query interface SQL/PGQ compatible.

**Source:** [statement rewriting](../../extension/compiler/src/syntax.rs#L628) handles Orchid language prefixes and graph DDL, not `GRAPH_TABLE`.

**Fix:** add a SQL/PGQ frontend lowering into shared Graph IR; retain host SQL composition and caller-owned sessions. Scope and document the supported subset.

**Acceptance:** graph projections join with ordinary tables, outer SQL predicates work, and graph/SQL parameters remain separately bound. Existing `orchid_cypher(...)` composition already works and is the immediate workaround.

### 5. P2 — Complete the extension's native type bridge

**Reproduced compatibility gap:** graph creation over properties of type UUID, TIMESTAMPTZ, or MAP fails with `Orchid does not yet support source type ...`. These are three manifestations of one bridge limitation.

**Source:** [CompilerType](../../extension/src/orchid_extension.cpp#L127) has no cases for these types. Explicitly excluding unsupported columns is already supported; the issue concerns actually using them.

**Fix:** define lossless shared representations and add binding, parameter, Arrow, result, and key handling where appropriate. A string cast is a workaround, not native parity; timestamp conversion must preserve the instant.

**Acceptance:** values round-trip with nulls; mapped UUID keys join correctly; map properties preserve key/value types; timezone behavior is explicit. Competitor support for every listed type was not established.

### 6. P2 — Infer unambiguous declared keys and endpoints

**Reproduced onboarding gap:** `CREATE TABLE p(id BIGINT PRIMARY KEY)` followed by `CREATE PROPERTY GRAPH g VERTEX TABLES(p)` fails despite available catalog metadata. This is a documented Orchid restriction, not a regression. DuckPGQ supports inference from declared constraints [D].

**Source:** [element parser](../../extension/compiler/src/syntax.rs#L198), especially the explicit-key check at line 293 and required endpoint clauses at line 270.

**Fix:** defer missing-key resolution until catalog binding; infer only declared, unambiguous constraints. Preserve explicit overrides. Do not guess relationships from similarly named columns or invent unstable identities.

**Acceptance:** single/composite PK inference succeeds; ambiguous FK choices produce precise diagnostics; sources without usable identity still require configuration.

### 7. P2 — Support one logical label over multiple mapped sources

**Reproduced modeling gap:** two tables mapped as `LABEL P` fail with `duplicate label P is not supported yet`. Regional or partitioned datasets cannot be combined through this DDL without a manually maintained union view.

**Source:** [mapping validation](../../extension/compiler/src/syntax.rs#L530) enforces one source per label.

**Fix:** represent label membership separately from source identity; merge compatible property schemas and preserve endpoint provenance. Define whether equal keys across sources identify different entities or require explicit unification.

**Acceptance:** two sources with overlapping key values return correct identities, counts, properties, and edges. Do not simply remove the duplicate-label check. Neither competitor is claimed to support precisely this mapping contract.

### 8. P2 — Implement `PROPERTIES ALL COLUMNS EXCEPT (...)`

**Reproduced compatibility gap:** excluding `name` with this clause fails during DDL parsing. DuckPGQ documents it [D]. Wide or evolving tables currently require explicit allowlists or views.

**Source:** [property-clause parser](../../extension/compiler/src/syntax.rs#L243) accepts ALL COLUMNS but has no exclusion representation.

**Fix:** persist exclusions in graph metadata and apply them after catalog column resolution; keep behavior explicit when source schemas evolve.

**Acceptance:** excluded properties remain unavailable after reopening/rebinding, identifier casing follows host rules, and adding source columns respects the chosen policy.

### 9. P2 — Support discriminator-based labels on mapped tables

**Reproduced modeling gap:** `p KEY(id) LABEL P IN typemask(A,B)` fails during parsing. DuckPGQ documents discriminator labels [D]. Orchid's managed dynamic labels do not provide this mapped-table capability.

**Source:** [Element](../../extension/compiler/src/syntax.rs#L500) stores one label string; [mapped node scan](../../src/ir/rel/mapping.rs#L1255) emits static membership.

**Fix:** add explicit discriminator metadata and predicate lowering while preserving the original row identity across base and subtype labels.

**Acceptance:** base/subtype label filters and `labels(n)` agree, subtype changes follow source updates, and one entity does not become duplicated merely because it has two labels.

### 10. P1 — Honor database-generated primary keys during mapped creation

**Reproduced write gap:** a table with `id BIGINT PRIMARY KEY DEFAULT nextval('seq')` works with ordinary SQL insertion, but `CREATE(n:P {name:'a'})` fails because the key property is absent. SQLg's mutation path supports assigning generated IDs at flush/commit [S].

**Source:** [mapped key construction](../../src/ir/catalog/keyed.rs#L104) requires components before insertion; [default discovery](../../src/ir/rel/host/mapped_storage.rs#L246) skips keys.

**Fix:** use engine-owned insertion/RETURNING or a capability-specific equivalent, reconcile returned identities into the graph overlay, then resolve dependent edges. Never evaluate sequence expressions during binding.

**Acceptance:** creating two connected nodes with generated keys works atomically and returns real database IDs; explicit supplied keys continue to work.

### 11. P1 — Let the database evaluate omitted non-key defaults

**Reproduced write gap:** `name VARCHAR DEFAULT upper('alice')` causes `CREATE(n:P {id:1})` to fail with `its source default is not a scalar constant`. Even this deterministic SQL expression is blocked; time/sequence defaults need stricter evaluation semantics too.

**Source:** [default classification](../../src/ir/rel/host/mapped_storage.rs#L246) and [mapped_insert_properties](../../src/ir/catalog/keyed.rs#L264) require explicit values for nonliteral defaults.

**Fix:** distinguish omitted properties from explicit nulls and let the host evaluate defaults during insertion, then hydrate returned properties. This differs from issue 10: no new identity allocation is needed, but omitted-column and value synchronization semantics are.

**Acceptance:** expression defaults work, explicit null stays null, volatile defaults execute at the correct frequency, and returning properties matches persisted values.

### 12. P2 — Batch old-row reads used for write dependency ordering

**Source-confirmed performance issue, not benchmarked:** [write_order](../../src/ir/rel/host/mapped_storage.rs#L409) loops over pending rows. For every existing row whose referenced table has pending deletions, `needs_old` issues a separate host query at lines 435–450. Deleting a parent with N affected children can therefore add N old-row reads before the already-batched writes. This is a concrete round-trip scaling issue; no latency ratio is asserted.

SQLg's batch modes provide the comparison target [S]. Orchid already batches final DML by statement shape; that implementation should be preserved.

**Fix:** group dependency reads by physical table, key shape, and FK projection; join a batch of pending keys once and build the dependency graph from that result.

**Acceptance:** instrument `HostRelational` on parent deletion with 1, 100, and 10,000 children. Old-row SELECT count should scale with table/projection groups or bounded chunks, not individual rows; dependency order and rollback behavior must remain correct.

## Work order and boundaries

Fix 1–2 first, then mapped write semantics (10–11). Address 12 with query-count instrumentation. Schedule type and mapping interoperability (5–9), then the frontend additions (3–4) according to product priorities.

Do not turn this into twelve claims that Orchid is universally behind. It already has SQL composition, mapped and managed graphs, composite keys, transactional definitions, writes, and shared execution across engines. DuckPGQ documents only ANY SHORTEST and currently lacks OPTIONAL MATCH [Q]; SQLg documents no GraphComputer support [S]. Orchid's JVM algorithm support is therefore not counted as missing.

No fixes were implemented, no competitors were benchmarked, and no additional findings were pursued after this twelve-item set was established.
