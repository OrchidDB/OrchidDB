# Managed graphs

```sql
CREATE PROPERTY GRAPH workspace;
CYPHER workspace CREATE (:Person {name: 'Ada', age: 37});
CYPHER workspace MATCH (p:Person) RETURN p.name, p.age;
GREMLIN workspace g.V().hasLabel('Person').values('name');
```

A definition without source mappings creates managed storage in the host DuckDB
database. It supports dynamic labels and properties, mutations, multi-properties,
and meta-properties using the shared graph implementation and codecs.

Storage persists when the database is reopened. Load Orchid again before graph
queries. Managed and heterogeneous results use the [native value transport](results.md).
Use ordinary DuckDB [transactions](transactions.md) to group changes.
