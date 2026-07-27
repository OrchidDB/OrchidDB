//! Hybrid execution: SQL islands spliced into an otherwise-interpreted plan.
//!
//! The point of the partitioner is that an operator the relational backend
//! cannot lower no longer costs us the *whole* query. These tests pin both
//! halves of that claim: a fully-lowerable plan becomes one island, a plan
//! with an unsupported operator still islands the part beneath it, and in
//! both cases the rows that come out match the plain interpreter.

use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, StringArray};

use new_graph::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use new_graph::ir::exec::{
    DataFusionTarget, ExecStats, IslandTarget, SqlTarget, execute_with_islands, plan_with_islands,
};
use new_graph::ir::interpreter::execute;
use new_graph::ir::rel::RelBackend;
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner;

/// Two people, one `knows` edge between them.
fn fixture() -> PropertyGraph {
    let names: ArrayRef = Arc::new(StringArray::from(vec!["alice", "bob"]));
    let ages: ArrayRef = Arc::new(Int64Array::from(vec![30, 40]));
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "person",
        vec![("name", names), ("age", ages)],
    ));
    graph
        .add_edges(edges_from_columns(
            "knows",
            "person",
            "person",
            vec![0],
            vec![1],
            Vec::new(),
        ))
        .unwrap();
    graph
}

fn rows_of(graph: &PropertyGraph, query: &str) -> Vec<String> {
    let parsed = parse_query(query).expect("parse");
    let plan = CypherPlanner::new().plan(&parsed).expect("plan");
    render(execute(&plan, graph).expect("run"))
}

fn render(returned: new_graph::ir::interpreter::ReturnedBatches) -> Vec<String> {
    let batch = returned.batch;
    (0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|col| {
                    arrow::util::display::array_value_to_string(batch.column(col), row)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

/// Partition `query` against `target`, returning its stats and result rows.
fn islanded_on(
    graph: &PropertyGraph,
    query: &str,
    target: &dyn IslandTarget,
) -> (ExecStats, Vec<String>) {
    let parsed = parse_query(query).expect("parse");
    let plan = CypherPlanner::new().plan(&parsed).expect("plan");
    let backend = RelBackend::new();
    let (hybrid, stats) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(plan_with_islands(&plan, graph, &backend, target));
    let returned = execute(&hybrid, graph).expect("run hybrid");
    (stats, render(returned))
}

/// Partition `query` on the default engine (DuckDB).
fn islanded(graph: &PropertyGraph, query: &str) -> (usize, Vec<String>) {
    let (stats, rows) = islanded_on(graph, query, &SqlTarget::duckdb());
    (stats.islands, rows)
}

#[test]
fn a_fully_lowerable_plan_becomes_a_single_island() {
    let graph = fixture();
    let query = "MATCH (p:person) WHERE p.age > 35 RETURN p.name";
    let (islands, rows) = islanded(&graph, query);
    assert_eq!(islands, 1, "expected the whole plan to island");
    assert_eq!(rows, rows_of(&graph, query));
}

#[test]
fn complete_reads_return_target_batches_without_a_residual_plan() {
    let graph = fixture();
    let query = "MATCH (p:person) WHERE p.age > 35 RETURN p.name";
    let parsed = parse_query(query).expect("parse");
    let plan = CypherPlanner::new().plan(&parsed).expect("plan");
    let (returned, stats) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(execute_with_islands(
            &plan,
            &graph,
            &RelBackend::new(),
            &SqlTarget::duckdb(),
        ))
        .expect("direct SQL read");
    assert!(stats.fully_pushed_down());
    assert_eq!(stats.residual_ops, 0);
    assert_eq!(render(returned), rows_of(&graph, query));
}

#[test]
fn island_results_match_the_interpreter_for_an_expand() {
    let graph = fixture();
    let query = "MATCH (a:person)-[:knows]->(b:person) RETURN a.name, b.name";
    let (islands, rows) = islanded(&graph, query);
    assert!(islands >= 1, "expected at least one island");
    assert_eq!(rows, rows_of(&graph, query));
}

/// The whole point: an operator the relational backend cannot lower must not
/// stop the subtree beneath it from running as SQL.
#[test]
fn an_unlowerable_operator_still_islands_the_subtree_beneath_it() {
    let graph = fixture();
    // `list_append` has no relational lowering, so the projection cannot be
    // part of an island — but the MATCH below it can.
    let query = "MATCH (p:person) RETURN list_append([1], p.age)";
    let plan_islands = islanded(&graph, query);
    assert!(
        plan_islands.0 >= 1,
        "expected the MATCH beneath the unlowerable projection to island, got {plan_islands:?}"
    );
    assert_eq!(plan_islands.1, rows_of(&graph, query));
}

/// Writes must never be executed relationally — a relational run computes a
/// result set without ever touching the catalog, so the write would vanish.
#[test]
fn mutations_are_never_islanded() {
    let graph = fixture();
    let parsed = parse_query("CREATE (n:person {id: 99}) RETURN n.id").expect("parse");
    let plan = CypherPlanner::new().plan(&parsed).expect("plan");
    let backend = RelBackend::new();
    let (_, stats) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(plan_with_islands(
            &plan,
            &graph,
            &backend,
            &SqlTarget::duckdb(),
        ));
    assert_eq!(stats.islands, 0, "a CREATE plan must not be islanded");
}

/// `collect()` produces a real list column. An island that cannot decode a
/// column type used to substitute NULL, which turned every collected list
/// into an empty cell — a wrong answer rather than a fallback.
#[test]
fn island_results_carry_collected_lists() {
    let graph = fixture();
    let query = "MATCH (p:person) RETURN collect(p.name)";
    let (stats, rows) = islanded_on(&graph, query, &SqlTarget::duckdb());
    assert_eq!(rows, rows_of(&graph, query));
    assert!(
        rows.iter().all(|row| !row.trim().is_empty()),
        "collected list came back empty: {rows:?} (islands={})",
        stats.islands
    );
}

/// A column the island cannot decode must make the island decline, so the
/// subtree is evaluated directly instead of yielding a fabricated NULL.
#[test]
fn undecodable_columns_decline_rather_than_null_out() {
    let graph = fixture();
    // Whatever the engine returns for these, the rows must agree with direct
    // evaluation — either by decoding faithfully or by declining the island.
    for query in [
        "MATCH (p:person) RETURN collect(p.age)",
        "MATCH (p:person) RETURN p.age, collect(p.name)",
        "MATCH (a:person)-[:knows]->(b:person) RETURN a.name, collect(b.age)",
    ] {
        let (_, rows) = islanded_on(&graph, query, &SqlTarget::duckdb());
        assert_eq!(rows, rows_of(&graph, query), "mismatch for `{query}`");
    }
}

/// A `x.*` projection fans one field into one column per property in the
/// relational plan, but the residual still refers to the single field `x.*`.
/// The island has to collapse those columns back into one map binding, in
/// projection order, or the row comes back blank.
#[test]
fn island_results_carry_star_projections() {
    let graph = fixture();
    for query in [
        "MATCH (p:person) RETURN p.*",
        "MATCH (a:person)-[:knows]->(b:person) RETURN a.name, b.*",
    ] {
        let (stats, rows) = islanded_on(&graph, query, &SqlTarget::duckdb());
        assert_eq!(rows, rows_of(&graph, query), "mismatch for `{query}`");
        assert!(
            rows.iter().all(|row| !row.trim().is_empty()),
            "star projection came back blank for `{query}`: {rows:?}"
        );
        assert!(
            stats.islands >= 1,
            "expected `{query}` to still island, got {stats:?}"
        );
    }
}

/// The engine is a swappable target, not a hardcoded dependency: the same
/// plan must produce the same rows on DuckDB and on in-process DataFusion.
#[test]
fn targets_are_interchangeable() {
    let graph = fixture();
    let query = "MATCH (p:person) WHERE p.age > 25 RETURN p.name, p.age";
    let (duck_stats, duck_rows) = islanded_on(&graph, query, &SqlTarget::duckdb());
    let (df_stats, df_rows) = islanded_on(&graph, query, &DataFusionTarget);
    assert_eq!(duck_rows, rows_of(&graph, query));
    assert_eq!(duck_rows, df_rows);
    assert_eq!(duck_stats.islands, df_stats.islands);
}

/// A query that lowers completely leaves the interpreter with no work — only
/// the result-shaping boundary over already-computed rows. This is the
/// condition that has to hold corpus-wide before the interpreter can go.
#[test]
fn a_fully_lowered_query_leaves_no_interpreted_operators() {
    let graph = fixture();
    let (stats, _) = islanded_on(
        &graph,
        "MATCH (p:person) WHERE p.age > 25 RETURN p.name",
        &SqlTarget::duckdb(),
    );
    assert!(
        stats.fully_pushed_down(),
        "expected no interpreted operators, got {stats:?}"
    );
}

/// ...and one that does not lower completely reports the gap rather than
/// silently falling back, so the remaining work is enumerable.
#[test]
fn an_unlowerable_query_reports_why() {
    let graph = fixture();
    let (stats, _) = islanded_on(
        &graph,
        "MATCH (p:person) RETURN list_append([1], p.age)",
        &SqlTarget::duckdb(),
    );
    assert!(!stats.fully_pushed_down());
    assert!(
        !stats.declined.is_empty(),
        "expected a recorded reason for the residual"
    );
}
