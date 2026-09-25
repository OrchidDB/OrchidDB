# RDF datasets

Map RDF statements in DuckDB tables to a named dataset and query them with SPARQL.

## Describe a quad source

An IRI quad source has subject, predicate, and object columns containing IRI strings. An optional graph column selects the default or a named graph.

```sql
CREATE TABLE user_quads (
  graph VARCHAR, subject VARCHAR, predicate VARCHAR, object VARCHAR
);
INSERT INTO user_quads VALUES
  (NULL, 'https://example.com/alice', 'https://example.com/knows', 'https://example.com/bob'),
  (NULL, 'https://example.com/bob', 'https://example.com/knows', 'https://example.com/cara');
```

A null graph value denotes the default graph. A graph IRI denotes a named graph. If the source has no graph column, its rows belong to the default graph.

## Build the RDF engine

Add `duckdb = { version = "1.10502.0", features = ["bundled"] }` to the application dependencies for this connection example, alongside the [base dependencies](installation.md#use-the-rust-library).

```rust
use std::sync::Arc;
use arrow::datatypes::{DataType, Field, Schema};
use new_graph::ir::rel::mapping::schema_only_provider;
use new_graph::ir::rel::rdf::{IriQuadSource, RdfDatasetMapping};
use new_graph::ir::rel::sql::DuckDbExecutor;
use new_graph::rdf_engine::RdfGraphEngine;

let connection = duckdb::Connection::open("rdf.duckdb")
    .map_err(|error| error.to_string())?;
// The user_quads table above must exist in this database.
let schema = Arc::new(Schema::new(vec![
    Field::new("graph", DataType::Utf8, true),
    Field::new("subject", DataType::Utf8, false),
    Field::new("predicate", DataType::Utf8, false),
    Field::new("object", DataType::Utf8, false),
]));
let mut mapping = RdfDatasetMapping::new();
mapping.register_table("user_quads", schema_only_provider(schema))
    .map_iri_quads("people", IriQuadSource::table(
        "user_quads", "subject", "predicate", "object"
    ).graph_column("graph"));
let mut graph = RdfGraphEngine::new(
    DuckDbExecutor::from_connection(connection), Arc::new(mapping), "people"
);
let result = graph.sparql(
    "PREFIX ex: <https://example.com/> \
     SELECT ?friend WHERE { ex:alice ex:knows ?friend . }"
).await?;
```

The dataset name `people` selects the sources registered under that name. The returned friend is `https://example.com/bob`.

## Join triple patterns

```sparql
PREFIX ex: <https://example.com/>
SELECT ?friend WHERE {
  ex:alice ex:knows ?middle .
  ?middle ex:knows ?friend .
}
```

The shared `?middle` binding joins the two patterns and returns cara. A variable predicate, such as `ex:alice ?predicate ?object`, can inspect several relationship predicates from the same source.

## Default and named graphs

To query a named graph, store its IRI in the graph column and select it with `GRAPH`:

```sparql
PREFIX ex: <https://example.com/>
SELECT ?subject ?object WHERE {
  GRAPH ex:team { ?subject ex:knows ?object . }
}
```

Use `GRAPH ?graph` to bind the graph name. `FROM` chooses graphs to merge into the query's default graph; `FROM NAMED` selects named graphs available to `GRAPH` patterns.

```sparql
PREFIX ex: <https://example.com/>
SELECT ?subject ?object
FROM ex:team
WHERE { ?subject ex:knows ?object . }
```

## Combine sources

Register each physical table and call `map_iri_quads` repeatedly with the same dataset name. The dataset then includes each mapped source. This can expose several application-owned tables through one SPARQL entry point.

## Keep using the connection

After querying, `graph.into_executor()` returns the DuckDB executor. Use it to continue relational work in the same session. `graph.mapping()` and `graph.dataset()` expose the configuration while the engine is active.
