#[path = "../relational_fixture.rs"]
mod relational_fixture;
use std::{io::{self,BufRead,Write},collections::BTreeMap};
use arrow::{array::*,datatypes::DataType};
use orchiddb::{engine::GraphEngine,ir::value::Value as GValue};
use serde_json::{Value,json};

/// Observations from the executed DAG, never a second compilation of the query.
/// Keep this in the existing cost envelope so every assertion transport (including
/// the pinned Java Gremlin runner) preserves it without changing query results.
fn query_evidence(stats: &orchiddb::ir::exec::ExecStats) -> Value {
 let mut cost = stats.cost.report();
 let has_sql = !stats.sql_queries.is_empty() || !stats.write_queries.is_empty();
 // Multiple regions can require dependent execution even when every individual
 // region is SQL. Only a single region with no residual work is fully lowered.
 let fully_sql = stats.fully_pushed_down() && stats.sql_queries.len() == 1
     && stats.native_source_queries.is_empty() && stats.write_queries.is_empty();
 cost["execution_plan"] = json!({
  "format_version": 1,
  "capture": "executed",
  "logical_plan": stats.logical_plan,
  "physical_plan": stats.physical_plan,
  "sql_queries": stats.sql_queries,
  "write_queries": stats.write_queries,
  "storage": "mapped_relational_tables",
  "native_source_queries": stats.native_source_queries,
  "sql_regions": {"duckdb": stats.duckdb_regions, "trino": stats.other_sql_regions},
  "residual_operators": stats.datafusion_ops,
  "fully_lowered_to_sql": fully_sql,
  "lowering": if fully_sql { "full_sql" } else if has_sql { "mixed" } else { "runtime" }
 });
 cost
}

fn configure_sql_engine(engine: &mut GraphEngine) -> Result<(), String> {
 let Some(config) = std::env::var("ORCHIDDB_SQL_ENGINE_JSON").ok() else { return Ok(()); };
 let config: Value = serde_json::from_str(&config).map_err(|e| format!("Invalid SQL engine JSON: {e}"))?;
 match config["dialect"].as_str() {
  Some("duckdb") => Ok(()),
  Some("trino") => {
   use orchiddb::execution::{exchange::{ExchangeRegistry, ExchangeServer, ExchangeServerOptions}, trino::{TrinoOptions, TrinoSession}};
   static EXCHANGE: std::sync::OnceLock<Result<(ExchangeRegistry, ExchangeServer), String>> = std::sync::OnceLock::new();
   let (registry, _) = EXCHANGE.get_or_init(|| {
    let registry = ExchangeRegistry::default();
    let token = config["exchange_token"].as_str().ok_or("Trino exchange_token is required")?;
    let mut options = ExchangeServerOptions::local(token.to_owned());
    options.bind = config["exchange_bind"].as_str().unwrap_or("0.0.0.0:18883").parse().map_err(|e| format!("Invalid exchange bind: {e}"))?;
    let server = ExchangeServer::bind(registry.clone(), options)?;
    Ok((registry, server))
   }).as_ref().map_err(Clone::clone)?;
   let endpoint = config["endpoint"].as_str().ok_or("Trino endpoint is required")?;
   let mut options = TrinoOptions::new(endpoint, config["user"].as_str().unwrap_or("orchiddb-conformance"));
   options.exchange_catalog = config["exchange_catalog"].as_str().unwrap_or("orchid").to_owned();
   options.catalog = config["catalog"].as_str().map(str::to_owned);
   options.schema = config["schema"].as_str().map(str::to_owned);
   options.bearer_token = config["bearer_token"].as_str().map(str::to_owned);
   engine.set_sql_region_session(Box::new(TrinoSession::new(options, registry.clone())?));
   Ok(())
  },
  _ => Err("Unsupported SQL engine dialect".into()),
 }
}
fn cell(a:&dyn Array,i:usize)->Value {
 if a.is_null(i){return Value::Null}
 macro_rules! val{($t:ty)=>{json!(a.as_any().downcast_ref::<$t>().unwrap().value(i))}}
 match a.data_type(){
 DataType::Boolean=>val!(BooleanArray),DataType::Int8=>val!(Int8Array),DataType::Int16=>val!(Int16Array),DataType::Int32=>val!(Int32Array),DataType::Int64=>val!(Int64Array),
 DataType::UInt8=>val!(UInt8Array),DataType::UInt16=>val!(UInt16Array),DataType::UInt32=>val!(UInt32Array),DataType::UInt64=>val!(UInt64Array),DataType::Float32=>val!(Float32Array),DataType::Float64=>val!(Float64Array),
 DataType::Utf8=>val!(StringArray),DataType::LargeUtf8=>val!(LargeStringArray),
 DataType::List(_)=>{let v=a.as_any().downcast_ref::<ListArray>().unwrap().value(i);json!((0..v.len()).map(|j|cell(v.as_ref(),j)).collect::<Vec<_>>())},
 DataType::Struct(_)=>{let v=a.as_any().downcast_ref::<StructArray>().unwrap();json!(v.column_names().iter().zip(v.columns()).map(|(k,a)|(k.to_string(),cell(a.as_ref(),i))).collect::<BTreeMap<_,_>>())},
 _=>json!({"$unmapped_arrow":a.data_type().to_string(),"display":arrow::util::display::array_value_to_string(a,i).unwrap_or_default()})
 }
}
fn typed_cell(a:&dyn Array,i:usize)->Value {
 if a.is_null(i){return Value::Null}
 let kind=match a.data_type(){DataType::Int8=>Some("Byte"),DataType::Int16=>Some("Short"),DataType::Int32=>Some("Integer"),DataType::Int64=>Some("Long"),DataType::Float32=>Some("Float"),DataType::Float64=>Some("Double"),_=>None};
 if let Some(kind)=kind{return json!({"$type":kind,"value":cell(a,i)})}
 match a.data_type(){
 DataType::List(_)=>{let v=a.as_any().downcast_ref::<ListArray>().unwrap().value(i);json!((0..v.len()).map(|j|typed_cell(v.as_ref(),j)).collect::<Vec<_>>())},
 DataType::Struct(_)=>{let v=a.as_any().downcast_ref::<StructArray>().unwrap();json!(v.column_names().iter().zip(v.columns()).map(|(k,a)|(k.to_string(),typed_cell(a.as_ref(),i))).collect::<BTreeMap<_,_>>())},
 _=>cell(a,i)
 }
}
use orchiddb::ir::catalog::import::parameter as param;
#[cfg(test)] use orchiddb::ir::catalog::import::typed_property as fixture_property;
// Use the same parser, parameter binder, planner and executor as GraphEngine::cypher_with_params.
// Capture typed planner diagnostics before its public String error boundary.
fn cypher_plan(query:&str,params:&BTreeMap<String,GValue>,catalog:&orchiddb::ir::procedures::ProcedureCatalog)->Result<orchiddb::ir::plan::GraphPlan,(String,Option<Value>)>{
 orchiddb::language::cypher::preparation::prepare(query,params,Some(catalog))
  .map_err(|error| (error.message, error.classification.map(|value| serde_json::to_value(value).unwrap())))
}
#[tokio::main]
async fn main(){
 let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
 let sql_timeout_seconds=std::env::var("CONFORMANCE_SQL_TIMEOUT_SECONDS").ok().map(|value| value.parse::<u64>().expect("CONFORMANCE_SQL_TIMEOUT_SECONDS must be an integer")).unwrap_or(8);
 fixture.engine.set_sql_timeout(std::time::Duration::from_secs(sql_timeout_seconds));
 configure_sql_engine(&mut fixture.engine).expect("SQL engine configuration");
 for line in io::stdin().lock().lines(){
 let req:Value=match serde_json::from_str(&line.unwrap()){Ok(v)=>v,Err(e)=>{println!("{}",json!({"error":e.to_string()}));continue}};
 let op=req["op"].as_str().unwrap_or("cypher");
 let request_started=std::time::Instant::now();
 if matches!(op,"fixture"|"fixture-reset") {
  fixture.allow_relationship_expansion=req["allow_relationship_expansion"].as_bool().unwrap_or(true);
 }
 let result:Result<Value,String>=match op{
 "fixture"=>fixture.install(&req).map(|_|json!({"ok":true})),
 "fixture-reset"=>req["fixture_key"].as_str().ok_or_else(||"fixture_key is required".to_string()).and_then(|key|fixture.restore(key)).map(|_|json!({"ok":true})),
 "reset"=>fixture.reset().map(|_|json!({"ok":true})),
 "schema"=>fixture.schema_sql().map(|statements|json!({"statements":statements})),
 "configure-schema"=>{
  let params=req["params"].as_object().map(|m|m.iter().map(|(k,v)|(k.clone(),param(v))).collect()).unwrap_or_default();
  let plans=req["queries"].as_array().into_iter().flatten().filter_map(|query|
   cypher_plan(query.as_str().unwrap_or(""),&params,fixture.engine.procedure_catalog()).ok()
  ).collect::<Vec<_>>();
  fixture.configure_cypher(&plans).map(|_|json!({"ok":true}))
 },
 "cypher-snapshot"=>fixture.snapshot().map(|snapshot|json!({"native_snapshot":snapshot})),
 "register-procedure"=>{
  use orchiddb::ir::procedures::{ProcedureField,ProcedureSignature,TableProcedure};
  let fields=|key:&str|req[key].as_array().into_iter().flatten().map(|field|ProcedureField {
   name:field["name"].as_str().unwrap_or("").into(),type_name:field["type"].as_str().unwrap_or("ANY").into(),nullable:field["nullable"].as_bool().unwrap_or(true)
  }).collect();
  let procedure=TableProcedure{signature:ProcedureSignature{inputs:fields("inputs"),outputs:fields("outputs")},
   rows:req["rows"].as_array().into_iter().flatten().map(|row|row.as_array().into_iter().flatten().map(param).collect()).collect()};
  fixture.engine.register_table_procedure(req["name"].as_str().unwrap_or("").into(),procedure).map(|_|json!({"ok":true}))
 },
 "cypher"=>{
 let params=req["params"].as_object().map(|m|m.iter().map(|(k,v)|(k.clone(),param(v))).collect()).unwrap_or_default();
 let q=req["query"].as_str().unwrap_or("");
 let mut classification=None;
 let prepared=cypher_plan(q,&params,fixture.engine.procedure_catalog());
 let r=match prepared {
  Ok(plan)=>match fixture.prepare(&plan) {
   Ok(())=>fixture.engine.execute_plan_with_diagnostics(&plan).await.map_err(|error|{
   classification=error.diagnosis.map(|code|{let (kind,detail,phase)=code.classification();json!({"type":kind,"detail":detail,"phase":phase})});
   error.to_string()
   }),
   Err(error)=>Err(error),
  },
  Err((message,detail))=>{classification=detail;Err(message)}
 };
 r.map(|r|{let b=r.returned.batch;json!({"native_rows":b.schema().metadata().get(&format!("orchiddb.{}.typed_rows.v1",op)).and_then(|v|serde_json::from_str::<Value>(v).ok()),"native_columns":b.schema().metadata().get(&format!("orchiddb.{}.typed_columns.v1",op)).and_then(|v|serde_json::from_str::<Value>(v).ok()),"columns":b.schema().fields().iter().map(|f|f.name()).collect::<Vec<_>>(),"rows":(0..b.num_rows()).map(|i|b.columns().iter().map(|a|cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>(),"typed_rows":if op=="gremlin"{json!((0..b.num_rows()).map(|i|b.columns().iter().map(|a|typed_cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>())}else{Value::Null},"sql_regions":{"duckdb":r.stats.duckdb_regions,"trino":r.stats.other_sql_regions},"query_cost":query_evidence(&r.stats),"backend":format!("{:?}",r.backend)})}).or_else(|error|Ok(json!({"error":error,"classification":classification})))
 },
 _=>Err("Only Cypher is supported".into()),
 };
 let mut output=result.unwrap_or_else(|e|json!({"error":e}));
 if op=="cypher" {
  if output.get("query_cost").is_none() {output["query_cost"]=json!({"metric_version":1,"coverage":"elapsed_only","work_units":null,"reason":if output.get("error").is_some(){"query_error"}else{"non_dag_update"}});}
  if let Some(regions) = output.get("sql_regions").cloned() { output["query_cost"]["sql_regions"] = regions; }
  output["query_cost"]["request_elapsed_micros"]=json!(request_started.elapsed().as_micros().min(u64::MAX as u128) as u64);
 }
 output["storage_profile"]=json!("mapped_relational_tables");
 if let Some(plan)=output.get_mut("query_cost").and_then(|cost|cost.get_mut("execution_plan")) {
  plan["tables"]=json!(fixture.storage_tables());
  match fixture.schema_sql(){Ok(statements)=>plan["schema_sql"]=json!(statements),Err(error)=>plan["schema_error"]=json!(error)}
 }
 output["engine_instance"]=json!(std::process::id().to_string());println!("{output}");io::stdout().flush().unwrap();
 }
}

#[cfg(test)] mod adapter_tests {
 use super::*;
 #[test] fn fixture_width_comes_from_declared_type(){
  assert!(matches!(fixture_property(&json!(7),Some("Integer")).unwrap(),GValue::Int(7)));
  assert!(matches!(fixture_property(&json!(7),Some("Long")).unwrap(),GValue::Long(7)));
  assert!(matches!(fixture_property(&json!(0.5),Some("Float")).unwrap(),GValue::Float32(_)));
  assert!(fixture_property(&json!(2147483648_i64),Some("Integer")).is_err());
  assert!(fixture_property(&json!(true),Some("Integer")).is_err());
  assert!(fixture_property(&json!(1),Some("Unknown")).is_err());
 }
}
