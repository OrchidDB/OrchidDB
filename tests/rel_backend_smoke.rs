use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Int64Array, StringArray};

use orchiddb::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use orchiddb::ir::expr::Lit;
use orchiddb::ir::rel::RelBackend;
use orchiddb::language::cypher::parser::parse_query;
use orchiddb::language::cypher::planner::CypherPlanner as AstCypherPlanner;

fn fixture_graph() -> PropertyGraph {
    let names: ArrayRef = Arc::new(StringArray::from(vec!["alice", "bob", "carol"]));
    let ages: ArrayRef = Arc::new(Int64Array::from(vec![30, 28, 41]));
    let person = nodes_from_columns("Person", vec![("name", names), ("age", ages)]);

    let mut graph = PropertyGraph::new();
    graph.add_nodes(person);

    let edges = edges_from_columns(
        "KNOWS",
        "Person",
        "Person",
        vec![0, 0, 1],
        vec![1, 2, 2],
        Vec::new(),
    );
    graph.add_edges(edges).unwrap();
    graph
}

#[tokio::test]
async fn cypher_match_filter_expand_executes_with_datafusion() {
    let parsed = parse_query(
        "MATCH (p:Person)-[:KNOWS]->(f) WHERE p.name = 'alice' RETURN f.name ORDER BY f.name",
    )
    .expect("parse");
    let plan = AstCypherPlanner::new().plan(&parsed).expect("plan");
    let result = RelBackend::new()
        .execute(&plan, &fixture_graph())
        .await
        .expect("rel execute");

    let array = result
        .batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    let names = (0..array.len())
        .map(|row| array.value(row))
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["bob", "carol"]);
}
