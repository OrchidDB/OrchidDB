#[path = "support/duckdb_statistics.rs"]
mod adapter;
use orchiddb_client::Statistics;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = duckdb::Connection::open_in_memory()?;
    db.execute_batch("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Grace')")?;
    let request = json!({
        "version":1, "dialect":"duckdb", "language":"cypher",
        "query":"MATCH (p:Person) WHERE p.name='Ada' RETURN p.name AS name",
        "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],
        "nodes":[{"label":"Person","table":"people","id":"id","properties":{"name":"name"}}]
    });
    let mut statistics = Statistics::default();
    statistics
        .generate(request.clone(), |work| {
            let result = adapter::collect_statistics(&db, &work);
            async move { result }
        })
        .await
        .map_err(std::io::Error::other)?;
    let plan = statistics
        .compile(request)
        .await
        .map_err(std::io::Error::other)?;
    println!("{}", serde_json::to_string_pretty(&plan)?);
    let value: String = db.query_row(plan["sql"].as_str().unwrap(), [], |row| row.get(0))?;
    assert_eq!(value, "Ada");
    statistics.clear().await.map_err(std::io::Error::other)?;
    Ok(())
}
