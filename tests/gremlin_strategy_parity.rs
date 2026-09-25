use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::ir::interpreter::execute_rows;
use orchiddb::ir::value::Value;
use orchiddb::language::gremlin::{GremlinPlanner, parse_traversal};

fn values(query: &str, graph: &PropertyGraph) -> Vec<Value> {
    let plan = GremlinPlanner::new().plan(&parse_traversal(query).unwrap()).unwrap();
    execute_rows(&plan, graph).unwrap().into_iter().map(|r| r.bindings["current"].clone()).collect()
}

#[test]
fn readonly_verifies_nested_mutations_and_preserves_literal_strings() {
    for mutation in ["addV('person')", "V().property('name','josh')", "V().sideEffect(__.addV('person'))", "V().drop()", "addE('link').from(__.V('a')).to(__.V('b'))"] {
        let error = parse_traversal(&format!("g.withStrategies(ReadOnlyStrategy).{mutation}")).unwrap_err();
        assert!(error.to_string().contains("The provided traversal has a mutating step and thus is not read only"), "{error}");
    }
    assert!(parse_traversal("g.withStrategies(ReadOnlyStrategy).inject('g.addV(1)')").is_ok());
    assert!(parse_traversal("g.inject('ReadOnlyStrategy').addV('person')").is_ok());
    assert!(parse_traversal("g.withStrategies(ReadOnlyStrategy).withoutStrategies(ReadOnlyStrategy).addV('person')").is_ok());
}

#[test]
fn partition_empty_read_set_excludes_every_vertex() {
    let graph=PropertyGraph::new();
    graph.insert_node("person", [("_partition".into(), Value::String("a".into()))].into());
    assert!(values("g.withStrategies(new PartitionStrategy(partitionKey:'_partition',readPartitions:[])).V()", &graph).is_empty());
    assert_eq!(values("g.withStrategies(new PartitionStrategy(partitionKey:'_partition',readPartitions:['a'])).V().count()", &graph),vec![Value::Long(1)]);
}

#[test]
fn partition_writes_are_visible_only_in_selected_partitions() {
    let graph=PropertyGraph::new();
    let source="g.withStrategies(new PartitionStrategy(partitionKey:'_partition',writePartition:'a',readPartitions:['a']))";
    values(&format!("{source}.addV('person').property('name','alice')"), &graph);
    values("g.addV('person').property('_partition','b').property('name','bob')", &graph);
    assert_eq!(values(&format!("{source}.V().values('name')"), &graph),vec![Value::String("alice".into())]);
    values(&format!("{source}.mergeV([(T.label):'person','name':'bob'])"), &graph);
    assert_eq!(values("g.V().has('name','bob').count()", &graph),vec![Value::Long(2)]);
    assert_eq!(values(&format!("{source}.V().has('name','bob').count()"), &graph),vec![Value::Long(1)]);
}

#[test]
fn mixed_decimal_predicates_use_numberhelper_promotion() {
    let graph = PropertyGraph::new();
    for weight in [0.2, 0.4, 0.8] {
        graph.insert_node(
            "measurement",
            [("weight".into(), Value::Float(weight))].into(),
        );
    }
    for query in [
        "g.V().has('weight',eq(0.4M)).values('weight')",
        "g.withStrategies(new SubgraphStrategy(vertices:__.has('weight',eq(0.4M)))).V().values('weight')",
        "g.V().has('weight',within(0.4M)).values('weight')",
        "g.V().has('weight',between(0.4M,0.8M)).values('weight')",
    ] {
        assert_eq!(values(query, &graph), vec![Value::Float(0.4)], "{query}");
    }
    assert_eq!(
        values("g.inject(0.4M,0.2D,0.4D).order()", &graph),
        vec![
            Value::Float(0.2),
            Value::BigDecimal("0.4".parse().unwrap()),
            Value::Float(0.4),
        ]
    );
}
