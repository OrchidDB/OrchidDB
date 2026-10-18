#![cfg(feature = "duckdb")]
use std::{collections::BTreeMap, sync::Arc};
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::ir::{catalog::PropertyGraph, rel::{mapping::{GraphMapping, NodeMapping, EdgeMapping}, sql::{self, DuckDbExecutor, SqlDialect}, RelBackend, RelBackendOptions}};

fn fixture() -> (GraphMapping, DuckDbExecutor) {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE people(pk BIGINT PRIMARY KEY, name VARCHAR, logical_id VARCHAR, hidden VARCHAR); CREATE TABLE other(pk BIGINT PRIMARY KEY); CREATE TABLE links(pk BIGINT PRIMARY KEY, src BIGINT, dst BIGINT, title VARCHAR)").unwrap();
    let mut mapping = GraphMapping::new();
    for (table, fields) in [
        ("people", vec![("pk", DataType::Int64), ("name", DataType::Utf8), ("logical_id", DataType::Utf8), ("hidden", DataType::Utf8)]),
        ("other", vec![("pk", DataType::Int64)]),
        ("links", vec![("pk", DataType::Int64), ("src", DataType::Int64), ("dst", DataType::Int64), ("title", DataType::Utf8)]),
    ] {
        mapping.register_table_schema(table, Arc::new(Schema::new(fields.into_iter().map(|(name, ty)| Field::new(name, ty, name != "pk")).collect::<Vec<_>>())));
    }
    mapping.map_node(NodeMapping::table("Person", "people", "pk").property("name", "name").property("id", "logical_id"));
    mapping.map_node(NodeMapping::table("Other", "other", "pk"));
    mapping.map_edge(EdgeMapping::table("KNOWS", "links", "src", "dst", "Person", "Person").with_id("pk").property("title", "title"));
    (mapping, DuckDbExecutor::from_connection(connection))
}

async fn prepared(mapping: &GraphMapping, query: &str) -> sql::PreparedSql {
    let plan = orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), None).unwrap();
    let lowered = RelBackend::with_options(RelBackendOptions { mapping: Some(Arc::new(mapping.clone())), language_functions: true, ..Default::default() }).lower(&plan, &PropertyGraph::new()).unwrap();
    sql::prepare_with_external(&lowered, SqlDialect::DuckDb, &mapping.physical_table_names()).await.unwrap()
}
fn result(executor: &mut DuckDbExecutor, query: &sql::PreparedSql) -> Vec<String> {
    let result = sql::execute_prepared(executor, query).unwrap_or_else(|e| panic!("{e}\n{}",query.query));
    let mut rows = (0..result.batch.num_rows()).map(|row| result.batch.columns().iter().map(|column| arrow::util::display::array_value_to_string(column.as_ref(),row).unwrap()).collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
    rows.sort(); rows
}
#[tokio::test]
async fn declared_node_keys_remain_valid_when_empty_tables_are_populated() {
    let (mapping, mut executor) = fixture();
    let query = prepared(&mapping,"MATCH (n) RETURN keys(n)").await;
    assert!(result(&mut executor,&query).is_empty());
    executor.connection().unwrap().execute_batch("INSERT INTO people VALUES (1,'Alice','public-id','secret'),(2,NULL,NULL,'secret'),(3,'Bob',NULL,NULL); INSERT INTO other VALUES (4)").unwrap();
    assert_eq!(result(&mut executor,&query), ["[]", "[]", "[id, name]", "[name]"]);
    let query = prepared(&mapping,"MATCH (n:Person) UNWIND keys(n) AS key RETURN DISTINCT key").await;
    assert_eq!(result(&mut executor,&query), ["id", "name"]);
}
#[tokio::test]
async fn relationship_keys_and_optional_missing_values_keep_null_semantics() {
    let (mapping, mut executor) = fixture();
    let missing_node = prepared(&mapping,"OPTIONAL MATCH (n:Person) RETURN keys(n)").await;
    let missing_edge = prepared(&mapping,"OPTIONAL MATCH ()-[r:KNOWS]->() RETURN keys(r)").await;
    assert_eq!(result(&mut executor,&missing_node), [""]);
    assert_eq!(result(&mut executor,&missing_edge), [""]);
    executor.connection().unwrap().execute_batch("INSERT INTO people VALUES (1,NULL,NULL,NULL),(2,NULL,NULL,NULL); INSERT INTO links VALUES (10,1,2,'friend'),(11,2,1,NULL)").unwrap();
    let query = prepared(&mapping,"MATCH ()-[r:KNOWS]->() RETURN keys(r)").await;
    assert_eq!(result(&mut executor,&query), ["[]", "[title]"]);
    let query = prepared(&mapping,"MATCH (n:Person) RETURN 'name' IN keys(n)").await;
    assert_eq!(result(&mut executor,&query), ["false", "false"]);
}
