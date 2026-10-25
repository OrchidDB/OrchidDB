//! Public JSON functions, literals, graph predicates and transport share one type.
use arrow::{
    array::{Array, ArrayRef, Int64Array, RecordBatch},
    datatypes::{DataType, Field, Schema},
};
use datafusion::{common::ScalarValue, datasource::MemTable};
use orchiddb::{
    ir::{
        catalog::PropertyGraph,
        functions::domain,
        rel::{LoweredPlan, RelBackend, RelBackendOptions, execute_lowered, mapping::GraphMapping},
    },
    language::cypher::{parse_query, planner::CypherPlanner},
};
use std::sync::Arc;

fn lower(mapping: GraphMapping, query: &str) -> LoweredPlan {
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping)),
        ..Default::default()
    })
    .lower(&plan, &PropertyGraph::new())
    .unwrap()
}
fn json_column(values: &[Option<&str>]) -> ArrayRef {
    ScalarValue::iter_to_array(values.iter().map(|value| match value {
        Some(text) => domain::json_scalar(text).unwrap(),
        None => ScalarValue::try_from(&domain::json_type()).unwrap(),
    }))
    .unwrap()
}
fn mapping() -> GraphMapping {
    let mut mapping = GraphMapping::from_toml(
        r#"
[node.Document]
table = "documents"
id = "id"
[node.Document.properties]
id = "id"
payload = "payload"
[node.Requirement]
table = "requirements"
id = "id"
[node.Requirement.properties]
id = "id"
criteria = "criteria"
[edge.MATCHES]
source = "Document"
target = "Requirement"
predicate = "json.contains(source.payload, target.criteria)"
"#,
    )
    .unwrap();
    for (table, property, values) in [
        (
            "documents",
            "payload",
            vec![
                Some(r#"{"category":"book","tags":["graph","sql"]}"#),
                Some(r#"{"category":"film"}"#),
                None,
            ],
        ),
        (
            "requirements",
            "criteria",
            vec![
                Some(r#"{"category":"book"}"#),
                Some(r#"{"tags":["sql"]}"#),
                Some(r#"{"category":"music"}"#),
            ],
        ),
    ] {
        let batch = RecordBatch::try_from_iter(vec![
            ("id", Arc::new(Int64Array::from(vec![1, 2, 3])) as ArrayRef),
            (property, json_column(&values)),
        ])
        .unwrap();
        mapping.register_table(
            table,
            Arc::new(MemTable::try_new(batch.schema(), vec![vec![batch]]).unwrap()),
        );
    }
    mapping
}
#[tokio::test]
async fn json_literal_and_public_functions_execute_without_sql_backend() {
    let output = execute_lowered(lower(GraphMapping::new(), r#"RETURN json.value(JSON '{"author":{"name":"Ada"},"n":7}', '$.author.name', 'string') AS name, json.value(JSON '{"n":7}', '$.n', 'int64') AS n, JSON 'null' AS j, null AS s"#)).await.unwrap();
    assert_eq!(
        ScalarValue::try_from_array(output.batch.column(0), 0).unwrap(),
        ScalarValue::Utf8(Some("Ada".into()))
    );
    assert_eq!(
        ScalarValue::try_from_array(output.batch.column(1), 0).unwrap(),
        ScalarValue::Int64(Some(7))
    );
    assert!(domain::is_json(output.batch.column(2).data_type()));
    assert!(!output.batch.column(2).is_null(0));
    assert!(
        ScalarValue::try_from_array(output.batch.column(3), 0)
            .unwrap()
            .is_null()
    );
}
#[tokio::test]
async fn containment_relationship_uses_explicit_function_and_reverses_edges() {
    for query in [
        "MATCH (d:Document)-[:MATCHES]->(r:Requirement) RETURN d.id, r.id ORDER BY r.id",
        "MATCH (r:Requirement)<-[:MATCHES]-(d:Document) RETURN d.id, r.id ORDER BY r.id",
    ] {
        let output = execute_lowered(lower(mapping(), query)).await.unwrap();
        assert_eq!(output.batch.num_rows(), 2);
        let ids = output
            .batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(ids.values().as_ref(), &[1, 1]);
        let ids = output
            .batch
            .column(1)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap();
        assert_eq!(ids.values().as_ref(), &[1, 2]);
    }
}
#[tokio::test]
async fn compiler_accepts_json_schema_and_keeps_literals_typed() {
    for dialect in ["postgres", "duckdb"] {
        let request = serde_json::json!({"version":1,"dialect":dialect,"language":"cypher",
            "query":"MATCH (d:Document) RETURN json.value(d.payload, '$.category', 'string') AS category",
            "tables":[{"name":"documents","columns":[{"name":"id","data_type":"int64"},{"name":"payload","data_type":"json"}]}],
            "nodes":[{"label":"Document","table":"documents","id":"id","properties":{"payload":"payload"}}]});
        let output = orchiddb::compiler::compile_json(&request.to_string())
            .await
            .unwrap();
        let response: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(
            !response["sql"]
                .as_str()
                .unwrap()
                .contains("__orchiddb_json_value"),
            "{output}"
        );
    }
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn compiled_containment_relationship_executes_in_duckdb() {
    let request = serde_json::json!({"version":1,"dialect":"duckdb","language":"cypher",
        "query":"MATCH (d:Document)-[:MATCHES]->(r:Requirement) RETURN d.id, r.id ORDER BY d.id, r.id",
        "tables":[
            {"name":"documents","columns":[{"name":"id","data_type":"int64"},{"name":"payload","data_type":"json"}]},
            {"name":"requirements","columns":[{"name":"id","data_type":"int64"},{"name":"criteria","data_type":"json"}]}],
        "nodes":[
            {"label":"Document","table":"documents","id":"id","properties":{"id":"id","payload":"payload"}},
            {"label":"Requirement","table":"requirements","id":"id","properties":{"id":"id","criteria":"criteria"}}],
        "computed_relationships":[{"name":"MATCHES","source":"Document","target":"Requirement","predicate":"json.contains(source.payload, target.criteria)"}]});
    let output = orchiddb::compiler::compile_json(&request.to_string())
        .await
        .unwrap();
    let response: serde_json::Value = serde_json::from_str(&output).unwrap();
    let sql = response["sql"].as_str().unwrap();
    assert!(!sql.contains("__orchiddb_json_"), "{sql}");
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch(r#"CREATE TABLE documents(id BIGINT, payload JSON);
        INSERT INTO documents VALUES (1,'{"tags":["graph","sql"]}'),(2,'{"nested":{"tags":["sql"]}}'),(3,NULL);
        CREATE TABLE requirements(id BIGINT, criteria JSON);
        INSERT INTO requirements VALUES (1,'{"tags":["sql"]}'),(2,'{"tags":["sql","sql"]}'),(3,'{"tags":["geo"]}');"#).unwrap();
    let mut statement = db.prepare(sql).unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows, vec![(1, 1), (1, 2)]);
}
#[test]
fn public_schema_type_and_driver_restore_preserve_domain_nulls() {
    assert_eq!(
        orchiddb::compiler::data_type("json").unwrap(),
        domain::json_type()
    );
    let input = Arc::new(arrow::array::StringArray::from(vec![
        Some("null"),
        None,
        Some(r#"{"n":9007199254740993}"#),
    ])) as ArrayRef;
    let restored = domain::restore(&input, &domain::json_type()).unwrap();
    assert!(!restored.is_null(0));
    assert!(restored.is_null(1));
    assert_eq!(
        domain::json_text(&ScalarValue::try_from_array(&restored, 2).unwrap())
            .unwrap()
            .unwrap(),
        r#"{"n":9007199254740993}"#
    );
    let _schema = Schema::new(vec![
        Field::new("json", domain::json_type(), true),
        Field::new("text", DataType::Utf8, true),
    ]);
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn graph_engine_stores_json_properties_without_reinterpreting_them_as_maps() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    engine
        .cypher(r#"CREATE (:Document {payload: JSON '{"count":7,"empty":null}'})"#)
        .await
        .unwrap();
    let result = engine.cypher("MATCH (d:Document) RETURN json.value(d.payload, '$.count', 'int64') AS n, json.query(d.payload, '$.empty') AS j").await.unwrap();
    assert_eq!(
        ScalarValue::try_from_array(result.returned.batch.column(0), 0).unwrap(),
        ScalarValue::Int64(Some(7))
    );
    assert_eq!(
        domain::json_text(
            &ScalarValue::try_from_array(result.returned.batch.column(1), 0).unwrap()
        )
        .unwrap(),
        Some("null".into())
    );
}
