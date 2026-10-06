# Graph and RDF updates

Managed graphs accept existing Cypher and Gremlin mutations:

```sql
CYPHER workspace MATCH (p:Person {name: 'Ada'}) SET p.age = 38 RETURN p.age;
GREMLIN workspace g.V().has('Person', 'name', 'Ada').property('age', 39);
```

Create `workspace` using the [managed graph example](managed-graphs.md) first.
Mapped source writes require explicitly writable mappings in the advanced mapping
protocol and write support from the underlying DuckDB source. Graph DDL alone does
not make external tables writable.

Existing SPARQL updates use `CALL orchid_sparql_update(request_json)` with writable
RDF sources. See [RDF storage](rdf.md). All writes use the host [transaction](transactions.md).
