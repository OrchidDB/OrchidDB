use super::{expand::expand_op, path_pattern::path_pattern_op};
use crate::ir::{
    catalog::PropertyGraph,
    plan::{Direction, LabelExpr, Length, PathPart, PathSelector, PathTies, TargetMode},
    policy::{MatchMode, PathMode},
    runtime::{Row, RuntimeError, context::ExecutionContext},
    value::Value,
};

fn chain(edges: usize) -> (PropertyGraph, Vec<Value>) {
    let graph = PropertyGraph::new();
    let nodes = (0..=edges)
        .map(|_| graph.insert_node("N", Default::default()))
        .collect::<Vec<_>>();
    for pair in nodes.windows(2) {
        graph
            .insert_edge("R", &pair[0], &pair[1], Default::default())
            .unwrap();
    }
    (graph, nodes)
}
fn parts(length: Length) -> Vec<PathPart> {
    vec![
        PathPart::Node {
            bind: "a".into(),
            labels: LabelExpr::Any,
        },
        PathPart::Rel {
            bind: Some("r".into()),
            types: LabelExpr::Any,
            dir: Direction::Out,
            length,
        },
        PathPart::Node {
            bind: "b".into(),
            labels: LabelExpr::Any,
        },
    ]
}
fn pattern(
    graph: &PropertyGraph,
    row: Row,
    length: Length,
    selector: PathSelector,
    ctx: &mut ExecutionContext,
) -> Result<Vec<Row>, RuntimeError> {
    path_pattern_op(
        "p",
        &selector,
        &parts(length),
        PathMode::Walk,
        MatchMode::DifferentRelationships,
        vec![row],
        graph,
        ctx,
    )
}
fn expand(
    graph: &PropertyGraph,
    start: &Value,
    length: Length,
    types: LabelExpr,
    ctx: &mut ExecutionContext,
) -> Result<Vec<Row>, RuntimeError> {
    expand_op(
        "a",
        "b",
        TargetMode::BindNewOrReplaceCurrent,
        &LabelExpr::Any,
        None,
        &types,
        Direction::Out,
        &length,
        None,
        None,
        PathMode::Walk,
        MatchMode::DifferentRelationships,
        vec![Row::new().with("a", start.clone())],
        graph,
        ctx,
    )
}

#[test]
fn native_unbounded_expansion_passes_thirty_hops_and_honors_large_minimum() {
    let (graph, nodes) = chain(80);
    let rows = expand(
        &graph,
        &nodes[0],
        Length::unbounded(1),
        LabelExpr::Any,
        &mut ExecutionContext::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 80);
    assert_eq!(rows.last().unwrap().bindings["b"], nodes[80]);
    let rows = expand(
        &graph,
        &nodes[0],
        Length::unbounded(65),
        LabelExpr::Any,
        &mut ExecutionContext::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 16);
    let rows = expand(
        &graph,
        &nodes[0],
        Length::bounded(0, 2),
        LabelExpr::Any,
        &mut ExecutionContext::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 3);
}

#[test]
fn trail_terminates_on_cycle_without_projecting_a_path() {
    let (graph, nodes) = chain(2);
    graph
        .insert_edge("R", &nodes[2], &nodes[0], Default::default())
        .unwrap();
    let rows = expand(
        &graph,
        &nodes[0],
        Length::unbounded(0),
        LabelExpr::Any,
        &mut ExecutionContext::with_step_limit(100),
    )
    .unwrap();
    assert_eq!(rows.len(), 4);
}

#[test]
fn relationship_label_expressions_preserve_boolean_semantics() {
    let (graph, nodes) = chain(1);
    graph
        .insert_edge("S", &nodes[0], &nodes[1], Default::default())
        .unwrap();
    for (types, expected) in [
        (LabelExpr::Not(Box::new(LabelExpr::label("R"))), 1),
        (LabelExpr::Not(Box::new(LabelExpr::Any)), 0),
        (LabelExpr::AnyOf(vec![]), 0),
        (LabelExpr::AnyOf(vec!["R".into(), "S".into()]), 2),
        (LabelExpr::AllOf(vec!["R".into(), "S".into()]), 0),
        (LabelExpr::AllOf(vec![]), 2),
        (LabelExpr::AllOf(vec!["R".into(), "R".into()]), 1),
    ] {
        let rows = expand(
            &graph,
            &nodes[0],
            Length::ONE,
            types,
            &mut ExecutionContext::default(),
        )
        .unwrap();
        assert_eq!(rows.len(), expected);
    }
}

#[test]
fn path_pattern_preserves_unbounded_and_zero_length_ranges() {
    let (graph, nodes) = chain(80);
    let seed = Row::new().with("a", nodes[0].clone());
    let rows = pattern(
        &graph,
        seed.clone(),
        Length::unbounded(0),
        PathSelector::All,
        &mut ExecutionContext::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 81);
    assert_eq!(rows[0].bindings["p"], Value::Path(vec![nodes[0].clone()]));
    assert_eq!(rows[0].bindings["r"], Value::Null);
    let Value::Path(path) = &rows[80].bindings["p"] else {
        panic!()
    };
    assert_eq!(path.len(), 161);
    assert_eq!(path.last(), Some(&nodes[80]));
    let rows = pattern(
        &graph,
        seed,
        Length::bounded(0, 0),
        PathSelector::All,
        &mut ExecutionContext::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn path_pattern_prunes_shortest_search_and_handles_cycles() {
    let (graph, nodes) = chain(2);
    graph
        .insert_edge("S", &nodes[0], &nodes[2], Default::default())
        .unwrap();
    graph
        .insert_edge("T", &nodes[0], &nodes[2], Default::default())
        .unwrap();
    graph
        .insert_edge("R", &nodes[2], &nodes[0], Default::default())
        .unwrap();
    let seed = Row::new()
        .with("a", nodes[0].clone())
        .with("b", nodes[2].clone());
    for (selector, expected, lengths) in [
        (PathSelector::Any, 1, vec![3]),
        (
            PathSelector::Shortest {
                k: 1,
                ties: PathTies::Any,
            },
            1,
            vec![3],
        ),
        (
            PathSelector::Shortest {
                k: 2,
                ties: PathTies::Any,
            },
            2,
            vec![3, 3],
        ),
        (
            PathSelector::Shortest {
                k: 1,
                ties: PathTies::All,
            },
            2,
            vec![3, 3],
        ),
    ] {
        let rows = pattern(
            &graph,
            seed.clone(),
            Length::unbounded(1),
            selector,
            &mut ExecutionContext::with_step_limit(100),
        )
        .unwrap();
        assert_eq!(rows.len(), expected);
        assert_eq!(
            rows.iter()
                .map(|row| match &row.bindings["p"] {
                    Value::Path(p) => p.len(),
                    _ => panic!(),
                })
                .collect::<Vec<_>>(),
            lengths
        );
    }
    // Repeatable walk has infinitely many candidates, but ANY SHORTEST ends.
    let rows = path_pattern_op(
        "p",
        &PathSelector::Shortest {
            k: 1,
            ties: PathTies::Any,
        },
        &parts(Length::unbounded(1)),
        PathMode::Walk,
        MatchMode::RepeatableElements,
        vec![seed],
        &graph,
        &mut ExecutionContext::with_step_limit(100),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
}

#[test]
fn search_charges_work_and_observes_cancellation() {
    let (graph, nodes) = chain(2);
    graph
        .insert_edge("R", &nodes[2], &nodes[0], Default::default())
        .unwrap();
    let row = Row::new().with("a", nodes[0].clone());
    let error = path_pattern_op(
        "p",
        &PathSelector::All,
        &parts(Length::unbounded(1)),
        PathMode::Walk,
        MatchMode::RepeatableElements,
        vec![row],
        &graph,
        &mut ExecutionContext::with_step_limit(100),
    )
    .unwrap_err();
    assert!(matches!(error, RuntimeError::ExecutionLimit(_)));
    let mut ctx = ExecutionContext::default();
    ctx.jvm
        .cancelled
        .store(true, std::sync::atomic::Ordering::Release);
    assert!(
        expand(
            &graph,
            &nodes[0],
            Length::unbounded(1),
            LabelExpr::Any,
            &mut ctx
        )
        .unwrap_err()
        .to_string()
        .contains("cancelled")
    );
}

#[test]
fn thousands_of_pattern_parts_execute_without_recursive_calls() {
    let (graph, nodes) = chain(0);
    let mut pattern = vec![PathPart::Node {
        bind: "a".into(),
        labels: LabelExpr::Any,
    }];
    for _ in 0..10000 {
        pattern.push(PathPart::Rel {
            bind: None,
            types: LabelExpr::Any,
            dir: Direction::Out,
            length: Length::bounded(0, 0),
        });
        pattern.push(PathPart::Node {
            bind: "a".into(),
            labels: LabelExpr::Any,
        });
    }
    let rows = path_pattern_op(
        "p",
        &PathSelector::All,
        &pattern,
        PathMode::Walk,
        MatchMode::DifferentRelationships,
        vec![Row::new().with("a", nodes[0].clone())],
        &graph,
        &mut ExecutionContext::default(),
    )
    .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].bindings["p"], Value::Path(vec![nodes[0].clone()]));
}
