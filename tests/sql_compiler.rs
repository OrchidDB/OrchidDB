//! SQL-only boundary regression tests: run without the duckdb feature.
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};
fn request(query: &str) -> Value {
    json!({"version":1,"dialect":"duckdb","language":"cypher","query":query,
        "tables":[{"name":"\"Graph Data\".\"People\"","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],
        "nodes":[{"label":"Person","table":"\"Graph Data\".\"People\"","id":"id","properties":{"name":"name"}}]})
}
async fn compile(r: Value) -> Result<Value, String> {
    Ok(serde_json::from_str(&compile_json(&r.to_string()).await?).unwrap())
}
#[tokio::test]
async fn compiles_qualified_external_table_without_materializing() {
    let r = compile(request("MATCH (p:Person) RETURN p.name AS name"))
        .await
        .unwrap();
    assert!(
        r["sql"]
            .as_str()
            .unwrap()
            .contains("\"Graph Data\".\"People\"")
    );
    assert_eq!(r["fields"], json!(["name"]));
}
#[tokio::test]
async fn constant_query_needs_no_internal_table() {
    let r = compile(request("RETURN 42 AS answer")).await.unwrap();
    assert!(r["sql"].as_str().unwrap().contains("42"));
    assert!(!r["sql"].as_str().unwrap().contains("one_row_"));
}

#[tokio::test]
async fn signed_integer_widths_compare_without_lossy_numeric_promotion() {
    for dialect in ["duckdb", "postgres"] {
        for ty in ["int8", "int16", "int32", "int64"] {
            let mut r = request("MATCH (p:Person) WHERE p.age > 35 RETURN p.age");
            r["dialect"] = json!(dialect);
            r["tables"][0]["columns"].as_array_mut().unwrap()
                .push(json!({"name":"age", "data_type":ty}));
            r["nodes"][0]["properties"]["age"] = json!("age");
            compile(r).await.unwrap();
        }
        // This relaxation must not admit uint64/int64 or float/int64 promotion,
        // where values can be rounded or exceed the other operand's range.
        for ty in ["uint64", "float64"] {
            let mut r = request("MATCH (p:Person) WHERE p.age = 9007199254740993 RETURN p.age");
            r["dialect"] = json!(dialect);
            r["tables"][0]["columns"].as_array_mut().unwrap()
                .push(json!({"name":"age", "data_type":ty}));
            r["nodes"][0]["properties"]["age"] = json!("age");
            assert!(compile(r).await.unwrap_err().contains("lossless runtime values"));
        }
    }
}
#[tokio::test]
async fn validates_protocol_dialect_and_schema() {
    let mut r = request("RETURN 1");
    r["version"] = json!(99);
    assert!(compile(r).await.unwrap_err().contains("version"));
    let mut r = request("RETURN 1");
    r["dialect"] = json!("clickhouse");
    assert!(compile(r).await.unwrap_err().contains("dialect"));
    let mut r = request("RETURN 1");
    r["nodes"][0]["id"] = json!("missing");
    assert!(compile(r).await.unwrap_err().contains("missing column"));
    let mut r = request("RETURN 1");
    r["tables"][0]["columns"][0]["data_type"] = json!("list");
    assert!(compile(r).await.unwrap_err().contains("unsupported schema type"));
}
#[tokio::test]
async fn rejects_mutations_and_missing_bindings() {
    let e = compile(request("MATCH (p:Person) DELETE p"))
        .await
        .unwrap_err();
    assert!(e.contains("mutation"), "{e}");
    assert!(
        compile(request("MATCH (p:Person) WHERE p.name=$name RETURN p.name"))
            .await
            .is_err()
    );
}
#[tokio::test]
async fn parameters_are_literals_and_oversized_integers_fail() {
    let mut r = request("MATCH (p:Person) WHERE p.name=$name RETURN p.name");
    r["parameters"] = json!({"name":"O'Reilly; DROP TABLE people; --"});
    let sql = compile(r).await.unwrap()["sql"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(sql.contains("O''Reilly; DROP TABLE people; --"), "{sql}");
    let mut r = request("RETURN $n");
    r["parameters"] = json!({"n":u64::MAX});
    assert!(compile(r).await.unwrap_err().contains("64-bit"));
}

#[tokio::test]
async fn membership_never_duplicates_a_computed_left_operand() {
    let mut r =
        request("MATCH (p:Person) WHERE externalName(p.name) IN ['Ada', 'Grace'] RETURN p.name");
    r["functions"] = json!([{"name":"externalName","target":"external_name","parameters":["string"],"returns":"string"}]);
    let error = compile(r).await.unwrap_err();
    assert!(error.contains("single-evaluation"), "{error}");
}

#[tokio::test]
async fn mapped_gremlin_ids_and_scalar_order_need_no_runtime_functions() {
    for query in ["g.V(1).values('name')", "g.V().order().by('name').values('name')"] {
        let mut r = request(query);
        r["language"] = json!("gremlin");
        let compiled = compile(r).await.unwrap();
        let sql = compiled["sql"].as_str().unwrap();
        assert!(!sql.contains("gremlin_id"), "{sql}");
        assert!(!sql.contains("gremlin_order_key"), "{sql}");
    }
}

#[test]
fn service_and_service_silent_are_rejected_during_planning() {
    use orchiddb::language::sparql::{SparqlPlanner, SparqlError};
    for modifier in ["", "SILENT "] {
        let query = format!("SELECT ?n WHERE {{ SERVICE {modifier}<http://127.0.0.1:1/unreachable> {{ BIND(42 AS ?n) }} }}");
        let error = SparqlPlanner::default().plan_str(&query).unwrap_err();
        assert!(matches!(error, SparqlError::Unsupported(_)), "{error}");
        assert!(error.to_string().contains("SERVICE"), "{error}");
    }
}

#[tokio::test]
async fn every_declared_scalar_type_can_be_a_primary_key() {
    for ty in [
        "boolean", "int8", "int16", "int32", "int64",
        "uint8", "uint16", "uint32", "uint64", "float32", "float64",
        "string", "binary", "date", "time", "timestamp", "duration", "interval",
        "decimal:38:2",
    ] {
        for dialect in ["duckdb", "postgres"] {
            let mut r = request("MATCH (p:Person) RETURN id(p)");
            r["dialect"] = json!(dialect);
            r["tables"][0]["columns"][0]["data_type"] = json!(ty);
            compile(r).await.unwrap_or_else(|e| panic!("{dialect} {ty}: {e}"));
        }
    }
}

#[tokio::test]
async fn rdf_rules_compile_without_an_ontology_or_type_root() {
    let request=json!({"version":1,"dialect":"duckdb","language":"sparql","query":"SELECT ?name WHERE {?s <urn:name> ?name}","tables":[{"name":"customers","columns":[{"name":"tenant","data_type":"int64"},{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],"rdf":[{"table":"customers","subject":{"kind":"template","prefix":"urn:c:","columns":["tenant","id"]},"predicate":{"kind":"constant","value":"urn:name"},"object":{"kind":"literal","column":"name"}}]});
    let result=compile(request).await.unwrap();
    assert!(result["sql"].as_str().unwrap().contains("customers"));
    assert_eq!(result["fields"],json!(["?name"]));
}

#[tokio::test]
async fn legacy_sparql_constant_retains_its_native_numeric_type() {
    let result=compile(json!({"version":1,"dialect":"duckdb","language":"sparql","query":"SELECT (42 AS ?answer) WHERE {}","tables":[]})).await.unwrap();
    assert!(result["sql"].as_str().unwrap().contains("BIGINT"),"{}",result["sql"]);
}


#[tokio::test]
async fn audit_permission_scope_wraps_query_source() {
    let mut r = json!({"version":1,"dialect":"duckdb","language":"cypher","query":"MATCH (p:Person) RETURN p.name AS name",
        "authorization":{"subject_type":"user","subject_id":"alice"},
        "tables":[{"name":"people","columns":[{"name":"id","data_type":"string"},{"name":"name","data_type":"string"}]},
        {"name":"permissions","columns":[{"name":"resource_type","data_type":"string"},{"name":"permission","data_type":"string"},{"name":"resource_id","data_type":"string"},{"name":"subject_type","data_type":"string"},{"name":"subject_relation","data_type":"string"},{"name":"subject_id","data_type":"string"}]}],
        "nodes":[{"label":"Person","table":"people","id":"id","properties":{"name":"name"},
          "source_query":"SELECT id, name FROM people WHERE name <> 'hidden'",
          "permission_scopes":[{"resource_column":"id","relation":{"table":"permissions","resource_type":"person","permission":"read","permission_column":"permission","subject_relation_column":"subject_relation"}}]}]});
    for dialect in ["duckdb", "postgres"] {
        r["dialect"] = json!(dialect);
        let result = compile(r.clone()).await.unwrap();
        #[cfg(feature = "duckdb")]
        if dialect == "duckdb" {
            let conn = duckdb::Connection::open_in_memory().unwrap();
            conn.execute_batch("CREATE TABLE people(id VARCHAR, name VARCHAR); INSERT INTO people VALUES ('1','Ada'), ('2','Bob'), ('3','hidden'); CREATE TABLE permissions(resource_type VARCHAR, permission VARCHAR, resource_id VARCHAR, subject_type VARCHAR, subject_relation VARCHAR, subject_id VARCHAR); INSERT INTO permissions VALUES ('person','read','1','user','','alice'), ('person','read','3','user','','alice')").unwrap();
            let mut stmt = conn.prepare(result["sql"].as_str().unwrap()).unwrap();
            let names = stmt
                .query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(names, vec!["Ada"]);
        }
        assert!(result["sql"].as_str().unwrap().contains("permissions"));
    }
}

#[tokio::test]
async fn compiler_owned_constant_relations_do_not_require_materialization() {
    for query in [
        "UNWIND [1,2,3] AS x RETURN x",
        "UNWIND range(3,1,-1) AS x RETURN x",
        "MATCH (n:Missing) RETURN count(n)",
    ] {
        let result = compile(request(query)).await.unwrap();
        assert!(!result["sql"].as_str().unwrap().contains("__graph_rel_"));
    }
    let mut r = request("g.inject(x)");
    r["language"] = json!("gremlin");
    r["bindings"] = json!({"x":{"type":"string","value":"x'); DROP TABLE people; //"}});
    compile(r).await.unwrap();
}

#[tokio::test]
async fn portable_function_compiles_for_both_engines_without_a_host_catalog() {
    for dialect in ["duckdb", "postgres"] {
        let mut r = request("MATCH (p:Person) RETURN fn.coalesce(p.name, 'unknown') AS name");
        r["dialect"] = json!(dialect);
        let result = compile(r).await.unwrap();
        assert_eq!(result["dialect"], dialect);
        assert_eq!(result["fields"], json!(["name"]));
        let sql = result["sql"].as_str().unwrap();
        assert!(sql.to_lowercase().contains("coalesce("), "{sql}");
        assert!(!sql.contains("__orchiddb_logical_"), "{sql}");
        assert!(!sql.contains("__engine_function_"), "{sql}");
    }
}
