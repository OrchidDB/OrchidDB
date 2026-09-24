use new_graph::ir::{PropertyGraph, Value, execute_rows};
use new_graph::language::gremlin::{GremlinPlanner, parse_traversal};

fn graph() -> PropertyGraph {
    let g = PropertyGraph::new();
    let a = g.insert_node("start", Default::default());
    let b = g.insert_node("middle", Default::default());
    let c = g.insert_node("leaf", [("name".into(),Value::String("zulu".into())),("lang".into(),Value::String("java".into()))].into());
    let d = g.insert_node("leaf", [("name".into(),Value::String("alpha".into())),("lang".into(),Value::String("java".into()))].into());
    for (from,to) in [(&a,&b),(&b,&c),(&b,&d)] { g.insert_edge("link",from,to,Default::default()).unwrap(); }
    g
}

#[tokio::test]
async fn path_order_keeps_element_prefix_before_projected_values() {
    let g = graph();
    let query = "g.V().out().out().values().as('head').path().order().select('head')";
    let plan = GremlinPlanner::new().plan(&parse_traversal(query).unwrap()).unwrap();
    let direct = execute_rows(&plan,&g).unwrap().into_iter().map(|r|r.bindings["current"].clone()).collect::<Vec<_>>();
    assert_eq!(direct, vec![Value::String("java".into()),Value::String("zulu".into()),Value::String("alpha".into()),Value::String("java".into())]);
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine.replace_graph(g).unwrap();
    let result = engine.gremlin(query).await.unwrap();
    let rows: serde_json::Value = serde_json::from_str(&result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"]).unwrap();
    assert_eq!(rows.as_array().unwrap().iter().map(|row|row[0]["value"].as_str().unwrap()).collect::<Vec<_>>(),vec!["java","zulu","alpha","java"], "backend {:?}",result.backend);
}

#[tokio::test]
async fn property_paths_order_by_public_ids_in_both_directions() {
    use new_graph::ir::catalog::Cardinality;
    let g = PropertyGraph::new();
    let root = g.insert_node("root", Default::default());
    let high = g.insert_node("leaf", Default::default());
    let low = g.insert_node("leaf", Default::default());
    g.set_element_public_id(&high, Value::Long(30)).unwrap();
    g.set_element_public_id(&low, Value::Long(10)).unwrap();
    for (vertex, text) in [(&high, "first"), (&low, "second")] {
        g.set_vertex_property(vertex, "name", Value::String(text.into()), Cardinality::Single, Default::default()).unwrap();
        g.insert_edge("link", &root, vertex, Default::default()).unwrap();
    }
    let late = g.set_vertex_property(&root, "tag", Value::String("late".into()), Cardinality::List, Default::default()).unwrap();
    let early = g.set_vertex_property(&root, "tag", Value::String("early".into()), Cardinality::List, Default::default()).unwrap();
    g.set_vertex_property_public_id(&late, Value::Long(300)).unwrap();
    g.set_vertex_property_public_id(&early, Value::Long(100)).unwrap();
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine.replace_graph(g.clone()).unwrap();
    for (source, asc) in [
        ("g.V().hasLabel('root').out().properties('name').as('head').path()", vec!["second", "first"]),
        ("g.V().hasLabel('root').properties('tag').as('head').path()", vec!["early", "late"]),
    ] {
        for descending in [false, true] {
            let direction = if descending { "desc" } else { "asc" };
            let query = format!("{source}.order().by(Order.{direction}).select('head').value()");
            let mut expected = asc.clone();
            if descending { expected.reverse(); }
            let plan = GremlinPlanner::new().plan(&parse_traversal(&query).unwrap()).unwrap();
            let direct = execute_rows(&plan, &g).unwrap().into_iter().map(|r| r.bindings["current"].clone()).collect::<Vec<_>>();
            assert_eq!(direct, expected.iter().map(|s| Value::String((*s).into())).collect::<Vec<_>>(), "{query}");
            let result = engine.gremlin(&query).await.unwrap();
            let rows: serde_json::Value = serde_json::from_str(&result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"]).unwrap();
            assert_eq!(rows.as_array().unwrap().iter().map(|row| row[0]["value"].as_str().unwrap()).collect::<Vec<_>>(), expected, "{query}");
        }
    }
}

#[test]
fn native_property_set_and_ordinary_property_shaped_map_keep_distinct_order_classes() {
    use new_graph::ir::catalog::Cardinality;
    let g = PropertyGraph::new();
    let vertex = g.insert_node("person", Default::default());
    g.set_vertex_property(&vertex, "tag", Value::Int(1), Cardinality::Single,
        [("meta".into(), Value::Int(2))].into()).unwrap();
    let query = "g.V().properties('tag').union(__.identity(),__.properties('meta'),__.constant([1,2]).dedup(local),__.constant(['element':1,'key':'tag','value':2,'__id':0])).order()";
    let plan = GremlinPlanner::new().plan(&parse_traversal(query).unwrap()).unwrap();
    let values = execute_rows(&plan, &g).unwrap().into_iter().map(|r| r.bindings["current"].clone()).collect::<Vec<_>>();
    assert!(matches!(&values[..], [Value::VertexProperty { .. }, Value::Property { .. }, Value::Set(_), Value::Map(_)]), "{values:?}");
    assert_ne!(values[0], values[3]);
    assert_ne!(values[1], values[3]);
}
