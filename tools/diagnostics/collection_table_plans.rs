//! Run with `cargo run --example collection_table_plans`.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};
#[tokio::main]
async fn main() {
    let request = json!({
        "version":1,"dialect":"duckdb","language":"cypher",
        "query":"MATCH (i:Item) WHERE i.order_id = 42 AND i.quantity > 1 RETURN i.sku, i.quantity",
        "tables":[{"name":"orders","columns":[
            {"name":"id","data_type":"int64"},
            {"name":"items","data_type":"list:struct:{\"item_id\":\"int64\",\"sku\":\"string\",\"quantity\":\"int64\"}"}
        ]}],
        "collection_sources":[{
            "name":"order_items","table":"orders","column":"items",
            "parent_columns":{"order_id":"id"},
            "fields":{"item_id":"item_id","sku":"sku","quantity":"quantity"}
        }],
        "nodes":[{"label":"Item","table":"order_items","id":["order_id","item_id"],
            "properties":{"order_id":"order_id","sku":"sku","quantity":"quantity"}}]
    });
    let compiled: Value = serde_json::from_str(&compile_json(&request.to_string()).await.unwrap()).unwrap();
    println!("{}",serde_json::to_string_pretty(&json!({"mapping":request,"compiled":compiled})).unwrap());
}
