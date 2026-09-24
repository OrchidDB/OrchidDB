//! Index map keys and PropertyMapStep option / traversal-ring semantics.
use new_graph::ir::catalog::PropertyGraph;
use new_graph::ir::interpreter::execute_rows;
use new_graph::ir::value::{STRUCT_ORDER_KEY, Value};
use new_graph::language::gremlin::{GremlinPlanner, parse_traversal};

fn evaluate(query: &str, graph: &PropertyGraph) -> Vec<Value> {
    let plan = GremlinPlanner::new()
        .plan(&parse_traversal(query).unwrap())
        .unwrap();
    execute_rows(&plan, graph)
        .unwrap()
        .into_iter()
        .map(|row| row.bindings["current"].clone())
        .collect()
}

fn graph() -> PropertyGraph {
    let graph = PropertyGraph::new();
    graph.insert_node(
        "person",
        [
            ("name".into(), Value::String("ana".into())),
            ("age".into(), Value::Int(29)),
            ("city".into(), Value::String("oslo".into())),
        ]
        .into(),
    );
    graph.insert_node(
        "software",
        [("name".into(), Value::String("graph".into()))].into(),
    );
    graph
}

fn property_entries(value: &Value) -> Vec<(Value, Value)> {
    match value {
        Value::TypedMap(entries) => entries.clone(),
        Value::Map(map) => map
            .iter()
            .filter(|(key, _)| key.as_str() != STRUCT_ORDER_KEY)
            .map(|(key, value)| (Value::String(key.clone()), value.clone()))
            .collect(),
        other => panic!("expected map, got {other:?}"),
    }
}

#[test]
fn index_numeric_and_symbolic_options_use_integer_keys() {
    for option in [
        "WithOptions.indexer, WithOptions.map",
        "'~tinkerpop.index.indexer', 1",
    ] {
        assert_eq!(
            evaluate(
                &format!("g.inject('a','b').fold().index().with({option})"),
                &PropertyGraph::new()
            ),
            vec![Value::TypedMap(vec![
                (Value::Int(0), Value::String("a".into())),
                (Value::Int(1), Value::String("b".into()))
            ])]
        );
        assert_eq!(
            evaluate(
                &format!("g.V().hasLabel('software').index().with({option})"),
                &graph()
            ),
            vec![Value::TypedMap(vec![(
                Value::Int(0),
                Value::Node {
                    label: "software".into(),
                    id: 0
                }
            )])]
        );
    }
    assert_eq!(
        evaluate(
            "g.inject('a').index().with('~tinkerpop.index.indexer',0)",
            &PropertyGraph::new()
        ),
        vec![Value::List(vec![Value::List(vec![
            Value::String("a".into()),
            Value::Int(0)
        ])])]
    );
}

#[test]
fn value_map_token_bitmasks_are_independent() {
    for (option, id, label) in [
        ("0", false, false),
        ("1", true, false),
        ("2", false, true),
        ("3", true, true),
        ("15", true, true),
        ("WithOptions.ids", true, false),
        ("WithOptions.labels", false, true),
        ("WithOptions.none", false, false),
    ] {
        let result = evaluate(
            &format!(
                "g.V().hasLabel('person').valueMap('name','age').with(WithOptions.tokens,{option}).by(__.unfold())"
            ),
            &graph(),
        );
        let entries = property_entries(&result[0]);
        assert_eq!(
            entries
                .iter()
                .any(|(key, _)| *key == Value::Token("id".into())),
            id,
            "{option}"
        );
        assert_eq!(
            entries
                .iter()
                .any(|(key, _)| *key == Value::Token("label".into())),
            label,
            "{option}"
        );
        assert!(entries.contains(&(Value::String("name".into()), Value::String("ana".into()))));
        assert!(entries.contains(&(Value::String("age".into()), Value::Int(29))));
    }
}

#[test]
fn value_map_unproductive_modulators_remove_entries_and_keep_empty_maps() {
    let results = evaluate(
        "g.V().valueMap('name','age').by(__.is('x')).by(__.unfold())",
        &graph(),
    );
    assert_eq!(
        property_entries(&results[0]),
        vec![(Value::String("age".into()), Value::Int(29))]
    );
    assert!(property_entries(&results[1]).is_empty());
}

#[test]
fn value_map_ring_cycles_over_present_entries_and_resets_for_each_map() {
    let results = evaluate(
        "g.V().valueMap('missing','name','age','city').by(__.constant('first')).by(__.unfold())",
        &graph(),
    );
    let entries = property_entries(&results[0]);
    assert!(entries.contains(&(Value::String("name".into()), Value::String("first".into()))));
    assert!(entries.contains(&(Value::String("age".into()), Value::Int(29))));
    assert!(entries.contains(&(Value::String("city".into()), Value::String("first".into()))));
    assert_eq!(
        property_entries(&results[1]),
        vec![(Value::String("name".into()), Value::String("first".into()))]
    );
}

#[test]
fn value_map_ring_includes_tokens_and_preserves_first_productive_scalar() {
    let result = evaluate(
        "g.V().hasLabel('person').valueMap('name','age').with(WithOptions.tokens,2).by(__.constant('token')).by(__.unfold())",
        &graph(),
    );
    assert_eq!(
        property_entries(&result[0]),
        vec![
            (Value::Token("label".into()), Value::String("token".into())),
            (Value::String("name".into()), Value::String("ana".into())),
            (Value::String("age".into()), Value::String("token".into())),
        ]
    );
    let graph = PropertyGraph::new();
    graph.insert_node(
        "person",
        [(
            "names".into(),
            Value::List(vec![
                Value::String("first".into()),
                Value::String("second".into()),
            ]),
        )]
        .into(),
    );
    let result = evaluate("g.V().valueMap('names').by(__.unfold())", &graph);
    assert_eq!(
        property_entries(&result[0]),
        vec![(Value::String("names".into()), Value::String("first".into()))]
    );
}

#[test]
fn multi_key_values_preserve_types_order_and_path() {
    assert_eq!(
        evaluate("g.V().values('name','age',null)", &graph()),
        vec![
            Value::String("ana".into()),
            Value::Int(29),
            Value::String("graph".into()),
        ]
    );
    let rows = evaluate(
        "g.V().hasLabel('person').values('name','age').path()",
        &graph(),
    );
    assert_eq!(
        rows,
        vec![
            Value::Path(vec![
                Value::Node {
                    label: "person".into(),
                    id: 0
                },
                Value::String("ana".into())
            ]),
            Value::Path(vec![
                Value::Node {
                    label: "person".into(),
                    id: 0
                },
                Value::Int(29)
            ]),
        ]
    );
}

#[test]
fn multi_key_values_evaluate_the_input_once() {
    let graph = PropertyGraph::new();
    let result = evaluate(
        "g.addV('person').property('name','ana').property('age',29).values('name','age')",
        &graph,
    );
    assert_eq!(result, vec![Value::String("ana".into()), Value::Int(29)]);
    assert_eq!(evaluate("g.V().count()", &graph), vec![Value::Long(1)]);
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn multi_key_values_preserve_native_types_through_graph_engine() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:person {name:'ana',age:29}), (:software {name:'graph'})")
        .await
        .unwrap();
    let result = engine
        .gremlin("g.V().values('name','age',null)")
        .await
        .unwrap();
    let schema = result.returned.batch.schema();
    let rows: serde_json::Value = serde_json::from_str(
        schema
            .metadata()
            .get("crabgraph.gremlin.typed_rows.v1")
            .expect("native typed rows"),
    )
    .unwrap();
    let types = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row[0]["type"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(types, vec!["string", "int", "string"]);
}

#[test]
fn value_map_keeps_productive_null_modulator_results() {
    for query in [
        "g.V().hasLabel('person').valueMap('name').by(__.constant(null))",
        "g.withStrategies(ProductiveByStrategy).V().hasLabel('person').valueMap('name').by(__.values('missing'))",
    ] {
        let values = evaluate(query, &graph());
        assert_eq!(property_entries(&values[0]), vec![(Value::String("name".into()), Value::Null)]);
    }
}
