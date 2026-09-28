//! Run with `cargo run --example representation_plans`.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};
#[tokio::main]
async fn main() {
    let mapping: Value =
        serde_json::from_str(include_str!("data/representation_sources.json")).unwrap();
    let mut plans = Vec::new();
    for query in [
        "MATCH (i:Item) WHERE i.order_id = 1 RETURN i.sku",
        "MATCH (i:Item) WHERE i.sku = 'a' RETURN i.order_id",
        "MATCH (a:Item), (b:Item) WHERE a.order_id = 1 AND b.sku = 'a' RETURN a.sku,b.order_id LIMIT 2",
    ] {
        let mut request = mapping.clone();
        request["query"] = json!(query);
        let compiled: Value =
            serde_json::from_str(&compile_json(&request.to_string()).await.unwrap()).unwrap();
        plans.push(json!({"query":query,"compiled":compiled}));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({"mapping":mapping,"plans":plans})).unwrap()
    );
}
