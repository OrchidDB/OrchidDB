# Arrow results

Read Arrow batches, handle nulls, and inspect execution metadata from graph queries.

## Client Arrow results

Compiler-only clients return results from your engine, without passing data through the compiler. Rust uses `RecordBatchReader`, Python a PyArrow reader, Java `ArrowResult`, JavaScript an async batch iterator, and C++/Elixir the Arrow C Stream interface.

Rust/PyArrow batches retain their buffers after reader closure. Java's reusable vectors are borrowed until the next batch or close. C++ batches have independent release callbacks; Elixir's stream pointer is valid only inside its callback. JavaScript follows the producer's lifetime contract. Always close results on early exit and keep the caller connection alive until result production finishes.

Arrow removes per-cell row conversion from the result boundary, but does not promise zero-copy or streaming execution by the database. See [client examples](client-apis.md).

## Optional managed runtime

The following result containers belong to the core runtime, not the compiler-only clients.

## Result containers

Managed engine methods return `QueryResult`. Its `returned` field holds a `ReturnedBatches` value with `fields` and `batch`. Mapped property-graph and RDF engine reads return `ReturnedBatches` directly.

| Result | Access |
| --- | --- |
| Managed Arrow batch | `result.returned.batch` |
| Managed field names | `result.returned.fields` |
| Managed execution metadata | `result.backend`, `result.stats` |
| Mapped or RDF Arrow batch | `result.batch` |
| Mapped or RDF field names | `result.fields` |

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

For a mapped or RDF result, bind `batch` to `&result.batch` instead.

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

For a managed query:

```rust
println!("backend: {:?}", result.backend);
println!("SQL islands: {}", result.stats.islands);
println!("island rows: {}", result.stats.island_rows);
println!("residual operators: {}", result.stats.residual_ops);
```

The backend is `DuckDb`, `Hybrid`, or `GraphRuntime`. See [execution and plans](execution.md) for how the read policy selects the execution path.
