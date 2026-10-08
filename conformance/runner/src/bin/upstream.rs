#[path = "../gremlin_bindings.rs"]
mod gremlin_bindings;
#[path = "../fixture_cache.rs"]
mod fixture_cache;
#[path = "../starrocks.rs"]
mod starrocks;
use std::{io::{self,BufRead,Write},sync::Arc,collections::BTreeMap};
use arrow::{array::*,datatypes::{DataType,Field,Schema}};
use datafusion::datasource::MemTable;
use orchiddb::{engine::GraphEngine,ir::catalog::PropertyGraph,ir::value::Value as GValue,
 ir::rel::rdf::{RdfDatasetMapping,IriQuadSource,RdfTermColumns},
 rdf_engine::{RdfTermValue,SparqlResults}};
use serde_json::{Value,json};
fn configure_sql_engine(engine: &mut GraphEngine) -> Result<(), String> {
 let Some(config) = std::env::var("ORCHIDDB_SQL_ENGINE_JSON").ok() else { return Ok(()); };
 let config: Value = serde_json::from_str(&config).map_err(|e| format!("Invalid SQL engine JSON: {e}"))?;
 match config["dialect"].as_str() {
  Some("duckdb") => Ok(()),
  Some("starrocks") => {
   engine.set_sql_region_session(Box::new(starrocks::Session::new()?));
   Ok(())
  },
  Some("postgres") => {
   let url = config["connection"].as_str().ok_or("PostgreSQL connection is required")?;
   let url = url.to_owned();
   let client = std::thread::spawn(move || { let mut config: postgres::Config = url.parse()?; config.options("-c statement_timeout=8000 -c jit=off"); config.connect(postgres::NoTls) }).join().map_err(|_| "PostgreSQL connection worker failed")?.map_err(|e| e.to_string())?;
   engine.set_sql_region_session(Box::new(orchiddb::ir::rel::sql::region::PostgresRegionSession::new(client)));
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
use orchiddb::ir::catalog::import::{parameter as param,typed_property as fixture_property,import_graph as fixture_graph};
// Use the same parser, parameter binder, planner and executor as GraphEngine::cypher_with_params.
// Capture typed planner diagnostics before its public String error boundary.
fn cypher_plan(query:&str,params:&BTreeMap<String,GValue>,catalog:&orchiddb::ir::procedures::ProcedureCatalog)->Result<orchiddb::ir::plan::GraphPlan,(String,Option<Value>)>{
 orchiddb::language::cypher::preparation::prepare(query,params,Some(catalog))
  .map_err(|error| (error.message, error.classification.map(|value| serde_json::to_value(value).unwrap())))
}
fn term(t:RdfTermValue)->Value{match t{RdfTermValue::Iri(v)=>json!({"type":"uri","value":v}),RdfTermValue::BlankNode(v)=>json!({"type":"bnode","value":v}),RdfTermValue::Literal{lexical,datatype,language}=>json!({"type":"literal","value":lexical,"datatype":datatype,"lang":language})}}
async fn rdf(req:&Value)->Result<Value,String>{
 let fields=["g","s","s_kind","s_dt","s_lang","p","p_kind","p_dt","p_lang","o","o_kind","o_dt","o_lang"];
 let schema=Arc::new(Schema::new(fields.iter().map(|n|Field::new(*n,DataType::Utf8,true)).collect::<Vec<_>>()));
 let batch=RecordBatch::new_empty(schema.clone());
 let mut mapping=RdfDatasetMapping::new();
 mapping.register_table("terms",Arc::new(MemTable::try_new(schema,vec![vec![batch]]).map_err(|e|e.to_string())?)).map_typed_quads("default",IriQuadSource::table("terms","s","p","o").graph_column("g").writable().typed_term_columns(RdfTermColumns::new("s","s_kind").datatype("s_dt").language("s_lang"),RdfTermColumns::new("p","p_kind").datatype("p_dt").language("p_lang"),RdfTermColumns::new("o","o_kind").datatype("o_dt").language("o_lang")));
 let conn=duckdb::Connection::open_in_memory().map_err(|e|e.to_string())?;
 let graphs_schema=Arc::new(Schema::new(vec![Field::new("iri",DataType::Utf8,false)]));
 mapping.register_table("graph_names",Arc::new(MemTable::try_new(graphs_schema.clone(),vec![vec![RecordBatch::new_empty(graphs_schema)]]).map_err(|e|e.to_string())?)).map_writable_named_graphs("default","graph_names","iri");
 conn.execute_batch("CREATE TABLE graph_names(iri VARCHAR PRIMARY KEY)").map_err(|e|e.to_string())?;
 if let Some(names)=req["named_graphs"].as_array(){for name in names{
  conn.execute("INSERT INTO graph_names VALUES (?) ON CONFLICT DO NOTHING",[name.as_str().ok_or("graph name must be an IRI string")?]).map_err(|e|e.to_string())?;
 }}
 conn.execute_batch(&format!("CREATE TABLE terms({});",fields.iter().map(|f|format!("{f} VARCHAR")).collect::<Vec<_>>().join(","))).map_err(|e|e.to_string())?;
 if let Some(rows)=req["quads"].as_array(){for row in rows{
 let values=row.as_array().ok_or("quad row must be array")?.iter().map(|v|v.as_str().map(str::to_string)).collect::<Vec<_>>();
 conn.execute("INSERT INTO terms VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",duckdb::params_from_iter(values)).map_err(|e|e.to_string())?;
 }}
 let mut engine=GraphEngine::mapped(conn,Arc::new(orchiddb::ir::rel::mapping::GraphMapping::new().with_rdf_mapping(mapping)))?;
 engine.set_sql_timeout(std::time::Duration::from_secs(8));
 configure_sql_engine(&mut engine)?;
 if req["update"].as_bool().unwrap_or(false) {
  engine.sparql_update(req["query"].as_str().unwrap_or(""),"default",req["base"].as_str()).await?;
  let SparqlResults::Solutions{rows,..}=engine.sparql_query("SELECT ?g ?s ?p ?o WHERE { { ?s ?p ?o } UNION { GRAPH ?g { ?s ?p ?o } } }","default").await? else {return Err("Expected updated dataset rows".into())};
  let quads=rows.into_iter().map(|row|row.into_iter().map(|v|v.map(term)).collect::<Vec<_>>()).collect::<Vec<_>>();
  let SparqlResults::Solutions{rows,..}=engine.sparql_query("SELECT ?g WHERE { GRAPH ?g {} }","default").await? else {return Err("Expected updated graph names".into())};
  let names=rows.into_iter().filter_map(|row|match row.into_iter().next().flatten(){Some(RdfTermValue::Iri(v))=>Some(v),_=>None}).collect::<Vec<_>>();
  return Ok(json!({"quads":quads,"named_graphs":names}));
 }
 let output=engine.sparql_dataset(req["query"].as_str().unwrap_or(""),"default").await?;
 let stats=output.stats;
 let result=orchiddb::rdf_engine::decode_results(&output.returned)?;
 let response:Result<Value,String>=match result {
 SparqlResults::Boolean(v)=>Ok(json!({"boolean":v})),
 SparqlResults::Solutions{variables,rows}=>Ok(json!({"variables":variables.iter().map(|s|s.trim_start_matches('?')).collect::<Vec<_>>(),"rows":rows.into_iter().map(|r|r.into_iter().map(|v|v.map(term)).collect::<Vec<_>>()).collect::<Vec<_>>()})),
 SparqlResults::Graph(rows)=>Ok(json!({"graph":rows.into_iter().map(|r|r.into_iter().map(term).collect::<Vec<_>>()).collect::<Vec<_>>()}))
 };
 let mut output=response?;
 output["query_cost"]=stats.cost.report();
 output["sql_regions"]=json!({"duckdb":stats.duckdb_regions,"postgres":stats.postgres_regions,"starrocks":stats.other_sql_regions});
 Ok(output)
}
#[tokio::main]
async fn main(){
 let mut engine=GraphEngine::in_memory().unwrap();engine.set_sql_timeout(std::time::Duration::from_secs(8));
 configure_sql_engine(&mut engine).expect("SQL engine configuration");
 let mut fixtures=fixture_cache::FixtureCache::default();
 for line in io::stdin().lock().lines(){
 let req:Value=match serde_json::from_str(&line.unwrap()){Ok(v)=>v,Err(e)=>{println!("{}",json!({"error":e.to_string()}));continue}};
 let op=req["op"].as_str().unwrap_or("cypher");
 let request_started=std::time::Instant::now();
 let result:Result<Value,String>=match op{
 "fixture"=>fixture_graph(&req).and_then(|graph|fixtures.install(&mut engine,req["fixture_key"].as_str(),graph).map(|_|json!({"ok":true}))),
 "fixture-reset"=>req["fixture_key"].as_str().ok_or_else(||"fixture_key is required".to_string()).and_then(|key|fixtures.reset(&mut engine,key)).map(|_|json!({"ok":true})),
 "reset"=>{let graph=PropertyGraph::new();graph.enable_null_property_values(req["allow_null_property_values"].as_bool().unwrap_or(false));engine.replace_graph(graph).map(|_|json!({"ok":true}))},
 "cypher-snapshot"=>engine.cypher_state_snapshot().map(|snapshot|json!({"native_snapshot":snapshot})),
 "rdf"=>rdf(&req).await,
 "register-procedure"=>{
  use orchiddb::ir::procedures::{ProcedureField,ProcedureSignature,TableProcedure};
  let fields=|key:&str|req[key].as_array().into_iter().flatten().map(|field|ProcedureField {
   name:field["name"].as_str().unwrap_or("").into(),type_name:field["type"].as_str().unwrap_or("ANY").into(),nullable:field["nullable"].as_bool().unwrap_or(true)
  }).collect();
  let procedure=TableProcedure{signature:ProcedureSignature{inputs:fields("inputs"),outputs:fields("outputs")},
   rows:req["rows"].as_array().into_iter().flatten().map(|row|row.as_array().into_iter().flatten().map(param).collect()).collect()};
  engine.register_table_procedure(req["name"].as_str().unwrap_or("").into(),procedure).map(|_|json!({"ok":true}))
 },
 "sparql-syntax"=>{let q=req["query"].as_str().unwrap_or("");let base=req["base"].as_str();
  if req["update"].as_bool().unwrap_or(false){orchiddb::language::sparql::parse_update(q,base).map(|_|json!({"parsed":true})).map_err(|e|e.to_string())}
  else {match base {Some(b)=>orchiddb::language::sparql::parse_query_with_base(q,b),None=>orchiddb::language::sparql::parse_query(q)}.map(|_|json!({"parsed":true})).map_err(|e|e.to_string())}},
 _=>{
 let params=req["params"].as_object().map(|m|m.iter().map(|(k,v)|(k.clone(),param(v))).collect()).unwrap_or_default();
 let q=req["query"].as_str().unwrap_or("");
 let mut classification=None;
 let r=if op=="gremlin"{match gremlin_bindings::bindings(&req["bindings"]) {Ok(bindings)=>engine.gremlin_with_bindings(q,&bindings).await,Err(error)=>Err(error)}}else{match cypher_plan(q,&params,engine.procedure_catalog()){
  Ok(plan)=>engine.execute_plan_with_diagnostics(&plan).await.map_err(|error|{
   classification=error.diagnosis.map(|code|{let (kind,detail,phase)=code.classification();json!({"type":kind,"detail":detail,"phase":phase})});
   error.to_string()
  }),
  Err((message,detail))=>{classification=detail;Err(message)}
 }};
 r.map(|r|{let b=r.returned.batch;json!({"native_rows":b.schema().metadata().get(&format!("orchiddb.{}.typed_rows.v1",op)).and_then(|v|serde_json::from_str::<Value>(v).ok()),"native_columns":b.schema().metadata().get(&format!("orchiddb.{}.typed_columns.v1",op)).and_then(|v|serde_json::from_str::<Value>(v).ok()),"columns":b.schema().fields().iter().map(|f|f.name()).collect::<Vec<_>>(),"rows":(0..b.num_rows()).map(|i|b.columns().iter().map(|a|cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>(),"typed_rows":if op=="gremlin"{json!((0..b.num_rows()).map(|i|b.columns().iter().map(|a|typed_cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>())}else{Value::Null},"sql_regions":{"duckdb":r.stats.duckdb_regions,"postgres":r.stats.postgres_regions,"starrocks":r.stats.other_sql_regions},"query_cost":r.stats.cost.report(),"backend":format!("{:?}",r.backend)})}).or_else(|error|Ok(json!({"error":error,"classification":classification})))
 }};
 let mut output=result.unwrap_or_else(|e|json!({"error":e}));
 if matches!(op,"cypher"|"gremlin"|"rdf"|"sparql-syntax") {
  if output.get("query_cost").is_none() {output["query_cost"]=json!({"metric_version":1,"coverage":"elapsed_only","work_units":null,"reason":if op=="sparql-syntax"{"syntax_only"}else if output.get("error").is_some(){"query_error"}else{"non_dag_update"}});}
  if let Some(regions) = output.get("sql_regions").cloned() { output["query_cost"]["sql_regions"] = regions; }
  output["query_cost"]["request_elapsed_micros"]=json!(request_started.elapsed().as_micros().min(u64::MAX as u128) as u64);
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
