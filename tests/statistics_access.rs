#![cfg(feature = "duckdb")]
use orchiddb::{compiler::compile_json, ir::rel::statistics};
use serde_json::{Value, json};
fn fixture() -> (duckdb::Connection, Value) {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE parents AS SELECT i::BIGINT id, list_transform(range(32), x -> {'v': x}) items FROM range(2000) t(i); CREATE TABLE flat AS SELECT id AS owner, unnest(items).v AS value FROM parents; CREATE TABLE customers AS SELECT i::BIGINT id, CASE WHEN i=1 THEN 'rare' ELSE 'common' END kind FROM range(2000) t(i);").unwrap();
    let request = json!({"version":1,"dialect":"duckdb","language":"cypher","query":"MATCH (t:Item), (c:Customer) WHERE t.owner=c.id AND c.kind='rare' RETURN t.owner,t.value ORDER BY t.owner,t.value",
 "tables":[{"name":"parents","columns":[{"name":"id","data_type":"int64"},{"name":"items","data_type":"list:struct:{\"v\":\"int64\"}"}]},{"name":"flat","columns":[{"name":"owner","data_type":"int64"},{"name":"value","data_type":"int64"}]},{"name":"customers","columns":[{"name":"id","data_type":"int64"},{"name":"kind","data_type":"string"}]}],
 "collection_sources":[{"name":"nested","table":"parents","column":"items","parent_columns":{"owner":"id"},"fields":{"value":"v"}}],
 "representation_sources":[{"name":"items","default_representation":"flat","representations":[{"name":"flat","source":{"kind":"table","name":"flat"}},{"name":"nested","source":{"kind":"table","name":"nested"}}]}],
 "nodes":[{"label":"Item","table":"items","id":["owner","value"],"properties":{"owner":"owner","value":"value"}},{"label":"Customer","table":"customers","id":"id","properties":{"id":"id","kind":"kind"}}]});
    (db, request)
}
async fn compile(request: &Value) -> Value {
    serde_json::from_str(&compile_json(&request.to_string()).await.unwrap()).unwrap()
}
fn rows(db: &duckdb::Connection, plan: &Value) -> String {
    let mut stmt = db.prepare(plan["sql"].as_str().unwrap()).unwrap();
    let batches = stmt.query_arrow([]).unwrap().collect::<Vec<_>>();
    arrow::util::pretty::pretty_format_batches(&batches)
        .unwrap()
        .to_string()
}
#[tokio::test]
async fn connected_collection_choice_restricts_before_expansion() {
    let (db, mut request) = fixture();
    let before = compile(&request).await;
    request["statistics"] =
        serde_json::to_value(statistics::generate_duckdb(&db, request.clone()).unwrap()).unwrap();
    let after = compile(&request).await;
    assert_eq!(
        after["representation_selections"][0]["representation"], "nested",
        "{after}"
    );
    assert!(
        after["optimizer_decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["optimization"] == "restrict_collection_parents"),
        "{after}"
    );
    for disabled in [false, true] {
        db.execute_batch(if disabled {
            "SET disabled_optimizers='join_order,build_side_probe_side'"
        } else {
            "SET disabled_optimizers=''"
        })
        .unwrap();
        assert_eq!(rows(&db, &before), rows(&db, &after));
    }
    request["query"] = json!("MATCH (t:Item) RETURN t.owner,t.value");
    let broad = compile(&request).await;
    assert_eq!(
        broad["representation_selections"][0]["representation"],
        "flat"
    );
}
#[tokio::test]
async fn connected_three_way_join_changes_sql_and_preserves_results() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE a AS SELECT i::BIGINT id,(i%100)::BIGINT k FROM range(10000) t(i); CREATE TABLE b AS SELECT i::BIGINT id,(i%100)::BIGINT k FROM range(10000) t(i); CREATE TABLE c AS SELECT i::BIGINT id,CASE WHEN i%100=1 THEN 'rare' ELSE 'common' END kind FROM range(10000) t(i)").unwrap();
    let mut r = json!({"version":1,"dialect":"duckdb","language":"cypher","query":"MATCH (a:A), (b:B), (c:C) WHERE a.k=b.k AND a.id=c.id AND c.kind='rare' RETURN count(*) AS n","tables":[{"name":"a","columns":[{"name":"id","data_type":"int64"},{"name":"k","data_type":"int64"}]},{"name":"b","columns":[{"name":"id","data_type":"int64"},{"name":"k","data_type":"int64"}]},{"name":"c","columns":[{"name":"id","data_type":"int64"},{"name":"kind","data_type":"string"}]}],"nodes":[{"label":"A","table":"a","id":"id","properties":{"id":"id","k":"k"}},{"label":"B","table":"b","id":"id","properties":{"id":"id","k":"k"}},{"label":"C","table":"c","id":"id","properties":{"id":"id","kind":"kind"}}]});
    let before = compile(&r).await;
    r["statistics"] =
        serde_json::to_value(statistics::generate_duckdb(&db, r.clone()).unwrap()).unwrap();
    let after = compile(&r).await;
    assert!(
        after["optimizer_decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["optimization"] == "connected_join_order"),
        "{after}"
    );
    assert_ne!(before["sql"], after["sql"]);
    for disabled in [false, true] {
        db.execute_batch(if disabled {
            "SET disabled_optimizers='join_order,build_side_probe_side'"
        } else {
            "SET disabled_optimizers=''"
        })
        .unwrap();
        assert_eq!(rows(&db, &before), rows(&db, &after));
    }
}
#[tokio::test]
async fn connected_access_preserves_gremlin_and_rdf_bindings() {
    let (db, mut r) = fixture();
    r["edges"] = json!([{"label":"OF","table":"items","id":["owner","value"],"source":["owner","value"],"target":"owner","source_label":"Item","target_label":"Customer"}]);
    r["rdf"] = json!([
 {"table":"items","subject":{"kind":"template","prefix":"urn:item:","columns":["owner","value"]},"predicate":{"kind":"constant","value":"urn:owner"},"object":{"kind":"template","prefix":"urn:customer:","columns":["owner"]}},
 {"table":"customers","subject":{"kind":"template","prefix":"urn:customer:","columns":["id"]},"predicate":{"kind":"constant","value":"urn:kind"},"object":{"kind":"literal","column":"kind"}}]);
    let snapshot = statistics::generate_duckdb(&db, r.clone()).unwrap();
    for (language, query) in [
        (
            "gremlin",
            "g.V().hasLabel('Customer').has('kind','rare').in('OF').values('value').order()",
        ),
        (
            "sparql",
            "SELECT ?s WHERE { ?s <urn:owner> ?c . ?c <urn:kind> \"rare\" } ORDER BY ?s",
        ),
    ] {
        r["language"] = json!(language);
        r["query"] = json!(query);
        r.as_object_mut().unwrap().remove("statistics");
        let before = compile(&r).await;
        r["statistics"] = serde_json::to_value(&snapshot).unwrap();
        let after = compile(&r).await;
        assert!(
            !after["optimizer_decisions"].as_array().unwrap().is_empty(),
            "{language}: expected actionable statistics"
        );
        assert!(!after["plan_estimates"].as_array().unwrap().is_empty());
        assert_eq!(rows(&db, &before), rows(&db, &after), "{language}");
    }
}

#[tokio::test]
async fn parent_restriction_keeps_duplicate_bindings_and_optional_rows() {
    let (db, mut r) = fixture();
    db.execute_batch("DROP TABLE customers; CREATE TABLE customers(id BIGINT,owner BIGINT,kind VARCHAR); INSERT INTO customers VALUES (1,1,'rare'),(2,1,'rare'),(3,4000,'rare')").unwrap();
    r["tables"][2]["columns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"owner","data_type":"int64"}));
    r["nodes"][1]["properties"]["id"] = json!("owner");
    let snapshot = statistics::generate_duckdb(&db, r.clone()).unwrap();
    for query in [
        "MATCH (t:Item), (c:Customer) WHERE t.owner=c.id AND c.kind='rare' RETURN t.owner,t.value ORDER BY t.owner,t.value",
        "MATCH (c:Customer) OPTIONAL MATCH (t:Item) WHERE t.owner=c.id RETURN c.id,t.value ORDER BY c.id,t.value",
        "MATCH (t:Item), (c:Customer) WHERE t.owner=c.id AND c.kind='rare' RETURN t.owner,t.value ORDER BY t.owner,t.value LIMIT 3",
    ] {
        r["query"] = json!(query);
        r.as_object_mut().unwrap().remove("statistics");
        let before = compile(&r).await;
        r["statistics"] = serde_json::to_value(&snapshot).unwrap();
        let after = compile(&r).await;
        assert_eq!(rows(&db, &before), rows(&db, &after), "{query}");
    }
}

#[tokio::test]
async fn connected_search_retains_manifest_and_file_costs() {
    let (db, mut r) = fixture();
    r["query"] = json!("MATCH (t:Item) RETURN t.value");
    r["representation_sources"][0]["representations"][0]["statistics"] = json!([{
        "table":"flat","specs":[{"spec_id":0,"fields":[]}],
        "partitions":[{"spec_id":0,"bytes":1000000000,"files":100,"rows":64000,"bounds":{}}]
    }]);
    r["statistics"] =
        serde_json::to_value(statistics::generate_duckdb(&db, r.clone()).unwrap()).unwrap();
    let after = compile(&r).await;
    assert_eq!(
        after["representation_selections"][0]["representation"], "nested",
        "{after}"
    );
}
