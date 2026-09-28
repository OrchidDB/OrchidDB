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
