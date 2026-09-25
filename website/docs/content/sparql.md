# SPARQL

Query a graph with RDF vocabulary using an ontology mapping, or query existing RDF quad tables directly.

## Choose the data model

For a property graph, define an `OntologyMapping` that maps class and predicate IRIs to graph labels and properties. Use it with `GraphEngine::sparql` or `MappedGraphEngine::sparql`.

For existing RDF quad tables, define an `RdfDatasetMapping` and use `RdfGraphEngine::sparql`. The [RDF dataset guide](rdf.md) covers that workflow.

## Define a vocabulary

This vocabulary describes the people and social relationships in the [mapped tutorial](mapped-graphs.md):

```rust
use orchiddb::ir::plan::Direction;
use orchiddb::language::sparql::OntologyMapping;

let ontology = OntologyMapping::new()
    .class("https://example.com/Person", "Person")
    .property("https://example.com/name", "Person", "name")
    .property("https://example.com/age", "Person", "age")
    .relationship_between(
        "https://example.com/follows", "FOLLOWS", Direction::Out,
        "Person", "Person",
    );
```

The [ontology guide](ontology.md) explains identity properties and the builder methods.

## Select properties

```sparql
PREFIX ex: <https://example.com/>
SELECT ?name ?age WHERE {
  ?person a ex:Person ; ex:name ?name ; ex:age ?age .
  FILTER(?age >= 30)
}
ORDER BY ?name
```

The prefix abbreviates IRIs. `a` expresses an RDF type. Variables begin with `?`, and `FILTER` applies a condition to the matched bindings.

Run the query through a mapped engine:

```rust
let result = graph.sparql(
    "PREFIX ex: <https://example.com/> \
     SELECT ?name WHERE { ?p a ex:Person ; ex:name ?name . } ORDER BY ?name",
    ontology.clone(),
).await?;
```

`result.batch` contains Arrow data, and `result.fields` contains output binding names.

## Join relationships

```sparql
PREFIX ex: <https://example.com/>
SELECT ?name WHERE {
  ?person a ex:Person ; ex:name "alice" ; ex:follows ?friend .
  ?friend ex:name ?name .
}
ORDER BY ?name
```

Shared variables connect patterns. The ontology resolves `ex:follows` to the mapped `FOLLOWS` relationship.

## Shape a result

Use `SELECT DISTINCT` to remove repeated projected bindings. Use `ORDER BY` for result ordering and `LIMIT` with `OFFSET` for a result window.

```sparql
PREFIX ex: <https://example.com/>
SELECT DISTINCT ?name WHERE {
  ?person a ex:Person ; ex:name ?name .
}
ORDER BY ?name
LIMIT 20
OFFSET 0
```

## Query named graphs

RDF datasets can include a graph column. Use `GRAPH` to choose a named graph and `FROM` or `FROM NAMED` to define the active dataset for a query. See [RDF datasets](rdf.md#default-and-named-graphs) for the source mapping and examples.
