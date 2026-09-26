//! Distinct semantics for the relational (SQL) backend.
//!
//! Keyed deduplication retains the first row's other bindings and input order.

#![cfg(feature = "duckdb")]

#[path = "common/execution.rs"]
mod datafusion_test;
use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, StringArray};

use orchiddb::ir::bridge::cypher as cb;
use orchiddb::ir::catalog::{PropertyGraph, nodes_from_columns};
use crate::datafusion_test::execute_rows;
use orchiddb::ir::plan::{DistinctBulk, DistinctMode, GraphPlan, Node};
use orchiddb::ir::policy::{GraphPlanPolicy, ResultForm};
use orchiddb::ir::rel::RelBackend;
use orchiddb::ir::rel::sql::{self, DuckDbExecutor, SqlDialect, SqlExecutor, SqlValue};
use orchiddb::ir::value::Value;
use orchiddb::language::gremlin::parser::parse_traversal;
use orchiddb::language::gremlin::planner::GremlinPlanner as AstGremlinPlanner;
use orchiddb::planner::CypherPlanner;

/// Nodes labelled `P` with `group = [a, a, b]`. `name` and `score` let a test
/// distinguish *which* row of a repeated group is the "first" one.
fn group_graph() -> PropertyGraph {
    let group: ArrayRef = Arc::new(StringArray::from(vec!["a", "a", "b"]));
    let name: ArrayRef = Arc::new(StringArray::from(vec!["x1", "x2", "x3"]));
    let score: ArrayRef = Arc::new(Int64Array::from(vec![1, 5, 2]));
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "P",
        vec![("group", group), ("name", name), ("score", score)],
    ));
    graph
}

fn gremlin_plan(source: &str) -> GraphPlan {
    let traversal = parse_traversal(source).expect("parse Gremlin");
    AstGremlinPlanner::new()
        .plan(&traversal)
        .expect("plan Gremlin")
}

async fn sql_rows(plan: &GraphPlan, graph: &PropertyGraph) -> Vec<Vec<SqlValue>> {
    let lowered = RelBackend::new().lower(plan, graph).expect("lower");
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb)
        .await
        .expect("prepare sql");
    DuckDbExecutor::new()
        .run_with_tables(&prepared.tables, &prepared.setup, &prepared.query)
        .expect("duckdb execute")
}

fn text(s: &str) -> SqlValue {
    SqlValue::Text(s.to_string())
}

fn int(n: i64) -> SqlValue {
    SqlValue::Int(n)
}

/// Hand-built `GraphValues -> GraphDistinct(keys) -> GraphReturn(fields)`.
/// `keys` may be empty (full-row distinct) or a subset of `bindings`.
fn values_distinct_plan(bindings: Vec<&str>, rows: Vec<Vec<Value>>, keys: Vec<&str>) -> GraphPlan {
    let bindings: Vec<String> = bindings.into_iter().map(String::from).collect();
    let keys: Vec<String> = keys.into_iter().map(String::from).collect();
    let fields = bindings.clone();
    GraphPlan::new(
        GraphPlanPolicy::gremlin(),
        Node::GraphReturn {
            fields,
            result_form: ResultForm::RowSet,
            input: Box::new(Node::GraphDistinct {
                keys,
                mode: DistinctMode::Row,
                bulk: DistinctBulk::NotApplicable,
                input: Box::new(Node::GraphValues {
                    bindings,
                    rows,
                    bulk: None,
                }),
            }),
        },
    )
}

#[tokio::test]
async fn gremlin_dedup_by_group_keeps_first_row_only() {
    let plan = gremlin_plan(r#"g.V().hasLabel("P").dedup().by("group").values("name")"#);
    let actual = sql_rows(&plan, &group_graph()).await;
    assert_eq!(actual, vec![vec![text("x1")], vec![text("x3")]]);
}

#[tokio::test]
async fn gremlin_order_then_dedup_by_group() {
    let plan = gremlin_plan(
        r#"g.V().hasLabel("P").order().by("score", Order.desc).dedup().by("group").values("name")"#,
    );
    let actual = sql_rows(&plan, &group_graph()).await;
    assert_eq!(actual, vec![vec![text("x2")], vec![text("x3")]]);
}

#[tokio::test]
async fn cypher_return_distinct_by_property_ignores_hidden_columns() {
    let query = cb::CypherQuery {
        matches: vec![cb::MatchClause {
            optional: false,
            pattern: cb::Pattern {
                start: cb::NodePattern {
                    binding: "p".into(),
                    label: Some("P".into()),
                    property_filters: Vec::new(),
                },
                chains: Vec::new(),
            },
            r#where: None,
        }],
        r#where: None,
        r#return: cb::ReturnClause {
            items: vec![cb::ReturnItem {
                alias: "g".into(),
                value: cb::ReturnValue::Expr(cb::Predicate::Property {
                    binding: "p".into(),
                    name: "group".into(),
                }),
            }],
            distinct: true,
            order_by: Vec::new(),
            skip: None,
            limit: None,
        },
    };
    let plan = CypherPlanner::new().plan(&query);
    let actual = sql_rows(&plan, &group_graph()).await;
    assert_eq!(actual, vec![vec![text("a")], vec![text("b")]]);
}

#[tokio::test]
async fn handbuilt_distinct_ignores_non_key_columns() {
    let plan = values_distinct_plan(
        vec!["group", "name", "score"],
        vec![
            vec![
                Value::String("a".into()),
                Value::String("x1".into()),
                Value::Int(1),
            ],
            vec![
                Value::String("a".into()),
                Value::String("x2".into()),
                Value::Int(5),
            ],
            vec![
                Value::String("b".into()),
                Value::String("x3".into()),
                Value::Int(2),
            ],
        ],
        vec!["group"],
    );
    let actual = sql_rows(&plan, &PropertyGraph::new()).await;
    assert_eq!(
        actual,
        vec![
            vec![text("a"), text("x1"), int(1)],
            vec![text("b"), text("x3"), int(2)],
        ]
    );
}

#[tokio::test]
async fn handbuilt_distinct_groups_null_keys_together() {
    let plan = values_distinct_plan(
        vec!["group", "name"],
        vec![
            vec![Value::String("a".into()), Value::String("x1".into())],
            vec![Value::Null, Value::String("x2".into())],
            vec![Value::Null, Value::String("x3".into())],
            vec![Value::String("b".into()), Value::String("x4".into())],
        ],
        vec!["group"],
    );
    let actual = sql_rows(&plan, &PropertyGraph::new()).await;
    assert_eq!(
        actual,
        vec![
            vec![text("a"), text("x1")],
            vec![SqlValue::Null, text("x2")],
            vec![text("b"), text("x4")],
        ]
    );
}

#[tokio::test]
async fn handbuilt_distinct_empty_input() {
    let plan = values_distinct_plan(vec!["group", "name"], Vec::new(), vec!["group"]);
    let actual = sql_rows(&plan, &PropertyGraph::new()).await;
    assert!(actual.is_empty(), "empty input must stay empty: {actual:?}");
}

#[tokio::test]
async fn handbuilt_distinct_composite_keys() {
    let plan = values_distinct_plan(
        vec!["g1", "g2", "name"],
        vec![
            vec![
                Value::String("a".into()),
                Value::Int(1),
                Value::String("x1".into()),
            ],
            vec![
                Value::String("a".into()),
                Value::Int(1),
                Value::String("x2".into()),
            ],
            vec![
                Value::String("a".into()),
                Value::Int(2),
                Value::String("x3".into()),
            ],
            vec![
                Value::String("b".into()),
                Value::Int(1),
                Value::String("x4".into()),
            ],
        ],
        vec!["g1", "g2"],
    );
    let actual = sql_rows(&plan, &PropertyGraph::new()).await;
    assert_eq!(
        actual,
        vec![
            vec![text("a"), int(1), text("x1")],
            vec![text("a"), int(2), text("x3")],
            vec![text("b"), int(1), text("x4")],
        ]
    );
}

#[test]
fn interpreter_node_identity_includes_label() {
    let plan = GraphPlan::new(
        GraphPlanPolicy::gremlin(),
        Node::GraphDistinct {
            keys: Vec::new(),
            mode: DistinctMode::Row,
            bulk: DistinctBulk::NotApplicable,
            input: Box::new(Node::GraphValues {
                bindings: vec!["current".into()],
                rows: vec![
                    vec![Value::Node {
                        label: "P".into(),
                        id: 0,
                    }],
                    vec![Value::Node {
                        label: "Q".into(),
                        id: 0,
                    }],
                ],
                bulk: None,
            }),
        },
    );
    let rows = execute_rows(&plan, &PropertyGraph::new()).expect("interpret");
    assert_eq!(
        rows.len(),
        2,
        "same local id under two labels must be distinct nodes"
    );
}

#[tokio::test]
async fn sql_element_key_includes_label() {
    use orchiddb::ir::plan::LabelExpr;
    let mut graph = PropertyGraph::new();
    for label in ["P", "Q"] {
        graph.add_nodes(nodes_from_columns(
            label,
            vec![(
                "name",
                Arc::new(StringArray::from(vec!["same"])) as ArrayRef,
            )],
        ));
    }
    let plan = GraphPlan::new(
        GraphPlanPolicy::gremlin(),
        Node::GraphReturn {
            fields: vec!["n".into()],
            result_form: ResultForm::RowSet,
            input: Box::new(Node::GraphDistinct {
                keys: vec!["n".into()],
                mode: DistinctMode::Row,
                bulk: DistinctBulk::NotApplicable,
                input: Box::new(Node::GraphNodeScan {
                    graph: "default".into(),
                    binding: "n".into(),
                    labels: LabelExpr::Any,
                }),
            }),
        },
    );
    let rows = sql_rows(&plan, &graph).await;
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0], rows[1]);
}
