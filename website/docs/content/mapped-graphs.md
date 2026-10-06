# Map existing tables

Use `CREATE PROPERTY GRAPH` as shown in the [quickstart](quickstart.md).
DuckDB discovers each source's schema during binding. Declare keys and relationships
explicitly; Orchid does not infer relationships from similarly named columns.

Both table and view sources work. Use fully qualified names for attached Iceberg
or Lance catalogs. Graph metadata lives in the graph's writable DuckDB catalog.
Every query revalidates source columns and endpoint types.

Mapped definitions support `CREATE OR REPLACE PROPERTY GRAPH`,
`CREATE PROPERTY GRAPH IF NOT EXISTS`, and `DROP PROPERTY GRAPH [IF EXISTS]`.
Definitions persist in transactionally managed `__orchid_graph_<name>` views.
Reserve that prefix for Orchid. Reopen the database, load the required extensions,
and reattach external catalogs under the same names to use saved definitions.
