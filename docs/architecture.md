# Architecture

## Compiler and caller-owned execution

`compiler::compile` accepts query text, schema metadata, graph mappings, typed
parameters and function declarations. Language frontends parse and validate
Cypher, Gremlin and SPARQL, lower to Graph IR, and then to a DataFusion logical
plan. The SQL unparser emits DuckDB or Postgres SQL. The compiler does not
connect to a database, discover schemas, register UDFs, move rows or execute SQL.
DataFusion remains a planning dependency; this is not yet a minimal parser-only crate.

Applications can pass `CompiledSql.sql` directly to a driver. The optional
`execution::SqlSession` interface provides a checked hand-off with caller-defined
results, including cursors borrowing the session. It imposes neither Arrow nor
row buffering. See [compiler API](compiler.md) for protocol and limitations.

Mappings describe physical tables, typed identities, endpoints and
properties. Metadata must match the connection that executes the SQL. The caller
owns schema discovery, transactions, connection leasing, extensions, UDF setup,
result decoding, cancellation and caches. Compile-time function declarations
only describe signatures and SQL targets; they do not install implementations.

## Graph IR

Graph IR (graph intermediate representation) is the shared logical query plan
between the language frontends and relational lowering. It represents a query
as a tree of graph operators, including node scans, relationship expansion,
filters, projections, aggregation and path traversal. Operators produce named
bindings; value expressions refer to those bindings. Plan policies retain
language-specific rules for matching, missing values and paths.

Relational lowering uses the graph catalog and mappings to translate these
operations into table scans, joins and other relational operators. For example,
a single-hop relationship expansion becomes joins on mapped endpoint identities.
This common layer lets Cypher, Gremlin and SPARQL share relational lowering and
SQL generation while retaining their language semantics. Graph IR describes
the query rather than stored data, and is consumed before execution. See
[core concepts](../website/docs/content/concepts.md#graph-ir) for a query example.

## Language and IR ownership

Parsing normalizes syntax into a typed AST. Semantic analysis validates names,
scopes, binding kinds and language rules. Frontend planners lower validated ASTs
into Graph IR using scope/context helpers; reusable value behavior belongs in IR
helpers, not parser rewrites. Preserve null versus absence, graph identity,
ordering, bag multiplicity, exact numeric types and observable evaluation counts.

`src/language/` owns frontends; `src/ir/plan.rs` and `src/ir/expr.rs` define Graph IR;
`src/ir/rel/` owns relational lowering, optimization and SQL generation.
[Module ownership](code_ownership.md) and the
[relational ownership map](../src/ir/rel/README.md) identify implementation seams.

## Runtime boundaries

Managed and mapped graphs share one query runtime. `GraphEngine::mapped` uses
caller-owned DuckDB tables; the `MappedGraphEngine` compatibility API delegates
to the same executor. `RdfGraphEngine` also delegates to `GraphEngine`; RDF query
and update sessions borrow the shared connection and transaction owner. A
`GraphMapping` contains node, edge, and RDF rules together. Element identities
retain their scalar or ordered composite source types and
writes target mapped columns. Mapped scans lower directly into SQL islands;
residual kernels access referenced sources with typed frontier lookups or a
bounded source scan selected from statistics and cached for the statement.
Schema binding never loads the mapped graph. See [unified graph engine](unified-graph-engine.md)
for storage constraints, migration, and conformance evidence.

The [query runtime](runtime.md) executes a relational DAG, placing
eligible regions in DuckDB and residual native operators in DataFusion. Explicit
JVM operators use the [native JVM provider](jvm.md). These capabilities support
broader language behavior than a standalone SQL statement can express. Graph IR
remains a compiler representation; the standalone recursive interpreter and
legacy island-rewrite execution path have been removed. Runtime scalar helpers
and DataFusion operator kernels live in `ir::runtime`. SPARQL `SERVICE`, including
`SERVICE SILENT`, is unsupported in both compiler and managed execution.
`engine`, `mapped_engine`, `rdf_engine` and the CLI require `features = ["duckdb"]`.
That explicit feature still bundles DuckDB for compatibility. Neither it nor the
optional PostgreSQL driver is enabled by an ordinary library dependency.

The historical low-level `ir::rel::sql::SqlExecutor` supports fixture setup and
materialization. Use `execution::SqlSession` for client-owned databases; its
contract has no setup statements or implicit transaction changes.

## Multiple engines

SQL dialect selection is independent of connection ownership. Compile requests
can assign tables to named DuckDB or Postgres engines and select an execution
engine. The planner assigns closed SQL islands to their source engines and emits
typed transfers. Cross-engine joins run on the selected execution engine.

`federation::execute` and client coordinators query caller-owned sessions and bind
transferred rows into the final statement. Transfers are buffered; this path
provides neither streaming exchanges nor distributed snapshots or transactions.
ClickHouse rendering is not implemented. See [SQL engines](sql-engines.md) for
the protocol, adapters, and execution constraints.

## Permission filtering

The compile request's `authorization` and node `permission_scopes` filter node
sources against caller-owned effective grants before traversal and projection.
Scopes combine with OR; membership filters preserve row multiplicity even when
grants repeat. Protected mappings require a principal.

Applications choose the permission system and supply resolved grants, including
group membership. The compiler does not connect to or synchronize that system.
See the [permission protocol](../website/docs/content/sql-compiler.md) and Java's
`PermissionRelation`, `protectWith`, and `Authorization` helpers.

## Functions

Function registration lives in `src/ir/functions/`. Language names resolve through
operator tables and overload signatures before lowering. SQL target names and
argument/result types are explicit in compiler requests. Runtime callbacks and
unknown/volatile functions must not be silently treated as movable SQL expressions.
Add regressions for nulls, overload resolution and evaluation counts when changing
function lowering. Runtime-specific function APIs remain documented in rustdoc.

## Derived sources and plan visibility

`GraphMapping::register_logical_source` declares equivalent physical table layouts;
selection uses supplied partition statistics and pushed predicates, with optional
generated source costs when manifest costs are unavailable.
`register_collection_source` exposes one native list as a read-only relation with
explicit parent columns and scalar element/struct-field projections. Collection
parent scans participate in layout selection; element predicates do not become
parent partition predicates. These sources are shared by graph and RDF mappings.

`CompiledSql` includes `logical_plan`, source selections, `constraint_proofs`,
`statistics_usage`, `plan_estimates` and `optimizer_decisions`. `QueryResult.stats`
also exposes the physical DAG, SQL island queries, and native source queries/rows.
Generated estimates cover operator cardinality, collection expansion and work;
they remain separate from measured storage I/O. See the
[mapping reference](../website/docs/content/mapping-reference.md) and
[statistics overview](../website/docs/content/statistics.md).

The `ir::rel::statistics` module owns one-time bounded acquisition requests,
summaries, immutable snapshots, catalog handles and shared estimation. Clients
execute requests and forward batches; they do not implement estimators. Compile
uses retained metadata without source reads. Statistics feed equivalent source
selection and total-filter ordering, and are exposed as inexact DataFusion hints.
They never establish constraints or authorize approximate query answers.

Generated statistics also drive connected inner-join ordering and build-side
costing, whole-plan representation selection, and parent semijoins before
collection expansion. Native mapped access chooses bounded scan caching or typed
batched lookup from actual frontier cardinality. Decisions are exposed through
`optimizer_decisions`; estimates and measured execution costs remain distinct.

The [portable scalar catalog](portable-functions.md) exposes native functions with explicit DuckDB and PostgreSQL mappings through `fn.*`.
See [Maintaining portable scalar functions](portable-functions-contributing.md)
for capability APIs, mapping templates, catalog generation, and regression tests.
