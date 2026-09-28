//! Reproducible connected-access plans and DuckDB results.
#[cfg(feature = "duckdb")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use orchiddb::{compiler::compile_json, ir::rel::statistics};
    use serde_json::{Value, json};
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE a AS SELECT i::BIGINT id,(i%100)::BIGINT k FROM range(10000) t(i); CREATE TABLE b AS SELECT i::BIGINT id,(i%100)::BIGINT k FROM range(10000) t(i); CREATE TABLE c AS SELECT i::BIGINT id,CASE WHEN i%100=1 THEN 'rare' ELSE 'common' END kind FROM range(10000) t(i)").unwrap();
    let mut request = json!({"version":1,"dialect":"duckdb","language":"cypher","query":"MATCH (a:A), (b:B), (c:C) WHERE a.k=b.k AND a.id=c.id AND c.kind='rare' RETURN count(*) AS n","tables":[{"name":"a","columns":[{"name":"id","data_type":"int64"},{"name":"k","data_type":"int64"}]},{"name":"b","columns":[{"name":"id","data_type":"int64"},{"name":"k","data_type":"int64"}]},{"name":"c","columns":[{"name":"id","data_type":"int64"},{"name":"kind","data_type":"string"}]}],"nodes":[{"label":"A","table":"a","id":"id","properties":{"id":"id","k":"k"}},{"label":"B","table":"b","id":"id","properties":{"id":"id","k":"k"}},{"label":"C","table":"c","id":"id","properties":{"id":"id","kind":"kind"}}]});
    let snapshot = statistics::generate_duckdb(&db, request.clone())?;
    let before: Value = serde_json::from_str(&compile_json(&request.to_string()).await?)?;
    request["statistics"] = serde_json::to_value(&snapshot)?;
    let after: Value = serde_json::from_str(&compile_json(&request.to_string()).await?)?;
    let mut runs = Vec::new();
    for disabled in [false, true] {
        db.execute_batch(if disabled {
            "SET disabled_optimizers='join_order,build_side_probe_side'"
        } else {
            "SET disabled_optimizers=''"
        })?;
        let mut answers = Vec::new();
        for (label, plan) in [("without_statistics", &before), ("with_statistics", &after)] {
            let sql = plan["sql"].as_str().unwrap();
            let mut stmt = db.prepare(sql)?;
            let batches = stmt.query_arrow([])?.collect::<Vec<_>>();
            let answer = arrow::util::pretty::pretty_format_batches(&batches)?.to_string();
            answers.push(answer.clone());
            let mut times = Vec::new();
            for _ in 0..7 {
                let start = std::time::Instant::now();
                let _ = stmt.query_arrow([])?.collect::<Vec<_>>();
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            let mut explain = db.prepare(&format!("EXPLAIN ANALYZE {sql}"))?;
            let physical = explain
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?
                .join("\n");
            runs.push(json!({"optimizers_disabled":disabled,"variant":label,"median_ms":times[3],"physical_plan":physical,"results":answer}));
        }
        assert_eq!(answers[0], answers[1]);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"build_profile":if cfg!(debug_assertions){"debug"}else{"release"},"duckdb_version":db.query_row("SELECT version()",[],|r|r.get::<_,String>(0))?,"mapping":request,"snapshot":snapshot,"before":before,"after":after,"runs":runs})
        )?
    );
    Ok(())
}
#[cfg(not(feature = "duckdb"))]
fn main() {
    eprintln!("Run with --features duckdb");
}
