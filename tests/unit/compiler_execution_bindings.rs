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
