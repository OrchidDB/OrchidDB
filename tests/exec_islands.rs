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
use new_graph::ir::exec::plan_with_islands;
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

/// Partition `query`, returning (island count, rows).
fn islanded(graph: &PropertyGraph, query: &str) -> (usize, Vec<String>) {
    let parsed = parse_query(query).expect("parse");
    let plan = CypherPlanner::new().plan(&parsed).expect("plan");
    let backend = RelBackend::new();
    let (hybrid, stats) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(plan_with_islands(&plan, graph, &backend));
    let returned = execute(&hybrid, graph).expect("run hybrid");
    (stats.islands, render(returned))
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
        .block_on(plan_with_islands(&plan, &graph, &backend));
    assert_eq!(stats.islands, 0, "a CREATE plan must not be islanded");
}
