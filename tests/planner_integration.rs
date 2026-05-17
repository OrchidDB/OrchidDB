//! Black-box integration tests for the public planner API. Demonstrates
//! that a Cypher query AST and a Gremlin traversal AST flow through the
//! planner into the interpreter and produce Arrow record batches.

use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Int64Array, StringArray};

use new_graph::ir::bridge::cypher as cb;
use new_graph::ir::bridge::gremlin as gb;
use new_graph::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use new_graph::ir::expr::{BinaryOp, Lit};
use new_graph::ir::interpreter::execute;
use new_graph::ir::plan::{Direction, Length, SortDir};
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner as AstCypherPlanner;
use new_graph::planner::{CypherPlanner, GremlinPlanner};

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

#[test]
fn cypher_planner_round_trips_match_return() {
    let query = cb::CypherQuery {
        matches: vec![cb::MatchClause {
            optional: false,
            pattern: cb::Pattern {
                start: cb::NodePattern {
                    binding: "p".into(),
                    label: Some("Person".into()),
                    property_filters: Vec::new(),
                },
                chains: vec![cb::RelChain {
                    rel: cb::RelPattern {
                        binding: None,
                        rel_types: vec!["KNOWS".into()],
                        direction: Direction::Out,
                        length: Length::ONE,
                    },
                    node: cb::NodePattern {
                        binding: "f".into(),
                        label: None,
                        property_filters: Vec::new(),
                    },
                }],
            },
            r#where: Some(cb::Predicate::Compare {
                op: BinaryOp::Eq,
                lhs: Box::new(cb::Predicate::Property {
                    binding: "p".into(),
                    name: "name".into(),
                }),
                rhs: Box::new(cb::Predicate::Lit(Lit::String("alice".into()))),
            }),
        }],
        r#where: None,
        r#return: cb::ReturnClause {
            distinct: false,
            order_by: vec![cb::OrderItem {
                value: cb::Predicate::Property {
                    binding: "f".into(),
                    name: "name".into(),
                },
                dir: SortDir::Asc,
            }],
            skip: None,
            limit: None,
            items: vec![cb::ReturnItem {
                alias: "name".into(),
                value: cb::ReturnValue::Expr(cb::Predicate::Property {
                    binding: "f".into(),
                    name: "name".into(),
                }),
            }],
        },
    };

    let plan = CypherPlanner::new().plan(&query);
    let graph = fixture_graph();
    let result = execute(&plan, &graph).expect("execute");
    let array = result
        .batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    let names: Vec<&str> = (0..array.len()).map(|i| array.value(i)).collect();
    assert_eq!(names, vec!["bob", "carol"]);
}

#[test]
fn gremlin_planner_round_trips_v_count() {
    let traversal = gb::GremlinTraversal {
        steps: vec![
            gb::Step::V {
                label: Some("Person".into()),
            },
            gb::Step::Count,
        ],
    };
    let plan = GremlinPlanner::new().plan(&traversal);
    let result = execute(&plan, &fixture_graph()).expect("execute");
    let array = result
        .batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();
    assert_eq!(array.value(0), 3);
}

#[test]
fn cypher_count_if_uses_aggregate_path() {
    let parsed = parse_query("MATCH (p:Person) RETURN count_if(p.age > 29)").expect("parse");
    let plan = AstCypherPlanner::new().plan(&parsed).expect("plan");
    let result = execute(&plan, &fixture_graph()).expect("execute");
    let array = result
        .batch
        .column(0)
        .as_any()
        .downcast_ref::<Int64Array>()
        .unwrap();
    assert_eq!(array.value(0), 2);
}

#[test]
fn cypher_left_uses_kuzu_string_coercion() {
    let parsed =
        parse_query("RETURN left(to_double(1.34), 8), left(to_bool('true'), 4)").expect("parse");
    let plan = AstCypherPlanner::new().plan(&parsed).expect("plan");
    let result = execute(&plan, &PropertyGraph::new()).expect("execute");
    let double_text = result
        .batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    let bool_text = result
        .batch
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(double_text.value(0), "1.340000");
    assert_eq!(bool_text.value(0), "True");
}

#[test]
fn gremlin_planner_filters_then_traverses() {
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
    let result = execute(&plan, &fixture_graph()).expect("execute");
    let array = result
        .batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    let mut names: Vec<&str> = (0..array.len()).map(|i| array.value(i)).collect();
    names.sort();
    assert_eq!(names, vec!["bob", "carol"]);
}

fn plan_cypher_error(query: &str) -> String {
    let parsed = parse_query(query).expect("parse");
    AstCypherPlanner::new()
        .plan(&parsed)
        .expect_err("query should fail planning")
        .to_string()
}

#[test]
fn cypher_rejects_relationship_reused_as_node_pattern() {
    let err = plan_cypher_error("MATCH ()-[r]-() MATCH (r) RETURN r");
    assert!(err.contains("Binder exception: Cannot bind r as node pattern."));
}

#[test]
fn cypher_rejects_node_reused_as_relationship_pattern() {
    let err = plan_cypher_error("MATCH (r) MATCH ()-[r]-() RETURN r");
    assert!(err.contains("Binder exception: r has data type NODE but REL was expected."));
}

#[test]
fn cypher_rejects_path_reused_as_relationship_pattern() {
    let err = plan_cypher_error("MATCH r = ()-[*1..2]->() MATCH ()-[r]-() RETURN r");
    assert!(err.contains("Binder exception: r has data type RECURSIVE_REL but REL was expected."));
}

#[test]
fn cypher_rejects_rebinding_visible_path_variable() {
    let err = plan_cypher_error("MATCH (p) MATCH p = ()-[]-() RETURN p");
    assert!(err.contains("SyntaxError: VariableAlreadyBound"));
}
