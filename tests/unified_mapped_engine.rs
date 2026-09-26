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
async fn mapped_scalar_keys_use_shared_native_traversal() {
    for (kind, a, b) in [
        ("VARCHAR", "'alpha'", "'beta'"),
        ("BOOLEAN", "false", "true"),
        ("UBIGINT", "18446744073709551614", "18446744073709551615"),
        (
            "DECIMAL(30,4)",
            "12345678901234567890.1234",
            "12345678901234567890.5678",
        ),
        ("BLOB", "from_hex('ff00')", "from_hex('00ff')"),
        ("DATE", "DATE '2020-01-01'", "DATE '2020-01-02'"),
        (
            "TIMESTAMP",
            "TIMESTAMP '2020-01-01 01:02:03'",
            "TIMESTAMP '2020-01-02 01:02:03'",
        ),
        ("DOUBLE", "1.25", "2.5"),
    ] {
        let connection = duckdb::Connection::open_in_memory().unwrap();
        connection.execute_batch(&format!("CREATE TABLE nodes(key {kind} PRIMARY KEY,name VARCHAR); CREATE TABLE edges(key {kind} PRIMARY KEY,src {kind},dst {kind}); INSERT INTO nodes VALUES ({a},'a'),({b},'b'); INSERT INTO edges VALUES ({a},{a},{b});")).unwrap();
        let mut engine = GraphEngine::mapped(connection, mapping()).unwrap();
        for query in [
            "MATCH (a:N)-[:E]->(b:N) RETURN a.name,b.name",
            "MATCH (a:N)-[:E*1..3]->(b:N) RETURN a.name,b.name",
        ] {
            let result = engine
                .cypher(query)
                .await
                .unwrap_or_else(|e| panic!("{kind}: {e}"));
            assert_eq!(result.returned.batch.num_rows(), 1, "{kind}");
            assert_eq!(
                array_value_to_string(result.returned.batch.column(0), 0).unwrap(),
                "a"
            );
            assert_eq!(
                array_value_to_string(result.returned.batch.column(1), 0).unwrap(),
                "b"
            );
        }
        let repeated = engine
            .gremlin("g.V().has('name','a').repeat(out('E')).times(1).values('name')")
            .await
            .unwrap();
        assert_eq!(repeated.returned.batch.num_rows(), 1);
        assert_eq!(
            array_value_to_string(repeated.returned.batch.column(0), 0).unwrap(),
            "b"
        );
        let keys = engine.cypher("MATCH (n:N) RETURN id(n)").await.unwrap();
        assert_eq!(keys.returned.batch.num_rows(), 2);
        engine
            .cypher("MATCH (n:N {name:'a'}) SET n.name='updated'")
            .await
            .unwrap();
        assert!(engine.cypher("MATCH (n:N) SET n.unmapped=1").await.is_err());
        let count = engine
            .cypher("MATCH (n:N {name:'updated'}) RETURN count(n)")
            .await
            .unwrap();
        assert_eq!(
            array_value_to_string(count.returned.batch.column(0), 0).unwrap(),
            "1"
        );
    }
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
async fn gremlin_supplies_scalar_primary_keys_without_property_aliases() {
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("N", "nodes", "key").property("name", "name"));
    mapping.map_edge(EdgeMapping::table("E", "edges", "src", "dst", "N", "N").with_id("key"));
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(mapping));
    engine.execute_sql("CREATE TABLE nodes(key VARCHAR PRIMARY KEY,name VARCHAR); CREATE TABLE edges(key VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR)").unwrap();
    engine
        .gremlin("g.addV('N').property(T.id,'alpha').property('name','a')")
        .await
        .unwrap();
    engine
        .gremlin("g.addV('N').property(T.id,'beta').property('name','b')")
        .await
        .unwrap();
    engine
        .gremlin("g.V('alpha').as('a').V('beta').addE('E').from('a').property(T.id,'edge')")
        .await
        .unwrap();
    let key: String = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row(
            "SELECT key FROM edges WHERE src='alpha' AND dst='beta'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(key, "edge");
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
async fn mapped_defaults_and_external_changes_share_statement_state() {
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), mapping());
    engine.execute_sql("CREATE TABLE nodes(key VARCHAR PRIMARY KEY,name VARCHAR DEFAULT 'default'); CREATE TABLE edges(key VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR)").unwrap();
    let result = engine
        .cypher("CREATE (n:N {key:'alpha'}) RETURN n.name")
        .await
        .unwrap();
    assert_eq!(
        array_value_to_string(result.batch.column(0), 0).unwrap(),
        "default"
    );
    engine
        .execute_sql("UPDATE nodes SET name='external' WHERE key='alpha'")
        .unwrap();
    let result = engine.gremlin("g.V('alpha').values('name')").await.unwrap();
    assert_eq!(
        array_value_to_string(result.batch.column(0), 0).unwrap(),
        "external"
    );
    engine
        .execute_sql("ALTER TABLE nodes ALTER COLUMN name SET DEFAULT uuid()::VARCHAR")
        .unwrap();
    assert!(engine.cypher("CREATE (:N {key:'beta'})").await.is_err());
    let count: i64 = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row("SELECT count(*) FROM nodes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1);
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
async fn compatibility_api_uses_shared_frontend_validation_and_bindings() {
    use orchiddb::{
        ir::Value,
        language::gremlin::{GremlinBinding, semantics::GValue},
    };
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), mapping());
    engine.execute_sql("CREATE TABLE nodes(key VARCHAR PRIMARY KEY,name VARCHAR); CREATE TABLE edges(key VARCHAR PRIMARY KEY,src VARCHAR,dst VARCHAR); INSERT INTO nodes VALUES ('alpha','a')").unwrap();
    let parameters = std::collections::BTreeMap::from([(
        "props".into(),
        Value::Map(std::collections::BTreeMap::from([(
            "name".into(),
            Value::String("a".into()),
        )])),
    )]);
    let error = engine
        .cypher_with_params("MATCH (n:N $props) RETURN n", &parameters)
        .await
        .unwrap_err();
    assert!(
        error.contains("MATCH and MERGE patterns require explicit property keys"),
        "{error}"
    );
    let bindings = std::collections::HashMap::from([(
        "key".into(),
        GremlinBinding::Value(GValue::String("alpha".into())),
    )]);
    let result = engine
        .gremlin_with_bindings("g.V(key).values('name')", &bindings)
        .await
        .unwrap();
    assert_eq!(
        array_value_to_string(result.batch.column(0), 0).unwrap(),
        "a"
    );
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
