//! Run with `cargo run --example managed_graph`.
#[cfg(feature = "duckdb")]
#[tokio::main]
async fn main() -> Result<(), String> {
    use orchiddb::engine::GraphEngine;
    use orchiddb::ir::Value;
    use std::collections::BTreeMap;

    let mut graph = GraphEngine::in_memory()?;
    graph.begin()?;
    graph
        .cypher_with_params(
            "CREATE (:Person {name:$name})-[:KNOWS]->(:Person {name:'Bob'})",
            &BTreeMap::from([("name".into(), Value::String("Alice".into()))]),
        )
        .await?;
    graph.commit()?;
    let result = graph
        .cypher("MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name")
        .await?;
    println!("backend: {:?}", result.backend);
    for row in 0..result.returned.batch.num_rows() {
        let cells = result
            .returned
            .batch
            .columns()
            .iter()
            .map(|column| {
                arrow::util::display::array_value_to_string(column, row).map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        println!("{}", cells.join(" | "));
    }
    Ok(())
}

#[cfg(not(feature = "duckdb"))]
fn main() {
    eprintln!("This example requires the duckdb feature.");
}
