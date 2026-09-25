# Architecture

## Compiler and caller-owned execution

`compiler::compile` accepts query text, schema metadata, graph mappings, typed
parameters and function declarations. Language frontends parse and validate
Cypher, Gremlin and SPARQL, lower to Graph IR, and then to a DataFusion logical
plan. The SQL unparser emits DuckDB or PostgreSQL SQL. The compiler does not
connect to a database, discover schemas, register UDFs, move rows or execute SQL.
DataFusion remains a planning dependency; this is not yet a minimal parser-only crate.

Applications can pass `CompiledSql.sql` directly to a driver. The optional
`execution::SqlSession` interface provides a checked hand-off with caller-defined
results, including cursors borrowing the session. It imposes neither Arrow nor
row buffering. See [compiler API](compiler.md) for protocol and limitations.

Mappings describe physical tables, signed integer identities, endpoints and
properties. Metadata must match the connection that executes the SQL. The caller
owns schema discovery, transactions, connection leasing, extensions, UDF setup,
result decoding, cancellation and caches. Compile-time function declarations
only describe signatures and SQL targets; they do not install implementations.

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

The optional [managed runtime](runtime.md) executes a relational DAG, placing
eligible regions in DuckDB and residual native operators in DataFusion. Explicit
JVM operators use the [native JVM provider](jvm.md). These capabilities support
broader language behavior than a standalone SQL statement can express.
`engine`, `mapped_engine`, `rdf_engine` and the CLI require `features = ["duckdb"]`.
That explicit feature still bundles DuckDB for compatibility. Neither it nor the
optional PostgreSQL driver is enabled by an ordinary library dependency.

The historical low-level `ir::rel::sql::SqlExecutor` supports fixture setup and
materialization. Use `execution::SqlSession` for client-owned databases; its
contract has no setup statements or implicit transaction changes.

## Multiple engines

SQL dialect selection is independent of connection ownership. Today each compiled
query targets one engine. A caller can keep several sessions and route independent
queries to them. PostgreSQL SQL rendering is available; ClickHouse rendering and
cross-engine joins are not implemented. Federation will need a coordinator for
capability checks, fragments, exchanges, type conversion and transaction policy;
a session interface alone does not provide those semantics.

## Functions

Function registration lives in `src/ir/functions/`. Language names resolve through
operator tables and overload signatures before lowering. SQL target names and
argument/result types are explicit in compiler requests. Runtime callbacks and
unknown/volatile functions must not be silently treated as movable SQL expressions.
Add regressions for nulls, overload resolution and evaluation counts when changing
function lowering. Runtime-specific function APIs remain documented in rustdoc.
