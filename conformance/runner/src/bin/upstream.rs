#[path = "../gremlin_bindings.rs"]
mod gremlin_bindings;
#[path = "../relational_fixture.rs"]
mod relational_fixture;
use std::{io::{self,BufRead,Write},sync::Arc,collections::BTreeMap};
use arrow::{array::*,datatypes::{DataType,Field,Schema}};
use datafusion::datasource::MemTable;
use orchiddb::{engine::GraphEngine,ir::value::Value as GValue,
 ir::rel::rdf::{RdfDatasetMapping,IriQuadSource,RdfTermColumns},
 rdf_engine::{RdfTermValue,SparqlResults}};
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
fn term(t:RdfTermValue)->Value{match t{RdfTermValue::Iri(v)=>json!({"type":"uri","value":v}),RdfTermValue::BlankNode(v)=>json!({"type":"bnode","value":v}),RdfTermValue::Literal{lexical,datatype,language}=>json!({"type":"literal","value":lexical,"datatype":datatype,"lang":language})}}
async fn rdf(req:&Value)->Result<Value,String>{
 let fields=["graph","subject","subject_kind","subject_datatype","subject_language","predicate","predicate_kind","predicate_datatype","predicate_language","object","object_kind","object_datatype","object_language"];
 let schema=Arc::new(Schema::new(fields.iter().map(|n|Field::new(*n,DataType::Utf8,true)).collect::<Vec<_>>()));
 let batch=RecordBatch::new_empty(schema.clone());
 let mut mapping=RdfDatasetMapping::new();
 mapping.register_table("terms",Arc::new(MemTable::try_new(schema,vec![vec![batch]]).map_err(|e|e.to_string())?)).map_typed_quads("default",IriQuadSource::table("terms","subject","predicate","object").graph_column("graph").writable().typed_term_columns(RdfTermColumns::new("subject","subject_kind").datatype("subject_datatype").language("subject_language"),RdfTermColumns::new("predicate","predicate_kind").datatype("predicate_datatype").language("predicate_language"),RdfTermColumns::new("object","object_kind").datatype("object_datatype").language("object_language")));
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
 let schema_sql=conn.prepare("SELECT sql FROM duckdb_tables() WHERE NOT internal ORDER BY table_name").map_err(|e|e.to_string())?
  .query_map([],|row|row.get::<_,String>(0)).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
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
 output["query_cost"]=query_evidence(&stats);
 output["query_cost"]["execution_plan"]["schema_sql"]=json!(schema_sql);
 output["sql_regions"]=json!({"duckdb":stats.duckdb_regions,"trino":stats.other_sql_regions});
 Ok(output)
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
 "rdf"=>rdf(&req).await,
 "register-procedure"=>{
  use orchiddb::ir::procedures::{ProcedureField,ProcedureSignature,TableProcedure};
  let fields=|key:&str|req[key].as_array().into_iter().flatten().map(|field|ProcedureField {
   name:field["name"].as_str().unwrap_or("").into(),type_name:field["type"].as_str().unwrap_or("ANY").into(),nullable:field["nullable"].as_bool().unwrap_or(true)
  }).collect();
  let procedure=TableProcedure{signature:ProcedureSignature{inputs:fields("inputs"),outputs:fields("outputs")},
   rows:req["rows"].as_array().into_iter().flatten().map(|row|row.as_array().into_iter().flatten().map(param).collect()).collect()};
  fixture.engine.register_table_procedure(req["name"].as_str().unwrap_or("").into(),procedure).map(|_|json!({"ok":true}))
 },
 "sparql-syntax"=>{let q=req["query"].as_str().unwrap_or("");let base=req["base"].as_str();
  if req["update"].as_bool().unwrap_or(false){orchiddb::language::sparql::parse_update(q,base).map(|_|json!({"parsed":true})).map_err(|e|e.to_string())}
  else {match base {Some(b)=>orchiddb::language::sparql::parse_query_with_base(q,b),None=>orchiddb::language::sparql::parse_query(q)}.map(|_|json!({"parsed":true})).map_err(|e|e.to_string())}},
 _=>{
 let params=req["params"].as_object().map(|m|m.iter().map(|(k,v)|(k.clone(),param(v))).collect()).unwrap_or_default();
 let q=req["query"].as_str().unwrap_or("");
 let mut classification=None;
 let prepared=if op=="gremlin" {
  (|| {
   let bindings=gremlin_bindings::bindings(&req["bindings"])?;
   let (source,values)=orchiddb::language::gremlin::callables::prepare(q,&bindings).map_err(|e|e.to_string())?;
   let parsed=orchiddb::language::gremlin::parse_traversal_with_bindings(&source,&values).map_err(|e|e.to_string())?;
   orchiddb::language::gremlin::GremlinPlanner::new().plan(&parsed).map_err(|e|e.to_string())
  })().map_err(|error|(error,None))
 }else{cypher_plan(q,&params,fixture.engine.procedure_catalog())};
 let r=match prepared {
  Ok(plan)=>match fixture.prepare(&plan) {
   Ok(()) if op=="gremlin"=>match gremlin_bindings::bindings(&req["bindings"]) {Ok(bindings)=>fixture.engine.gremlin_with_bindings(q,&bindings).await,Err(error)=>Err(error)},
   Ok(())=>fixture.engine.execute_plan_with_diagnostics(&plan).await.map_err(|error|{
   classification=error.diagnosis.map(|code|{let (kind,detail,phase)=code.classification();json!({"type":kind,"detail":detail,"phase":phase})});
   error.to_string()
   }),
   Err(error)=>Err(error),
  },
  Err(_) if op=="gremlin"=>match gremlin_bindings::bindings(&req["bindings"]) {Ok(bindings)=>fixture.engine.gremlin_with_bindings(q,&bindings).await,Err(error)=>Err(error)},
  Err((message,detail))=>{classification=detail;Err(message)}
 };
 r.map(|r|{let b=r.returned.batch;json!({"native_rows":b.schema().metadata().get(&format!("orchiddb.{}.typed_rows.v1",op)).and_then(|v|serde_json::from_str::<Value>(v).ok()),"native_columns":b.schema().metadata().get(&format!("orchiddb.{}.typed_columns.v1",op)).and_then(|v|serde_json::from_str::<Value>(v).ok()),"columns":b.schema().fields().iter().map(|f|f.name()).collect::<Vec<_>>(),"rows":(0..b.num_rows()).map(|i|b.columns().iter().map(|a|cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>(),"typed_rows":if op=="gremlin"{json!((0..b.num_rows()).map(|i|b.columns().iter().map(|a|typed_cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>())}else{Value::Null},"sql_regions":{"duckdb":r.stats.duckdb_regions,"trino":r.stats.other_sql_regions},"query_cost":query_evidence(&r.stats),"backend":format!("{:?}",r.backend)})}).or_else(|error|Ok(json!({"error":error,"classification":classification})))
 }};
 let mut output=result.unwrap_or_else(|e|json!({"error":e}));
 if matches!(op,"cypher"|"gremlin"|"rdf"|"sparql-syntax") {
  if output.get("query_cost").is_none() {output["query_cost"]=json!({"metric_version":1,"coverage":"elapsed_only","work_units":null,"reason":if op=="sparql-syntax"{"syntax_only"}else if output.get("error").is_some(){"query_error"}else{"non_dag_update"}});}
  if let Some(regions) = output.get("sql_regions").cloned() { output["query_cost"]["sql_regions"] = regions; }
  output["query_cost"]["request_elapsed_micros"]=json!(request_started.elapsed().as_micros().min(u64::MAX as u128) as u64);
 }
 output["storage_profile"]=json!("mapped_relational_tables");
 if let Some(plan)=output.get_mut("query_cost").and_then(|cost|cost.get_mut("execution_plan")) {
  plan["tables"]=if op=="rdf" {json!(["terms","graph_names"])} else {json!(fixture.storage_tables())};
  if op!="rdf" {match fixture.schema_sql(){Ok(statements)=>plan["schema_sql"]=json!(statements),Err(error)=>plan["schema_error"]=json!(error)}}
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
