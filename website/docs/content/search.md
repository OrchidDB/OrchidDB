# Lance search and graph queries

Expose existing Lance search functions such as `lance_vector_search(...)` through
a DuckDB view, then map that view as a vertex source. Join its candidates to graph
relationships stored in ordinary tables or Iceberg sources.

Lance continues to execute its index and scoring implementation. Orchid does not
copy vectors into a separate graph index. The integration suite creates a real
IVF_FLAT index and joins search results with Iceberg relationships.

Native DDL does not yet expose dynamic per-row search or computed-relationship
declarations. The shared search planner remains reusable, but that surface needs
an adapter. See [Iceberg and Lance sources](views.md).
