//! Reproducible connected-access plans and DuckDB results.
#[cfg(feature = "duckdb")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use orchiddb::{compiler::compile_json, ir::rel::statistics};
    use serde_json::{Value, json};
    let db = duckdb::Connection::open_in_memory()?;
    db.execute_batch("CREATE TABLE parents AS SELECT i::BIGINT id, list_transform(range(32), x -> {'v': x}) items FROM range(2000) t(i); CREATE TABLE flat AS SELECT id AS owner, unnest(items).v AS value FROM parents; CREATE TABLE customers AS SELECT i::BIGINT id, CASE WHEN i=1 THEN 'rare' ELSE 'common' END kind FROM range(2000) t(i);")?;
    let mut request = json!({"version":1,"dialect":"duckdb","language":"cypher","query":"MATCH (t:Item), (c:Customer) WHERE t.owner=c.id AND c.kind='rare' RETURN t.owner,t.value ORDER BY t.owner,t.value",
 "tables":[{"name":"parents","columns":[{"name":"id","data_type":"int64"},{"name":"items","data_type":"list:struct:{\"v\":\"int64\"}"}]},{"name":"flat","columns":[{"name":"owner","data_type":"int64"},{"name":"value","data_type":"int64"}]},{"name":"customers","columns":[{"name":"id","data_type":"int64"},{"name":"kind","data_type":"string"}]}],
 "collection_sources":[{"name":"nested","table":"parents","column":"items","parent_columns":{"owner":"id"},"fields":{"value":"v"}}],
 "representation_sources":[{"name":"items","default_representation":"flat","representations":[{"name":"flat","source":{"kind":"table","name":"flat"}},{"name":"nested","source":{"kind":"table","name":"nested"}}]}],
 "nodes":[{"label":"Item","table":"items","id":["owner","value"],"properties":{"owner":"owner","value":"value"}},{"label":"Customer","table":"customers","id":"id","properties":{"id":"id","kind":"kind"}}]});
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
