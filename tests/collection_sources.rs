use arrow::array::Array;

use orchiddb::compiler::compile_json;
use serde_json::{Value, json};
fn request() -> Value {
    json!({"version":1,"dialect":"duckdb","language":"cypher",
        "query":"MATCH (t:Tag) WHERE t.owner = 1 RETURN t.value",
        "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"tags","data_type":"list:string"}]}],
        "collection_sources":[{"name":"tags","table":"people","column":"tags","parent_columns":{"owner":"id"},"element":"value"}],
        "nodes":[{"label":"Tag","table":"tags","id":["owner","value"],"properties":{"owner":"owner","value":"value"}}]})
}
async fn compile(r: Value) -> Value {
    serde_json::from_str(&compile_json(&r.to_string()).await.unwrap()).unwrap()
}
#[tokio::test]
async fn compiler_inlines_collection() {
    let result = compile(request()).await;
    let plan = result["logical_plan"].as_str().unwrap();
    assert!(
        plan.contains("Unnest") && plan.contains("TableScan: people"),
        "{result}"
    );
    assert!(
        !result["sql"].as_str().unwrap().contains("FROM tags"),
        "{result}"
    );
}
#[tokio::test]
async fn invalid_collection_definitions() {
    for (key, value) in [
        ("column", json!("id")),
        ("element", json!("owner")),
        ("fields", json!({"x":"y"})),
        ("table", json!("tags")),
    ] {
        let mut r = request();
        r["collection_sources"][0][key] = value;
        assert!(compile_json(&r.to_string()).await.is_err(), "{r}");
    }
}
#[tokio::test]
async fn struct_elements_and_rdf() {
    let mut r = request();
    r["tables"][0]["columns"][1]["data_type"] =
        json!("list:struct:{\"key\":\"string\",\"score\":\"int64\"}");
    r["collection_sources"][0]
        .as_object_mut()
        .unwrap()
        .remove("element");
    r["collection_sources"][0]["fields"] = json!({"value":"key","score":"score"});
    let result = compile(r.clone()).await;
    assert!(
        result["sql"]
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("unnest"),
        "{result}"
    );
    r["language"] = json!("sparql");
    r["query"] = json!("SELECT ?tag WHERE { ?s <urn:tag> ?tag }");
    r["rdf"] = json!([{"table":"tags","subject":{"kind":"template","prefix":"urn:person:","columns":["owner"]},"predicate":{"kind":"constant","value":"urn:tag"},"object":{"kind":"literal","column":"value"}}]);
    let result = compile(r).await;
    assert!(
        result["logical_plan"].as_str().unwrap().contains("Unnest"),
        "{result}"
    );
}
#[tokio::test]
async fn in_process_preserves_collection_rows() {
    use arrow::{
        array::{Int64Array, ListArray, RecordBatch},
        datatypes::{DataType, Field, Int64Type, Schema},
    };
    use datafusion::{datasource::MemTable, prelude::SessionContext};
    use orchiddb::ir::rel::mapping::GraphMapping;
    use std::sync::Arc;
    let lists = ListArray::from_iter_primitive::<Int64Type, _, _>([
        Some(vec![Some(7), Some(7), None]),
        Some(vec![]),
        None,
    ]);
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("items", lists.data_type().clone(), true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(Int64Array::from(vec![1, 2, 3])), Arc::new(lists)],
    )
    .unwrap();
    let mut mapping = GraphMapping::new();
    mapping.register_table(
        "parents",
        Arc::new(MemTable::try_new(schema, vec![vec![batch]]).unwrap()),
    );
    mapping.register_collection_source(serde_json::from_value(json!({"name":"items","table":"parents","column":"items","parent_columns":{"parent":"id"},"element":"value"})).unwrap()).unwrap();
    let plan = mapping.relational_plan("SELECT * FROM items").unwrap();
    let result = SessionContext::new()
        .execute_logical_plan(plan)
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(result.iter().map(|b| b.num_rows()).sum::<usize>(), 3);
    assert_eq!(
        result
            .iter()
            .map(|b| b.column(1).null_count())
            .sum::<usize>(),
        1
    );
    let mut restored = GraphMapping::from_toml(&mapping.to_toml()).unwrap();
    restored.register_table_schema("parents", mapping.table_schema("parents").unwrap());
    assert_eq!(
        restored.table_schema("items"),
        mapping.table_schema("items")
    );
}
#[cfg(feature = "duckdb")]
#[tokio::test]
async fn merged_engine_reads_collection_without_database_view() {
    use arrow::datatypes::{DataType, Field, Schema};
    use orchiddb::{
        engine::GraphEngine,
        ir::rel::mapping::{GraphMapping, NodeMapping},
    };
    use std::sync::Arc;
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id BIGINT, tags VARCHAR[]); INSERT INTO people VALUES (1,['a','b','c']),(2,[]),(3,NULL)").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.register_table_schema(
        "people",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, true),
            Field::new(
                "tags",
                DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
                true,
            ),
        ])),
    );
    mapping
        .register_collection_source(
            serde_json::from_value(request()["collection_sources"][0].clone()).unwrap(),
        )
        .unwrap();
    mapping.map_node(
        NodeMapping::table("Tag", "tags", ["owner", "value"])
            .property("owner", "owner")
            .property("value", "value"),
    );
    let mut engine = GraphEngine::mapped(db, Arc::new(mapping)).unwrap();
    let result = engine
        .cypher("MATCH (t:Tag) WHERE t.owner = 1 RETURN t.value")
        .await
        .unwrap();
    assert_eq!(result.returned.batch.num_rows(), 3);
    assert!(
        result.stats.logical_plan.contains("Unnest"),
        "{}",
        result.stats.logical_plan
    );
    let elements = engine
        .cypher("MATCH (t:Tag) WHERE t.owner = 1 RETURN t")
        .await
        .unwrap();
    assert_eq!(elements.returned.batch.num_rows(), 3);
    let error = engine.cypher("MATCH (t:Tag) DELETE t").await.unwrap_err();
    assert!(error.to_string().contains("read-only"), "{error}");
    assert_eq!(
        engine
            .cypher("MATCH (t:Tag) RETURN t.value")
            .await
            .unwrap()
            .returned
            .batch
            .num_rows(),
        3
    );
}

#[tokio::test]
async fn parent_predicate_selects_layout_and_element_predicate_stays_above_unnest() {
    let mut r = request();
    let columns = r["tables"][0]["columns"].clone();
    r["tables"] = json!([{"name":"people_default","columns":columns},{"name":"people_by_id","columns":columns}]);
    let partition = |min: &str, max: &str| json!({"spec_id":0,"bytes":1000,"files":1,"bounds":{"id":{"min":min,"max":max}}});
    let specs = json!([{"spec_id":0,"fields":[{"source_column":"id","source_id":1,"field_id":1000,"transform":{"kind":"identity"}}]}]);
    r["logical_sources"] = json!([{"name":"people","default_table":"people_default","layouts":[
        {"table":"people_default","specs":specs,"partitions":[partition("0","99"),partition("0","99")]},
        {"table":"people_by_id","specs":specs,"partitions":[partition("0","49"),partition("50","99")]}
    ]}]);
    r["query"] = json!("MATCH (t:Tag) WHERE t.owner = 1 AND t.value = 'a' RETURN t.value");
    let result = compile(r.clone()).await;
    assert_eq!(
        result["layout_selections"][0]["table"], "people_by_id",
        "{result}"
    );
    assert_eq!(
        result["layout_selections"][0]["estimated_bytes"], 1000,
        "{result}"
    );
    r["query"] = json!("MATCH (t:Tag) WHERE t.value = 'a' RETURN t.value");
    let result = compile(r).await;
    assert_eq!(
        result["layout_selections"][0]["table"], "people_default",
        "{result}"
    );
    assert_eq!(
        result["layout_selections"][0]["estimated_bytes"], 2000,
        "{result}"
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn struct_sql_executes_and_retains_parent_correlation() {
    let mut r = request();
    r["tables"][0]["columns"][1]["data_type"] =
        json!("list:struct:{\"key\":\"string\",\"score\":\"int64\"}");
    r["collection_sources"][0]
        .as_object_mut()
        .unwrap()
        .remove("element");
    r["collection_sources"][0]["fields"] = json!({"value":"key","score":"score"});
    let result = compile(r).await;
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id BIGINT, tags STRUCT(key VARCHAR, score BIGINT)[]); INSERT INTO people VALUES (1,[{'key':'a','score':7},{'key':'b','score':8}]),(2,[{'key':'c','score':9}]),(3,[]),(4,NULL)").unwrap();
    let mut stmt = db.prepare(result["sql"].as_str().unwrap()).unwrap();
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows, vec!["a", "b"]);
}

#[tokio::test]
async fn collection_edges_use_explicit_parent_and_child_keys() {
    let mut r = request();
    r["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"label":"Person","table":"people","id":"id","properties":{"id":"id"}}));
    r["edges"] = json!([{"label":"HAS_TAG","table":"tags","id":["owner","value"],"source":"owner","target":["owner","value"],"source_label":"Person","target_label":"Tag"}]);
    r["query"] = json!("MATCH (p:Person)-[:HAS_TAG]->(t:Tag) WHERE p.id = 1 RETURN t.value");
    let result = compile(r).await;
    assert!(
        result["logical_plan"].as_str().unwrap().contains("Unnest"),
        "{result}"
    );
    #[cfg(feature = "duckdb")]
    {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people(id BIGINT, tags VARCHAR[]); INSERT INTO people VALUES (1,['a','b']),(2,['c'])").unwrap();
        let mut stmt = db.prepare(result["sql"].as_str().unwrap()).unwrap();
        let mut rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows.sort();
        assert_eq!(rows, vec!["a", "b"]);
    }
}
