#![cfg(feature = "duckdb")]
use std::{collections::BTreeMap, sync::Arc};
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::ir::{catalog::PropertyGraph, rel::{mapping::{GraphMapping, NodeMapping, EdgeMapping}, sql::{self, DuckDbExecutor, SqlDialect}, RelBackend, RelBackendOptions}};

#[tokio::test]
async fn distinct_graph_ordering_preserves_qualified_columns_and_topk() {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE people(pk BIGINT PRIMARY KEY, id BIGINT, name VARCHAR); INSERT INTO people VALUES (1,30,'Z'),(2,10,'A'),(3,20,NULL); CREATE TABLE links(pk BIGINT PRIMARY KEY, src BIGINT, dst BIGINT); INSERT INTO links VALUES (1,1,2),(2,1,2),(3,1,3)").unwrap();
    let mut mapping = GraphMapping::new();
    for (table, fields) in [
        ("people", vec![("pk", DataType::Int64), ("id", DataType::Int64), ("name", DataType::Utf8)]),
        ("links", vec![("pk", DataType::Int64), ("src", DataType::Int64), ("dst", DataType::Int64)]),
    ] {
        mapping.register_table_schema(table, Arc::new(Schema::new(fields.into_iter().map(|(n,t)| Field::new(n,t,n!="pk")).collect::<Vec<_>>())));
    }
    mapping.map_node(NodeMapping::table("Person","people","pk").property("id","id").property("name","name"));
    mapping.map_edge(EdgeMapping::table("LINK","links","src","dst","Person","Person").with_id("pk"));
    let mut executor = DuckDbExecutor::from_connection(connection);
    for (query, count, values) in [
        ("MATCH (a) RETURN DISTINCT a ORDER BY a.name",3,None),
        ("MATCH (a)-->(b) RETURN DISTINCT b ORDER BY b.name",2,None),
        ("MATCH (n) RETURN DISTINCT n ORDER BY n.id",3,None),
        ("MATCH (n) WITH DISTINCT n ORDER BY n.id RETURN n.id",3,Some(vec!["10","20","30"])),
        ("MATCH (n) WITH DISTINCT n ORDER BY n.id SKIP 1 LIMIT 1 RETURN n.id",1,Some(vec!["20"])),
        ("MATCH (n) WITH DISTINCT n ORDER BY n.name DESC RETURN n.id",3,Some(vec!["20","30","10"])),
    ] {
        let plan = orchiddb::language::cypher::preparation::prepare(query,&BTreeMap::new(),None).unwrap();
        let lowered = RelBackend::with_options(RelBackendOptions {mapping:Some(Arc::new(mapping.clone())),language_functions:true,..Default::default()}).lower(&plan,&PropertyGraph::new()).unwrap();
        let prepared = sql::prepare_with_external(&lowered,SqlDialect::DuckDb,&mapping.physical_table_names()).await.unwrap_or_else(|e|panic!("{query}: {e}"));
        let result = sql::execute_prepared(&mut executor,&prepared).unwrap_or_else(|e|panic!("{query}: {e}\n{}",prepared.query));
        assert_eq!(result.batch.num_rows(),count,"{query}");
        if let Some(values)=values {
            let actual=(0..count).map(|i|arrow::util::display::array_value_to_string(result.batch.column(0),i).unwrap()).collect::<Vec<_>>();
            assert_eq!(actual,values,"{query}\n{}",prepared.query);
        }
    }
}
