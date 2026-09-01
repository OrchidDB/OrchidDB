//! Ad-hoc Cypher debugging harness.
//!
//! Usage:
//!   cargo run --example cypher_dbg -- "<setup cypher;another stmt>" "<query>"
//!
//! Builds an empty PropertyGraph, applies each `;`-separated setup
//! statement (CREATE ...), then plans + executes the query and prints
//! the plan and returned rows.

use new_graph::ir::catalog::PropertyGraph;
use new_graph::ir::interpreter::execute;
use new_graph::ir::plan::explain;
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: cypher_dbg \"<setup;setup>\" \"<query>\"");
        std::process::exit(2);
    }
    let setup = &args[1];
    let query = &args[2];

    let mut graph = PropertyGraph::new();
    if setup.trim() == "@selfloop" {
        // One node `A {name: 'x'}` with a T self-loop plus a second
        // node B and an edge A->B, for testing loop patterns.
        use arrow::array::{ArrayRef, StringArray};
        use new_graph::ir::catalog::{edges_from_columns, nodes_from_columns};
        use std::sync::Arc;
        let names: ArrayRef = Arc::new(StringArray::from(vec!["x"]));
        graph.add_nodes(nodes_from_columns("A", vec![("name", names)]));
        let names_b: ArrayRef = Arc::new(StringArray::from(vec!["y"]));
        graph.add_nodes(nodes_from_columns("B", vec![("name", names_b)]));
        graph
            .add_edges(edges_from_columns("T", "A", "A", vec![0], vec![0], vec![]))
            .unwrap();
        graph
            .add_edges(edges_from_columns("T", "A", "B", vec![0], vec![0], vec![]))
            .unwrap();
    }
    for stmt in setup.split(';') {
        if stmt.trim() == "@selfloop" {
            continue;
        }
        let stmt = stmt.trim();
        if stmt.is_empty() {
            continue;
        }
        let parsed = parse_query(stmt).unwrap_or_else(|e| panic!("setup parse: {e}"));
        let plan = CypherPlanner::new()
            .plan(&parsed)
            .unwrap_or_else(|e| panic!("setup plan: {e}"));
        execute(&plan, &graph).unwrap_or_else(|e| panic!("setup run: {e}"));
    }

    let parsed = match parse_query(query) {
        Ok(q) => q,
        Err(e) => {
            println!("PARSE ERROR: {e}");
            return;
        }
    };
    let plan = match CypherPlanner::new().plan(&parsed) {
        Ok(p) => p,
        Err(e) => {
            println!("PLAN ERROR: {e}");
            return;
        }
    };
    println!("--- plan ---\n{}", explain(&plan));
    match execute(&plan, &graph) {
        Ok(returned) => {
            println!("--- fields: {:?}", returned.fields);
            println!(
                "{}",
                arrow::util::pretty::pretty_format_batches(&[returned.batch]).unwrap()
            );
        }
        Err(e) => println!("RUN ERROR: {e}"),
    }
}
