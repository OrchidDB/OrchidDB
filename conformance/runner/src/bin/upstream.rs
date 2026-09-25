#[path = "../gremlin_bindings.rs"]
mod gremlin_bindings;
use std::{io::{self,BufRead,Write},sync::Arc,collections::BTreeMap};
use arrow::{array::*,datatypes::{DataType,Field,Schema}};
use datafusion::datasource::MemTable;
use new_graph::{engine::GraphEngine,ir::catalog::PropertyGraph,ir::value::Value as GValue,
 ir::rel::{rdf::{RdfDatasetMapping,IriQuadSource,RdfTermColumns},sql::DuckDbExecutor},
 rdf_engine::{RdfGraphEngine,RdfTermValue,SparqlResults}};
use serde_json::{Value,json};
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
fn param(v:&Value)->GValue{match v{Value::Null=>GValue::Null,Value::Bool(x)=>GValue::Bool(*x),Value::Number(x)=>if let Some(i)=x.as_i64(){GValue::Int(i)}else{GValue::Float(x.as_f64().unwrap())},Value::String(s)=>GValue::String(s.clone()),Value::Array(a)=>GValue::List(a.iter().map(param).collect()),Value::Object(m)=>GValue::Map(m.iter().map(|(k,v)|(k.clone(),param(v))).collect())}}
fn fixture_property(value:&Value,declared:Option<&str>)->Result<GValue,String>{
 let invalid=||format!("Fixture value {value} does not match declared type {declared:?}");
 match declared {
  None=>Ok(param(value)),
  Some("Integer")=>value.as_i64().filter(|v|i32::try_from(*v).is_ok()).map(GValue::Int).ok_or_else(invalid),
  Some("Long")=>value.as_i64().map(GValue::Long).ok_or_else(invalid),
  Some("Byte")=>value.as_i64().and_then(|v|i8::try_from(v).ok()).map(GValue::Byte).ok_or_else(invalid),
  Some("Short")=>value.as_i64().and_then(|v|i16::try_from(v).ok()).map(GValue::Short).ok_or_else(invalid),
  Some("Float")=>value.as_f64().map(|v|GValue::Float32(v as f32)).ok_or_else(invalid),
  Some("Double")=>value.as_f64().map(GValue::Float).ok_or_else(invalid),
  Some("String")=>value.as_str().map(|v|GValue::String(v.to_owned())).ok_or_else(invalid),
  Some("Boolean")=>value.as_bool().map(GValue::Bool).ok_or_else(invalid),
  Some(other)=>Err(format!("Unmapped fixture property type {other}")),
 }
}
fn fixture_graph(req:&Value)->Result<PropertyGraph,String>{
 fn properties(item:&Value)->Result<BTreeMap<String,GValue>,String>{
  item["properties"].as_object().ok_or("Fixture properties must be an object")?.iter()
   .map(|(key,value)|fixture_property(value,item["property_types"][key].as_str()).map(|v|(key.clone(),v))).collect()
 }
 let graph=PropertyGraph::new();graph.enable_null_property_values(req["allow_null_property_values"].as_bool().unwrap_or(false));let mut nodes=BTreeMap::new();
 for n in req["nodes"].as_array().ok_or("Fixture nodes must be an array")?{
  let v=graph.insert_node(n["label"].as_str().ok_or("Fixture node label missing")?,if n["property_records"].is_array(){BTreeMap::new()}else{properties(n)?});
  graph.set_element_public_id(&v,fixture_property(&n["id"],n["id_type"].as_str())?).map_err(|e|e.to_string())?;
  if let Some(records)=n["property_records"].as_array(){for record in records {
    let key=record["key"].as_str().ok_or("Fixture property key missing")?;
    let value=fixture_property(&record["value"],record["type"].as_str())?;
    let mut meta=BTreeMap::new();
    if let Some(entries)=record["meta"].as_object(){for (key,value) in entries{meta.insert(key.clone(),fixture_property(value,record["meta_types"][key].as_str())?);}}
    let property=graph.set_vertex_property(&v,key,value,new_graph::ir::catalog::Cardinality::List,meta).map_err(|e|e.to_string())?;
    if !record["id"].is_null(){graph.set_vertex_property_public_id(&property,fixture_property(&record["id"],record["id_type"].as_str())?).map_err(|e|e.to_string())?;}
  }}
  nodes.insert(n["id"].to_string(),v);
 }
 for e in req["edges"].as_array().ok_or("Fixture edges must be an array")?{
  let src=nodes.get(&e["src"].to_string()).ok_or("Fixture edge source missing")?;
  let dst=nodes.get(&e["dst"].to_string()).ok_or("Fixture edge target missing")?;
  let edge=graph.insert_edge(e["label"].as_str().ok_or("Fixture edge label missing")?,src,dst,properties(e)?).map_err(|e|e.to_string())?;
  graph.set_element_public_id(&edge,fixture_property(&e["id"],e["id_type"].as_str())?).map_err(|e|e.to_string())?;
 }

 Ok(graph)
}
// Use the same parser, parameter binder, planner and executor as GraphEngine::cypher_with_params.
// Capture typed planner diagnostics before its public String error boundary.
fn cypher_plan(query:&str,params:&BTreeMap<String,GValue>,catalog:&new_graph::ir::procedures::ProcedureCatalog)->Result<new_graph::ir::plan::GraphPlan,(String,Option<Value>)>{
 use new_graph::language::cypher;
 let mut parsed=cypher::parser::parse_query(query).map_err(|e|{
  let classification=e.classification().map(|(kind,detail)|json!({"type":kind,"detail":detail,"phase":"compile time"}));
  (e.to_string(),classification)
 })?;
 let plan_error=|e:cypher::planner::CypherPlanError| {
  let classification=e.classification().map(|(kind,detail)|json!({"type":kind,"detail":detail,"phase":"compile time"}));
  (e.to_string(),classification)
 };
 cypher::procedures::prepare(&mut parsed,catalog).map_err(plan_error)?;
 cypher::parameters::bind_parameters_with_diagnostics(&mut parsed,params).map_err(|e|{
  let classification=e.classification().map(|(kind,detail)|json!({"type":kind,"detail":detail,"phase":"compile time"}));
  (e.to_string(),classification)
 })?;
 cypher::procedures::prepare(&mut parsed,catalog).map_err(plan_error)?;
 cypher::planner::CypherPlanner::new().plan(&parsed).map_err(|e|{
  let classification=e.classification().map(|(kind,detail)|json!({"type":kind,"detail":detail,"phase":"compile time"}));
  (e.to_string(),classification)
 })
}
fn term(t:RdfTermValue)->Value{match t{RdfTermValue::Iri(v)=>json!({"type":"uri","value":v}),RdfTermValue::BlankNode(v)=>json!({"type":"bnode","value":v}),RdfTermValue::Literal{lexical,datatype,language}=>json!({"type":"literal","value":lexical,"datatype":datatype,"lang":language})}}
async fn rdf(req:&Value)->Result<Value,String>{
 let fields=["g","s","s_kind","s_dt","s_lang","p","p_kind","p_dt","p_lang","o","o_kind","o_dt","o_lang"];
 let schema=Arc::new(Schema::new(fields.iter().map(|n|Field::new(*n,DataType::Utf8,true)).collect::<Vec<_>>()));
 let batch=RecordBatch::new_empty(schema.clone());
 let mut mapping=RdfDatasetMapping::new();
 mapping.register_table("terms",Arc::new(MemTable::try_new(schema,vec![vec![batch]]).map_err(|e|e.to_string())?)).map_typed_quads("default",IriQuadSource::table("terms","s","p","o").graph_column("g").typed_term_columns(RdfTermColumns::new("s","s_kind").datatype("s_dt").language("s_lang"),RdfTermColumns::new("p","p_kind").datatype("p_dt").language("p_lang"),RdfTermColumns::new("o","o_kind").datatype("o_dt").language("o_lang")));
 let conn=duckdb::Connection::open_in_memory().map_err(|e|e.to_string())?;
 let graphs_schema=Arc::new(Schema::new(vec![Field::new("iri",DataType::Utf8,false)]));
 mapping.register_table("graph_names",Arc::new(MemTable::try_new(graphs_schema.clone(),vec![vec![RecordBatch::new_empty(graphs_schema)]]).map_err(|e|e.to_string())?)).map_named_graphs("default","graph_names","iri");
 conn.execute_batch("CREATE TABLE graph_names(iri VARCHAR PRIMARY KEY)").map_err(|e|e.to_string())?;
 if let Some(names)=req["named_graphs"].as_array(){for name in names{
  conn.execute("INSERT INTO graph_names VALUES (?) ON CONFLICT DO NOTHING",[name.as_str().ok_or("graph name must be an IRI string")?]).map_err(|e|e.to_string())?;
 }}
 conn.execute_batch(&format!("CREATE TABLE terms({});",fields.iter().map(|f|format!("{f} VARCHAR")).collect::<Vec<_>>().join(","))).map_err(|e|e.to_string())?;
 if let Some(rows)=req["quads"].as_array(){for row in rows{
 let values=row.as_array().ok_or("quad row must be array")?.iter().map(|v|v.as_str().map(str::to_string)).collect::<Vec<_>>();
 conn.execute("INSERT INTO terms VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",duckdb::params_from_iter(values)).map_err(|e|e.to_string())?;
 }}
 let mut executor=DuckDbExecutor::from_connection(conn);executor.set_timeouts(std::time::Duration::from_secs(8),std::time::Duration::from_secs(8));
 let mut engine=RdfGraphEngine::new(executor,Arc::new(mapping),"default");
 match engine.query(req["query"].as_str().unwrap_or("")).await? {
 SparqlResults::Boolean(v)=>Ok(json!({"boolean":v})),
 SparqlResults::Solutions{variables,rows}=>Ok(json!({"variables":variables.iter().map(|s|s.trim_start_matches('?')).collect::<Vec<_>>(),"rows":rows.into_iter().map(|r|r.into_iter().map(|v|v.map(term)).collect::<Vec<_>>()).collect::<Vec<_>>()})),
 SparqlResults::Graph(rows)=>Ok(json!({"graph":rows.into_iter().map(|r|r.into_iter().map(term).collect::<Vec<_>>()).collect::<Vec<_>>()}))
 }
}
#[tokio::main]
async fn main(){
 let mut engine=GraphEngine::in_memory().unwrap();engine.set_sql_timeout(std::time::Duration::from_secs(8));
 for line in io::stdin().lock().lines(){
 let req:Value=match serde_json::from_str(&line.unwrap()){Ok(v)=>v,Err(e)=>{println!("{}",json!({"error":e.to_string()}));continue}};
 let op=req["op"].as_str().unwrap_or("cypher");
 let result:Result<Value,String>=match op{
 "fixture"=>fixture_graph(&req).and_then(|graph|engine.replace_graph(graph).map(|_|json!({"ok":true}))),
 "reset"=>{let graph=PropertyGraph::new();graph.enable_null_property_values(req["allow_null_property_values"].as_bool().unwrap_or(false));engine.replace_graph(graph).map(|_|json!({"ok":true}))},
 "rdf"=>rdf(&req).await,
 "register-procedure"=>{
  use new_graph::ir::procedures::{ProcedureField,ProcedureSignature,TableProcedure};
  let fields=|key:&str|req[key].as_array().into_iter().flatten().map(|field|ProcedureField {
   name:field["name"].as_str().unwrap_or("").into(),type_name:field["type"].as_str().unwrap_or("ANY").into(),nullable:field["nullable"].as_bool().unwrap_or(true)
  }).collect();
  let procedure=TableProcedure{signature:ProcedureSignature{inputs:fields("inputs"),outputs:fields("outputs")},
   rows:req["rows"].as_array().into_iter().flatten().map(|row|row.as_array().into_iter().flatten().map(param).collect()).collect()};
  engine.register_table_procedure(req["name"].as_str().unwrap_or("").into(),procedure).map(|_|json!({"ok":true}))
 },
 "sparql-syntax"=>{let q=req["query"].as_str().unwrap_or("");let base=req["base"].as_str();
  if req["update"].as_bool().unwrap_or(false){new_graph::language::sparql::parse_update(q,base).map(|_|json!({"parsed":true})).map_err(|e|e.to_string())}
  else {let parser=spargebra::SparqlParser::new();let parser=if let Some(b)=base{parser.with_base_iri(b).unwrap()}else{parser};parser.parse_query(q).map(|_|json!({"parsed":true})).map_err(|e|e.to_string())}},
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
 r.map(|r|{let b=r.returned.batch;json!({"native_rows":b.schema().metadata().get(&format!("crabgraph.{}.typed_rows.v1",op)).and_then(|v|serde_json::from_str::<Value>(v).ok()),"native_columns":b.schema().metadata().get(&format!("crabgraph.{}.typed_columns.v1",op)).and_then(|v|serde_json::from_str::<Value>(v).ok()),"columns":b.schema().fields().iter().map(|f|f.name()).collect::<Vec<_>>(),"rows":(0..b.num_rows()).map(|i|b.columns().iter().map(|a|cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>(),"typed_rows":if op=="gremlin"{json!((0..b.num_rows()).map(|i|b.columns().iter().map(|a|typed_cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>())}else{Value::Null},"backend":format!("{:?}",r.backend)})}).or_else(|error|Ok(json!({"error":error,"classification":classification})))
 }};
 let mut output=result.unwrap_or_else(|e|json!({"error":e}));output["engine_instance"]=json!(std::process::id().to_string());println!("{output}");io::stdout().flush().unwrap();
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
