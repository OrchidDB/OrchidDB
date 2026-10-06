# Transactions and storage

```sql
CREATE PROPERTY GRAPH workspace;
BEGIN;
CYPHER workspace CREATE (:Person {name: 'Ada'});
CYPHER workspace MATCH (p:Person) RETURN p.name;
ROLLBACK;
CYPHER workspace MATCH (p:Person) RETURN count(p);
```

The final count is zero. Graph reads and writes use the calling DuckDB transaction.
There is no second database connection. Prepared executions receive fresh graph
state; `EXPLAIN` and `PREPARE` do not apply effects. Failed effect execution rolls
back its statement changes.

The extension forwards host cancellation to existing kernel interruption checks.
External catalog write support and cross-catalog guarantees belong to the source
extensions. No distributed transaction or multi-format snapshot is added.
