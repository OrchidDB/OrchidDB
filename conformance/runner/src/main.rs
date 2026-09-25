use std::io::{self, BufRead, Write};
use arrow::array::*;
use arrow::datatypes::DataType;
use orchiddb::engine::{GraphEngine, ReadMode};
use orchiddb::language::sparql::OntologyMapping;
use orchiddb::ir::plan::Direction;
use serde_json::{Value, json};

fn cell(a: &dyn Array, i: usize) -> Value {
    if a.is_null(i) { return Value::Null; }
    macro_rules! typed { ($t:ty) => { json!(a.as_any().downcast_ref::<$t>().unwrap().value(i)) }; }
    match a.data_type() {
        DataType::Boolean => typed!(BooleanArray),
        DataType::Int8 => typed!(Int8Array), DataType::Int16 => typed!(Int16Array),
        DataType::Int32 => typed!(Int32Array), DataType::Int64 => typed!(Int64Array),
        DataType::UInt8 => typed!(UInt8Array), DataType::UInt16 => typed!(UInt16Array),
        DataType::UInt32 => typed!(UInt32Array), DataType::UInt64 => typed!(UInt64Array),
        DataType::Float32 => typed!(Float32Array), DataType::Float64 => typed!(Float64Array),
        DataType::Utf8 => typed!(StringArray), DataType::LargeUtf8 => typed!(LargeStringArray),
        DataType::List(_) => { let v=a.as_any().downcast_ref::<ListArray>().unwrap().value(i); json!((0..v.len()).map(|j|cell(v.as_ref(),j)).collect::<Vec<_>>()) },
        _ => json!({"arrow_type":a.data_type().to_string(),"display":arrow::util::display::array_value_to_string(a,i).unwrap_or_default()})
    }
}
#[tokio::main]
async fn main() {
    let mut graph=GraphEngine::in_memory().unwrap();
    graph.set_sql_timeout(std::time::Duration::from_secs(10));
    if std::env::var("CONFORMANCE_READ_MODE").as_deref()==Ok("sql-only") { graph.set_read_mode(ReadMode::SqlOnly); }
    let ontology=OntologyMapping::new().class("https://example.com/Person","Person")
      .property("https://example.com/name","Person","name")
      .property("https://example.com/age","Person","age")
      .relationship_between("https://example.com/knows","KNOWS",Direction::Out,"Person","Person");
    for line in io::stdin().lock().lines() {
        let line=line.unwrap();
        let request:Value=match serde_json::from_str(&line) { Ok(x)=>x,Err(e)=>{println!("{}",json!({"error":e.to_string()}));continue;} };
        let q=request["query"].as_str().unwrap_or("");
        let mutates=request["mutates"].as_bool().unwrap_or(false);
        if mutates { let _=graph.begin(); }
        let result=match request["language"].as_str().unwrap_or("cypher") {
          "gremlin"=>graph.gremlin(q).await,
          "sparql"=>graph.sparql(q,ontology.clone()).await,
          _=>graph.cypher(q).await,
        };
        let output=match result {
          Ok(r)=>{let b=r.returned.batch;json!({"rows":(0..b.num_rows()).map(|i|b.columns().iter().map(|a|cell(a.as_ref(),i)).collect::<Vec<_>>()).collect::<Vec<_>>(),"backend":format!("{:?}",r.backend)})},
          Err(e)=>json!({"error":e}),
        };
        if mutates { let _=graph.rollback(); }
        println!("{output}");io::stdout().flush().unwrap();
    }
}
