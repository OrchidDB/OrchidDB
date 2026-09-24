use std::sync::Arc;

use arrow::array::Int64Array;

use super::*;

#[test]
fn cloned_graph_shares_base_indexes_and_isolates_new_edges() {
    let mut original = PropertyGraph::new();
    original.add_nodes(nodes_from_columns(
        "P",
        vec![("n", Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef)],
    ));
    original
        .add_edges(edges_from_columns("R", "P", "P", vec![0], vec![1], vec![]))
        .unwrap();
    let mut cloned = original.clone();
    assert!(Arc::ptr_eq(&original.out_adj, &cloned.out_adj));
    assert!(Arc::ptr_eq(&original.in_adj, &cloned.in_adj));
    assert!(Arc::ptr_eq(
        &original.edge_row_locations,
        &cloned.edge_row_locations
    ));
    cloned
        .add_edges(edges_from_columns("R", "P", "P", vec![1], vec![0], vec![]))
        .unwrap();
    assert!(original.out_edges("P", 1, &[]).is_empty());
    assert!(original.in_edges("P", 0, &[]).is_empty());
    assert_eq!(original.edge_ids("R"), vec![0]);
    assert_eq!(
        cloned.out_edges("P", 1, &[]),
        vec![("R".into(), 1, "P".into(), 0)]
    );
    assert_eq!(
        cloned.in_edges("P", 0, &[]),
        vec![("R".into(), 1, "P".into(), 1)]
    );
    assert_eq!(cloned.edge_ids("R"), vec![0, 1]);
}

#[test]
fn property_lookup_falls_back_to_unique_case_insensitive_column() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "person",
        vec![("ID", Arc::new(Int64Array::from(vec![7])) as ArrayRef)],
    ));

    assert_eq!(graph.node_property("person", 0, "id"), Value::Int(7));
    assert_eq!(graph.node_property("person", 0, "ID"), Value::Int(7));
}

#[test]
fn property_lookup_keeps_ambiguous_case_misses_null() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "person",
        vec![
            ("ID", Arc::new(Int64Array::from(vec![7])) as ArrayRef),
            ("id", Arc::new(Int64Array::from(vec![8])) as ArrayRef),
        ],
    ));

    assert_eq!(graph.node_property("person", 0, "Id"), Value::Null);
}

#[test]
fn insert_edge_rejects_unknown_endpoint_node() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![("n", Arc::new(Int64Array::from(vec![1])) as ArrayRef)],
    ));
    let src = Value::Node {
        label: "P".into(),
        id: 0,
    };
    let dst = Value::Node {
        label: "P".into(),
        id: 99,
    };
    assert!(graph.insert_edge("R", &src, &dst, BTreeMap::new()).is_err());
}

#[test]
fn insert_edge_rejects_endpoint_label_mismatch() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "A",
        vec![("x", Arc::new(Int64Array::from(vec![1])) as ArrayRef)],
    ));
    graph.add_nodes(nodes_from_columns(
        "B",
        vec![("y", Arc::new(Int64Array::from(vec![1])) as ArrayRef)],
    ));
    graph
        .add_edges(edges_from_columns("R", "A", "A", vec![0], vec![0], vec![]))
        .unwrap();
    let src = Value::Node {
        label: "B".into(),
        id: 0,
    };
    let dst = Value::Node {
        label: "B".into(),
        id: 0,
    };
    assert!(graph.insert_edge("R", &src, &dst, BTreeMap::new()).is_err());
}

#[test]
fn delete_node_with_incident_edge_errors_without_detach() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![("n", Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef)],
    ));
    graph
        .add_edges(edges_from_columns("R", "P", "P", vec![0], vec![1], vec![]))
        .unwrap();
    let a = Value::Node {
        label: "P".into(),
        id: 0,
    };
    assert!(graph.delete_value(&a, false).is_err());
    assert_eq!(graph.node_ids("P").unwrap(), vec![0, 1]);
    assert_eq!(graph.edge_ids("R"), vec![0]);
}

#[test]
fn detach_delete_removes_edges_and_repeat_is_idempotent() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![("n", Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef)],
    ));
    graph
        .add_edges(edges_from_columns("R", "P", "P", vec![0], vec![1], vec![]))
        .unwrap();
    let a = Value::Node {
        label: "P".into(),
        id: 0,
    };
    graph.delete_value(&a, true).unwrap();
    graph.delete_value(&a, true).unwrap();
    graph.delete_value(&a, false).unwrap();
    assert_eq!(graph.node_ids("P").unwrap(), vec![1]);
    assert!(graph.edge_ids("R").is_empty());
}

#[test]
fn null_property_is_not_tracked_as_a_key() {
    let mut graph = PropertyGraph::new();
    graph.insert_node("P", BTreeMap::from([("x".to_string(), Value::Null)]));
    assert!(graph.node_property_keys("P").is_empty());
    assert_eq!(graph.node_property("P", 0, "x"), Value::Null);
}

#[test]
fn set_property_to_null_does_not_register_the_key() {
    let mut graph = PropertyGraph::new();
    let n = graph.insert_node("P", BTreeMap::new());
    graph.set_property(&n, "x", Value::Null).unwrap();
    assert!(graph.node_property_keys("P").is_empty());
}

#[test]
fn set_property_to_null_shadows_base_table_value() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![("x", Arc::new(Int64Array::from(vec![5])) as ArrayRef)],
    ));
    let n = Value::Node {
        label: "P".into(),
        id: 0,
    };
    graph.set_property(&n, "x", Value::Null).unwrap();
    assert_eq!(graph.node_property("P", 0, "x"), Value::Null);
}
