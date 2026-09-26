#![cfg(feature = "duckdb")]

//! Gremlin traverser-state semantics (repeat/loops, sacks, side-effect
//! reducers) executed through the relational backend on DuckDB and checked
//! against explicit expected results. Ignored probes compare the DataFusion DAG.
//!
//! `GREMLIN_STATE_PROBE="g.V()...;g.inject(1)..."` with the ignored
//! `probe` test prints DataFusion DAG and DuckDB output side by side.

#[path = "common/execution.rs"]
mod datafusion_test;
mod gremlin_case_runner;

use gremlin_case_runner::{compare, dataset, format};
use orchiddb::ir::catalog::PropertyGraph;
use crate::datafusion_test::execute_async as interpret;
use orchiddb::ir::rel::RelBackend;
use orchiddb::ir::rel::sql::{self, DuckDbExecutor};
use orchiddb::language::gremlin::planner::GremlinPlanner;

fn plan(query: &str) -> orchiddb::ir::plan::GraphPlan {
    let traversal =
        gremlin_case_runner::parse::gremlin_with_case(query, "").expect("parse gremlin");
    GremlinPlanner::new()
        .plan(&traversal)
        .expect("plan gremlin")
}

async fn dag_lines(query: &str, graph: &PropertyGraph) -> Result<Vec<String>, String> {
    interpret(&plan(query), graph).await
        .map(|batches| format::lines_from_batch(&batches))
        .map_err(|error| error.to_string())
}

async fn duckdb_lines(query: &str, graph: &PropertyGraph) -> Result<Vec<String>, String> {
    let lowered = RelBackend::new()
        .lower(&plan(query), graph)
        .map_err(|error| format!("lower: {error}"))?;
    let mut executor = DuckDbExecutor::new();
    sql::execute_lowered_sql(&mut executor, &lowered)
        .await
        .map(|batches| format::lines_from_batch(&batches))
        .map_err(|error| format!("execute: {error}"))
}

/// Run on DuckDB and require an (unordered) match with `expected`, written in
/// the corpus notation (`v[marko]`, `d[3].i`, ...).
async fn assert_duckdb(query: &str, graph: &PropertyGraph, expected: &[&str]) {
    let actual = duckdb_lines(query, graph)
        .await
        .unwrap_or_else(|error| panic!("{query}: {error}"));
    let expected = expected.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match compare::matches(&actual, &expected, false, "rows") {
        compare::Verdict::Match => {}
        compare::Verdict::Mismatch { reason } => {
            panic!("{query}: {reason}\n actual={actual:?}\n expected={expected:?}")
        }
    }
}

/// Stateful dedup must be rejected during lowering, before SQL execution.
fn assert_declines(query: &str, graph: &PropertyGraph) {
    let result = RelBackend::new().lower(&plan(query), graph);
    assert!(
        matches!(result, Err(orchiddb::ir::rel::RelError::Unsupported(_))),
        "{query}: expected an explicit unsupported lowering"
    );
}

fn modern() -> PropertyGraph {
    dataset::modern_graph()
}

#[tokio::test]
#[ignore]
async fn probe() {
    let graph = match std::env::var("GREMLIN_STATE_DATASET").as_deref() {
        Ok("crew") => dataset::crew_graph(),
        _ => modern(),
    };
    let queries = std::env::var("GREMLIN_STATE_PROBE").unwrap_or_default();
    for query in queries.split(';').map(str::trim).filter(|q| !q.is_empty()) {
        println!("=== {query}");
        println!("DAG: {:?}", dag_lines(query, &graph).await);
        println!("duckdb: {:?}", duckdb_lines(query, &graph).await);
    }
}

#[tokio::test]
async fn repeat_times_beyond_old_unroll_cap() {
    // times(10) exceeds the old 8-iteration unroll; both directions keep the
    // frontier alive, so the answer depends on every iteration running.
    assert_duckdb(
        "g.V().has('name','marko').repeat(__.both()).times(10).count()",
        &modern(),
        &["d[8119].l"],
    )
    .await;
}

#[tokio::test]
async fn repeat_empty_seed_yields_nothing() {
    assert_duckdb(
        "g.V().has('name','nobody').repeat(__.out()).times(2).values('name')",
        &modern(),
        &[],
    )
    .await;
    assert_duckdb(
        "g.V().has('name','nobody').emit().repeat(__.out()).times(2).values('name')",
        &modern(),
        &[],
    )
    .await;
}

#[tokio::test]
async fn repeat_emit_until_placements() {
    let graph = modern();
    // until before repeat: checked on the seed too (while-do).
    assert_duckdb(
        "g.V().has('name','marko').until(__.has('name','marko')).repeat(__.out()).values('name')",
        &graph,
        &["marko"],
    )
    .await;
    // until after repeat: do-while, the seed is not tested.
    assert_duckdb(
        "g.V().has('name','marko').repeat(__.out()).until(__.has('lang')).values('name')",
        &graph,
        &["lop", "ripple", "lop"],
    )
    .await;
    // emit before repeat emits the seed.
    assert_duckdb(
        "g.V().has('name','marko').emit().repeat(__.out()).times(2).values('name')",
        &graph,
        &["marko", "lop", "vadas", "josh", "ripple", "lop"],
    )
    .await;
    // emit after repeat does not.
    assert_duckdb(
        "g.V().has('name','marko').repeat(__.out()).times(2).emit().values('name')",
        &graph,
        &["lop", "vadas", "josh", "ripple", "lop"],
    )
    .await;
}

#[tokio::test]
async fn repeat_until_sub_traversal() {
    assert_duckdb(
        "g.V().has('name','marko').repeat(__.out()).until(__.outE().count().is(0)).values('name')",
        &modern(),
        &["vadas", "lop", "ripple", "lop"],
    )
    .await;
}

#[tokio::test]
async fn repeat_emit_sub_traversal() {
    assert_duckdb(
        "g.V().has('name','marko').repeat(__.out()).times(2).emit(__.out()).values('name')",
        &modern(),
        &["josh"],
    )
    .await;
}

#[tokio::test]
async fn loops_counter_drives_until() {
    assert_duckdb(
        "g.V().has('name','marko').repeat(__.both()).until(__.loops().is(3)).count()",
        &modern(),
        &["d[17].l"],
    )
    .await;
}

#[tokio::test]
async fn sack_arithmetic_and_assign() {
    let graph = modern();
    assert_duckdb(
        "g.withSack(0.0d).V().outE().sack(Operator.sum).by('weight').inV().sack().sum()",
        &graph,
        &["d[3.5].d"],
    )
    .await;
    assert_duckdb(
        "g.V().sack(assign).by('age').sack()",
        &graph,
        &["d[29].i", "d[27].i", "d[32].i", "d[35].i"],
    )
    .await;
}

#[tokio::test]
async fn sack_inside_repeat() {
    assert_duckdb(
        "g.withSack(0.0d).V().repeat(__.outE().sack(Operator.sum).by('weight').inV()).times(2).sack()",
        &modern(),
        &["d[2.0].d", "d[1.4].d"],
    )
    .await;
}

#[tokio::test]
async fn side_effect_reducer_cap() {
    let graph = modern();
    assert_duckdb(
        "g.withSideEffect('a', 1, Operator.sum).V().aggregate('a').by('age').cap('a')",
        &graph,
        &["d[124].i"],
    )
    .await;
    assert_duckdb(
        "g.withSideEffect('a', 100, Operator.min).V().aggregate('a').by('age').cap('a')",
        &graph,
        &["d[27].i"],
    )
    .await;
}

#[tokio::test]
async fn aggregate_cap_collects_the_bag() {
    assert_duckdb(
        "g.V().aggregate('x').by('name').cap('x')",
        &modern(),
        &["josh", "lop", "marko", "peter", "ripple", "vadas"],
    )
    .await;
}

#[tokio::test]
async fn cross_iteration_dedup_is_not_answered_per_round() {
    // `repeat(dedup())` shares one seen-set across rounds; a per-iteration
    // SQL DISTINCT would answer 6, the correct answer is 0.
    assert_declines("g.V().repeat(__.dedup()).times(2).count()", &modern());
}

#[test]
#[ignore]
fn explain_probe() {
    let queries = std::env::var("GREMLIN_STATE_PROBE").unwrap_or_default();
    for query in queries.split(';').map(str::trim).filter(|q| !q.is_empty()) {
        println!(
            "=== {query}\n{}",
            orchiddb::ir::plan::explain(&plan(query))
        );
    }
}

#[tokio::test]
async fn repeat_until_first_traversal_preserves_exiting_seeds() {
    assert_duckdb(
        "g.V().until(__.outE().count().is(0)).repeat(__.out()).values('name')",
        &modern(),
        &[
            "vadas", "lop", "ripple", "vadas", "lop", "ripple", "lop", "lop", "ripple", "lop",
        ],
    )
    .await;
}

#[tokio::test]
async fn reducer_seed_survives_empty_stream() {
    assert_duckdb(
        "g.withSideEffect('a', 100, Operator.min).V().has('name','nobody').aggregate('a').by('age').cap('a')",
        &modern(), &["d[100].i"],
    ).await;
    assert_duckdb(
        "g.withSideEffect('a', 10, Operator.max).V().aggregate('a').by('age').cap('a')",
        &modern(),
        &["d[35].i"],
    )
    .await;
}
