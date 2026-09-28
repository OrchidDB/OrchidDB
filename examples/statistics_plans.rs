//! cargo run --features duckdb --example statistics_plans > statistics-plans.json
#[cfg(feature = "duckdb")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use orchiddb::{compiler::compile_json, ir::rel::statistics};
    use serde_json::{Value, json};
    let db = duckdb::Connection::open_in_memory()?;
    db.execute_batch("CREATE TABLE people AS SELECT i::BIGINT id, CASE WHEN i=0 THEN 'rare' ELSE 'common' END AS name FROM range(100) t(i); CREATE TABLE totals AS SELECT name,count(*)::BIGINT n FROM people GROUP BY name;")?;
    let mapping = json!({"version":1,"dialect":"duckdb","language":"cypher","query":"MATCH (s:Summary) RETURN s.name,s.n ORDER BY s.name", "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]},{"name":"totals","columns":[{"name":"name","data_type":"string"},{"name":"n","data_type":"int64"}]}],"representation_sources":[{"name":"summary","default_representation":"grouped","representations":[{"name":"grouped","source":{"kind":"query","sql":"SELECT name,count(*) AS n FROM people GROUP BY name"}},{"name":"materialized","source":{"kind":"table","name":"totals"}}]}],"nodes":[{"label":"Summary","table":"summary","id":"name","properties":{"name":"name","n":"n"}}]});
    let snapshot = statistics::generate_duckdb(&db, mapping.clone())?;
    let mut examples = Vec::new();
    for (language, query) in [
        ("cypher", mapping["query"].as_str().unwrap()),
        ("gremlin", "g.V().hasLabel('Summary').values('n').order()"),
        ("sparql", "SELECT ?n WHERE { ?s <urn:n> ?n } ORDER BY ?n"),
    ] {
        let mut r = mapping.clone();
        r["language"] = json!(language);
        r["query"] = json!(query);
        r["rdf"] = json!([{"table":"summary","subject":{"kind":"template","prefix":"urn:summary:","columns":["name"]},"predicate":{"kind":"constant","value":"urn:n"},"object":{"kind":"literal","column":"n"}}]);
        let before: Value = serde_json::from_str(&compile_json(&r.to_string()).await?)?;
        r["statistics"] = serde_json::to_value(&snapshot)?;
        let after: Value = serde_json::from_str(&compile_json(&r.to_string()).await?)?;
        let rows = |plan: &Value| -> Result<String, Box<dyn std::error::Error>> {
            let mut stmt = db.prepare(plan["sql"].as_str().unwrap())?;
            let batches = stmt.query_arrow([])?.collect::<Vec<_>>();
            Ok(arrow::util::pretty::pretty_format_batches(&batches)?.to_string())
        };
        let result = rows(&after)?;
        assert_eq!(rows(&before)?, result);
        examples.push(json!({"language":language,"query":query,"before":before,"after":after,"results":result}));
    }
    let mut filter = mapping.clone();
    filter["nodes"] = json!([{"label":"Person","table":"people","id":"id","properties":{"id":"id","name":"name"}}]);
    filter["query"] = json!("MATCH (p:Person) WHERE p.id>=0 AND p.name='rare' RETURN p.id");
    filter["statistics"] = serde_json::to_value(&snapshot)?;
    let filters: Value = serde_json::from_str(&compile_json(&filter.to_string()).await?)?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"mapping":mapping,"snapshot":snapshot,"materialization":examples,"filter_order":filters})
        )?
    );
    Ok(())
}
#[cfg(not(feature = "duckdb"))]
fn main() {
    eprintln!("Run with --features duckdb");
}
