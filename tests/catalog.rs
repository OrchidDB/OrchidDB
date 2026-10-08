use orchiddb::{
    catalog::{Catalog, InMemoryCatalog, RelationshipDeclaration},
    session::{Query, Schema},
};
use serde_json::{Value, json};
fn schema() -> Value {
    json!({"tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"score","data_type":"int64"}]}],"nodes":[{"label":"Person","table":"people","id":"id","properties":{"id":"id","score":"score"}}],"cypher_relationships":[{"name":"NEAREST","source":"Person","target":"Person","parameters":[{"name":"k","schema":{"type":"integer","minimum":1},"default":1}],"cypher":"WITH source MATCH (target:Person) WHERE target.id <> source.id RETURN target, target.score AS score ORDER BY score DESC, target.id LIMIT $k","returns":{"target":"target","properties":{"score":{"type":"integer"}}}}]})
}
#[tokio::test]
async fn memory_preserves_schema_and_revision() {
    let value = schema();
    let catalog = InMemoryCatalog::new(Schema::from_value(value.clone()).unwrap());
    assert_eq!(
        serde_json::to_value(catalog.snapshot().await.unwrap().schema).unwrap(),
        value
    );
    let mut relationship = value["cypher_relationships"][0].clone();
    relationship["name"] = "OTHER".into();
    assert_eq!(
        catalog
            .register_relationship(
                1,
                RelationshipDeclaration::Cypher(
                    serde_json::from_value(relationship.clone()).unwrap()
                )
            )
            .unwrap(),
        2
    );
    assert!(
        catalog
            .register_relationship(
                1,
                RelationshipDeclaration::Cypher(
                    serde_json::from_value(relationship.clone()).unwrap()
                )
            )
            .is_err()
    );
    relationship["description"] = "updated".into();
    assert_eq!(
        catalog
            .replace_relationship(
                2,
                "OTHER",
                RelationshipDeclaration::Cypher(serde_json::from_value(relationship).unwrap())
            )
            .unwrap(),
        3
    );
    assert_eq!(catalog.remove_relationship(3, "OTHER").unwrap(), 4);
    assert!(catalog.remove_relationship(4, "OTHER").is_err());
    assert_eq!(
        serde_json::to_value(catalog.current().unwrap().schema).unwrap(),
        value
    );
}
#[test]
fn host_bindings_preserve_schema() {
    let value = schema();
    let original = Schema::from_value(value.clone()).unwrap();
    let bound = original
        .with_bindings(&json!({"execution_engine":"local"}))
        .unwrap();
    let bound = serde_json::to_value(bound).unwrap();
    assert_eq!(bound["cypher_relationships"], value["cypher_relationships"]);
    assert_eq!(bound["execution_engine"], "local");
    assert!(original.with_bindings(&json!({"tables":[]})).is_err());
    assert!(Schema::from_value(json!({"catalog":{"endpoint":"http://localhost","scope":"s","graph":"g","token_env":"TOKEN"},"tables":[]})).is_err());
}
#[tokio::test]
async fn declared_edges_execute_on_duckdb() {
    for dialect in ["duckdb"] {
        for text in [
            "MATCH (p:Person)-[r:NEAREST]->(q:Person) RETURN p.id AS source, q.id AS target, r.score AS score ORDER BY source",
            "MATCH (p:Person) CALL NEAREST(p, 2) YIELD target, score RETURN p.id AS source, target.id AS target, score ORDER BY source, target",
        ] {
            let request = Schema::from_value(schema())
                .unwrap()
                .request(dialect, &Query::cypher(text))
                .unwrap();
            let result = orchiddb::compiler::compile(request)
                .await
                .unwrap_or_else(|e| panic!("{dialect}: {text}: {e}"));
            assert!(!result.sql.is_empty());
            let rows = execute(dialect, &result.sql);
            let expected = if text.contains("CALL") {
                json!([
                    [1, 2, 20],
                    [1, 3, 30],
                    [2, 1, 10],
                    [2, 3, 30],
                    [3, 1, 10],
                    [3, 2, 20]
                ])
            } else {
                json!([[1, 3, 30], [2, 3, 30], [3, 2, 20]])
            };
            assert_eq!(rows, expected, "{dialect}: {text}");
        }
    }
}

fn execute(dialect: &str, sql: &str) -> Value {
    let fixture = "people AS (SELECT CAST(1 AS BIGINT) AS id, CAST(10 AS BIGINT) AS score UNION ALL SELECT 2,20 UNION ALL SELECT 3,30)";
    let sql = if let Some(rest) = sql.strip_prefix("WITH ") {
        format!("WITH {fixture}, {rest}")
    } else {
        format!("WITH {fixture} {sql}")
    };
    assert_eq!(dialect, "duckdb");
    let output = std::process::Command::new("extension/vendor/test-env/bin/python").args(["-c","import duckdb,sys,json; print(json.dumps(duckdb.connect().execute(sys.argv[1]).fetchall()))",&sql]).output().unwrap();
    assert!(
        output.status.success(),
        "{dialect}: {}\n{sql}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[cfg(feature = "orchid-catalog")]
#[tokio::test]
#[ignore = "requires ORCHID_CATALOG_TEST_ENDPOINT and a local catalog service"]
async fn remote_registration_publication_and_live_grants() {
    use orchiddb::catalog::{CatalogGrants, CatalogGraph, CatalogPublish, OrchidCatalog};
    let endpoint = std::env::var("ORCHID_CATALOG_TEST_ENDPOINT").unwrap();
    let scope = format!(
        "catalog_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let admin = OrchidCatalog::new(&endpoint, "catalog-test-admin", &scope, "graph").unwrap();
    let reader = OrchidCatalog::new(&endpoint, "catalog-test-reader", &scope, "graph").unwrap();
    let value = schema();
    let table = &value["tables"][0];
    admin.register_object("people",0,json!({"kind":"dataset","description":"people","table":"people","columns":table["columns"]})).await.unwrap();
    admin.register_object("person",0,json!({"kind":"entity","description":"person","dataset":"people","mapping":value["nodes"][0]})).await.unwrap();
    let mut relationship = value["cypher_relationships"][0].clone();
    relationship["kind"] = "cypher_relationship".into();
    relationship["description"] = "nearest person".into();
    relationship["source"] = "person".into();
    relationship["target"] = "person".into();
    admin
        .register_object("nearest", 0, relationship.clone())
        .await
        .unwrap();
    assert!(
        admin
            .register_object("nearest", 0, relationship)
            .await
            .is_err()
    );
    admin
        .register_graph(
            0,
            CatalogGraph {
                description: "integration".into(),
                objects: vec!["nearest".into()],
                execution_connector: None,
            },
        )
        .await
        .unwrap();
    admin
        .set_grants(
            0,
            CatalogGrants {
                discover: vec!["reader".into()],
                execute: vec!["reader".into()],
            },
        )
        .await
        .unwrap();
    admin
        .publish(CatalogPublish {
            expected_revision: 0,
            graph_version: 1,
            object_versions: std::collections::BTreeMap::from([
                ("people".into(), 1),
                ("person".into(), 1),
                ("nearest".into(), 1),
            ]),
        })
        .await
        .unwrap();
    let resolved = reader.resolve().await.unwrap();
    assert_eq!(resolved.manifest.protocol_version, 2);
    assert_eq!(resolved.manifest.objects.len(), 3);
    let snapshot = reader.snapshot().await.unwrap();
    assert_eq!(
        serde_json::to_value(&snapshot.schema).unwrap()["cypher_relationships"][0]["source"],
        "Person"
    );
    orchiddb::compiler::compile(
        snapshot
            .schema
            .request(
                "duckdb",
                &Query::cypher("MATCH (p:Person)-[:NEAREST]->(q:Person) RETURN q.id"),
            )
            .unwrap(),
    )
    .await
    .unwrap();
    assert!(reader.discover(None).await.is_ok());
    let pinned = reader.at_revision(1).unwrap();
    admin.set_grants(1, CatalogGrants::default()).await.unwrap();
    assert!(pinned.snapshot().await.is_err());
}

#[test]
fn relationship_parameter_contracts() {
    use std::collections::BTreeMap;
    let mut value = schema()["cypher_relationships"][0].clone();
    let declaration: orchiddb::catalog::CypherRelationship =
        serde_json::from_value(value.clone()).unwrap();
    assert_eq!(declaration.arguments(&BTreeMap::new()).unwrap()["k"], 1);
    assert!(
        declaration
            .arguments(&BTreeMap::from([("k".into(), json!(0))]))
            .is_err()
    );
    assert!(
        declaration
            .arguments(&BTreeMap::from([("unknown".into(), json!(1))]))
            .is_err()
    );
    value["parameters"][0]
        .as_object_mut()
        .unwrap()
        .remove("default");
    let declaration: orchiddb::catalog::CypherRelationship =
        serde_json::from_value(value.clone()).unwrap();
    assert!(declaration.arguments(&BTreeMap::new()).is_err());
    value["parameters"][0]["schema"] = json!({"type":["integer","null"]});
    value["parameters"][0]["default"] = Value::Null;
    let declaration: orchiddb::catalog::CypherRelationship = serde_json::from_value(value).unwrap();
    assert!(declaration.arguments(&BTreeMap::new()).unwrap()["k"].is_null());
}

#[tokio::test]
async fn relationship_definition_errors() {
    for body in [
        "WITH source CREATE (target:Person) RETURN target, 1 AS score",
        "WITH source RETURN 1 AS target, 1 AS score",
        "WITH source MATCH (target:Person)-[:NEAREST]->(other:Person) RETURN target, 1 AS score",
    ] {
        let mut value = schema();
        value["cypher_relationships"][0]["cypher"] = body.into();
        match Schema::from_value(value) {
            Err(_) => {}
            Ok(schema) => assert!(
                orchiddb::compiler::compile(
                    schema
                        .request(
                            "duckdb",
                            &Query::cypher("MATCH (p:Person)-[:NEAREST]->(q:Person) RETURN q.id")
                        )
                        .unwrap()
                )
                .await
                .is_err(),
                "{body}"
            ),
        }
    }
}

async fn rows(value: Value, query: Query) -> Value {
    let request = Schema::from_value(value)
        .unwrap()
        .request("duckdb", &query)
        .unwrap();
    execute(
        "duckdb",
        &orchiddb::compiler::compile(request).await.unwrap().sql,
    )
}
#[tokio::test]
async fn composite_keys_and_call_defaults() {
    let mut value = schema();
    value["nodes"][0]["id"] = json!(["score", "id"]);
    let result=rows(value,Query::cypher("MATCH (p:Person) CALL NEAREST(p) YIELD target, score RETURN p.id AS source, target.id AS target, score ORDER BY source")).await;
    assert_eq!(result, json!([[1, 3, 30], [2, 3, 30], [3, 2, 20]]));
}
#[tokio::test]
async fn duplicates_and_property_filters() {
    let mut value = schema();
    value["cypher_relationships"][0]["cypher"]="WITH source MATCH (target:Person) WHERE target.id <> source.id UNWIND [1,1] AS copy RETURN target, target.score AS score".into();
    let result=rows(value,Query::cypher("MATCH (p:Person)-[r:NEAREST {score:30}]->(q:Person) RETURN p.id AS source, q.id AS target, r.score AS score ORDER BY source")).await;
    assert_eq!(
        result,
        json!([[1, 3, 30], [1, 3, 30], [2, 3, 30], [2, 3, 30]])
    );
}
#[tokio::test]
async fn registered_read_procedure_in_relationship() {
    let mut value = schema();
    value["procedures"] = json!({"neighbors":{"inputs":[{"name":"id","type":"INTEGER","nullable":false}],"outputs":[{"name":"target_id","type":"INTEGER","nullable":false},{"name":"score","type":"INTEGER","nullable":false}],"rows":[[1,3,90],[2,3,80],[3,2,70]]}});
    value["cypher_relationships"][0]["cypher"]="WITH source CALL neighbors(source.id) YIELD target_id, score MATCH (target:Person {id:target_id}) RETURN target, score".into();
    let result=rows(value,Query::cypher("MATCH (p:Person)-[r:NEAREST]->(q:Person) RETURN p.id AS source, q.id AS target, r.score AS score ORDER BY source")).await;
    assert_eq!(result, json!([[1, 3, 90], [2, 3, 80], [3, 2, 70]]));
}
