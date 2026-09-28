# Arrow results

Read Arrow batches, handle nulls, and inspect execution metadata from graph queries.

## Client Arrow results

Language clients return results from your engine, without passing data through the compiler. Rust uses `RecordBatchReader`, Python a PyArrow reader, Java `ArrowResult`, JavaScript an async batch iterator, and C++/Elixir the Arrow C Stream interface.

Rust/PyArrow batches retain their buffers after reader closure. Java's reusable vectors are borrowed until the next batch or close. C++ batches have independent release callbacks; Elixir's stream pointer is valid only inside its callback. JavaScript follows the producer's lifetime contract. Always close results on early exit and keep the caller connection alive until result production finishes.

Arrow removes per-cell row conversion from the result boundary, but does not promise zero-copy or streaming execution by the database. See [client examples](client-apis.md).

## Result containers

`GraphEngine` methods `cypher`, `gremlin`, `sparql`, and `sparql_dataset` return
`QueryResult` for both managed and mapped storage. Its `returned` field holds
`ReturnedBatches` with `fields` and `batch`. `sparql_query` instead returns decoded
`SparqlResults`. Only the compatibility facades retain their older direct
`ReturnedBatches` interfaces (`MappedGraphEngine` reads and `RdfGraphEngine::sparql`).

| Result | Access |
| --- | --- |
| GraphEngine Arrow batch | `result.returned.batch` |
| GraphEngine field names | `result.returned.fields` |
| GraphEngine execution metadata | `result.backend`, `result.stats` |
| Compatibility facade Arrow batch | `result.batch` |
| Compatibility facade field names | `result.fields` |

A batch is an Arrow `RecordBatch`: a schema plus equally sized column arrays. Read its schema to inspect each column's physical Arrow type.

## Print rows

Arrow's display helper is convenient for CLI output and diagnostics:

```rust
let batch = &result.returned.batch;
println!("{}", result.returned.fields.join("\t"));
for row in 0..batch.num_rows() {
    let cells = batch.columns().iter().map(|column|
        arrow::util::display::array_value_to_string(column, row)
            .map_err(|error| error.to_string())
    ).collect::<Result<Vec<_>, _>>()?;
    println!("{}", cells.join("\t"));
}
```

For a compatibility facade returning `ReturnedBatches`, bind `batch` to `&result.batch` instead.

`sparql_query` returns `SparqlResults::Solutions { variables, rows }`,
`Boolean(bool)`, or `Graph(Vec<[RdfTermValue; 3]>)`. Unbound solution values are
`None`; RDF terms preserve IRI, blank-node, datatype, and language identity.
The raw `sparql_dataset` batch includes `__rdf:term:kind:?variable`,
`__rdf:term:datatype:?variable`, and `__rdf:term:language:?variable` columns alongside
lexical values. `returned.fields` lists visible variables. The ontology convenience
method `sparql(query, ontology)` keeps the older scalar-column contract.

## Read typed values

When the application expects a string column, check its type and handle nulls:

```rust
use arrow::array::{Array, StringArray};

let names = batch.column(0).as_any().downcast_ref::<StringArray>()
    .ok_or_else(|| "expected a UTF-8 name column".to_string())?;
for row in 0..names.len() {
    if names.is_null(row) {
        println!("no name");
    } else {
        println!("{}", names.value(row));
    }
}
```

Use `Int64Array`, `Float64Array`, or other Arrow arrays according to the result schema. Check nullability before reading a nullable value.

## Stable application output

Choose aliases in the query to make result fields explicit. Add an order when row ordering matters. Project the properties your consumer needs so downstream code can operate on a small, well-defined schema.

```cypher
MATCH (p:Person)
RETURN p.name AS name, p.age AS age
ORDER BY name
```

## Inspect execution metadata

For a `GraphEngine` query, including mapped RDF reads:

```rust
println!("backend: {:?}", result.backend);
println!("SQL islands: {}", result.stats.islands);
println!("island rows: {}", result.stats.island_rows);
println!("DataFusion operators: {}", result.stats.datafusion_ops);
println!("logical plan: {}", result.stats.logical_plan);
println!("physical plan: {}", result.stats.physical_plan);
println!("table choices: {:?}", result.stats.layout_selections);
```

The backend is `DuckDb`, `Hybrid`, or `DataFusion`. See [execution and plans](execution.md) for how the read policy selects the execution path.
