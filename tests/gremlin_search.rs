//! The search service must inspect graph properties, independently of fixture IDs.
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use arrow::array::StringArray;
use new_graph::ir::catalog::{Cardinality, PropertyGraph, nodes_from_columns};
use new_graph::ir::interpreter::execute_rows;
use new_graph::ir::value::Value;
use new_graph::language::gremlin::parser::parse_traversal_with_bindings;
use new_graph::language::gremlin::semantics::GValue;
use new_graph::language::gremlin::{GremlinPlanner, parse_traversal};

fn values(query: &str, graph: &PropertyGraph) -> Vec<Value> {
    let plan = GremlinPlanner::new()
        .plan(&parse_traversal(query).unwrap())
        .unwrap();
    execute_rows(&plan, graph)
        .unwrap()
        .into_iter()
        .map(|row| row.bindings["current"].clone())
        .collect()
}

fn props(key: &str, value: &str) -> BTreeMap<String, Value> {
    BTreeMap::from([(key.into(), Value::String(value.into()))])
}

#[test]
fn bound_search_map_finds_properties_by_value_with_reordered_ids_and_labels() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "account",
        vec![(
            "name",
            Arc::new(StringArray::from(vec!["vadas", "ripple", "marko"])),
        )],
    ));
    let expected = Value::Node {
        label: "account".into(),
        id: 2,
    };
    for suffix in [".element()", ".with('type','Vertex').element()"] {
        let bindings = HashMap::from([(
            "parameters".into(),
            GValue::Map(BTreeMap::from([(
                "search".into(),
                GValue::String("mar".into()),
            )])),
        )]);
        let query = format!("g.call('tinker.search',parameters){suffix}");
        let traversal = parse_traversal_with_bindings(&query, &bindings).unwrap();
        let plan = GremlinPlanner::new().plan(&traversal).unwrap();
        let found: Vec<_> = execute_rows(&plan, &graph)
            .unwrap()
            .into_iter()
            .map(|row| row.bindings["current"].clone())
            .collect();
        assert_eq!(found, vec![expected.clone()], "{query}");
    }
    let found = values("g.call('tinker.search',['search':'mar'])", &graph);
    assert!(
        matches!(&found[..], [Value::VertexProperty { owner, key, value, .. }]
        if owner.as_ref() == &expected && key == "name" && value.as_ref() == &Value::String("marko".into()))
    );
}

#[test]
fn search_accepts_literal_maps_traversal_maps_and_with_parameters() {
    let graph = PropertyGraph::new();
    graph.insert_node("book", props("title", "ordinary"));
    let expected = graph.insert_node("book", props("title", "blue moon"));
    for query in [
        "g.call('tinker.search',['search':'moon']).element()",
        "g.call('tinker.search',__.project('search').by(__.constant('moon'))).element()",
        "g.call('tinker.search').with('search','moon').element()",
        "g.call('tinker.search').with('search',__.constant('moon')).element()",
        "g.call('tinker.search',['search':'ordinary'],__.project('search').by(__.constant('moon'))).element()",
        "g.call('tinker.search',['search':'ordinary']).with('search',__.constant('moon')).with('type',__.constant('Vertex')).element()",
    ] {
        assert_eq!(values(query, &graph), vec![expected.clone()], "{query}");
    }
}

#[test]
fn search_reads_current_graph_state_at_execution() {
    let graph = PropertyGraph::new();
    let query = parse_traversal("g.call('tinker.search',['search':'comet']).element()").unwrap();
    let plan = GremlinPlanner::new().plan(&query).unwrap();
    assert!(execute_rows(&plan, &graph).unwrap().is_empty());
    let vertex = graph.insert_node("object", props("name", "comet"));
    let rows = execute_rows(&plan, &graph).unwrap();
    assert_eq!(rows[0].bindings["current"], vertex);
    graph
        .set_property(&vertex, "name", Value::String("asteroid".into()))
        .unwrap();
    assert!(execute_rows(&plan, &graph).unwrap().is_empty());
    graph
        .set_property(&vertex, "name", Value::String("comet".into()))
        .unwrap();
    graph.delete_value(&vertex, false).unwrap();
    assert!(execute_rows(&plan, &graph).unwrap().is_empty());
}

#[test]
fn search_type_filters_property_owners_including_meta_properties() {
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node("object", props("name", "needle"));
    let other = graph.insert_node("other", Default::default());
    let edge = graph
        .insert_edge("references", &vertex, &other, props("note", "needle"))
        .unwrap();
    let property = graph.properties(&vertex, &["name".into()]).pop().unwrap();
    graph
        .set_property(&property, "source", Value::String("needle".into()))
        .unwrap();
    assert_eq!(
        values(
            "g.call('tinker.search',['search':'needle']).count()",
            &graph
        ),
        vec![Value::Long(3)]
    );
    for (kind, owner) in [
        ("Vertex", vertex),
        ("Edge", edge),
        ("VertexProperty", property),
    ] {
        let query =
            format!("g.call('tinker.search',['search':'needle','type':'{kind}']).element()");
        assert_eq!(values(&query, &graph), vec![owner], "{query}");
    }
}

#[test]
fn search_regex_has_full_match_semantics_and_overrides_search() {
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node(
        "object",
        BTreeMap::from([
            ("name".into(), Value::String("planet".into())),
            ("other".into(), Value::String("planets".into())),
            ("count".into(), Value::Int(42)),
        ]),
    );
    assert!(values("g.call('tinker.search',['regex':'plan'])", &graph).is_empty());
    assert_eq!(
        values(
            "g.call('tinker.search',['regex':'planet','search':'missing']).element()",
            &graph
        ),
        vec![vertex]
    );
    assert_eq!(
        values("g.call('tinker.search',['search':'plan']).count()", &graph),
        vec![Value::Long(2)]
    );
    assert_eq!(
        values("g.call('tinker.search',['regex':'4[0-9]']).value()", &graph),
        vec![Value::Int(42)]
    );
}

#[test]
fn search_validates_parameters_even_on_an_empty_graph() {
    for (query, message) in [
        ("g.call('tinker.search')", "Missing search/regex parameter"),
        (
            "g.call('tinker.search',['search':'a','type':'Bad'])",
            "Type must be one of",
        ),
        (
            "g.call('tinker.search',['regex':'['])",
            "Invalid search regex",
        ),
        ("g.call('tinker.search',['regex':42])", "must be a string"),
        (
            "g.call('tinker.search',__.constant('invalid'))",
            "must be a map",
        ),
    ] {
        let plan = GremlinPlanner::new()
            .plan(&parse_traversal(query).unwrap())
            .unwrap();
        let error = execute_rows(&plan, &PropertyGraph::new())
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{query}: {error}");
    }
    assert!(parse_traversal("g.call('tinker.search',missing)").is_err());
}

#[test]
fn search_returns_each_matching_multi_property_and_tracks_property_removal() {
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node("object", Default::default());
    let first = graph
        .set_vertex_property(
            &vertex,
            "tag",
            Value::String("amber".into()),
            Cardinality::List,
            Default::default(),
        )
        .unwrap();
    let second = graph
        .set_vertex_property(
            &vertex,
            "tag",
            Value::String("amber".into()),
            Cardinality::List,
            Default::default(),
        )
        .unwrap();
    let query = "g.call('tinker.search',['search':'amber'])";
    assert_eq!(values(query, &graph), vec![first.clone(), second.clone()]);
    graph.delete_value(&first, false).unwrap();
    assert_eq!(values(query, &graph), vec![second]);
}

#[test]
fn property_intrinsics_are_distinct_from_same_named_meta_properties() {
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node("object", props("name", "atlas"));
    let property = graph.properties(&vertex, &["name".into()]).pop().unwrap();
    graph
        .set_property(&property, "value", Value::String("meta-value".into()))
        .unwrap();
    graph
        .set_property(&property, "key", Value::String("meta-key".into()))
        .unwrap();
    for (step, expected) in [
        ("value()", "atlas"),
        ("key()", "name"),
        ("values('value')", "meta-value"),
        ("values('key')", "meta-key"),
    ] {
        assert_eq!(
            values(&format!("g.V().properties('name').{step}"), &graph),
            vec![Value::String(expected.into())]
        );
    }
    assert_eq!(
        values(
            "g.V().properties('name').properties('value').value()",
            &graph
        ),
        vec![Value::String("meta-value".into())]
    );
    assert_eq!(
        values("g.inject(['x':42]).unfold().key()", &graph),
        vec![Value::String("x".into())]
    );
    assert_eq!(
        values("g.inject(['x':42]).unfold().value()", &graph),
        vec![Value::Int(42)]
    );
    let plan = GremlinPlanner::new()
        .plan(&parse_traversal("g.inject(['value':42]).value()").unwrap())
        .unwrap();
    assert!(execute_rows(&plan, &graph).is_err());
}
