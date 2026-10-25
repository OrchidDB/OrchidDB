//! Engine execution regressions with independent, explicit expected results.
use orchiddb::ir::jvm::JvmExecution;
use orchiddb::ir::rel::runtime::execute_rows_with_jvm;
use orchiddb::ir::{GraphPlan, PropertyGraph, Value};
use orchiddb::language::gremlin::{GremlinPlanner, parse_traversal};
use std::collections::BTreeMap;

fn plan(query: &str) -> GraphPlan {
    GremlinPlanner::new()
        .plan(&parse_traversal(query).unwrap())
        .unwrap()
}
fn graph() -> PropertyGraph {
    let graph = PropertyGraph::new();
    let a = graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), Value::String("a".into())),
            ("age".into(), Value::Int(29)),
        ]),
    );
    let b = graph.insert_node(
        "person",
        BTreeMap::from([
            ("name".into(), Value::String("b".into())),
            ("age".into(), Value::Int(35)),
        ]),
    );
    let c = graph.insert_node(
        "software",
        BTreeMap::from([("name".into(), Value::String("c".into()))]),
    );
    graph.insert_edge("knows", &a, &b, BTreeMap::new()).unwrap();
    graph
        .insert_edge("created", &a, &c, BTreeMap::new())
        .unwrap();
    graph
}
fn current_values(rows: Vec<orchiddb::ir::runtime::Row>) -> Vec<Value> {
    rows.into_iter()
        .flat_map(|row| std::iter::repeat_n(row.bindings["current"].clone(), row.bulk as usize))
        .collect()
}
fn sorted(mut values: Vec<Value>) -> Vec<Value> {
    fn canonical_map(value: &mut Value) {
        match value {
            Value::TypedMap(entries) => {
                for (key, value) in entries.iter_mut() {
                    canonical_map(key);
                    canonical_map(value);
                }
                entries.sort_by_key(|(key, value)| format!("{key:?}:{value:?}"));
            }
            Value::Map(entries) => entries.values_mut().for_each(canonical_map),
            Value::List(items) => items.iter_mut().for_each(canonical_map),
            _ => {}
        }
    }
    values.iter_mut().for_each(canonical_map);
    values.sort_by_key(|v| format!("{v:?}"));
    values
}

#[tokio::test]
async fn grouped_property_collection_drops_absent_values() {
    let (rows, _) = execute_rows_with_jvm(&plan("g.V().group().by('name').by('age')"), &graph(), JvmExecution::default()).await.unwrap();
    let values = current_values(rows);
    let entries = match &values[0] {
        Value::Map(entries) => entries.clone(),
        Value::TypedMap(entries) => entries.iter().map(|(key, value)| {
            let Value::String(key) = key else { panic!("expected string group key") };
            (key.clone(), value.clone())
        }).collect(),
        other => panic!("expected grouped map, got {other:?}"),
    };
    assert_eq!(entries["a"], Value::List(vec![Value::Int(29)]));
    assert_eq!(entries["b"], Value::List(vec![Value::Int(35)]));
    assert_eq!(entries["c"], Value::List(vec![]));
}
#[tokio::test]
async fn fold_map_reduction_preserves_injected_order() {
    let query = "g.inject(['a':1], ['b':2], ['b':4]).fold([:], Operator.addAll)";
    let (rows, _) = execute_rows_with_jvm(&plan(query), &graph(), JvmExecution::default()).await.unwrap();
    assert_eq!(current_values(rows), vec![Value::Map(BTreeMap::from([("a".into(), Value::Int(1)), ("b".into(), Value::Int(4))]))]);
}
#[tokio::test]
async fn mutation_aggregate_labels_survive_sql_reentry() {
    for query in [
        "g.V().limit(3).addV('software').aggregate('a1').by(T.label).cap('a1')",
        "g.V().limit(3).addV('software').aggregate('a1').by(T.label).aggregate('a2').by(T.label).cap('a1','a2').select('a1','a2').by(unfold().fold())",
    ] {
        let (rows, _) = execute_rows_with_jvm(&plan(query), &graph(), JvmExecution::default()).await.unwrap();
        let labels = Value::List(vec![Value::String("software".into()); 3]);
        let expected = if query.contains("a2") { Value::Map(BTreeMap::from([("a1".into(), labels.clone()), ("a2".into(), labels)])) } else { Value::BulkSet(vec![Value::String("software".into()); 3]) };
        assert_eq!(current_values(rows), vec![expected], "{query}");
    }
}
#[tokio::test]
async fn sql_tail_keeps_order_and_logical_multiplicity() {
    for (query, expected) in [
        ("g.V().values('name').order().tail(2)", vec![Value::String("b".into()), Value::String("c".into())]),
        ("g.V().values('name').order().tail(0)", vec![]),
        ("g.V().values('name').order().tail(7)", vec![Value::String("a".into()), Value::String("b".into()), Value::String("c".into())]),
        ("g.inject(3,1,2).order().tail()", vec![Value::Int(3)]),
        ("g.V().repeat(both()).times(3).tail(7).count()", vec![Value::Long(7)]),
    ] {
        let (rows, _) = execute_rows_with_jvm(&plan(query), &graph(), JvmExecution::default()).await.unwrap();
        assert_eq!(current_values(rows), expected, "{query}");
    }
}

#[tokio::test]
async fn graph_service_reads_resume_sql_projection() {
    for (query, expected) in [
        ("g.V().call('tinker.degree.centrality').order()", vec![Value::Long(0), Value::Long(1), Value::Long(1)]),
        ("g.V().call('tinker.degree.centrality').with('direction',OUT).order()", vec![Value::Long(0), Value::Long(0), Value::Long(2)]),
        ("g.call('tinker.search').with('search','b').element().values('name')", vec![Value::String("b".into())]),
    ] {
        let (rows, _) = execute_rows_with_jvm(&plan(query), &graph(), JvmExecution::default())
            .await.unwrap_or_else(|error| panic!("{query}: {error}"));
        assert_eq!(current_values(rows), expected, "{query}");
    }
}
#[tokio::test]
async fn element_reference_sources_feed_sql_projections() {
    use orchiddb::language::gremlin::{parse_traversal_with_bindings, semantics::GValue};
    let graph = PropertyGraph::new();
    let a = graph.insert_node("person", BTreeMap::new());
    let b = graph.insert_node("person", BTreeMap::from([("name".into(), Value::String("b".into()))]));
    let c = graph.insert_node("person", BTreeMap::from([("name".into(), Value::String("c".into()))]));
    graph.insert_edge("knows", &a, &b, BTreeMap::new()).unwrap();
    graph.insert_edge("knows", &a, &c, BTreeMap::new()).unwrap();
    let key = |value: &Value| match graph.element_public_id(value) {
        Value::String(id) => GValue::String(id),
        Value::Int(id) | Value::Long(id) => GValue::Long(id),
        _ => unreachable!(),
    };
    let mut bindings = std::collections::HashMap::from([
        ("vid1".into(), key(&a)),
        ("v2".into(), GValue::VertexRef { id: Box::new(key(&b)), label: "descriptive-only".into() }),
    ]);
    let query = "g.withSideEffect('a',v2).V(vid1).out().where(P.neq('a')).values('name')";
    let compile = |bindings: &std::collections::HashMap<String, GValue>| {
        let arguments = bindings.iter().map(|(key, value)| (key.clone(),
            orchiddb::language::gremlin::GremlinBinding::Value(value.clone()))).collect();
        let (query, values) = orchiddb::language::gremlin::callables::prepare(query, &arguments).unwrap();
        GremlinPlanner::new().plan(&parse_traversal_with_bindings(&query, &values).unwrap()).unwrap()
    };
    let (rows, stats) = execute_rows_with_jvm(&compile(&bindings), &graph, JvmExecution::default()).await.unwrap();
    assert_eq!(current_values(rows), vec![Value::String("c".into())]);
    assert_eq!(stats.datafusion_operators, 0);
    let property = graph.properties(&b, &["name".into()]).remove(0);
    let property_binding = GValue::VertexPropertyRef {
        id: Box::new(key(&property)),
        owner: Box::new(GValue::VertexRef { id: Box::new(key(&b)), label: "person".into() }),
        key: "name".into(),
    };
    let arguments = std::collections::HashMap::from([("p".into(),
        orchiddb::language::gremlin::GremlinBinding::Value(property_binding))]);
    let (query, values) = orchiddb::language::gremlin::callables::prepare("g.inject(p).value()", &arguments).unwrap();
    let property_plan = GremlinPlanner::new().plan(&parse_traversal_with_bindings(&query, &values).unwrap()).unwrap();
    let (rows, _) = execute_rows_with_jvm(&property_plan, &graph, JvmExecution::default()).await.unwrap();
    assert_eq!(current_values(rows), vec![Value::String("b".into())]);
    bindings.insert("v2".into(), GValue::VertexRef { id: Box::new(GValue::Long(i64::MAX)), label: "person".into() });
    assert!(execute_rows_with_jvm(&compile(&bindings), &graph, JvmExecution::default()).await
        .unwrap_err().contains("does not exist"));
}

#[tokio::test]
async fn merge_edge_resolves_references_inside_sql_maps_and_lists() {
    use orchiddb::language::gremlin::{parse_traversal_with_bindings, semantics::GValue};
    for query in ["g.mergeE(criteria)", "g.inject(criteria).mergeE()"] {
        let graph = PropertyGraph::new();
        let a = graph.insert_node("person", BTreeMap::new());
        let b = graph.insert_node("person", BTreeMap::new());
        let reference = |value: &Value| GValue::VertexRef {
            id: Box::new(match graph.element_public_id(value) {
                Value::String(id) => GValue::String(id),
                Value::Int(id) | Value::Long(id) => GValue::Long(id),
                _ => unreachable!(),
            }),
            label: "person".into(),
        };
        let bindings: std::collections::HashMap<String, GValue> = std::collections::HashMap::from([("criteria".into(), GValue::TypedMap(vec![
            (GValue::Token("label".into()), GValue::String("knows".into())),
            (GValue::DirectionToken("OUT".into()), reference(&a)),
            (GValue::DirectionToken("IN".into()), reference(&b)),
        ]))]);
        let arguments = bindings.iter().map(|(key, value)| (key.clone(),
            orchiddb::language::gremlin::GremlinBinding::Value(value.clone()))).collect();
        let (query, values) = orchiddb::language::gremlin::callables::prepare(query, &arguments).unwrap();
        let plan = GremlinPlanner::new().plan(&parse_traversal_with_bindings(&query, &values).unwrap()).unwrap();
        let (rows, stats) = execute_rows_with_jvm(&plan, &graph, JvmExecution::default())
            .await.unwrap_or_else(|error| panic!("{query}: {error}"));
        let values = current_values(rows);
        assert_eq!(values.len(), 1, "{query}");
        assert!(matches!(&values[0], Value::Edge { rel_type, .. } if rel_type == "knows"));
        assert_eq!(stats.datafusion_operators, 0);
    }
}

#[tokio::test]
async fn regex_predicates_keep_boolean_types_across_sql_boundaries() {
    let graph = graph();
    graph.insert_node("software", BTreeMap::from([
        ("name".into(), Value::String("TinkerGraph ©".into())),
    ]));
    for (query, expected) in [
        ("g.V().has('name',TextP.regex('^a')).values('name')", vec![Value::String("a".into())]),
        ("g.V().has('name',TextP.regex('Tinker')).values('name')", vec![Value::String("TinkerGraph ©".into())]),
        ("g.V().has('name',TextP.regex('Tinker.*©')).values('name')", vec![Value::String("TinkerGraph ©".into())]),
    ] {
        let (rows, stats) = execute_rows_with_jvm(&plan(query), &graph, JvmExecution::default())
            .await.unwrap_or_else(|error| panic!("{query}: {error}"));
        assert_eq!(current_values(rows), expected, "{query}");
        assert_eq!(stats.datafusion_operators, 0);
    }
}

#[tokio::test]
async fn typed_traversers_and_correlated_state_have_expected_results() {
    let text = |s: &str| Value::String(s.into());
    let a = Value::Node {
        label: "person".into(),
        id: 0.into(),
    };
    let b = Value::Node {
        label: "person".into(),
        id: 1.into(),
    };
    let c = Value::Node {
        label: "software".into(),
        id: 0.into(),
    };
    for (query, expected) in [
        (
            "g.V().groupCount().by('age')",
            vec![Value::TypedMap(vec![
                (Value::Int(29), Value::Long(1)),
                (Value::Int(35), Value::Long(1)),
            ])],
        ),
        (
            "g.V().group().by(__.label()).by(__.count())",
            vec![Value::Map(BTreeMap::from([
                ("person".into(), Value::Long(2)),
                ("software".into(), Value::Long(1)),
            ]))],
        ),
        (
            "g.V().choose(__.has('name','a'),__.values('name'),__.identity())",
            vec![text("a"), b, c],
        ),
        (
            "g.V().outE().inV().values('name')",
            vec![text("b"), text("c")],
        ),
        (
            "g.V().local(__.out().count())",
            vec![Value::Long(2), Value::Long(0), Value::Long(0)],
        ),
        (
            "g.V().local(__.out().has('name','b').values('age'))",
            vec![Value::Int(35)],
        ),
        (
            "g.V().filter(__.out().has('name','b')).values('name')",
            vec![text("a")],
        ),
        (
            "g.V().has('name','a').as('a').out().as('a').select(Pop.first,'a')",
            vec![a.clone(), a],
        ),
        (
            "g.inject(null).coalesce(__.identity(),__.constant('fallback'))",
            vec![Value::Null],
        ),
        (
            "g.inject(1,2,3).store('x').limit(1).cap('x')",
            vec![Value::BulkSet(vec![Value::Int(1)])],
        ),
        (
            "g.inject(1).repeat(__.groupCount('m')).times(3).cap('m')",
            vec![Value::TypedMap(vec![(Value::Int(1), Value::Long(3))])],
        ),
    ] {
        let (actual, stats) =
            execute_rows_with_jvm(&plan(query), &graph(), JvmExecution::default())
                .await
                .unwrap_or_else(|e| panic!("{query}: {e}"));
        assert!(!stats.physical_plan.is_empty(), "{query}");
        assert!(
            !stats.physical_plan.contains("GraphJvm"),
            "{}",
            stats.physical_plan
        );
        assert_eq!(sorted(current_values(actual)), sorted(expected), "{query}");
    }
}
#[tokio::test]
#[cfg(feature = "duckdb")]
async fn branch_scans_observe_prior_writes_and_outer_rollback() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    let result = engine
        .gremlin("g.inject(1).union(__.addV('person'),__.V()).count()")
        .await
        .unwrap();
    assert_eq!(result.stats.datafusion_ops, 0);
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 2);
    engine.begin().unwrap();
    engine.gremlin("g.addV('pending')").await.unwrap();
    engine.rollback().unwrap();
    let result = engine
        .gremlin("g.V().hasLabel('pending').count()")
        .await
        .unwrap();
    let rows: serde_json::Value = serde_json::from_str(
        &result.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"],
    )
    .unwrap();
    assert_eq!(rows[0][0]["value"], 0);
}
#[test]
fn graph_checkpoints_isolate_writes_in_both_directions() {
    let graph = graph();
    let checkpoint = graph.clone();
    graph.insert_node("later", BTreeMap::new());
    assert!(checkpoint.node_ids("later").is_err());
    checkpoint.insert_node("checkpoint", BTreeMap::new());
    assert!(graph.node_ids("checkpoint").is_err());
}

#[tokio::test]
#[cfg(feature = "duckdb")]
async fn reused_dag_resources_do_not_reuse_graph_contents() {
    let mut first = orchiddb::engine::GraphEngine::in_memory().unwrap();
    let mut second = orchiddb::engine::GraphEngine::in_memory().unwrap();
    async fn count(engine: &mut orchiddb::engine::GraphEngine) -> String {
        let result = engine.gremlin("g.V().count()").await.unwrap();
        result.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"].clone()
    }
    first.replace_graph(graph()).unwrap();
    let original = count(&mut first).await;
    let empty = count(&mut second).await;
    assert_ne!(original, empty);
    for _ in 0..3 {
        first.replace_graph(PropertyGraph::new()).unwrap();
        assert_eq!(count(&mut first).await, empty);
        first.replace_graph(graph()).unwrap();
        assert_eq!(count(&mut first).await, original);
        assert_eq!(count(&mut second).await, empty);
    }
    first.begin().unwrap();
    first.gremlin("g.addV('pending')").await.unwrap();
    assert_ne!(count(&mut first).await, original);
    first.rollback().unwrap();
    assert_eq!(count(&mut first).await, original);
    assert!(
        first
            .gremlin("g.inject(1).fail('expected error')")
            .await
            .is_err()
    );
    assert_eq!(count(&mut first).await, original);
}

#[tokio::test]
async fn unary_operators_preserve_exact_rows_and_bulk() {
    let query = plan("g.inject(1,2,3).map(__.constant(5)).identity().constant(7)");
    let graph = PropertyGraph::new();
    let (actual, _) = execute_rows_with_jvm(&query, &graph, JvmExecution::default())
        .await
        .unwrap();
    assert_eq!(current_values(actual), vec![Value::Int(7); 3]);
}

#[tokio::test]
async fn ranking_windows_allow_unordered_and_ordered_dedup() {
    for query in [
        "g.V().dedup().values('name')",
        "g.V().order().by('name').dedup().values('name')",
    ] {
        let graph = graph();
        let plan = plan(query);
        let (actual, _) = execute_rows_with_jvm(&plan, &graph, JvmExecution::default())
            .await
            .unwrap();
        assert_eq!(
            sorted(current_values(actual)),
            vec![
                Value::String("a".into()),
                Value::String("b".into()),
                Value::String("c".into())
            ],
            "{query}"
        );
    }
}

#[tokio::test]
async fn batched_apply_preserves_occurrences_bulk_and_optional_results() {
    use orchiddb::ir::expr::{BinaryOp, IrExpr, Lit};
    use orchiddb::ir::plan::{ApplyKind, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
    use orchiddb::ir::policy::{GraphPlanPolicy, OptionalMissing};
    for kind in [
        ApplyKind::Inner,
        ApplyKind::Optional,
        ApplyKind::Semi,
        ApplyKind::Anti,
        ApplyKind::Scalar,
    ] {
        let right = Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            error_policy: ProjectErrorPolicy::PropagateError,
            items: vec![ProjectionItem {
                alias: "answer".into(),
                expr: IrExpr::Binding("current".into()),
            }],
            input: Box::new(Node::GraphFilter {
                condition: IrExpr::Binary {
                    op: BinaryOp::Eq,
                    lhs: Box::new(IrExpr::Binding("current".into())),
                    rhs: Box::new(IrExpr::Lit(Lit::Int(1))),
                },
                input: Box::new(Node::GraphCorrelate {
                    bindings: vec!["current".into()],
                }),
            }),
        };
        let root = Node::GraphApply {
            kind,
            correlation: vec!["current".into()],
            outputs: vec!["answer".into()],
            optional_missing: OptionalMissing::Null,
            left: Box::new(Node::GraphValues {
                bindings: vec!["current".into()],
                rows: vec![
                    vec![Value::Int(1)],
                    vec![Value::Int(1)],
                    vec![Value::Int(2)],
                ],
                bulk: Some(vec![2, 3, 4]),
            }),
            right: Box::new(right),
        };
        let plan = GraphPlan::new(GraphPlanPolicy::gremlin(), root);
        let graph = PropertyGraph::new();
        let (actual, stats) = execute_rows_with_jvm(&plan, &graph, JvmExecution::default())
            .await
            .unwrap();
        assert!(
            stats.physical_plan.contains("LateralApply"),
            "{}",
            stats.physical_plan
        );
        let expected = match kind {
            ApplyKind::Inner => {
                vec![(1, Some(Value::Int(1)), 2), (1, Some(Value::Int(1)), 3)]
            }
            ApplyKind::Optional | ApplyKind::Scalar => vec![
                (1, Some(Value::Int(1)), 2),
                (1, Some(Value::Int(1)), 3),
                (2, Some(Value::Null), 4),
            ],
            ApplyKind::Semi => vec![(1, None, 2), (1, None, 3)],
            ApplyKind::Anti => vec![(2, None, 4)],
        };
        let observed = actual
            .iter()
            .map(|r| {
                (
                    match r.bindings["current"] {
                        Value::Int(n) => n,
                        _ => panic!("unexpected value"),
                    },
                    r.bindings.get("answer").cloned(),
                    r.bulk,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(observed, expected, "{kind:?}");
    }
}

#[tokio::test]
async fn projection_composition_preserves_shadowing_and_missing_bindings() {
    use orchiddb::ir::expr::{IrExpr, Lit};
    use orchiddb::ir::plan::{Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
    use orchiddb::ir::policy::GraphPlanPolicy;
    let item = |alias: &str, expr| ProjectionItem {
        alias: alias.into(),
        expr,
    };
    for mode in [ProjectMode::PreserveVisible, ProjectMode::ReplaceScope] {
        let root = Node::GraphProject {
            mode,
            error_policy: ProjectErrorPolicy::PropagateError,
            items: vec![
                item("x", IrExpr::Binding("y".into())),
                item("missing", IrExpr::Binding("missing".into())),
                item("copy", IrExpr::Binding("y".into())),
            ],
            input: Box::new(Node::GraphProject {
                mode: ProjectMode::PreserveVisible,
                error_policy: ProjectErrorPolicy::PropagateError,
                items: vec![
                    item("x", IrExpr::Lit(Lit::Int(9))),
                    item("y", IrExpr::Binding("x".into())),
                ],
                input: Box::new(Node::GraphValues {
                    bindings: vec!["x".into()],
                    rows: vec![vec![Value::Int(1)]],
                    bulk: Some(vec![3]),
                }),
            }),
        };
        let plan = GraphPlan::new(GraphPlanPolicy::gremlin(), root);
        let graph = PropertyGraph::new();
        let (actual, stats) = execute_rows_with_jvm(&plan, &graph, JvmExecution::default())
            .await
            .unwrap();
        assert!(
            stats.duckdb_regions > 0 && stats.datafusion_operators == 0,
            "{}",
            stats.physical_plan
        );
        assert_eq!(actual.len(), 1);
        let mut expected = BTreeMap::from([
            ("x".into(), Value::Int(1)),
            ("copy".into(), Value::Int(1)),
            ("missing".into(), Value::Null),
        ]);
        if matches!(mode, ProjectMode::PreserveVisible) {
            expected.insert("y".into(), Value::Int(1));
        }
        assert_eq!(actual[0].bindings, expected);
        assert_eq!(actual[0].bulk, 3);
        assert_eq!(actual[0].bindings["missing"], Value::Null);
    }
}

#[tokio::test]
async fn required_bindings_preserve_projection_inputs_and_errors() {
    use orchiddb::ir::expr::{BinaryOp, IrExpr, Lit};
    use orchiddb::ir::plan::{Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
    use orchiddb::ir::policy::{GraphPlanPolicy, PropertyMissing, ResultForm};
    let source = || Node::GraphValues {
        bindings: vec!["x".into(), "unused".into()],
        rows: vec![vec![
            Value::Int(2),
            Value::String("large payload".repeat(100)),
        ]],
        bulk: Some(vec![2]),
    };
    let project = |expr| Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        error_policy: ProjectErrorPolicy::PropagateError,
        items: vec![ProjectionItem {
            alias: "answer".into(),
            expr,
        }],
        input: Box::new(source()),
    };
    let graph = PropertyGraph::new();
    let plan = GraphPlan::new(
        GraphPlanPolicy::gremlin(),
        Node::GraphReturn {
            fields: vec!["answer".into()],
            result_form: ResultForm::TraverserStream,
            input: Box::new(project(IrExpr::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(IrExpr::Binding("x".into())),
                rhs: Box::new(IrExpr::Lit(Lit::Int(1))),
            })),
        },
    );
    let (actual, stats) = orchiddb::ir::rel::runtime::execute(&plan, &graph, None)
        .await
        .unwrap();
    assert_eq!(actual.fields, vec!["answer"]);
    assert_eq!(actual.batch.num_rows(), 2);
    for row in 0..2 {
        assert_eq!(
            arrow::util::display::array_value_to_string(actual.batch.column(0), row).unwrap(),
            "3"
        );
    }
    assert!(
        stats.physical_plan.contains("Pruned("),
        "{}",
        stats.physical_plan
    );
    // A filter rejecting every row must not suppress a preceding expression
    // error, even when the failed alias is absent from the return fields.
    let error_plan = GraphPlan::new(
        GraphPlanPolicy::gremlin(),
        Node::GraphReturn {
            fields: vec!["x".into()],
            result_form: ResultForm::TraverserStream,
            input: Box::new(Node::GraphFilter {
                condition: IrExpr::Lit(Lit::Bool(false)),
                input: Box::new(project(IrExpr::Property {
                    binding: "missing".into(),
                    name: "p".into(),
                    policy: PropertyMissing::Error,
                })),
            }),
        },
    );
    let error = orchiddb::ir::rel::runtime::execute(&error_plan, &graph, None)
        .await
        .unwrap_err();
    assert!(
        error.contains("missing") || error.contains("property"),
        "{error}"
    );
}

#[tokio::test]
async fn pure_filter_moves_before_unrelated_projection() {
    use orchiddb::ir::expr::{BinaryOp, IrExpr, Lit};
    use orchiddb::ir::plan::{Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
    use orchiddb::ir::policy::GraphPlanPolicy;
    let root = Node::GraphFilter {
        condition: IrExpr::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(IrExpr::Binding("x".into())),
            rhs: Box::new(IrExpr::Lit(Lit::Int(1))),
        },
        input: Box::new(Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            error_policy: ProjectErrorPolicy::PropagateError,
            items: vec![ProjectionItem {
                alias: "answer".into(),
                expr: IrExpr::Lit(Lit::String("payload".repeat(100))),
            }],
            input: Box::new(Node::GraphValues {
                bindings: vec!["x".into()],
                rows: vec![vec![Value::Int(1)], vec![Value::Int(2)]],
                bulk: Some(vec![2, 3]),
            }),
        }),
    };
    let plan = GraphPlan::new(GraphPlanPolicy::gremlin(), root);
    let graph = PropertyGraph::new();
    let (actual, stats) = execute_rows_with_jvm(&plan, &graph, JvmExecution::default())
        .await
        .unwrap();
    assert!(
        stats.duckdb_regions > 0 && stats.datafusion_operators == 0,
        "{}",
        stats.physical_plan
    );
    assert_eq!(actual.len(), 1);
    assert_eq!(
        actual[0].bindings,
        BTreeMap::from([
            ("x".into(), Value::Int(1)),
            ("answer".into(), Value::String("payload".repeat(100)))
        ])
    );
    assert_eq!(actual[0].bulk, 2);
}

#[tokio::test]
async fn batched_fanout_keeps_duplicate_outputs_and_parent_bulk() {
    use orchiddb::ir::expr::IrExpr;
    use orchiddb::ir::plan::{ApplyKind, Node};
    use orchiddb::ir::policy::{GraphPlanPolicy, OptionalMissing};
    let list = Value::List(vec![Value::Int(1), Value::Int(1)]);
    let plan = GraphPlan::new(
        GraphPlanPolicy::gremlin(),
        Node::GraphApply {
            kind: ApplyKind::Inner,
            correlation: vec!["current".into()],
            outputs: vec!["answer".into()],
            optional_missing: OptionalMissing::Null,
            left: Box::new(Node::GraphValues {
                bindings: vec!["current".into()],
                rows: vec![vec![list.clone()], vec![list]],
                bulk: Some(vec![2, 3]),
            }),
            right: Box::new(Node::GraphUnwind {
                input_expr: IrExpr::Binding("current".into()),
                bind: "answer".into(),
                outer: false,
                input: Box::new(Node::GraphCorrelate {
                    bindings: vec!["current".into()],
                }),
            }),
        },
    );
    let graph = PropertyGraph::new();
    let (actual, stats) = execute_rows_with_jvm(&plan, &graph, JvmExecution::default())
        .await
        .unwrap();
    assert!(stats.physical_plan.contains("LateralApply"));
    assert_eq!(actual.len(), 4);
    assert_eq!(
        actual.iter().map(|r| r.bulk).collect::<Vec<_>>(),
        vec![2, 2, 3, 3]
    );
    for row in actual {
        assert_eq!(
            row.bindings["current"],
            Value::List(vec![Value::Int(1), Value::Int(1)])
        );
        assert_eq!(row.bindings["answer"], Value::Int(1));
    }
}

#[tokio::test]
async fn streaming_batches_preserve_global_order_aggregation_and_bulk() {
    use orchiddb::ir::expr::{AggCall, AggKind, BinaryOp, IrExpr, Lit};
    use orchiddb::ir::plan::{Node, NullsOrder, ProjectErrorPolicy, ProjectMode, ProjectionItem, SortDir, SortKey};
    use orchiddb::ir::policy::GraphPlanPolicy;
    // More than two default Arrow batches; filtering leaves both full and
    // partial output batches before whole-input sort/aggregate consumers.
    let source = Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        error_policy: ProjectErrorPolicy::PropagateError,
        items: vec![ProjectionItem { alias: "copy".into(), expr: IrExpr::Binding("x".into()) }],
        input: Box::new(Node::GraphFilter {
            condition: IrExpr::Binary { op: BinaryOp::Lt, lhs: Box::new(IrExpr::Binding("x".into())), rhs: Box::new(IrExpr::Lit(Lit::Int(17000))) },
            input: Box::new(Node::GraphValues { bindings: vec!["x".into()], rows: (0..20000).map(|i| vec![Value::Int(i)]).collect(), bulk: Some(vec![2; 20000]) }),
        }),
    };
    let graph = PropertyGraph::new();
    let sorted = GraphPlan::new(GraphPlanPolicy::gremlin(), Node::GraphSort {
        keys: vec![SortKey { expr: IrExpr::Binding("copy".into()), dir: SortDir::Desc, nulls: NullsOrder::Last }], input: Box::new(source.clone()),
    });
    let (rows, _) = execute_rows_with_jvm(&sorted, &graph, JvmExecution::default()).await.unwrap();
    assert_eq!(rows.len(), 17000);
    for (row, expected) in rows.iter().zip((0..17000).rev()) {
        assert_eq!(row.bindings["x"], Value::Int(expected));
        assert_eq!(row.bindings["copy"], Value::Int(expected));
        assert_eq!(row.bulk, 2);
    }
    let aggregated = GraphPlan::new(GraphPlanPolicy::gremlin(), Node::GraphAggregate {
        group: vec![], aggs: vec![AggCall { kind: AggKind::CountBulk, alias: "count".into(), arg: None, distinct: false }], fields: vec!["count".into()], input: Box::new(source),
    });
    let (rows, _) = execute_rows_with_jvm(&aggregated, &graph, JvmExecution::default()).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].bindings["count"], Value::Long(34000));
    // Compact multiplicity must also remain exact beyond signed-long range,
    // and an empty input still produces a single zero count.
    for (bulk, expected) in [(vec![], 0_i64), (vec![u64::MAX], -1_i64)] {
        let plan = GraphPlan::new(GraphPlanPolicy::gremlin(), Node::GraphAggregate {
            group: vec![], aggs: vec![AggCall { kind: AggKind::CountBulk, alias: "count".into(), arg: None, distinct: false }], fields: vec!["count".into()],
            input: Box::new(Node::GraphValues { bindings: vec!["x".into()], rows: bulk.iter().map(|_| vec![Value::Int(1)]).collect(), bulk: Some(bulk) }),
        });
        let (rows, _) = execute_rows_with_jvm(&plan, &graph, JvmExecution::default()).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].bindings["count"], Value::Long(expected));
    }
}

#[tokio::test]
async fn direct_relational_execution_accepts_deep_plans() {
    use datafusion::logical_expr::{LogicalPlanBuilder, lit};
    use orchiddb::ir::rel::{LoweredPlan, execute_lowered};
    let mut plan = LogicalPlanBuilder::empty(true).build().unwrap();
    for _ in 0..300 {
        plan = LogicalPlanBuilder::from(plan).filter(lit(true)).unwrap().build().unwrap();
    }
    let result = execute_lowered(LoweredPlan {
            rule_context: Default::default(),
        plan,
        fields: vec![],
        result_form: orchiddb::ir::policy::ResultForm::RowSet,
        islands: Default::default(),
    }).await.unwrap();
    assert_eq!(result.batch.num_rows(), 1);
}

#[tokio::test]
async fn nested_correlated_bodies_compile_in_their_own_scope() {
    use orchiddb::ir::plan::{ApplyKind, Node};
    use orchiddb::ir::policy::{GraphPlanPolicy, OptionalMissing};
    let correlate = || Node::GraphCorrelate { bindings: vec!["current".into()] };
    let apply = |left: Node, right: Node| Node::GraphApply {
        kind: ApplyKind::Inner,
        correlation: vec!["current".into()],
        outputs: vec!["current".into()],
        optional_missing: OptionalMissing::Null,
        left: left.boxed(),
        right: right.boxed(),
    };
    let mut body = correlate();
    for _ in 0..20 {
        body = apply(correlate(), body);
    }
    // Explicit traverser bulk keeps the seed on the native path.
    let seed = Node::GraphValues {
        bindings: vec!["current".into()],
        rows: vec![vec![Value::String("ok".into())]],
        bulk: Some(vec![1]),
    };
    let query = GraphPlan::new(GraphPlanPolicy::gremlin(), apply(seed, body));
    let (rows, _) = execute_rows_with_jvm(&query, &PropertyGraph::new(), Default::default()).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("current"), Value::String("ok".into()));
}

#[tokio::test]
#[cfg(feature = "duckdb")]
async fn same_statement_expansion_reads_created_edges() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    let result = engine.cypher("CREATE (a:Live {name:'a'})-[r:CREATED]->(b:Live {name:'b'}) WITH a,b MATCH (a)-[e:CREATED]->(b) RETURN count(e) AS n").await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0).unwrap(), "1");
    assert_eq!(result.stats.datafusion_ops, 0);
}

#[tokio::test]
#[cfg(feature = "duckdb")]
async fn correlated_expansion_preserves_typed_mapped_keys() {
    use orchiddb::ir::rel::mapping::{GraphMapping, NodeMapping, EdgeMapping};
    use std::sync::Arc;
    for (kind, first, second) in [("INTEGER", "1", "2"), ("VARCHAR", "'node-a'", "'node-b'")] {
        let storage = duckdb::Connection::open_in_memory().unwrap();
        storage.execute_batch(&format!("CREATE TABLE nodes(id {kind}, name VARCHAR); INSERT INTO nodes VALUES ({first},'a'),({second},'b'); CREATE TABLE links(id {kind},src {kind},dst {kind}); INSERT INTO links VALUES ({first},{first},{second});")).unwrap();
        let mut mapping = GraphMapping::new();
        mapping.map_node(NodeMapping::table("N", "nodes", "id").property("name", "name"));
        mapping.map_edge(EdgeMapping::table("LINK", "links", "src", "dst", "N", "N").with_id("id"));
        let mut engine = orchiddb::engine::GraphEngine::mapped(storage, Arc::new(mapping)).unwrap();
        let output = engine.gremlin("g.V().has('name','a').repeat(__.out('LINK')).times(1).values('name')").await.unwrap();
        assert_eq!(output.returned.batch.num_rows(), 1, "{kind}");
        assert_eq!(arrow::util::display::array_value_to_string(output.returned.batch.column(0), 0).unwrap(), "b", "{kind}");
        assert_eq!(output.stats.datafusion_ops, 0);
    }
}

#[tokio::test]
#[cfg(feature = "duckdb")]
async fn mapped_branch_hydration_releases_sql_stream_connection() {
    use orchiddb::ir::rel::mapping::{GraphMapping, NodeMapping, EdgeMapping};
    use std::sync::Arc;
    let storage = duckdb::Connection::open_in_memory().unwrap();
    storage.execute_batch("CREATE TABLE nodes(id INTEGER, name VARCHAR); INSERT INTO nodes VALUES (1,'a'),(2,'b'),(3,'c'),(4,'d'),(5,'e'),(6,'f'); CREATE TABLE links(id INTEGER,src INTEGER,dst INTEGER); INSERT INTO links VALUES(1,1,2),(2,1,3),(3,2,4),(4,3,5),(5,3,6);").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("N", "nodes", "id").property("name", "name"));
    mapping.map_edge(EdgeMapping::table("LINK", "links", "src", "dst", "N", "N").with_id("id"));
    let mut engine = orchiddb::engine::GraphEngine::mapped(storage, Arc::new(mapping)).unwrap();
    for _ in 0..4 {
        let result = engine.gremlin("g.V().branch(__.identity()).option(__.has('name','a'),__.out('LINK').values('name').order().fold()).option(P.neq(123),__.bothE().count())").await.unwrap();
        // Branch options receive all selected traversers: their count/fold
        // barriers each produce one result, rather than one per vertex.
        assert_eq!(result.returned.batch.num_rows(), 2);
        let mut values = (0..2).map(|row| arrow::util::display::array_value_to_string(result.returned.batch.column(0), row).unwrap()).collect::<Vec<_>>();
        values.sort();
        assert_eq!(values, vec!["d[10].l", "l[b,c]"]);
        assert_eq!(result.stats.datafusion_ops, 0);
        assert!(result.stats.physical_plan.contains("source reentry"));
    }
}

#[tokio::test]
#[cfg(feature = "duckdb")]
async fn multi_select_preserves_original_current_map() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    let output = engine.gremlin("g.inject(['src':1,'tgt':2]).select('src','tgt')").await.unwrap();
    assert_eq!(output.returned.batch.num_rows(), 1);
    assert_eq!(arrow::util::display::array_value_to_string(output.returned.batch.column(0), 0).unwrap(), "m[{\"src\":\"d[1].i\",\"tgt\":\"d[2].i\"}]");
    assert_eq!(output.stats.datafusion_ops, 0);
}

#[tokio::test]
#[cfg(feature = "duckdb")]
async fn mapped_both_endpoints_keep_strategy_properties() {
    use orchiddb::ir::rel::mapping::{GraphMapping, NodeMapping, EdgeMapping};
    use std::sync::Arc;
    let storage = duckdb::Connection::open_in_memory().unwrap();
    storage.execute_batch("CREATE TABLE nodes(id INTEGER,name VARCHAR); INSERT INTO nodes VALUES(1,'a'),(2,'b'); CREATE TABLE links(id INTEGER,src INTEGER,dst INTEGER); INSERT INTO links VALUES(1,1,2),(2,1,1);").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("N", "nodes", "id").property("name", "name"));
    mapping.map_edge(EdgeMapping::table("LINK", "links", "src", "dst", "N", "N").with_id("id"));
    let mut engine = orchiddb::engine::GraphEngine::mapped(storage, Arc::new(mapping)).unwrap();
    let result = engine.gremlin("g.withStrategies(new SubgraphStrategy(vertices: __.has('name',P.within('a','b')))).E().bothV().values('name').order()").await.unwrap();
    let values = (0..result.returned.batch.num_rows()).map(|row| arrow::util::display::array_value_to_string(result.returned.batch.column(0),row).unwrap()).collect::<Vec<_>>();
    assert_eq!(values, vec!["a", "a", "a", "b"]);
    assert_eq!(result.stats.datafusion_ops, 0);
}

#[tokio::test]
async fn floating_sum_preserves_traversal_encounter_order() {
    let query = plan("g.inject(0.4d,0.4d,0.2d,1.0d).sum()");
    let (rows, stats) = execute_rows_with_jvm(&query, &PropertyGraph::new(), Default::default()).await.unwrap();
    assert_eq!(current_values(rows), vec![Value::Float(2.0)]);
    assert_eq!(stats.datafusion_operators, 0);
    let graph = PropertyGraph::new();
    let source = graph.insert_node("person", BTreeMap::new());
    let a = graph.insert_node("software", BTreeMap::from([("name".into(), Value::String("a".into()))]));
    let b = graph.insert_node("software", BTreeMap::from([("name".into(), Value::String("b".into()))]));
    for (target, weight) in [(&a, 0.4), (&b, 1.0), (&a, 0.4), (&a, 0.2)] {
        graph.insert_edge("created", &source, target, BTreeMap::from([("weight".into(), Value::Float(weight))])).unwrap();
    }
    let query = plan("g.V().hasLabel('software').order().by('name').bothE().values('weight').sum()");
    let (rows, _) = execute_rows_with_jvm(&query, &graph, Default::default()).await.unwrap();
    assert_eq!(current_values(rows), vec![Value::Float(2.0)]);
}

#[tokio::test]
async fn repeat_labels_survive_multiple_select_modulators() {
    let graph = PropertyGraph::new();
    let nodes = ["a", "b", "c"].map(|name| graph.insert_node("N", BTreeMap::from([("name".into(), Value::String(name.into()))])));
    graph.insert_edge("R", &nodes[0], &nodes[1], BTreeMap::new()).unwrap();
    graph.insert_edge("R", &nodes[1], &nodes[2], BTreeMap::new()).unwrap();
    let (rows, stats) = execute_rows_with_jvm(&plan("g.V().until(__.out().out()).repeat(__.in().as('a').in().as('b')).select('a','b').by('name')"), &graph, JvmExecution::default()).await.unwrap();
    assert_eq!(current_values(rows), vec![Value::Map(BTreeMap::from([("a".into(),Value::String("b".into())),("b".into(),Value::String("a".into()))]))]);
    assert_eq!(stats.datafusion_operators, 0);
}

#[tokio::test]
async fn match_disjunction_anchors_existing_labels_in_each_predicate_arm() {
    let graph = PropertyGraph::new();
    let nodes = ["a", "b", "c", "d"].map(|name| graph.insert_node("N", BTreeMap::from([("name".into(), Value::String(name.into()))])));
    for (label, source, target) in [("knows",0,1),("created",0,2),("knows",0,3),("created",1,2),("created",3,2)] {
        graph.insert_edge(label, &nodes[source], &nodes[target], BTreeMap::new()).unwrap();
    }
    let query = "g.V().as('a').out().as('b').match(__.as('a').out().count().as('c'),__.or(__.as('a').out('knows').as('b'),__.as('b').in().count().as('c').and().as('c').is(P.gt(2)))).select('b').values('name')";
    let (rows, stats) = execute_rows_with_jvm(&plan(query), &graph, JvmExecution::default()).await.unwrap();
    assert_eq!(sorted(current_values(rows)), sorted(vec![Value::String("b".into()),Value::String("c".into()),Value::String("d".into())]));
    assert_eq!(stats.datafusion_operators, 0);
    let (rows, _) = execute_rows_with_jvm(&plan("g.inject(1,2).union(__.count(),__.fold())"), &PropertyGraph::new(), JvmExecution::default()).await.unwrap();
    assert_eq!(sorted(current_values(rows)), sorted(vec![Value::Long(2), Value::List(vec![Value::Int(1), Value::Int(2)])]));
}

#[tokio::test]
#[cfg(feature = "duckdb")]
async fn mapped_property_reference_survives_independent_sql_contexts() {
    use orchiddb::ir::rel::mapping::{GraphMapping, NodeMapping};
    use orchiddb::language::gremlin::{GremlinBinding, semantics::GValue};
    let storage = duckdb::Connection::open_in_memory().unwrap();
    storage.execute_batch("CREATE TABLE people(id INTEGER, name VARCHAR, age INTEGER); INSERT INTO people VALUES(1,'marko',29),(2,'vadas',27)").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("person", "people", "id").property("name", "name").property("age", "age"));
    let mut engine = orchiddb::engine::GraphEngine::mapped(storage, std::sync::Arc::new(mapping)).unwrap();
    let output = engine.gremlin("g.V(1).properties('name').id()").await.unwrap();
    let id = arrow::util::display::array_value_to_string(output.returned.batch.column(0), 0).unwrap();
    let property_output = engine.gremlin("g.V(1).properties('name')").await.unwrap();
    let typed: serde_json::Value = serde_json::from_str(&property_output.returned.batch.schema().metadata()["orchiddb.gremlin.typed_rows.v1"]).unwrap();
    assert_eq!(typed[0][0]["id"]["value"], id, "returned property object must expose the same public ID as id()");
    let id_bindings = std::collections::HashMap::from([("pid".into(), GremlinBinding::Value(GValue::String(id.clone())))]);
    let selected = engine.gremlin_with_bindings("g.V().has('age').properties().hasId(pid).value()", &id_bindings).await.unwrap();
    assert_eq!(selected.returned.batch.num_rows(), 1, "property ID {id:?}; selected {:?}", selected.returned.batch);
    assert_eq!(arrow::util::display::array_value_to_string(selected.returned.batch.column(0), 0).unwrap(), "marko");
    // Populate another query's property slots in a different order.
    engine.gremlin("g.V(2).properties('age','name').id()").await.unwrap();
    let property = |id: String| GValue::VertexPropertyRef {
        id: Box::new(GValue::String(id)),
        owner: Box::new(GValue::VertexRef {id: Box::new(GValue::Long(1)), label: "person".into()}),
        key: "name".into(),
    };
    let bindings = std::collections::HashMap::from([("p".into(), GremlinBinding::Value(property(id.clone())))]);
    let output = engine.gremlin_with_bindings("g.inject(p).value()", &bindings).await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(output.returned.batch.column(0), 0).unwrap(), "marko");
    let bindings = std::collections::HashMap::from([("p".into(), GremlinBinding::Value(property(format!("{id}-invalid"))))]);
    assert!(engine.gremlin_with_bindings("g.inject(p).value()", &bindings).await.err().unwrap().contains("does not exist"));
    engine.gremlin("g.V(1).properties('name').drop()").await.unwrap();
    assert_eq!(engine.gremlin("g.V(1).values('name')").await.unwrap().returned.batch.num_rows(), 0);
    let remaining = engine.gremlin("g.V(2).values('name')").await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(remaining.returned.batch.column(0), 0).unwrap(), "vadas");
}
