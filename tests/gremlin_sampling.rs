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
        .flat_map(|r| std::iter::repeat_n(r.get("current"), r.bulk as usize))
        .collect()
}

#[test]
fn seeded_coin_matches_java_and_reinitializes_for_each_execution() {
    let graph = PropertyGraph::new();
    let query = "g.withStrategies(new SeedStrategy(seed:999999)).inject('josh','lop','marko','peter','ripple','vadas').coin(0.5)";
    assert_eq!(values(query, &graph), vec![Value::String("josh".into())]);
    assert_eq!(values(query, &graph), vec![Value::String("josh".into())]);
    assert_eq!(values("g.inject(1,2,3).coin(1.0)", &graph).len(), 3);
    assert!(values("g.inject(1,2,3).coin(0.0)", &graph).is_empty());
}

#[test]
fn each_seeded_step_has_its_own_rng_and_children_inherit_seed() {
    let graph = PropertyGraph::new();
    let sampled = values(
        "g.withStrategies(new SeedStrategy(seed:0)).inject([0,1,2,3,4],[0,1,2,3,4]).map(__.sample(local,2))",
        &graph,
    );
    assert_eq!(
        sampled,
        vec![
            Value::List(vec![Value::Int(0), Value::Int(4)]),
            Value::List(vec![Value::Int(4), Value::Int(2)])
        ]
    );
    assert_eq!(
        values(
            "g.withStrategies(new SeedStrategy(seed:999999)).inject([0.2,0.4,0.4,0.5,0.5,1.0,1.0,1.0]).sample(local,5)",
            &graph
        ),
        vec![Value::List(
            vec![0.5, 1.0, 0.4, 0.2, 1.0]
                .into_iter()
                .map(Value::Float)
                .collect()
        )]
    );
    let single = values(
        "g.withStrategies(new SeedStrategy(seed:0)).inject(0,1,2,3,4).coin(0.5)",
        &graph,
    );
    let two_branches = values(
        "g.withStrategies(new SeedStrategy(seed:0)).inject(0,1,2,3,4).union(__.coin(0.5),__.coin(0.5))",
        &graph,
    );
    let mut expected: Vec<_> = single.into_iter().flat_map(|v| [v.clone(), v]).collect();
    let mut actual = two_branches;
    expected.sort_by_key(|v| v.as_i64());
    actual.sort_by_key(|v| v.as_i64());
    assert_eq!(actual, expected);
}

#[test]
fn weighted_sample_drops_nonproductive_weights_and_supports_traversal_by() {
    let graph = PropertyGraph::new();
    for (label, name, age) in [
        ("software", "app", None),
        ("person", "alice", Some(12)),
        ("person", "bob", Some(43)),
    ] {
        let mut props = [("name".into(), Value::String(name.into()))]
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        if let Some(age) = age {
            props.insert("age".into(), Value::Int(age));
        }
        graph.insert_node(label, props);
    }
    for seed in [0, 1, 2, 42, 999999] {
        for by in ["'age'", "__.values('age')"] {
            let query = format!(
                "g.withStrategies(new SeedStrategy(seed:{seed})).V().order().by(T.label,desc).sample(1).by({by}).label()"
            );
            assert_eq!(values(&query, &graph), vec![Value::String("person".into())]);
        }
    }
    assert_eq!(
        values("g.V().sample(100).by('age').count()", &graph),
        vec![Value::Long(2)]
    );
}

#[test]
fn grouped_samples_match_pinned_seeded_results() {
    let graph = PropertyGraph::new();
    let vertices: Vec<_> = [
        ("person", "marko"),
        ("person", "vadas"),
        ("software", "lop"),
        ("person", "josh"),
        ("software", "ripple"),
        ("person", "peter"),
    ]
    .into_iter()
    .map(|(label, name)| {
        graph.insert_node(label, [("name".into(), Value::String(name.into()))].into())
    })
    .collect();
    for (from, to, label, weight) in [
        (0, 1, "knows", 0.5),
        (0, 3, "knows", 1.0),
        (0, 2, "created", 0.4),
        (3, 4, "created", 1.0),
        (3, 2, "created", 0.4),
        (5, 2, "created", 0.2),
    ] {
        graph
            .insert_edge(
                label,
                &vertices[from],
                &vertices[to],
                [("weight".into(), Value::Float(weight))].into(),
            )
            .unwrap();
    }
    let source = "g.withStrategies(new SeedStrategy(seed:999999)).V().group().by(T.label).by(__.bothE().values('weight').order()";
    let expected = |person: Vec<f64>, software: Vec<f64>| {
        vec![Value::Map(
            [
                (
                    "person".into(),
                    Value::List(person.into_iter().map(Value::Float).collect()),
                ),
                (
                    "software".into(),
                    Value::List(software.into_iter().map(Value::Float).collect()),
                ),
            ]
            .into(),
        )]
    };
    assert_eq!(
        values(&format!("{source}.sample(2).fold())"), &graph),
        expected(vec![0.5, 1.0], vec![1.0, 0.4])
    );
    assert_eq!(
        values(&format!("{source}.fold().sample(local,5))"), &graph),
        expected(vec![0.5, 1.0, 0.4, 0.2, 1.0], vec![0.2, 0.4, 0.4, 1.0])
    );
}
