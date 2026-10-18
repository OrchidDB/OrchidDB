//! Internal audit of externally generated manufacturing fixtures.
use std::{sync::Arc,time::Instant};
use orchiddb::{engine::GraphEngine,ir::{rel::mapping::GraphMapping,procedures::ProcedureCatalog},language::cypher::{parse_query,planner::CypherPlanner}};
#[tokio::main]
async fn main() {
 let args:Vec<String>=std::env::args().collect();
 let result=run(&args).await;
 println!("{}",result.unwrap_or_else(|e|serde_json::json!({"error":e})));
}
async fn run(a:&[String])->Result<serde_json::Value,String>{
 let conn=duckdb::Connection::open(&a[1]).map_err(|e|e.to_string())?;
 conn.execute_batch("SET threads=2; SET memory_limit='2GB'").map_err(|e|e.to_string())?;
 let mapping=Arc::new(GraphMapping::from_toml(&std::fs::read_to_string(&a[2]).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?);
 let query=std::fs::read_to_string(&a[3]).map_err(|e|e.to_string())?;
 let plan=CypherPlanner::new().plan(&parse_query(&query).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
 let start=Instant::now();
 let mut out=orchiddb::ir::diagnostics::conformance_sql_with_estimates(conn.try_clone().map_err(|e|e.to_string())?,mapping.clone(),&plan,Arc::new(ProcedureCatalog::default()),true).await?;
 out["prepare_ms"]=serde_json::json!(start.elapsed().as_secs_f64()*1000.);
 if a.get(4).map(String::as_str)==Some("profile") && out["runtime_input_tables"].as_array().map(|v|v.is_empty()).unwrap_or(false) {
 let mut executor=orchiddb::ir::rel::sql::DuckDbExecutor::from_connection(conn.try_clone().map_err(|e|e.to_string())?);
 executor.set_language_functions(true).map_err(|e|e.to_string())?;
 let c=executor.connection().map_err(|e|e.to_string())?;
 let mut profiles=vec![];
 for region in out["sql_plans"].as_array().into_iter().flatten() {
 let sql=region["sql"].as_str().unwrap();
 let mut times=vec![];
 let mut count=0;
 for _ in 0..3 {
 let t=Instant::now();let mut st=c.prepare(sql).map_err(|e|e.to_string())?;
 let mut cursor=st.query([]).map_err(|e|e.to_string())?;count=0;
 while cursor.next().map_err(|e|e.to_string())?.is_some(){count+=1;}
 times.push(t.elapsed().as_secs_f64()*1000.);
 }
 let mut st=c.prepare(&format!("EXPLAIN (ANALYZE, FORMAT JSON) {sql}")).map_err(|e|e.to_string())?;
 let data:String=st.query_row([],|row|row.get(1)).map_err(|e|e.to_string())?;
 profiles.push(serde_json::json!({"sql_execution_ms":times,"rows":count,"analyze":serde_json::from_str::<serde_json::Value>(&data).unwrap_or(serde_json::json!(data))}));
 }
 out["profiles"]=serde_json::json!(profiles);
 }
 if a.get(4).map(String::as_str)==Some("execute") {
 let mut engine=GraphEngine::mapped(conn,mapping)?;
 let mut times=vec![];
 let repetitions=std::env::var("ORCHID_AUDIT_REPETITIONS").ok().and_then(|s|s.parse::<usize>().ok()).unwrap_or(3).clamp(1,10);
 for _ in 0..repetitions {
 let t=Instant::now();
 if std::env::var_os("ORCHID_AUDIT_TRACE").is_some() { eprintln!("ORCHID_EXEC_START {}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()); }
 let result=engine.cypher(&query).await?;
 times.push(t.elapsed().as_secs_f64()*1000.);
 out["rows"]=serde_json::json!(result.returned.batch.num_rows());
 if std::env::var_os("ORCHID_AUDIT_ALL_ROWS").is_some() {
 out["all_rows"]=serde_json::json!((0..result.returned.batch.num_rows()).map(|r|(0..result.returned.batch.num_columns()).map(|c|arrow::util::display::array_value_to_string(result.returned.batch.column(c).as_ref(),r).unwrap_or_default()).collect::<Vec<_>>()).collect::<Vec<_>>());
 }

 out["sample"]=serde_json::json!((0..result.returned.batch.num_rows().min(10)).map(|r|(0..result.returned.batch.num_columns()).map(|c|arrow::util::display::array_value_to_string(result.returned.batch.column(c).as_ref(),r).unwrap_or_default()).collect::<Vec<_>>()).collect::<Vec<_>>());
 }
 out["execution_ms"]=serde_json::json!(times);
 }
 Ok(out)
}
