#![cfg(feature = "duckdb")]
use arrow::util::display::array_value_to_string;
use orchiddb::{
    engine::GraphEngine,
    ir::rel::{
        mapping::{EdgeMapping, GraphMapping, NodeMapping},
        sql::DuckDbExecutor,
    },
    mapped_engine::MappedGraphEngine,
};
use std::sync::Arc;
fn mapping() -> Arc<GraphMapping> {
    let mut mapping = GraphMapping::new();
    mapping.map_node(
        NodeMapping::table("N", "nodes", "key")
            .property("key", "key")
            .property("name", "name"),
    );
    mapping.map_edge(
        EdgeMapping::table("E", "edges", "src", "dst", "N", "N")
            .with_id("key")
            .property("key", "key"),
    );
    Arc::new(mapping)
}

#[tokio::test]
async fn mapped_writes_keep_source_keys_and_executor_transactions() {
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), mapping());
    engine.execute_sql("CREATE TABLE nodes(key VARCHAR PRIMARY KEY,name VARCHAR); CREATE TABLE edges(key VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR)").unwrap();
    engine
        .cypher(
            "CREATE (a:N {key:'alpha',name:'a'})-[:E {key:'edge'}]->(b:N {key:'beta',name:'b'})",
        )
        .await
        .unwrap();
    let count: i64 = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM nodes WHERE key IN ('alpha','beta')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
    engine.executor_mut().begin().unwrap();
    engine
        .cypher("MATCH (n:N {key:'alpha'}) SET n.name='changed'")
        .await
        .unwrap();
    engine.executor_mut().rollback().unwrap();
    let name: String = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row("SELECT name FROM nodes WHERE key='alpha'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "a");
    assert!(
        engine
            .cypher("CREATE (:N {name:'missing key'})")
            .await
            .is_err()
    );
    engine
        .cypher("MATCH (n:N {key:'alpha'}) DETACH DELETE n")
        .await
        .unwrap();
    let count: i64 = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row("SELECT count(*) FROM edges", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}



#[tokio::test]
async fn scalar_parameters_create_full_width_unsigned_keys() {
    use datafusion::common::ScalarValue;
    use orchiddb::ir::Value;
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), mapping());
    engine.execute_sql("CREATE TABLE nodes(key UBIGINT PRIMARY KEY,name VARCHAR); CREATE TABLE edges(key UBIGINT PRIMARY KEY,src UBIGINT,dst UBIGINT)").unwrap();
    let params = std::collections::BTreeMap::from([(
        "key".into(),
        Value::Scalar(ScalarValue::UInt64(Some(u64::MAX))),
    )]);
    let result = engine
        .cypher_with_params("CREATE (n:N {key:$key,name:'max'}) RETURN id(n)", &params)
        .await
        .unwrap();
    assert_eq!(
        result
            .batch
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::UInt64Array>()
            .unwrap()
            .value(0),
        u64::MAX
    );
    let result = engine
        .cypher_with_params("MATCH (n:N) WHERE id(n)=$key RETURN n.name", &params)
        .await
        .unwrap();
    assert_eq!(result.batch.num_rows(), 1);
}



#[tokio::test]
async fn query_mappings_are_readable_and_reject_writes() {
    let mut mapping = GraphMapping::new();
    mapping.map_node(
        NodeMapping::query("N", "SELECT key,name FROM nodes", "key").property("name", "name"),
    );
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(mapping));
    engine.execute_sql("CREATE TABLE nodes(key VARCHAR PRIMARY KEY,name VARCHAR); INSERT INTO nodes VALUES ('alpha','a')").unwrap();
    let result = engine.cypher("MATCH (n:N) RETURN n.name").await.unwrap();
    assert_eq!(result.batch.num_rows(), 1);
    let error = engine
        .cypher("MATCH (n:N) SET n.name='changed'")
        .await
        .unwrap_err();
    assert!(error.contains("read-only"), "{error}");
    let name: String = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row("SELECT name FROM nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(name, "a");
}



#[tokio::test]
async fn mapped_refresh_preserves_registered_procedures() {
    use orchiddb::ir::{
        Value,
        procedures::{ProcedureField, ProcedureSignature, TableProcedure},
    };
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE nodes(key VARCHAR PRIMARY KEY,name VARCHAR); CREATE TABLE edges(key VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR)").unwrap();
    let mut engine = GraphEngine::mapped(connection, mapping()).unwrap();
    engine
        .register_table_procedure(
            "custom.value".into(),
            TableProcedure {
                signature: ProcedureSignature {
                    inputs: vec![],
                    outputs: vec![ProcedureField {
                        name: "answer".into(),
                        type_name: "STRING".into(),
                        nullable: false,
                    }],
                },
                rows: vec![vec![Value::String("kept".into())]],
            },
        )
        .unwrap();
    for create in [false, true] {
        if create {
            engine.cypher("CREATE (:N {key:'alpha'})").await.unwrap();
        }
        let result = engine
            .cypher("CALL custom.value() YIELD answer RETURN answer")
            .await
            .unwrap();
        assert_eq!(
            array_value_to_string(result.returned.batch.column(0), 0).unwrap(),
            "kept"
        );
    }
}
