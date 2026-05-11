//! End-to-end tests for the new Gremlin → Graph IR planner. We construct
//! `Traversal` AST values directly (skipping the parser, which lives in
//! the WIP grammar tree) and run the resulting `GraphPlan` through the
//! IR interpreter against the standard "modern" property graph.

use std::sync::Arc;

use arrow::array::{ArrayRef, Float64Array, Int64Array, StringArray};

use new_graph::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use new_graph::ir::interpreter::execute_rows;
use new_graph::ir::value::Value;
use new_graph::language::gremlin::ast::{OptionKey, Step, Traversal, TraversalOption};
use new_graph::language::gremlin::planner::GremlinPlanner;
use new_graph::language::gremlin::semantics::{Direction, GValue, Predicate};

// ---------------- fixture ----------------

fn modern_graph() -> PropertyGraph {
    let names: ArrayRef = Arc::new(StringArray::from(vec!["marko", "vadas", "josh", "peter"]));
    let ages: ArrayRef = Arc::new(Int64Array::from(vec![29, 27, 32, 35]));
    let countries: ArrayRef = Arc::new(StringArray::from(vec!["us", "us", "uk", "uk"]));
    let person = nodes_from_columns(
        "Person",
        vec![("name", names), ("age", ages), ("country", countries)],
    );
    let titles: ArrayRef = Arc::new(StringArray::from(vec!["lop", "ripple"]));
    let langs: ArrayRef = Arc::new(StringArray::from(vec!["java", "java"]));
    let software = nodes_from_columns("Software", vec![("name", titles), ("lang", langs)]);

    let mut graph = PropertyGraph::new();
    graph.add_nodes(person);
    graph.add_nodes(software);

    let knows_weight: ArrayRef = Arc::new(Float64Array::from(vec![0.5, 1.0]));
    let knows = edges_from_columns(
        "KNOWS",
        "Person",
        "Person",
        vec![0, 0],
        vec![1, 2],
        vec![("weight", knows_weight)],
    );
    graph.add_edges(knows).unwrap();

    let created_weight: ArrayRef = Arc::new(Float64Array::from(vec![0.4, 0.4, 1.0, 0.2]));
    let created = edges_from_columns(
        "CREATED",
        "Person",
        "Software",
        vec![0, 2, 2, 3],
        vec![0, 0, 1, 0],
        vec![("weight", created_weight)],
    );
    graph.add_edges(created).unwrap();

    graph
}

fn t(steps: Vec<Step>) -> Traversal {
    Traversal::new(steps)
}

fn plan(steps: Vec<Step>) -> new_graph::ir::plan::GraphPlan {
    GremlinPlanner::new().plan(&t(steps)).expect("plan ok")
}

fn current_strings(rows: &[new_graph::ir::interpreter::Row]) -> Vec<String> {
    rows.iter()
        .map(
            |row| match row.bindings.get("current").expect("current binding") {
                Value::String(value) => value.clone(),
                other => panic!("expected string current, got {other:?}"),
            },
        )
        .collect()
}

#[test]
fn choose_traversal_condition_runs_true_arm_once_per_input() {
    let rows = execute_rows(
        &plan(vec![
            Step::V {
                ids: vec![GValue::String("Person#0".into())],
            },
            Step::ChooseTraversal {
                condition: vec![Step::ExpandVertex {
                    direction: Direction::Out,
                    edge_labels: Vec::new(),
                }],
                then: vec![Step::Constant(GValue::String("hit".into()))],
                else_branch: Some(vec![Step::Constant(GValue::String("miss".into()))]),
            },
        ]),
        &modern_graph(),
    )
    .unwrap();
    assert_eq!(current_strings(&rows), vec!["hit"]);
}

#[test]
fn side_effect_traversal_preserves_the_main_stream() {
    let rows = execute_rows(
        &plan(vec![
            Step::V {
                ids: vec![GValue::String("Person#0".into())],
            },
            Step::SideEffect(vec![Step::Constant(GValue::String("changed".into()))]),
            Step::Values(vec!["name".into()]),
        ]),
        &modern_graph(),
    )
    .unwrap();
    assert_eq!(current_strings(&rows), vec!["marko"]);
}

#[test]
fn map_takes_one_result_instead_of_flattening_every_child_row() {
    let rows = execute_rows(
        &plan(vec![
            Step::Inject(vec![GValue::Int(0)]),
            Step::Map(vec![Step::Union(vec![
                vec![Step::Constant(GValue::Int(1))],
                vec![Step::Constant(GValue::Int(2))],
            ])]),
        ]),
        &modern_graph(),
    )
    .unwrap();
    let values: Vec<i64> = rows
        .iter()
        .map(
            |row| match row.bindings.get("current").expect("current binding") {
                Value::Int(value) => *value,
                other => panic!("expected int current, got {other:?}"),
            },
        )
        .collect();
    assert_eq!(values, vec![1]);
}

#[test]
fn choose_option_dispatch_handles_predicate_none_and_unproductive() {
    let rows = execute_rows(
        &plan(vec![
            Step::V { ids: Vec::new() },
            Step::BranchOptions {
                dispatch: vec![Step::Values(vec!["age".into()])],
                options: vec![
                    TraversalOption {
                        key: OptionKey::Predicate(Predicate::Range {
                            lo: GValue::Int(26),
                            hi: GValue::Int(30),
                            inclusive_lo: true,
                            inclusive_hi: true,
                        }),
                        traversal: vec![Step::Values(vec!["name".into()])],
                    },
                    TraversalOption {
                        key: OptionKey::PickNone,
                        traversal: vec![Step::Values(vec!["name".into()])],
                    },
                    TraversalOption {
                        key: OptionKey::PickUnproductive,
                        traversal: vec![Step::Label],
                    },
                ],
                is_choose: true,
            },
        ]),
        &modern_graph(),
    )
    .unwrap();
    assert_eq!(
        current_strings(&rows),
        vec!["marko", "vadas", "josh", "peter", "Software", "Software"]
    );
}

#[test]
fn branch_option_dispatch_emits_all_matching_arms_and_pick_any() {
    let rows = execute_rows(
        &plan(vec![
            Step::V {
                ids: vec![GValue::String("Person#0".into())],
            },
            Step::BranchOptions {
                dispatch: vec![Step::Label],
                options: vec![
                    TraversalOption {
                        key: OptionKey::Value(GValue::String("Person".into())),
                        traversal: vec![Step::Constant(GValue::String("first".into()))],
                    },
                    TraversalOption {
                        key: OptionKey::Value(GValue::String("Person".into())),
                        traversal: vec![Step::Constant(GValue::String("second".into()))],
                    },
                    TraversalOption {
                        key: OptionKey::PickAny,
                        traversal: vec![Step::Label],
                    },
                ],
                is_choose: false,
            },
        ]),
        &modern_graph(),
    )
    .unwrap();
    assert_eq!(current_strings(&rows), vec!["first", "second", "Person"]);
}
