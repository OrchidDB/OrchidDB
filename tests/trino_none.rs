//! Source-plan regressions: Trino must own all relational execution.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

fn request(query: &str) -> Value {
    json!({"version":1,"dialect":"duckdb","language":"cypher","engine":"none",
        "native_values":true,"query":query,"sources":{"db":{"adapter":"trino"}},
        "tables":[{"name":"people","source":"db","columns":[
            {"name":"id","data_type":"string"},{"name":"name","data_type":"string"}]}],
        "nodes":[{"label":"Person","table":"people","id":"id","properties":{"id":"id","name":"name"}}]})
}
async fn compile(input: Value) -> Result<Value, String> {
    compile_json(&input.to_string())
        .await
        .map(|v| serde_json::from_str(&v).unwrap())
}
#[tokio::test]
async fn source_only_string_grouping_and_escaped_parameter() {
    for query in [
        "MATCH(p:Person) RETURN p.name,count(p) AS n ORDER BY n DESC,p.name",
        "MATCH(p:Person) WHERE p.name=$name RETURN count(p) AS n",
    ] {
        let mut input = request(query);
        input["parameters"] = json!({"name":"x' OR 1=1 --"});
        let result = compile(input).await.unwrap();
        let nodes = result["execution_plan"]["nodes"].as_array().unwrap();
        assert!(
            nodes
                .iter()
                .all(|n| n["kind"] == "source_sql" || n["kind"] == "local_project")
        );
        assert!(nodes.iter().any(|n| n["kind"] == "source_sql"));
    }
}
#[tokio::test]
async fn source_free_list_uses_registered_source() {
    let result = compile(request(
        "RETURN [x IN range(1,5) WHERE x % 2=0 | x*2] AS xs",
    ))
    .await
    .unwrap();
    let nodes = result["execution_plan"]["nodes"].as_array().unwrap();
    assert!(nodes.iter().all(|n| n["kind"] != "duckdb"));
    let sql = nodes.iter().find(|n| n["kind"] == "source_sql").unwrap()["sql"]
        .as_str()
        .unwrap();
    assert!(sql.contains("transform") && sql.contains("filter"), "{sql}");
}
#[tokio::test]
async fn reports_unsupported_descendant_instead_of_outer_limit() {
    let error = compile(request("MATCH(p:Person) RETURN properties(p) LIMIT 1"))
        .await
        .unwrap_err();
    assert!(
        error.contains("language function") && error.contains("trino"),
        "{error}"
    );
}

#[tokio::test]
async fn graph_results_use_typed_source_descriptors_and_shared_decoder() {
    let result = compile(request("MATCH(p:Person) RETURN p LIMIT 1"))
        .await
        .unwrap();
    let nodes = result["execution_plan"]["nodes"].as_array().unwrap();
    assert!(nodes.iter().all(|n| n["kind"] != "duckdb"));
    assert!(nodes.iter().any(|n| {
        n["expressions"]
            .as_array()
            .is_some_and(|es| es.iter().any(|e| e["kind"] == "graph_element"))
    }));
    assert!(
        nodes
            .iter()
            .filter_map(|n| n["sql"].as_str())
            .all(|sql| !sql.contains("orchid_cypher_element"))
    );
}

#[tokio::test]
async fn bounded_path_output_stays_on_source() {
    let mut input = request("MATCH p=(a:Person)-[:LINK*0..2]->(b:Person) RETURN p");
    input["tables"].as_array_mut().unwrap().push(json!({"name":"links","source":"db","columns":[
        {"name":"id","data_type":"string"},{"name":"src","data_type":"string"},{"name":"dst","data_type":"string"}]}));
    input["edges"] = json!([{"label":"LINK","table":"links","id":"id","source_label":"Person",
        "target_label":"Person","source":"src","target":"dst","properties":{}}]);
    let mut unbounded = input.clone();
    unbounded["query"] = json!("MATCH(a:Person)-[:LINK*]->(b:Person) RETURN a.id,b.id LIMIT 1");
    let error = compile(unbounded).await.unwrap_err();
    assert!(error.contains("unbounded traversal"), "{error}");
    let result = compile(input).await.unwrap();
    let nodes = result["execution_plan"]["nodes"].as_array().unwrap();
    assert!(
        nodes
            .iter()
            .all(|n| n["kind"] == "source_sql" || n["kind"] == "local_project")
    );
    assert!(
        nodes
            .iter()
            .filter_map(|n| n["sql"].as_str())
            .any(|sql| sql.contains("orchid.graph.p1:"))
    );
}
