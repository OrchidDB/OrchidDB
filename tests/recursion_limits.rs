#![cfg(feature = "duckdb")]

//! Focused regressions for recursive path-length limits.
//!
//! Variable-length expansion is lowered to a `WITH RECURSIVE` CTE with no
//! implicit depth cap; trail semantics (no relationship may repeat) bound the
//! walk. These tests pin that unbounded and above-cap bounded traversals are
//! not silently truncated to the old unroll cap (`VARLEN_UNROLL_CAP = 6`) or
//! the repeat cap (`REPEAT_CAP = 8`).

use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::DataType;

use new_graph::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use new_graph::ir::plan::GraphPlan;
use new_graph::ir::rel::RelBackend;
use new_graph::ir::rel::sql::{self, DuckDbExecutor, SqlDialect};
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner as AstCypherPlanner;

/// A directed `NEXT` chain of `edges` edges (`edges + 1` nodes `n0..n{edges}`).
fn chain_graph(edges: usize) -> PropertyGraph {
    let names: ArrayRef = Arc::new(StringArray::from(
        (0..=edges)
            .map(|index| format!("n{index}"))
            .collect::<Vec<_>>(),
    ));
    let person = nodes_from_columns("Person", vec![("name", names)]);
    let mut graph = PropertyGraph::new();
    graph.add_nodes(person);
    graph
        .add_edges(edges_from_columns(
            "NEXT",
            "Person",
            "Person",
            (0..edges as i64).collect::<Vec<_>>(),
            (1..=edges as i64).collect::<Vec<_>>(),
            Vec::new(),
        ))
        .unwrap();
    graph
}

fn cypher_plan(query: &str) -> GraphPlan {
    let parsed = parse_query(query).expect("parse");
    AstCypherPlanner::new().plan(&parsed).expect("plan")
}

fn batch_lines(batch: &RecordBatch) -> Vec<String> {
    (0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|col| cell_to_string(batch.column(col), row))
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

fn cell_to_string(array: &ArrayRef, row: usize) -> String {
    if array.is_null(row) {
        return "null".into();
    }
    match array.data_type() {
        DataType::Utf8 => array
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Int64 => array
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(row)
            .to_string(),
        other => panic!("unhandled result type in recursion-limit test: {other}"),
    }
}

/// Exceeds both the old varlen unroll cap (6) and the repeat cap (8).
const CHAIN_EDGES: usize = 15;

#[tokio::test]
async fn unbounded_varlen_is_not_capped_at_the_old_unroll_cap() {
    let plan = cypher_plan(
        "MATCH (p:Person)-[:NEXT*1..]->(f) \
         WHERE p.name = 'n0' RETURN count(*)",
    );
    let lowered = RelBackend::new()
        .lower(&plan, &chain_graph(CHAIN_EDGES))
        .expect("lower");
    let mut executor = DuckDbExecutor::new();
    let returned = sql::execute_lowered_sql(&mut executor, &lowered)
        .await
        .expect("execute recursive path");
    assert_eq!(
        batch_lines(&returned.batch),
        vec![CHAIN_EDGES.to_string()],
        "unbounded `-[*1..]->` was truncated below {CHAIN_EDGES} hops"
    );
}

#[tokio::test]
async fn bounded_varlen_above_the_old_cap_is_not_capped() {
    // Upper bound exceeds both the unroll cap (6) and the repeat cap (8); the
    // recursive lowering must honour it instead of erroring or truncating.
    let upper = 10_usize;
    let plan = cypher_plan(&format!(
        "MATCH (p:Person)-[:NEXT*1..{upper}]->(f) \
         WHERE p.name = 'n0' RETURN count(*)"
    ));
    let lowered = RelBackend::new()
        .lower(&plan, &chain_graph(CHAIN_EDGES))
        .expect("lower");
    let mut executor = DuckDbExecutor::new();
    let returned = sql::execute_lowered_sql(&mut executor, &lowered)
        .await
        .expect("execute recursive path");
    assert_eq!(
        batch_lines(&returned.batch),
        vec![upper.to_string()],
        "bounded `-[*1..{upper}]->` was truncated"
    );
}

#[tokio::test]
async fn unbounded_varlen_lowers_to_a_recursive_cte_not_a_bounded_unroll() {
    let plan = cypher_plan(
        "MATCH (p:Person)-[:NEXT*1..]->(f) \
         WHERE p.name = 'n0' RETURN count(*)",
    );
    let lowered = RelBackend::new()
        .lower(&plan, &chain_graph(CHAIN_EDGES))
        .expect("lower");
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb)
        .await
        .expect("prepare sql");
    assert!(
        prepared.query.starts_with("WITH RECURSIVE"),
        "unbounded varlen did not lower to a recursive CTE:\n{}",
        prepared.query
    );
    // The bounded-unroll fallback emits `__vlr_…` hop aliases; a recursive
    // CTE must not.
    assert!(
        !prepared.query.contains("__vlr_"),
        "unbounded varlen fell back to bounded unrolling:\n{}",
        prepared.query
    );
}
