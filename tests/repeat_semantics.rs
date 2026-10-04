#[path = "common/execution.rs"]
mod datafusion_test;
use arrow::array::{ArrayRef, Int64Array};
use orchiddb::ir::{PropertyGraph, edges_from_columns, nodes_from_columns};
use orchiddb::language::gremlin::{GremlinPlanner, parse_traversal};
use std::sync::Arc;

#[test]
fn bounded_repeat_runs_beyond_sixteen_iterations() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![(
            "n",
            Arc::new(Int64Array::from_iter_values(0..26)) as ArrayRef,
        )],
    ));
    graph
        .add_edges(edges_from_columns(
            "R",
            "P",
            "P",
            (0..25).collect(),
            (1..26).collect(),
            vec![],
        ))
        .unwrap();
    let query =
        parse_traversal("g.V().has('n',0).repeat(__.out('R')).times(20).values('n')").unwrap();
    let plan = GremlinPlanner::new().plan(&query).unwrap();
    let result = execute(&plan, &graph).unwrap();
    assert_eq!(result.batch.num_rows(), 1);
    assert_eq!(
        arrow::util::display::array_value_to_string(result.batch.column(0), 0).unwrap(),
        "20"
    );
}

#[test]
fn nonterminating_repeat_obeys_caller_deadline() {
    let query = parse_traversal("g.inject(1).repeat(__.identity())").unwrap();
    let plan = GremlinPlanner::new().plan(&query).unwrap();
    let mut control = orchiddb::ir::jvm::JvmExecution::default();
    control.deadline = Some(std::time::Instant::now() + std::time::Duration::from_millis(100));
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let error = runtime.block_on(orchiddb::ir::rel::runtime::execute_rows_with_jvm(
        &plan, &PropertyGraph::new(), control,
    )).unwrap_err();
    assert!(error.contains("deadline exceeded"), "{error}");
}

#[test]
fn repeat_can_exceed_the_former_engine_iteration_ceiling() {
    let query = parse_traversal("g.inject(1).repeat(__.identity()).times(10001)").unwrap();
    let plan = GremlinPlanner::new().plan(&query).unwrap();
    let rows = datafusion_test::execute_rows(&plan, &PropertyGraph::new()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].bindings["current"], orchiddb::ir::value::Value::Int(1));
}

#[test]
fn repeat_dedup_keeps_seen_state_across_rounds() {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![("n", Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef)],
    ));
    let query = parse_traversal("g.V().repeat(__.dedup()).times(2).count()").unwrap();
    let plan = GremlinPlanner::new().plan(&query).unwrap();
    let result = execute(&plan, &graph).unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(result.batch.column(0), 0).unwrap(),
        "0"
    );
    let error = orchiddb::ir::rel::RelBackend::new()
        .lower(&plan, &graph)
        .unwrap_err();
    assert!(
        error.to_string().contains("stateful deduplication"),
        "{error}"
    );
}

use crate::datafusion_test::{execute};
