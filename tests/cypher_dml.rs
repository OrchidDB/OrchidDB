//! End-to-end coverage for Cypher write clauses against the interpreter.
//!
//! `CREATE` with relationship patterns, `MERGE`, whole-map `SET`, and
//! relationship `DELETE` all mutate the catalog overlay rather than the
//! immutable Arrow fixtures, so these tests double as overlay regression
//! tests (id allocation, adjacency, property replacement).

use new_graph::ir::catalog::PropertyGraph;
use new_graph::ir::interpreter::execute;
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner;

/// Run `statements` in order against a fresh graph, then return the rows of
/// the final `query` rendered as `|`-joined strings.
fn run(statements: &[&str], query: &str) -> Vec<String> {
    let graph = PropertyGraph::new();
    for statement in statements {
        let parsed = parse_query(statement).expect("setup parse");
        let plan = CypherPlanner::new().plan(&parsed).expect("setup plan");
        execute(&plan, &graph).expect("setup run");
    }
    let parsed = parse_query(query).expect("query parse");
    let plan = CypherPlanner::new().plan(&parsed).expect("query plan");
    let returned = execute(&plan, &graph).expect("query run");
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

#[test]
fn create_inline_relationship_chain_links_every_hop() {
    let rows = run(
        &["CREATE (a:P {n: 'a'})-[:R]->(b:P {n: 'b'})-[:R]->(c:P {n: 'c'})"],
        "MATCH (x)-[:R]->(y) RETURN x.n, y.n ORDER BY x.n",
    );
    assert_eq!(rows, vec!["a|b", "b|c"]);
}

#[test]
fn create_relationship_between_matched_nodes_carries_properties() {
    let rows = run(
        &[
            "CREATE (a:P {n: 'a'})",
            "CREATE (b:P {n: 'b'})",
            "MATCH (a:P {n: 'a'}), (b:P {n: 'b'}) CREATE (a)-[:Knows {since: 2020}]->(b)",
        ],
        "MATCH (x)-[e:Knows]->(y) RETURN x.n, e.since, y.n",
    );
    assert_eq!(rows, vec!["a|2020|b"]);
}

#[test]
fn create_self_loop_reuses_the_bound_variable() {
    let rows = run(
        &["CREATE (a:P {n: 'a'})", "MATCH (a:P) CREATE (a)-[:R]->(a)"],
        "MATCH (x)-[:R]->(y) RETURN x.n, y.n",
    );
    assert_eq!(rows, vec!["a|a"]);
}

#[test]
fn delete_removes_only_the_named_relationship() {
    let rows = run(
        &[
            "CREATE (a:P {n: 'a'})-[:R]->(b:P {n: 'b'})",
            "MATCH (:P)-[r:R]->(:P) DELETE r",
        ],
        "MATCH (x)-[:R]->(y) RETURN x.n, y.n",
    );
    assert!(rows.is_empty(), "expected no surviving edges, got {rows:?}");
}

#[test]
fn set_map_assignment_replaces_the_whole_property_bag() {
    let rows = run(
        &["CREATE (a:P {n: 'a', x: 1})"],
        "MATCH (p:P) SET p = {n: 'z'} RETURN p.n, p.x",
    );
    assert_eq!(rows, vec!["z|"]);
}

#[test]
fn set_map_merge_keeps_untouched_properties() {
    let rows = run(
        &["CREATE (a:P {n: 'a', x: 1})"],
        "MATCH (p:P) SET p += {y: 9} RETURN p.n, p.x, p.y",
    );
    assert_eq!(rows, vec!["a|1|9"]);
}

#[test]
fn merge_matches_an_existing_node_instead_of_creating() {
    let rows = run(
        &["CREATE (a:P {n: 'a', c: 0})"],
        "MERGE (p:P {n: 'a'}) ON MATCH SET p.c = 1 ON CREATE SET p.c = 9 RETURN p.n, p.c",
    );
    assert_eq!(rows, vec!["a|1"]);
}

#[test]
fn merge_creates_when_the_pattern_is_absent() {
    let rows = run(
        &["CREATE (a:P {n: 'a', c: 0})"],
        "MERGE (p:P {n: 'b'}) ON MATCH SET p.c = 1 ON CREATE SET p.c = 9 RETURN p.n, p.c",
    );
    assert_eq!(rows, vec!["b|9"]);
}

#[test]
fn merge_relationship_does_not_duplicate_an_existing_edge() {
    let rows = run(
        &[
            "CREATE (a:P {n: 'a'})-[:R]->(b:P {n: 'b'})",
            "MATCH (a:P {n: 'a'}), (b:P {n: 'b'}) MERGE (a)-[:R]->(b)",
        ],
        "MATCH ()-[r:R]->() RETURN count(r)",
    );
    assert_eq!(rows, vec!["1"]);
}

#[test]
fn merge_runs_once_per_correlated_input_row() {
    let rows = run(
        &["CREATE (a:P {n: 'a'})", "CREATE (b:P {n: 'b'})"],
        "MATCH (p:P) MERGE (t:T {n: p.n}) RETURN t.n ORDER BY t.n",
    );
    assert_eq!(rows, vec!["a", "b"]);
}

#[test]
fn detach_delete_drops_incident_relationships() {
    let rows = run(
        &[
            "CREATE (a:P {n: 'a'})-[:R]->(b:P {n: 'b'})",
            "MATCH (a:P {n: 'a'}) DETACH DELETE a",
        ],
        "MATCH ()-[r:R]->() RETURN count(r)",
    );
    assert_eq!(rows, vec!["0"]);
}
