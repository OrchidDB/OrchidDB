//! cargo run --example compile_sql
//! Pass the emitted SQL to your existing DuckDB connection in your application.
use orchiddb::compiler::{CompileRequest, compile};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: CompileRequest = serde_json::from_value(serde_json::json!({
        "version": 1, "dialect": "duckdb", "language": "cypher",
        "query": "MATCH (p:Person) WHERE p.name=$name RETURN p.name AS name",
        "parameters": {"name": "Ada"},
        "tables": [{"name": "people", "columns": [
            {"name": "id", "data_type": "int64", "nullable": false},
            {"name": "name", "data_type": "string"}
        ]}],
        "nodes": [{"label": "Person", "table": "people", "id": "id",
                   "properties": {"name": "name"}}]
    }))?;
    let query = compile(request).await.map_err(std::io::Error::other)?;
    println!("{}", query.sql);
    Ok(())
}
