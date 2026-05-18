use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Int64Array, StringArray};

use new_graph::ir::bridge::gremlin as gb;
use new_graph::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use new_graph::ir::expr::Lit;
use new_graph::ir::rel::RelBackend;
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner as AstCypherPlanner;
use new_graph::planner::GremlinPlanner;

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

#[tokio::test]
async fn gremlin_out_values_executes_with_datafusion() {
    let traversal = gb::GremlinTraversal {
        steps: vec![
            gb::Step::V {
                label: Some("Person".into()),
            },
            gb::Step::Has {
                key: "name".into(),
                value: Lit::String("alice".into()),
            },
            gb::Step::Out {
                rel_types: vec!["KNOWS".into()],
            },
            gb::Step::Values("name".into()),
        ],
    };
    let plan = GremlinPlanner::new().plan(&traversal);
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

#[tokio::test]
async fn gremlin_v_count_executes_with_datafusion() {
    let traversal = gb::GremlinTraversal {
        steps: vec![
            gb::Step::V {
                label: Some("Person".into()),
            },
            gb::Step::Count,
        ],
    };
    let plan = GremlinPlanner::new().plan(&traversal);
    let result = RelBackend::new()
        .execute(&plan, &fixture_graph())
        .await
        .expect("rel execute");

    let array = result
        .batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();
    assert_eq!(array.value(0), 3);
}
