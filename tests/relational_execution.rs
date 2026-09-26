//! DataFusion DAG regressions with independent, explicit expected results.
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
async fn typed_traversers_and_correlated_state_have_expected_results() {
    let text = |s: &str| Value::String(s.into());
    let a = Value::Node {
        label: "person".into(),
        id: 0,
    };
    let b = Value::Node {
        label: "person".into(),
        id: 1,
    };
    let c = Value::Node {
        label: "software".into(),
        id: 0,
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
    assert!(result.stats.datafusion_ops > 0);
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
async fn fused_unary_kernels_preserve_exact_rows_and_bulk() {
    let query = plan("g.inject(1,2,3).map(__.constant(5)).identity().constant(7)");
    let graph = PropertyGraph::new();
    let (actual, stats) = execute_rows_with_jvm(&query, &graph, JvmExecution::default())
        .await
        .unwrap();
    assert!(
        stats.physical_plan.contains("Fused("),
        "{}",
        stats.physical_plan
    );
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
            stats.physical_plan.contains("BatchableLateralApply"),
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
            stats.physical_plan.contains("CollapsedProject"),
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
        stats.physical_plan.contains("Values -> Filter"),
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
    assert!(stats.physical_plan.contains("BatchableLateralApply"));
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
