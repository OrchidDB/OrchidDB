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

#[tokio::test]
async fn query_relationship_mappings_preserve_parallel_edges_and_reject_writes() {
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("N", "nodes", "key").property("name", "name"));
    // Query mappings declare the schemas of their SQL dependencies explicitly.
    mapping.register_table_schema("edges", Arc::new(arrow::datatypes::Schema::new(
        ["key", "src", "dst", "weight"].into_iter().map(|name| arrow::datatypes::Field::new(name, arrow::datatypes::DataType::Int64, true)).collect::<Vec<_>>()
    )));
    mapping.map_edge(EdgeMapping::query("E", "SELECT key,src,dst,weight FROM edges WHERE weight >= 3", "src", "dst", "N", "N")
        .with_id("key").property("weight", "weight"));
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(mapping));
    engine.execute_sql("CREATE TABLE nodes(key BIGINT PRIMARY KEY,name VARCHAR); INSERT INTO nodes VALUES (1,'a'),(2,'b'),(3,'c'); CREATE TABLE edges(key BIGINT PRIMARY KEY,src BIGINT,dst BIGINT,weight BIGINT); INSERT INTO edges VALUES (10,1,2,3),(11,1,2,5),(12,2,3,7),(13,1,3,0)").unwrap();
    for (query, expected) in [
        ("MATCH (a:N)-[e:E]->(b:N) RETURN a.name,b.name,e.weight ORDER BY a.name,b.name,e.weight", vec!["a|b|3", "a|b|5", "b|c|7"]),
        ("MATCH (b:N {name:'b'})<-[e:E]-(a:N) RETURN a.name,e.weight ORDER BY e.weight", vec!["a|3", "a|5"]),
    ] {
        let result = engine.cypher(query).await.unwrap_or_else(|error| panic!("{query}: {error}"));
        let actual = (0..result.batch.num_rows()).map(|row| result.batch.columns().iter()
            .map(|column| array_value_to_string(column.as_ref(), row).unwrap()).collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
        assert_eq!(actual, expected, "{query}");
    }
    for query in ["MATCH ()-[e:E]->() SET e.weight=99", "MATCH ()-[e:E]->() DELETE e"] {
        let error = engine.cypher(query).await.unwrap_err();
        assert!(error.contains("read-only"), "{query}: {error}");
    }
    let total: i64 = engine.executor_mut().connection().unwrap()
        .query_row("SELECT sum(weight) FROM edges", [], |row| row.get(0)).unwrap();
    assert_eq!(total, 15);
}
