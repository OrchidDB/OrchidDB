# Execution and plans

Language frontends compile to the existing graph IR and relational plan. SQL
regions become DuckDB plans; remaining graph operations become existing kernel
operators scheduled by DuckDB. DataFusion remains a compiler dependency and does
not execute extension queries. The extension includes no PostgreSQL driver.

```sql
EXPLAIN CYPHER social MATCH (p:Person) WHERE p.age > 35 RETURN p.name;
EXPLAIN GREMLIN social g.V().values('name');
```

Scans, joins, and filters remain visible to the host optimizer. Multi-input kernel
branches execute in declared order, preserving mutation and side-effect semantics.
Immutable prepared subplans may be reused within a query; mutable execution state
is query-scoped. Reads, writes, cancellation, and cleanup follow the host connection.

Iceberg and Lance scans stay in their DuckDB extensions. See [sources](views.md)
and [transactions](transactions.md).
