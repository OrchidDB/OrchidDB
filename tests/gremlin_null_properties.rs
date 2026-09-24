use new_graph::ir::catalog::{Cardinality, PropertyGraph};
use new_graph::ir::interpreter::execute_rows;
use new_graph::ir::value::Value;
use new_graph::language::gremlin::{GremlinPlanner, parse_traversal};
use new_graph::storage::{decode_graph, encode_graph};
use std::collections::BTreeMap;

fn values(graph: &PropertyGraph, query: &str) -> Vec<Value> {
    let plan = GremlinPlanner::new().plan(&parse_traversal(query).unwrap()).unwrap();
    execute_rows(&plan, graph).unwrap().into_iter().map(|row| row.bindings["current"].clone()).collect()
}
fn graph() -> PropertyGraph {
    let graph = PropertyGraph::new();
    graph.enable_null_property_values(true);
    graph
}
fn count(graph: &PropertyGraph, query: &str, expected: i64) {
    assert_eq!(values(graph, &format!("{query}.count()")), vec![Value::Long(expected)], "{query}");
}

#[test]
fn null_vertex_edge_and_meta_values_are_present_and_productive() {
    let graph = graph();
    values(&graph, "g.addV('sensor').property('reading',null,'quality',null).as('a').addV('sensor').addE('link').from('a').property('reading',null)");
    for source in ["g.V()", "g.E()", "g.V().properties('reading')"] {
        let key = if source.ends_with("properties('reading')") { "quality" } else { "reading" };
        count(&graph, &format!("{source}.has('{key}')"), 1);
        count(&graph, &format!("{source}.has('{key}',null)"), 1);
        count(&graph, &format!("{source}.has('missing',null)"), 0);
        assert_eq!(values(&graph, &format!("{source}.values('{key}')")), vec![Value::Null]);
        assert_eq!(values(&graph, &format!("{source}.properties('{key}').value()")), vec![Value::Null]);
        count(&graph, &format!("{source}.values('missing')"), 0);
    }
    count(&graph, "g.V().hasNot('reading')", 1);
    count(&graph, "g.E().hasNot('reading')", 0);
    count(&graph, "g.V().properties('reading').hasNot('quality')", 0);
    count(&graph, "g.V().has('reading').valueMap().select('reading').unfold()", 1);
    assert_eq!(values(&graph, "g.E().valueMap().select('reading')"), vec![Value::Null]);
    values(&graph, "g.V().properties('reading').properties('quality').drop()");
    count(&graph, "g.V().properties('reading').has('quality')", 0);
    values(&graph, "g.E().properties('reading').drop()");
    count(&graph, "g.E().hasNot('reading')", 1);
    count(&graph, "g.E()", 1);
    values(&graph, "g.V().properties('reading').drop()");
    count(&graph, "g.V().has('reading')", 0);
}

#[test]
fn null_cardinality_snapshot_and_checkpoint_rollback() {
    let graph = graph();
    let vertex = graph.insert_node("item", BTreeMap::new());
    let first = graph.set_vertex_property(&vertex,"x",Value::Null,Cardinality::Set,BTreeMap::new()).unwrap();
    let duplicate = graph.set_vertex_property(&vertex,"x",Value::Null,Cardinality::Set,[("n".into(),Value::Null)].into()).unwrap();
    assert_eq!(first, duplicate);
    graph.set_vertex_property(&vertex,"x",Value::Null,Cardinality::List,BTreeMap::new()).unwrap();
    graph.set_vertex_property(&vertex,"x",Value::Int(9),Cardinality::List,BTreeMap::new()).unwrap();
    let edge = graph.insert_edge("loop", &vertex, &vertex, BTreeMap::new()).unwrap();
    graph.set_gremlin_property(&edge,"x",Value::Null).unwrap();
    let mut restored = decode_graph(&encode_graph(&graph).unwrap()).unwrap();
    assert!(restored.supports_null_property_values());
    assert_eq!(values(&restored,"g.V().values('x')"), vec![Value::Null,Value::Null,Value::Int(9)]);
    assert_eq!(values(&restored,"g.V().properties('x').values('n')"), vec![Value::Null]);
    assert_eq!(values(&restored,"g.E().values('x')"), vec![Value::Null]);
    let checkpoint = restored.clone();
    values(&restored,"g.V().property(single,'x',null)");
    assert_eq!(values(&restored,"g.V().values('x')"), vec![Value::Null]);
    values(&restored,"g.E().properties('x').drop()");
    count(&restored,"g.E().has('x')",0);
    restored = checkpoint;
    assert_eq!(values(&restored,"g.V().values('x')"), vec![Value::Null,Value::Null,Value::Int(9)]);
    assert_eq!(values(&restored,"g.E().values('x')"), vec![Value::Null]);
    // Scalar-language null writes continue to remove existing native records.
    restored.set_property(&vertex,"x",Value::Null).unwrap();
    restored.set_property(&edge,"x",Value::Null).unwrap();
    assert!(restored.properties(&vertex,&[]).is_empty());
    assert!(restored.properties(&edge,&[]).is_empty());
}

#[test]
fn merge_writes_and_repeat_presence_use_null_feature_contract() {
    for enabled in [false,true] {
        let graph = PropertyGraph::new();
        graph.enable_null_property_values(enabled);
        values(&graph,"g.addV('item').property('name','one').property('x',7).as('a').addV('item').property('name','two').addE('link').from('a').property('x',7)");
        values(&graph,"g.mergeV([(T.label):'item','name':'one']).option(Merge.onMatch,['x':null])");
        values(&graph,"g.mergeE([(T.label):'link']).option(Merge.onMatch,['x':null])");
        count(&graph,"g.V().has('x')",i64::from(enabled));
        count(&graph,"g.E().has('x')",i64::from(enabled));
        count(&graph,"g.V().has('name','one').repeat(identity()).until(hasNot('x')).times(1)",1);
        if enabled {
            count(&graph,"g.V().has('name','one').repeat(identity()).until(has('x',null)).times(1)",1);
        }
    }
}
