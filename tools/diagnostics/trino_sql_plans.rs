//! Inspect source-only Trino SQL without requiring a running SQL engine.
use serde_json::{Value, json};

#[tokio::main]
async fn main() {
    let queries = std::env::args().skip(1).collect::<Vec<_>>();
    let defaults = [
        "MATCH(p:Person) RETURN p.name",
        "MATCH(p:Person) RETURN count(p)",
        "MATCH(p:Person) WHERE p.id=42 RETURN p.name LIMIT 10",
        "MATCH(a:Person)-[:LINK]->(b:Person) RETURN a.name,b.name",
        "MATCH(a:Person) OPTIONAL MATCH(a)-[:LINK]->(b:Person) RETURN a.name,b.name",
        "MATCH(a:Person)-[:LINK*1..2]->(b:Person) RETURN a.id,b.id",
        "MATCH(p:Person) RETURN p.name,count(p) AS n ORDER BY n DESC",
        "MATCH(p:Person) RETURN DISTINCT p.name",
        "MATCH(p:Person) RETURN DISTINCT p.name ORDER BY p.name LIMIT 5",
        "MATCH(p:Person) RETURN p LIMIT 1",
    ];
    let queries = if queries.is_empty() { defaults.iter().map(|s| s.to_string()).collect() } else { queries };
    for query in queries {
        let request = json!({"version":1,"dialect":"trino","language":"cypher",
            "native_values":true,"query":query,"sources":{"db":{"adapter":"trino"}},
            "tables":[{"name":"people","source":"db","columns":[
                {"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]},
                {"name":"links","source":"db","columns":[
                {"name":"id","data_type":"int64"},{"name":"src","data_type":"int64"},{"name":"dst","data_type":"int64"}]}],
            "nodes":[{"label":"Person","table":"people","id":"id","properties":{"id":"id","name":"name"}}],
            "edges":[{"label":"LINK","table":"links","id":"id","source_label":"Person",
                "target_label":"Person","source":"src","target":"dst","properties":{}}]});
        println!("\n-- {query}");
        match orchiddb::compiler::compile_json(&request.to_string()).await {
            Ok(result) => {
                let result: Value = serde_json::from_str(&result).unwrap();
                for node in result["execution_plan"]["nodes"].as_array().unwrap() {
                    if let Some(sql) = node["sql"].as_str() { println!("{sql};"); }
                }
            }
            Err(error) => println!("-- ERROR: {error}"),
        }
    }
}
