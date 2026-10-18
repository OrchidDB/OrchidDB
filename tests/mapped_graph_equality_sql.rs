#![cfg(feature = "duckdb")]
use std::{collections::BTreeMap, sync::Arc};
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::ir::{catalog::PropertyGraph, rel::{mapping::{GraphMapping, NodeMapping, EdgeMapping}, sql::{self, DuckDbExecutor, SqlDialect}, RelBackend, RelBackendOptions}};

fn fixture() -> (GraphMapping, DuckDbExecutor) {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE nodes(pk BIGINT PRIMARY KEY); INSERT INTO nodes VALUES (1),(2); CREATE TABLE other(pk BIGINT PRIMARY KEY); INSERT INTO other VALUES (1); CREATE TABLE edges(pk BIGINT PRIMARY KEY, src BIGINT, dst BIGINT); INSERT INTO edges VALUES (1,1,2),(2,1,2)").unwrap();
    let mut mapping = GraphMapping::new();
    for (table, fields) in [
        ("nodes", vec!["pk"]), ("other", vec!["pk"]), ("edges", vec!["pk","src","dst"]),
    ] {
        mapping.register_table_schema(table, Arc::new(Schema::new(fields.into_iter().map(|name| Field::new(name, DataType::Int64, false)).collect::<Vec<_>>())));
    }
    mapping.map_node(NodeMapping::table("N","nodes","pk"));
    mapping.map_node(NodeMapping::table("Other","other","pk"));
    mapping.map_edge(EdgeMapping::table("T","edges","src","dst","N","N").with_id("pk"));
    mapping.map_edge(EdgeMapping::table("U","edges","src","dst","N","N").with_id("pk"));
    (mapping,DuckDbExecutor::from_connection(connection))
}
async fn run(mapping: &GraphMapping, executor: &mut DuckDbExecutor, query: &str) -> Vec<String> {
    let plan = orchiddb::language::cypher::preparation::prepare(query,&BTreeMap::new(),None).unwrap();
    let lowered = RelBackend::with_options(RelBackendOptions { mapping:Some(Arc::new(mapping.clone())), language_functions:true, ..Default::default() }).lower(&plan,&PropertyGraph::new()).unwrap();
    let prepared = sql::prepare_with_external(&lowered,SqlDialect::DuckDb,&mapping.physical_table_names()).await.unwrap();
    assert!(!prepared.query.contains("_ID:"),"{query}: {}",prepared.query);
    let result = sql::execute_prepared(executor,&prepared).unwrap_or_else(|e|panic!("{query}: {e}\n{}",prepared.query));
    let mut rows=(0..result.batch.num_rows()).map(|row| result.batch.columns().iter().map(|col|arrow::util::display::array_value_to_string(col,row).unwrap()).collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
    rows.sort();rows
}
#[tokio::test]
async fn mapped_graph_equality_compares_typed_keys_and_labels() {
    let (mapping,mut executor)=fixture();
    for (query,expected) in [
        ("MATCH (a:N), (b:N) RETURN a = b AS eq",vec!["false","false","true","true"]),
        ("MATCH (a:N), (b:N) WHERE a = b RETURN count(*) AS n",vec!["2"]),
        ("MATCH (a:N), (b:Other) RETURN a = b AS eq",vec!["false","false"]),
        ("MATCH ()-[r:T]->() MATCH ()-[s:T]->() RETURN r = s AS eq",vec!["false","false","true","true"]),
        ("MATCH ()-[r:T]->() MATCH ()-[s:U]->() RETURN r = s AS eq",vec!["false","false","false","false"]),
        ("MATCH (a:N)-[r:T]->() RETURN a = r AS eq",vec!["false","false"]),
        ("MATCH (a:N), (b:N) RETURN a <> b AS neq",vec!["false","false","true","true"]),
    ] { assert_eq!(run(&mapping,&mut executor,query).await,expected,"{query}"); }
}
#[tokio::test]
async fn missing_mapped_elements_keep_unknown_equality() {
    let (mapping,mut executor)=fixture();
    for query in [
        "MATCH (a:Other) OPTIONAL MATCH (a)-[r:T]->() RETURN a = r AS eq, a <> r AS neq",
        "MATCH (a:Other) OPTIONAL MATCH (a)-[r:T]->() RETURN r = r AS eq, r <> r AS neq",
    ] { assert_eq!(run(&mapping,&mut executor,query).await,vec!["|"],"{query}"); }
}
