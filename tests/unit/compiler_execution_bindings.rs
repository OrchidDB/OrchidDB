use super::*;
use crate::ir::rel::{host::{SharedHost, legacy::ExecutorHost, managed::ManagedStore}, sql::DuckDbExecutor};
use std::sync::Mutex;

fn host(sql: &str) -> SharedHost {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch(sql).unwrap();
    Arc::new(ExecutorHost::new(Arc::new(Mutex::new(DuckDbExecutor::from_connection(db)))))
}

#[test]
fn prepared_mapped_bindings_attach_to_independent_hosts_without_capturing_rows() {
    let request: CompileRequest = serde_json::from_value(serde_json::json!({
        "version":1, "language":"cypher", "dialect":"duckdb", "query":"MATCH (p:Person) RETURN p.name",
        "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],
        "nodes":[{"label":"Person","table":"people","id":"id","properties":{"name":"name"}}]
    })).unwrap();
    let prepared = prepare_graph(&request).unwrap();
    let bindings = prepared.bindings();
    assert!(!bindings.load_insert_defaults);
    for (name, sql) in [
        ("first", "CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES(1,'first')"),
        ("second", "CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES(1,'second')"),
    ] {
        let graph = bindings.execution_graph(host(sql)).unwrap();
        assert!(Arc::ptr_eq(&graph.procedures, &bindings.procedures));
        assert_eq!(graph.node_property("Person", 1i64.into(), "name"), Value::String(name.into()));
        graph.source.as_ref().unwrap().check().unwrap();
    }
    let mut write = request;
    write.query = "MATCH (p:Person) SET p.name = 'changed' RETURN p.name".into();
    assert!(prepare_graph(&write).unwrap().bindings().load_insert_defaults);
}

#[test]
fn prepared_managed_bindings_restore_properties_and_report_missing_storage() {
    let request: CompileRequest = serde_json::from_value(serde_json::json!({
        "version":1,"language":"cypher","dialect":"duckdb","query":"RETURN 1",
        "managed_table":"managed_graph","tables":[]
    })).unwrap();
    let bindings = prepare_graph(&request).unwrap().bindings();
    let storage = host("");
    assert!(bindings.execution_graph(storage.clone()).is_err());
    let store = ManagedStore::new("managed_graph");
    store.create(storage.as_ref()).unwrap();
    let original = PropertyGraph::new();
    let node = original.insert_node("Person", BTreeMap::from([("name".into(), Value::String("restored".into()))]));
    store.persist(storage.as_ref(), &original).unwrap();
    let restored = bindings.execution_graph(storage).unwrap();
    assert!(Arc::ptr_eq(&restored.procedures, &bindings.procedures));
    let Value::Node { label, id } = node else { panic!() };
    assert_eq!(restored.node_property(&label, id, "name"), Value::String("restored".into()));
}

#[test]
fn graph_preparation_rejects_invalid_keys_and_missing_mapping_columns() {
    let base = serde_json::json!({
        "version":1,"language":"cypher","dialect":"duckdb","query":"MATCH (p:Person) RETURN p.id",
        "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],
        "nodes":[{"label":"Person","table":"people","id":"id"}],
        "edges":[{"label":"LINK","table":"people","id":"id","source":"id","target":"id","source_label":"Person","target_label":"Person"}]
    });
    for (pointer, value, expected) in [
        ("/nodes/0/id",serde_json::json!([]),"key columns must be nonempty"),
        ("/nodes/0/id",serde_json::json!(["id","id"]),"key columns must be nonempty"),
        ("/nodes/0/id",serde_json::json!("missing"),"missing column `people.missing`"),
        ("/nodes/0/table",serde_json::json!("missing"),"unregistered table `missing`"),
        ("/nodes/0/label",serde_json::json!(""),"empty or duplicate node label"),
        ("/edges/0/id",serde_json::json!([]),"key columns must be nonempty"),
        ("/edges/0/source",serde_json::json!(["id","id"]),"key columns must be nonempty"),
        ("/edges/0/target",serde_json::json!("missing"),"missing column `people.missing`"),
        ("/edges/0/source_label",serde_json::json!("Missing"),"edge endpoints must reference mapped node labels"),
        ("/edges/0/label",serde_json::json!(""),"empty or duplicate edge label"),
    ] {
        let mut invalid = base.clone();
        *invalid.pointer_mut(pointer).unwrap() = value.clone();
        let request = match serde_json::from_value(invalid) {
            Ok(request) => request,
            Err(error) => {
                assert!(error.to_string().contains(expected), "{pointer}: {error}");
                // Rust callers can construct keys directly; preparation must also validate them.
                let mut request: CompileRequest = serde_json::from_value(base.clone()).unwrap();
                let key = KeyColumns::from(value.as_array().unwrap().iter().map(|v|v.as_str().unwrap().to_string()).collect::<Vec<_>>());
                match pointer {
                    "/nodes/0/id" => request.nodes[0].id = key,
                    "/edges/0/id" => request.edges[0].id = key,
                    "/edges/0/source" => request.edges[0].source = key,
                    _ => panic!("unexpected invalid wire field: {pointer}"),
                }
                request
            }
        };
        let error = prepare_graph(&request).err().expect("mapping must be rejected");
        assert!(error.contains(expected),"{pointer}: {error}");
    }
}

#[test]
fn graph_preparation_rejects_ambiguous_logical_sources_before_lowering() {
    for (name, default_table, expected) in [
        ("people","people","duplicate table or logical source `people`"),
        ("view","missing","unregistered default table `missing`"),
    ] {
        let request: CompileRequest = serde_json::from_value(serde_json::json!({
            "version":1,"language":"cypher","dialect":"duckdb","query":"RETURN 1",
            "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"}]}],
            "logical_sources":[{"name":name,"default_table":default_table,"layouts":[]}]
        })).unwrap();
        assert_eq!(prepare_graph(&request).err().unwrap(),expected);
    }
}

#[test]
fn diagnostic_classifications_resolve_nested_parameters_without_execution() {
    let request: CompileRequest = serde_json::from_value(serde_json::json!({
        "version":1,"language":"cypher","dialect":"duckdb","query":"RETURN absent",
        "tables":[],"parameters":{"nested":{"items":[null,true,7,"value"]}}
    })).unwrap();
    let diagnostic = cypher_diagnostic(&request).expect("undefined reference diagnostic");
    assert!(diagnostic.to_string().contains("absent"));
}
