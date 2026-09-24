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
fn term(t:RdfTermValue)->Value{match t{RdfTermValue::Iri(v)=>json!({"type":"uri","value":v}),RdfTermValue::BlankNode(v)=>json!({"type":"bnode","value":v}),RdfTermValue::Literal{lexical,datatype,language}=>json!({"type":"literal","value":lexical,"datatype":datatype,"lang":language})}}
async fn rdf(req:&Value)->Result<Value,String>{
 let fields=["g","s","s_kind","s_dt","s_lang","p","p_kind","p_dt","p_lang","o","o_kind","o_dt","o_lang"];
 let schema=Arc::new(Schema::new(fields.iter().map(|n|Field::new(*n,DataType::Utf8,true)).collect::<Vec<_>>()));
 let batch=RecordBatch::new_empty(schema.clone());
 let mut mapping=RdfDatasetMapping::new();
 mapping.register_table("terms",Arc::new(MemTable::try_new(schema,vec![vec![batch]]).map_err(|e|e.to_string())?)).map_typed_quads("default",IriQuadSource::table("terms","s","p","o").graph_column("g").typed_term_columns(RdfTermColumns::new("s","s_kind").datatype("s_dt").language("s_lang"),RdfTermColumns::new("p","p_kind").datatype("p_dt").language("p_lang"),RdfTermColumns::new("o","o_kind").datatype("o_dt").language("o_lang")));
 let conn=duckdb::Connection::open_in_memory().map_err(|e|e.to_string())?;
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
 "fixture"=>{
 let graph=PropertyGraph::new();let mut nodes=BTreeMap::new();
 for n in req["nodes"].as_array().unwrap(){let props=n["properties"].as_object().unwrap().iter().map(|(k,v)|(k.clone(),param(v))).collect();nodes.insert(n["id"].to_string(),graph.insert_node(n["label"].as_str().unwrap(),props));}
 let mut error=None;
 for e in req["edges"].as_array().unwrap(){let props=e["properties"].as_object().unwrap().iter().map(|(k,v)|(k.clone(),param(v))).collect();if let Err(err)=graph.insert_edge(e["label"].as_str().unwrap(),&nodes[&e["src"].to_string()],&nodes[&e["dst"].to_string()],props){error=Some(err.to_string());break;}}
 if let Some(err)=error{Err(err)}else{engine.replace_graph(graph).map(|_|json!({"ok":true}))}
 },
 "reset"=>engine.replace_graph(PropertyGraph::new()).map(|_|json!({"ok":true})),
 "rdf"=>rdf(&req).await,
 "sparql-syntax"=>{let q=req["query"].as_str().unwrap_or("");let base=req["base"].as_str();let parser=spargebra::SparqlParser::new();let parser=if let Some(b)=base{parser.with_base_iri(b).unwrap()}else{parser};parser.parse_query(q).map(|_|json!({"parsed":true})).map_err(|e|e.to_string())},
 _=>{
 let params=req["params"].as_object().map(|m|m.iter().map(|(k,v)|(k.clone(),param(v))).collect()).unwrap_or_default();
 let q=req["query"].as_str().unwrap_or("");
 let r=if op=="gremlin"{engine.gremlin(q).await}else{engine.cypher_with_params(q,&params).await};
 r.map(|r|{let b=r.returned.batch;json!({"columns":b.schema().fields().iter().map(|f|f.name()).collect::<Vec<_>>(),"rows":(0..b.num_rows()).map(|i|b.columns().iter().map(|a|cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>(),"typed_rows":if op=="gremlin"{json!((0..b.num_rows()).map(|i|b.columns().iter().map(|a|typed_cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>())}else{Value::Null},"backend":format!("{:?}",r.backend)})})
 }};
 let output=result.unwrap_or_else(|e|json!({"error":e}));println!("{output}");io::stdout().flush().unwrap();
 }
}
