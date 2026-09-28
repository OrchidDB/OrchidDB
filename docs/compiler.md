# SQL generation API

The maintained protocol and examples are in the [SQL API reference](../website/docs/content/sql-compiler.md).
It covers `compile`/`compile_json`, typed schemas, graph and RDF rules, logical
physical-layout sources, collection tables, parameters, functions, result fields,
and SQL execution through application-owned sessions.

SQL generation is a stage of OrchidDB's shared planning pipeline. The API emits
SQL without opening connections or moving rows; execution and transaction ownership
follow the API/session the application uses. `GraphEngine` executes through the
shared relational DAG for Cypher, Gremlin, and SPARQL.

Runnable examples: [SQL generation](../examples/compile_sql.rs),
[application-owned DuckDB session](../examples/duckdb-client/),
[physical layouts](../examples/partition_layout_plans.rs), and
[collection tables](../examples/collection_table_plans.rs).

Optional [generated statistics](../website/docs/content/statistics.md) are collected
explicitly through a separate shared protocol. Compile can use an inline snapshot
or retained catalog handle without database access. [The statistics example](../examples/statistics_plans.rs)
shows cheaper exact plans in all three graph languages and the emitted diagnostics.

Generated statistics also drive connected inner-join ordering and build-side
costing, whole-plan representation selection, and parent semijoins before
collection expansion. Native mapped access chooses bounded scan caching or typed
batched lookup from actual frontier cardinality. Decisions are exposed through
`optimizer_decisions`; estimates and measured execution costs remain distinct.
