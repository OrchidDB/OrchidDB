//! AddVertexStep property parameters run against the incoming traverser.
use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::ir::interpreter::execute_rows;
use orchiddb::ir::value::Value;
use orchiddb::language::gremlin::{GremlinPlanner, parse_traversal};

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

#[test]
fn add_vertex_property_traversals_observe_each_incoming_vertex() {
    let graph = PropertyGraph::new();
    for (label, name) in [("person", "marko"), ("software", "lop")] {
        graph.insert_node(label, [("name".into(), Value::String(name.into()))].into());
    }
    let animals = values(
        "g.V().addV('animal').property('name',__.values('name')).property('name','an animal').property(__.values('name'),__.label())",
        &graph,
    );
    assert_eq!(animals.len(), 2);
    for animal in animals {
        let names = graph.properties(&animal, &["name".into()]);
        assert_eq!(names.len(), 2);
        let Value::VertexProperty { value, .. } = &names[0] else {
            panic!("native vertex property expected")
        };
        let Value::String(name) = value.as_ref() else {
            panic!("name expected")
        };
        let expected_label = if name == "marko" {
            "person"
        } else {
            "software"
        };
        let copied_label = graph.properties(&animal, &[name.clone()]);
        assert!(
            matches!(&copied_label[..], [Value::VertexProperty { value, .. }] if value.as_ref() == &Value::String(expected_label.into()))
        );
    }
    assert_eq!(values("g.V().count()", &graph), vec![Value::Long(4)]);
}

#[test]
fn explicit_cardinality_remains_a_property_step_after_folded_parameters() {
    let graph = PropertyGraph::new();
    graph.insert_node(
        "person",
        [("name".into(), Value::String("alice".into()))].into(),
    );
    let result = values(
        "g.V().addV('animal').property(single,'copied',__.values('name')).property('name',__.values('name')).values('copied')",
        &graph,
    );
    assert_eq!(result, vec![Value::String("alice".into())]);
}

#[test]
fn add_vertex_source_and_dynamic_labels_use_the_same_modulator_contract() {
    let graph = PropertyGraph::new();
    assert_eq!(
        values(
            "g.addV('person').property('name',__.constant('alice')).values('name')",
            &graph
        ),
        vec![Value::String("alice".into())]
    );
    assert_eq!(
        values(
            "g.V().addV(__.label()).as('created').property('name',__.values('name')).select('created').values('name')",
            &graph
        ),
        vec![Value::String("alice".into())]
    );
    assert_eq!(
        values("g.V().hasLabel('person').count()", &graph),
        vec![Value::Long(2)]
    );
}
