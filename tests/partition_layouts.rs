use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

fn layout(table: &str, column: &str) -> Value {
    json!({"table":table,"specs":[{"spec_id":0,"fields":[{"source_column":column,"source_id":1,"field_id":1000,"transform":{"kind":"identity"}}]}],
    "partitions":[
        {"spec_id":0,"bytes":1000,"files":1,"bounds":{column:{"min":"0","max":"49"}}},
        {"spec_id":0,"bytes":1000,"files":1,"bounds":{column:{"min":"50","max":"99"}}}
    ]})
}
fn request(query: &str) -> Value {
    let columns = json!([{"name":"id","data_type":"int64"},{"name":"region","data_type":"int64"}]);
    json!({"version":1,"dialect":"duckdb","language":"cypher","query":query,
        "tables":[{"name":"by_id","columns":columns},{"name":"by_region","columns":columns}],
        "logical_sources":[{"name":"events","default_table":"by_id","layouts":[layout("by_id","id"),layout("by_region","region")]}],
        "nodes":[{"label":"Event","table":"events","id":"id","properties":{"id":"id","region":"region"}}]})
}
async fn compile(request: Value) -> Value {
    serde_json::from_str(&compile_json(&request.to_string()).await.unwrap()).unwrap()
}
#[tokio::test]
async fn selects_table_using_pushed_predicates_and_reports_cost() {
    let r = compile(request(
        "MATCH (e:Event) WHERE e.region = 75 RETURN e.region",
    ))
    .await;
    assert_eq!(r["layout_selections"][0]["table"], "by_region", "{r}");
    assert_eq!(r["layout_selections"][0]["estimated_bytes"], 1000);
    assert_eq!(r["layout_selections"][0]["estimated_files"], 1);
    let sql = r["sql"].as_str().unwrap();
    assert!(sql.contains("by_region") && !sql.contains("by_id"), "{sql}");
    assert!(sql.contains("75"), "residual predicate missing: {sql}");
}
#[tokio::test]
async fn chooses_independently_for_self_join() {
    let r = compile(request(
        "MATCH (a:Event), (b:Event) WHERE a.id = 5 AND b.region = 75 RETURN a.region, b.region",
    ))
    .await;
    let choices = r["layout_selections"].as_array().unwrap();
    assert_eq!(choices.len(), 2, "{r}");
    assert!(choices.iter().any(|c| c["table"] == "by_id"), "{r}");
    assert!(choices.iter().any(|c| c["table"] == "by_region"), "{r}");
}
#[tokio::test]
async fn falls_back_for_unknown_stats_and_excludes_stale_layout() {
    let mut r = request("MATCH (e:Event) WHERE e.region = 75 RETURN e.region");
    r["logical_sources"][0]["layouts"][1]["generation"] = json!("old");
    let result = compile(r).await;
    assert_eq!(result["layout_selections"][0]["table"], "by_id");
    assert_eq!(
        result["layout_selections"][0]["candidates"][1]["reason"],
        "stale generation"
    );
    let mut r = request("MATCH (e:Event) WHERE e.region = 75 RETURN e.region");
    r["logical_sources"][0]["layouts"][0]["partitions"] = Value::Null;
    let result = compile(r).await;
    assert_eq!(result["layout_selections"][0]["table"], "by_id");
    assert!(result["layout_selections"][0]["estimated_bytes"].is_null());
}
#[tokio::test]
async fn rdf_uses_the_same_layout_catalog() {
    let mut r = request("");
    r["language"] = json!("sparql");
    r["query"] = json!("SELECT ?region WHERE { ?s <urn:region> ?region }");
    r["rdf"] = json!([{"table":"events","subject":{"kind":"template","prefix":"urn:event:","columns":["id"]},"predicate":{"kind":"constant","value":"urn:region"},"object":{"kind":"literal","column":"region"}}]);
    // With no pruning predicate, choose the cheaper full scan.
    r["logical_sources"][0]["layouts"][1]["partitions"][0]["bytes"] = json!(10);
    r["logical_sources"][0]["layouts"][1]["partitions"][1]["bytes"] = json!(10);
    let result = compile(r).await;
    assert_eq!(
        result["layout_selections"][0]["table"], "by_region",
        "{result}"
    );
    assert!(result["sql"].as_str().unwrap().contains("by_region"));
}
#[tokio::test]
async fn invalid_layout_schema_is_rejected() {
    let mut r = request("MATCH (e:Event) RETURN e.region");
    r["tables"][1]["columns"][1]["data_type"] = json!("string");
    assert!(
        compile_json(&r.to_string())
            .await
            .unwrap_err()
            .contains("canonical")
    );
}

#[tokio::test]
async fn bucket_values_select_without_source_bounds() {
    let mut r = request("MATCH (e:Event) WHERE e.region = 34 RETURN e.region");
    let candidate = &mut r["logical_sources"][0]["layouts"][1];
    candidate["specs"][0]["fields"][0]["transform"] = json!({"kind":"bucket","buckets":16});
    // Iceberg's published vector: hash(34L) = 2017239379, bucket[16] = 3.
    candidate["partitions"] = json!([
        {"spec_id":0,"bytes":1000,"files":1,"values":{"1000":"3"}},
        {"spec_id":0,"bytes":1000,"files":1,"values":{"1000":"4"}}
    ]);
    let result = compile(r).await;
    assert_eq!(
        result["layout_selections"][0]["table"], "by_region",
        "{result}"
    );
    assert_eq!(result["layout_selections"][0]["estimated_bytes"], 1000);
    assert!(
        result["logical_plan"]
            .as_str()
            .unwrap()
            .contains("TableScan: by_region")
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn merged_engine_executes_selected_table_and_exposes_plan_statistics() {
    use arrow::datatypes::{DataType, Field, Schema};
    use orchiddb::{
        engine::GraphEngine,
        ir::rel::mapping::{GraphMapping, NodeMapping},
    };
    use std::sync::Arc;
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE by_id(id BIGINT, region BIGINT); INSERT INTO by_id VALUES (1,75),(2,12),(3,75); CREATE TABLE by_region AS SELECT * FROM by_id ORDER BY region;").unwrap();
    let mut mapping = GraphMapping::new();
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, true),
        Field::new("region", DataType::Int64, true),
    ]));
    for name in ["by_id", "by_region"] {
        mapping.register_table_schema(name, schema.clone());
    }
    mapping
        .register_logical_source(
            serde_json::from_value(request("")["logical_sources"][0].clone()).unwrap(),
        )
        .unwrap();
    mapping.map_node(NodeMapping::table("Event", "events", "id").property("region", "region"));
    let mut engine = GraphEngine::mapped(db, Arc::new(mapping)).unwrap();
    let result = engine
        .cypher("MATCH (e:Event) WHERE e.region = 75 RETURN e.region")
        .await
        .unwrap();
    assert_eq!(result.returned.batch.num_rows(), 2);
    assert_eq!(result.stats.layout_selections[0].table, "by_region");
    assert!(
        result.stats.logical_plan.contains("TableScan: by_region"),
        "{}",
        result.stats.logical_plan
    );
    assert!(
        result
            .stats
            .sql_queries
            .iter()
            .any(|s| s.contains("by_region"))
    );
}

#[test]
fn toml_round_trips_layouts_and_binds_after_physical_schemas() {
    use arrow::datatypes::{DataType, Field, Schema};
    use orchiddb::ir::rel::mapping::GraphMapping;
    use std::sync::Arc;
    let definition = request("")["logical_sources"][0].clone();
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, true),
        Field::new("region", DataType::Int64, true),
    ]));
    let mut mapping = GraphMapping::new();
    for name in ["by_id", "by_region"] {
        mapping.register_table_schema(name, schema.clone());
    }
    mapping
        .register_logical_source(serde_json::from_value(definition.clone()).unwrap())
        .unwrap();
    let mut parsed = GraphMapping::from_toml(&mapping.to_toml()).unwrap();
    assert_eq!(
        serde_json::to_value(parsed.logical_source("events").unwrap()).unwrap(),
        serde_json::to_value(mapping.logical_source("events").unwrap()).unwrap()
    );
    for name in ["by_id", "by_region"] {
        parsed.register_table_schema(name, schema.clone());
    }
    let filter = datafusion::prelude::col("region").eq(datafusion::prelude::lit(75_i64));
    assert_eq!(parsed.resolve_table("events", &[filter]), "by_region");
}

#[tokio::test]
async fn disjunction_ranges_and_partition_evolution_affect_plan_estimates() {
    let result = compile(request(
        "MATCH (e:Event) WHERE e.region < 20 OR e.region > 80 RETURN e.region",
    ))
    .await;
    assert_eq!(result["layout_selections"][0]["table"], "by_id");
    let mut r = request("MATCH (e:Event) WHERE e.region >= 60 AND e.region < 80 RETURN e.region");
    let candidate = &mut r["logical_sources"][0]["layouts"][1];
    candidate["specs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"spec_id":1,"fields":[]}));
    candidate["partitions"][1]["spec_id"] = json!(1);
    let result = compile(r).await;
    assert_eq!(
        result["layout_selections"][0]["table"], "by_region",
        "{result}"
    );
    assert_eq!(result["layout_selections"][0]["estimated_bytes"], 1000);
}

#[tokio::test]
async fn rdf_literal_filter_chooses_partitioned_source() {
    let mut r = request("");
    r["language"] = json!("sparql");
    r["query"] =
        json!("SELECT ?region WHERE { ?s <urn:region> ?region FILTER(?region = \"west\") }");
    r["rdf"] = json!([{"table":"events","subject":{"kind":"template","prefix":"urn:event:","columns":["id"]},"predicate":{"kind":"constant","value":"urn:region"},"object":{"kind":"literal","column":"region"}}]);
    for table in r["tables"].as_array_mut().unwrap() {
        table["columns"][1]["data_type"] = json!("string");
    }
    r["logical_sources"][0]["layouts"][1]["partitions"] = json!([
        {"spec_id":0,"bytes":1000,"files":1,"values":{"1000":"east"}},
        {"spec_id":0,"bytes":1000,"files":1,"values":{"1000":"west"}}
    ]);
    let result = compile(r).await;
    assert_eq!(
        result["layout_selections"][0]["table"], "by_region",
        "{result}"
    );
}
