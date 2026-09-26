#[path = "common/execution.rs"]
mod datafusion_test;
use orchiddb::ir::catalog::{Cardinality, PropertyGraph};
use crate::datafusion_test::execute_rows;
use orchiddb::ir::value::Value;
use orchiddb::language::gremlin::{GremlinPlanner, parse_traversal};
use std::collections::BTreeMap;

fn values(graph: &PropertyGraph, query: &str) -> Vec<Value> {
    let plan = GremlinPlanner::new().plan(&parse_traversal(query).unwrap()).unwrap();
    execute_rows(&plan, graph).unwrap().into_iter().map(|row| row.bindings["current"].clone()).collect()
}

#[test]
fn dynamic_cardinality_values_are_unwrapped_in_merge_on_match() {
    let graph = PropertyGraph::new();
    values(&graph, "g.addV('item').property('x',1)");
    values(&graph,"g.mergeV([:]).option(Merge.onMatch,constant(['x':list(2)]))");
    values(&graph,"g.mergeV([:]).option(Merge.onMatch,constant(['x':Cardinality.set(2)]))");
    assert_eq!(values(&graph,"g.V().values('x')"),vec![Value::Int(1),Value::Int(2)]);
    values(&graph,"g.mergeV([:]).option(Merge.onMatch,constant(['x':single(3)]))");
    assert_eq!(values(&graph,"g.V().values('x')"),vec![Value::Int(3)]);
}

#[test]
fn runtime_maps_preserve_different_cardinalities_per_input_row() {
    let graph = PropertyGraph::new();
    values(&graph,"g.addV('item').property('name','a').property('x',1).addV('item').property('name','b').property('x',1)");
    values(&graph,"g.inject(['name':'a','updates':['x':list(2)]],['name':'b','updates':['x':single(9)]]).mergeV(project('name').by(select('name'))).option(Merge.onMatch,select('updates'))");
    assert_eq!(values(&graph,"g.V().has('name','a').values('x')"),vec![Value::Int(1),Value::Int(2)]);
    assert_eq!(values(&graph,"g.V().has('name','b').values('x')"),vec![Value::Int(9)]);
    values(&graph,"g.withSideEffect('update',['x':set(2)]).mergeV(['name':'a']).option(Merge.onMatch,select('update'))");
    assert_eq!(values(&graph,"g.V().has('name','a').values('x')"),vec![Value::Int(1),Value::Int(2)]);
}

#[test]
fn on_create_consumes_wrappers_and_keeps_list_payloads_as_single_values() {
    let graph = PropertyGraph::new();
    values(&graph,"g.mergeV(['name':'new']).option(Merge.onCreate,constant(['x':set([1,2])]))");
    values(&graph,"g.mergeV(['name':'new']).option(Merge.onMatch,constant(['x':set([1,2])]))");
    assert_eq!(values(&graph,"g.V().values('x')"),vec![Value::List(vec![Value::Int(1),Value::Int(2)])]);
    values(&graph,"g.mergeV(['name':'new']).option(Merge.onMatch,choose(has('name','new'),constant(['x':single(7)]),constant(['x':list(8)])))");
    assert_eq!(values(&graph,"g.V().values('x')"),vec![Value::Int(7)]);
}

#[test]
fn literal_option_maps_still_apply_each_cardinality() {
    let graph = PropertyGraph::new();
    values(&graph,"g.addV('item').property('x',1)");
    values(&graph,"g.mergeV([:]).option(Merge.onMatch,['x':list(2)])");
    values(&graph,"g.mergeV([:]).option(Merge.onMatch,['x':Cardinality.set(2)])");
    assert_eq!(values(&graph,"g.V().values('x')"),vec![Value::Int(1),Value::Int(2)]);
    values(&graph,"g.mergeV([:]).option(Merge.onMatch,['x':single(3)])");
    assert_eq!(values(&graph,"g.V().values('x')"),vec![Value::Int(3)]);
}

#[test]
fn ordinary_maps_never_gain_cardinality_wrapper_semantics() {
    let graph = PropertyGraph::new();
    values(&graph,"g.addV('item').property('x',1)");
    values(&graph,"g.mergeV([:]).option(Merge.onMatch,constant(['x':['cardinality':'list','value':2]]))");
    assert_eq!(values(&graph,"g.V().values('x')"),vec![Value::Map([
        ("cardinality".into(),Value::String("list".into())),
        ("value".into(),Value::Int(2)),
    ].into())]);
    assert_eq!(values(&graph,"g.inject(single(1),list(1),set(1),single(1)).dedup().count()"),vec![Value::Long(3)]);
}

#[test]
fn ephemeral_wrappers_cannot_be_stored_by_ordinary_property_setters() {
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node("item", [("x".into(),Value::Int(1))].into());
    let wrapper = Value::CardinalityValue { cardinality:"list".into(), value:Box::new(Value::Int(2)) };
    let nested = Value::Map([("data".into(),wrapper.clone())].into());
    assert!(graph.set_property(&vertex,"x",wrapper.clone()).is_err());
    assert!(graph.set_properties(&vertex,[("x".into(),nested.clone())].into(),true).is_err());
    assert!(graph.set_vertex_property(&vertex,"x",nested,Cardinality::Single,BTreeMap::new()).is_err());
    assert!(graph.insert_edge("link",&vertex,&vertex,[("x".into(),wrapper)].into()).is_err());
    let plan = GremlinPlanner::new().plan(&parse_traversal("g.V().property('x',single(2))").unwrap()).unwrap();
    assert!(execute_rows(&plan,&graph).is_err());
    assert_eq!(values(&graph,"g.V().values('x')"),vec![Value::Int(1)]);
    let edge = graph.insert_edge("link",&vertex,&vertex,BTreeMap::new()).unwrap();
    graph.set_gremlin_property(&edge,"x",Value::Int(1)).unwrap();
    let plan=GremlinPlanner::new().plan(&parse_traversal("g.mergeE([:]).option(Merge.onMatch,constant(['x':list(2)]))").unwrap()).unwrap();
    assert!(execute_rows(&plan,&graph).unwrap_err().to_string().contains("mergeE does not support"));
    assert_eq!(values(&graph,"g.E().values('x')"),vec![Value::Int(1)]);
}

#[test]
fn null_wrapper_payload_obeys_the_native_null_property_profile() {
    let graph = PropertyGraph::new();
    graph.enable_null_property_values(true);
    values(&graph,"g.mergeV(['name':'nullable']).option(Merge.onCreate,constant(['x':single(null)]))");
    values(&graph,"g.mergeV(['name':'nullable']).option(Merge.onMatch,constant(['x':list(null)]))");
    values(&graph,"g.mergeV(['name':'nullable']).option(Merge.onMatch,constant(['x':set(null)]))");
    assert_eq!(values(&graph,"g.V().values('x')"),vec![Value::Null,Value::Null]);
}

#[test]
fn invalid_wrappers_and_on_create_conflicts_fail_before_graph_mutation() {
    let nested_traversal = "g.mergeV([:]).option(Merge.onMatch,constant(['x':single(__.constant(2))]))";
    assert!(parse_traversal(nested_traversal).unwrap_err().to_string().contains("not a nested traversal"));
    let graph = PropertyGraph::new();
    let conflict = "g.mergeV(['x':2]).option(Merge.onCreate,constant(['x':single(2)]))";
    let plan = GremlinPlanner::new().plan(&parse_traversal(conflict).unwrap()).unwrap();
    assert!(execute_rows(&plan,&graph).unwrap_err().to_string().contains("cannot override"));
    assert_eq!(values(&graph,"g.V().count()"),vec![Value::Long(0)]);
    values(&graph,"g.addV('item').property('x',1)");
    let nested = "g.mergeV([:]).option(Merge.onMatch,constant(['x':single(3),'nested':['value':list(2)]]))";
    let plan = GremlinPlanner::new().plan(&parse_traversal(nested).unwrap()).unwrap();
    assert!(execute_rows(&plan,&graph).is_err());
    assert_eq!(values(&graph,"g.V().values('x')"),vec![Value::Int(1)]);
}
