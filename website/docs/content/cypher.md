# Cypher

```sql
CYPHER social
MATCH (a:Person)-[:FOLLOWS]->(b:Person)
WHERE a.age >= 30
RETURN a.name AS person, collect(b.name) AS friends;

EXPLAIN CYPHER social MATCH (p:Person) RETURN p.name;
```

Use the [quickstart graph](quickstart.md). Native statements need no quoted query
string. The existing Cypher frontend handles parsing, semantic analysis, and
lowering; DuckDB hosts execution and existing graph kernels.

For composition in SQL:

```sql
SELECT * FROM orchid_cypher('social', 'MATCH (p:Person) RETURN p.name AS name');
```

Nested unquoted `(CYPHER ...)` SQL syntax is not implemented. See
[parameters](parameters.md), [updates](updates.md), and [conformance](conformance.md#cypher).
