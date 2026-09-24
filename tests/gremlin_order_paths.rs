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
