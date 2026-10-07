# OrchidDB for Java

Graph schema is registered on a connection; queries contain only text and
parameters. The application supplies its JDBC driver and owns transactions.

```java
var engine = JdbcEngine.borrowed("lake", SqlDialect.DUCKDB, jdbcConnection);
var schema = new GraphMapping(
    List.of(NodeMapping.node("Person", Source.table("lake", "people"), "id")
        .property("name", "name")),
    List.of());
var graph = new OrchidDB(engine).graph(schema);
try (var rows = graph.query(Query.cypher(
    "MATCH (p:Person) WHERE p.name=$name RETURN p.name AS name",
    Map.of("name", "Ada")))) {
    while (rows.next()) System.out.println(rows.get("name"));
}
```

For JSON schema configuration, use `io.orchiddb.Connection.fromSchemaFile(path,
engine)` and call `query(text)` or `query(Query.cypher(text, parameters))`.
The schema contains no query, language, parameters, or dialect. Type metadata in
JSON schema must match the session's tables. Typed `GraphMapping` discovers
metadata through the same JDBC session used to execute each query.

Use `SqlDialect.POSTGRES` with a caller-owned PostgreSQL JDBC connection. Use
`Query.gremlin(text)` or `Query.sparql(text)` for other languages. SPARQL ontology,
RDF rules, and dataset live on `GraphMapping`, never on Query. Registered function
signatures go in `db.graph(mapping, functions)`. Native compilation is package-private;
applications receive results and have no public `plan`/compiler entry point.

`graph.queryArrow(query)` uses the Arrow exporter configured on your engine.
See [Arrow integration](docs/arrow.md). Result closure releases its lease, never
commits or closes a borrowed connection. `OrchidGremlin.traversal(graph)` provides
the optional native Java Gremlin API. Statistics generation and snapshots are
operations on the registered graph, not standalone query compilation.

## Build and run from this checkout

Java 17+, Maven, and Rust are required. From the repository root:

```sh
clients/java/scripts/run-example.sh BringYourOwnDuckDb
clients/java/scripts/run-gremlin-example.sh
```

The launchers build the local JNI library and Java API. Use the matching local
artifacts in your application; older published packages have the old API.
No new registry package is published by this source change.

[Examples](examples/README.md) · [Client builds](../README.md)
