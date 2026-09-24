//! MergeStep validation timing and onMatch traversal contracts from TinkerPop 3.7.4.
use new_graph::ir::catalog::PropertyGraph;
use new_graph::ir::interpreter::execute_rows;
use new_graph::ir::value::Value;
use new_graph::language::gremlin::{GremlinPlanner, parse_traversal};

fn run(query: &str, graph: &PropertyGraph) -> Result<Vec<Value>, String> {
    let traversal = parse_traversal(query).map_err(|error| error.to_string())?;
    let plan = GremlinPlanner::new()
        .plan(&traversal)
        .map_err(|error| error.to_string())?;
    execute_rows(&plan, graph)
        .map(|rows| {
            rows.into_iter()
                .map(|row| row.bindings["current"].clone())
                .collect()
        })
        .map_err(|error| error.to_string())
}

fn linked_graph() -> PropertyGraph {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("person", Default::default());
    let b = graph.insert_node("person", Default::default());
    graph
        .insert_edge("knows", &a, &b, [("weight".into(), Value::Int(1))].into())
        .unwrap();
    graph
}

#[test]
fn missing_merge_endpoints_use_the_upstream_diagnostic() {
    for query in [
        "g.mergeE([(Direction.OUT):'100',(Direction.IN):'200'])",
        "g.mergeE([(Direction.OUT):'100',(Direction.IN):'200']).option(Merge.onCreate,['weight':1]).option(Merge.onMatch,['weight':0])",
    ] {
        let graph = PropertyGraph::new();
        assert!(
            run(query, &graph)
                .unwrap_err()
                .contains("Vertex id could not be resolved from mergeE")
        );
        assert_eq!(run("g.E().count()", &graph).unwrap(), vec![Value::Long(0)]);
    }
}

#[test]
fn literal_on_match_map_is_validated_even_without_matches() {
    let graph = PropertyGraph::new();
    graph.insert_node("person", Default::default());
    let error = run("g.V().as('v').mergeE([(T.label):'self',(Direction.OUT):Merge.outV,(Direction.IN):Merge.inV]).option(Merge.onMatch,['~label':'vertex']).option(Merge.outV,__.select('v')).option(Merge.inV,__.select('v'))", &graph).unwrap_err();
    assert!(
        error.contains("Property key can not be a hidden key: ~label"),
        "{error}"
    );
    assert_eq!(run("g.E().count()", &graph).unwrap(), vec![Value::Long(0)]);
}

#[test]
fn on_create_cannot_override_public_id() {
    let graph = PropertyGraph::new();
    let error = run(
        "g.mergeV([(T.id):'1']).option(Merge.onCreate,[(T.id):'2'])",
        &graph,
    )
    .unwrap_err();
    assert!(
        error.contains("option(onCreate) cannot override values from merge() argument"),
        "{error}"
    );
    assert_eq!(run("g.V().count()", &graph).unwrap(), vec![Value::Long(0)]);
}

#[test]
fn on_match_element_guard_runs_before_the_child_mutates() {
    let graph = linked_graph();
    let error = run("g.V().mergeE([(T.label):'knows']).option(Merge.onMatch,__.sideEffect(__.property('weight',0)).constant([:]))", &graph).unwrap_err();
    assert!(
        error.contains("The incoming traverser for MergeEdgeStep cannot be an Element"),
        "{error}"
    );
    assert_eq!(
        run("g.E().values('weight')", &graph).unwrap(),
        vec![Value::Int(1)]
    );
    assert_eq!(
        run("g.V().has('weight').count()", &graph).unwrap(),
        vec![Value::Long(0)]
    );
    // A literal map remains legal on an incoming Element.
    assert_eq!(
        run(
            "g.V().mergeE([(T.label):'knows']).option(Merge.onMatch,['weight':2]).count()",
            &graph
        )
        .unwrap(),
        vec![Value::Long(2)]
    );
    // Traversal options are lazy and the guard does not reject the create path.
    assert_eq!(run("g.V().as('v').mergeE([(T.label):'self',(Direction.OUT):Merge.outV,(Direction.IN):Merge.inV]).option(Merge.outV,__.select('v')).option(Merge.inV,__.select('v')).option(Merge.onMatch,__.constant([:])).count()", &graph).unwrap(), vec![Value::Long(2)]);
}

#[test]
fn start_on_match_executes_side_effects_once_with_an_empty_return_map() {
    let graph = linked_graph();
    assert_eq!(run("g.mergeE([(T.label):'knows']).option(Merge.onMatch,__.sideEffect(__.property('weight',0)).sideEffect(__.addV('audit')).constant([:])).count()", &graph).unwrap(), vec![Value::Long(1)]);
    assert_eq!(
        run("g.E().values('weight')", &graph).unwrap(),
        vec![Value::Int(0)]
    );
    assert_eq!(
        run("g.V().hasLabel('audit').count()", &graph).unwrap(),
        vec![Value::Long(1)]
    );
    assert_eq!(
        run("g.V().has('weight').count()", &graph).unwrap(),
        vec![Value::Long(0)]
    );
}

#[test]
fn merges_create_and_refind_public_ids_without_duplicate_elements() {
    let graph = PropertyGraph::new();
    let a = run(
        "g.mergeV([(T.id):'1']).option(Merge.onCreate,[(T.label):'person','name':'mike'])",
        &graph,
    )
    .unwrap()
    .remove(0);
    assert_eq!(graph.element_public_id(&a), Value::String("1".into()));
    assert_eq!(
        run("g.mergeV([(T.id):'1'])", &graph).unwrap(),
        vec![a.clone()]
    );
    let b = run(
        "g.mergeV([(T.label):'person','name':'other']).option(Merge.onCreate,[(T.id):'2'])",
        &graph,
    )
    .unwrap()
    .remove(0);
    assert_eq!(graph.element_public_id(&b), Value::String("2".into()));
    let edge = run("g.mergeE([(Direction.OUT):'1',(Direction.IN):'2',(T.label):'knows']).option(Merge.onCreate,[(T.id):'201'])", &graph).unwrap().remove(0);
    assert_eq!(graph.element_public_id(&edge), Value::String("201".into()));
    assert_eq!(run("g.mergeE([(T.id):'201'])", &graph).unwrap(), vec![edge]);
    assert_eq!(run("g.V().count()", &graph).unwrap(), vec![Value::Long(2)]);
    assert_eq!(run("g.E().count()", &graph).unwrap(), vec![Value::Long(1)]);
    // A failed match with an existing public ID must not insert a partial node.
    assert!(
        run("g.mergeV([(T.id):'1','name':'conflict'])", &graph)
            .unwrap_err()
            .contains("already exists")
    );
    assert_eq!(run("g.V().count()", &graph).unwrap(), vec![Value::Long(2)]);
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn on_match_effects_commit_and_roll_back_with_the_traversal() {
    let mut engine = new_graph::engine::GraphEngine::in_memory().unwrap();
    engine.gremlin("g.addV('person').as('a').addV('person').as('b').addE('knows').from('a').to('b').property('weight',1).none()").await.unwrap();
    let error = engine.gremlin("g.mergeE([(T.label):'knows']).option(Merge.onMatch,__.sideEffect(__.property('weight',0)).constant(['~illegal':1]))").await.unwrap_err().to_string();
    assert!(
        error.contains("Property key can not be a hidden key: ~illegal"),
        "{error}"
    );
    let result = engine.gremlin("g.E().values('weight')").await.unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 1);
    engine.gremlin("g.mergeE([(T.label):'knows']).option(Merge.onMatch,__.sideEffect(__.property('weight',0)).constant([:])).none()").await.unwrap();
    let result = engine.gremlin("g.E().values('weight')").await.unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["crabgraph.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 0);
}

#[test]
fn merge_cardinality_overrides_and_defaults_preserve_property_records() {
    for criteria in ["['name':'alice']", "[(T.id):'alice']"] {
        let graph = PropertyGraph::new();
        let vertex = run(&format!("g.mergeV({criteria}).option(Merge.onCreate,['name':'alice','age':Cardinality.set(31)])"), &graph).unwrap().remove(0);
        for cardinality in ["list", "set"] {
            run(&format!("g.mergeV({criteria}).option(Merge.onMatch,['age':Cardinality.{cardinality}(32)])"), &graph).unwrap();
        }
        assert_eq!(graph.properties(&vertex, &["age".into()]).len(), 2);
        // Criteria can match the second record without creating another vertex.
        assert_eq!(
            run("g.mergeV(['age':32])", &graph).unwrap(),
            vec![vertex.clone()]
        );
        run(
            &format!("g.mergeV({criteria}).option(Merge.onMatch,['age':33],single)"),
            &graph,
        )
        .unwrap();
        assert_eq!(graph.properties(&vertex, &["age".into()]).len(), 1);
        run(&format!("g.mergeV({criteria}).option(Merge.onMatch,['name':'bob','age':Cardinality.list(34)],single)"), &graph).unwrap();
        assert_eq!(graph.properties(&vertex, &["age".into()]).len(), 2);
        let names = graph.properties(&vertex, &["name".into()]);
        assert!(
            matches!(&names[..], [Value::VertexProperty { value, .. }] if value.as_ref() == &Value::String("bob".into()))
        );
    }
    for criteria in ["['age':31]", "[(T.id):'alice','age':31]"] {
        let graph = PropertyGraph::new();
        let error = run(
            &format!("g.mergeV({criteria}).option(Merge.onCreate,['age':Cardinality.set(31)])"),
            &graph,
        )
        .unwrap_err();
        assert!(
            error.contains("option(onCreate) cannot override values from merge() argument"),
            "{error}"
        );
        assert_eq!(run("g.V().count()", &graph).unwrap(), vec![Value::Long(0)]);
    }
}

#[test]
fn ordinary_id_properties_do_not_replace_public_element_identity() {
    let graph = PropertyGraph::new();
    for query in [
        "g.mergeV(['id':'ordinary'])",
        "g.mergeV([(T.id):'public','id':'ordinary'])",
        "g.addV('person').property('id','ordinary')",
    ] {
        let element = run(query, &graph).unwrap().remove(0);
        assert_ne!(
            graph.element_public_id(&element),
            Value::String("ordinary".into())
        );
        assert_eq!(graph.properties(&element, &["id".into()]).len(), 1);
    }
}
