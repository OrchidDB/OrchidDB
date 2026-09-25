# Quickstart

Create a small graph, query a relationship, and run the same workflow from Rust.

## Create a persistent graph

From the repository root, create two people and a relationship:

```sh
cargo run --locked --bin orchiddb -- --database social.duckdb --query \
  "CREATE (:Person {name:'Alice'})-[:KNOWS]->(:Person {name:'Bob'})"
```

The database file stores the graph between invocations. Run this creation statement once for the example dataset.

## Query the relationship

```sh
cargo run --locked --bin orchiddb -- --database social.duckdb --query \
  "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name"
```

The result contains Alice and Bob. The CLI prints column names and tab-separated values to stdout, with execution metadata on stderr.

Use Gremlin against the same file:

```sh
cargo run --bin orchiddb -- --database social.duckdb --language gremlin \
  --query "g.V().hasLabel('Person').has('name','Alice').out('KNOWS').values('name')"
```

This traversal returns Bob's name.

## Embed the workflow

After [setting up the library](installation.md#use-the-rust-library), place this in your application's `src/main.rs`:

```rust
use orchiddb::engine::GraphEngine;

#[tokio::main]
async fn main() -> Result<(), String> {
    let mut graph = GraphEngine::in_memory()?;
    graph.cypher(
        "CREATE (:Person {name:'Alice'})-[:KNOWS]->(:Person {name:'Bob'})"
    ).await?;

    let result = graph.cypher(
        "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name"
    ).await?;

    for row in 0..result.returned.batch.num_rows() {
        let cells = result.returned.batch.columns().iter()
            .map(|column| arrow::util::display::array_value_to_string(column, row)
                .map_err(|error| error.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        println!("{}", cells.join(" | "));
    }
    Ok(())
}
```

Run `cargo run` in the application directory. It prints `Alice | Bob`.

## Choose the next workflow

Use [managed graphs](managed-graphs.md) when OrchidDB owns the graph data. Use [mapped graphs](mapped-graphs.md) to query existing relational tables with graph syntax. Both workflows return Arrow data, so the surrounding application can use the same result-processing tools.
