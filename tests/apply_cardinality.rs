#![cfg(feature = "duckdb")]

use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::ir::expr::{BinaryOp, IrExpr, Lit};
use orchiddb::ir::plan::{
    ApplyKind, CoalesceSuccess, GraphPlan, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem,
};
use orchiddb::ir::policy::{GraphPlanPolicy, OptionalMissing, ResultForm};
use orchiddb::ir::rel::RelBackend;
use orchiddb::ir::rel::sql::{self, DuckDbExecutor, SqlDialect, SqlExecutor, SqlValue};
use orchiddb::ir::value::Value;

fn plan(kind: ApplyKind, left_values: &[i64], inner_values: &[i64]) -> GraphPlan {
    let right = Node::GraphProject {
        mode: ProjectMode::ReplaceScope,
        items: vec![ProjectionItem {
            alias: "answer".into(),
            expr: IrExpr::Binding("item".into()),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: Box::new(Node::GraphUnwind {
            input_expr: IrExpr::List(
                inner_values
                    .iter()
                    .copied()
                    .map(|n| IrExpr::Lit(Lit::Int(n)))
                    .collect(),
            ),
            bind: "item".into(),
            outer: false,
            input: Box::new(Node::GraphCorrelate {
                bindings: vec!["x".into()],
            }),
        }),
    };
    GraphPlan::new(
        GraphPlanPolicy::cypher(),
        Node::GraphReturn {
            fields: vec!["x".into(), "answer".into()],
            result_form: ResultForm::RowSet,
            input: Box::new(Node::GraphApply {
                kind,
                correlation: vec!["x".into()],
                outputs: vec!["answer".into()],
                optional_missing: OptionalMissing::Null,
                left: Box::new(Node::GraphValues {
                    bindings: vec!["x".into()],
                    rows: left_values
                        .iter()
                        .copied()
                        .map(|n| vec![Value::Int(n)])
                        .collect(),
                    bulk: None,
                }),
                right: Box::new(right),
            }),
        },
    )
}

async fn execute(plan: GraphPlan) -> Result<Vec<Vec<SqlValue>>, String> {
    let lowered = RelBackend::new()
        .lower(&plan, &PropertyGraph::new())
        .map_err(|err| err.to_string())?;
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb)
        .await
        .map_err(|err| err.to_string())?;
    DuckDbExecutor::new()
        .run_with_tables(&prepared.tables, &prepared.setup, &prepared.query)
        .map_err(|err| format!("{err}\n{}", prepared.query))
}

#[tokio::test]
async fn scalar_apply_rejects_a_second_row_per_outer_row() {
    let error = execute(plan(ApplyKind::Scalar, &[1], &[10, 20]))
        .await
        .unwrap_err();
    assert!(
        error.contains("scalar subquery returned more than one row"),
        "{error}"
    );
}

#[tokio::test]
async fn scalar_apply_keeps_duplicate_outer_rows_independent() {
    let rows = execute(plan(ApplyKind::Scalar, &[1, 1], &[10]))
        .await
        .unwrap();
    assert_eq!(rows, vec![vec![SqlValue::Int(1), SqlValue::Int(10)]; 2]);
}

#[tokio::test]
async fn scalar_apply_with_no_inner_row_returns_null() {
    let rows = execute(plan(ApplyKind::Scalar, &[1], &[])).await.unwrap();
    assert_eq!(rows, vec![vec![SqlValue::Int(1), SqlValue::Null]]);
}

#[tokio::test]
async fn optional_apply_preserves_duplicate_outer_rows() {
    let rows = execute(plan(ApplyKind::Optional, &[1, 1], &[10]))
        .await
        .unwrap();
    assert_eq!(rows, vec![vec![SqlValue::Int(1), SqlValue::Int(10)]; 2]);
}

#[tokio::test]
async fn inner_apply_prefers_the_right_gremlin_path_on_an_uncorrelated_join() {
    let plan = GraphPlan::new(
        GraphPlanPolicy::gremlin(),
        Node::GraphReturn {
            fields: vec!["__path".into()],
            result_form: ResultForm::RowSet,
            input: Box::new(Node::GraphApply {
                kind: ApplyKind::Inner,
                correlation: Vec::new(),
                outputs: vec!["__path".into()],
                optional_missing: OptionalMissing::Null,
                left: Box::new(Node::GraphValues {
                    bindings: vec!["__path".into(), "left_only".into()],
                    rows: vec![vec![Value::String("outer-path".into()), Value::Int(1)]],
                    bulk: None,
                }),
                right: Box::new(Node::GraphValues {
                    bindings: vec!["__path".into(), "right_only".into()],
                    rows: vec![vec![Value::String("inner-path".into()), Value::Int(2)]],
                    bulk: None,
                }),
            }),
        },
    );
    let rows = execute(plan).await.unwrap();
    assert_eq!(rows, vec![vec![SqlValue::Text("inner-path".into())]]);
}

fn coalesce_arm(value: i64, answers: &[i64]) -> Node {
    Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: "answer".into(),
            expr: IrExpr::Binding("item".into()),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: Box::new(Node::GraphUnwind {
            input_expr: IrExpr::List(
                answers
                    .iter()
                    .copied()
                    .map(|n| IrExpr::Lit(Lit::Int(n)))
                    .collect(),
            ),
            bind: "item".into(),
            outer: false,
            input: Box::new(Node::GraphFilter {
                condition: IrExpr::Binary {
                    op: BinaryOp::Eq,
                    lhs: Box::new(IrExpr::Binding("x".into())),
                    rhs: Box::new(IrExpr::Lit(Lit::Int(value))),
                },
                input: Box::new(Node::GraphCorrelate {
                    bindings: vec!["x".into()],
                }),
            }),
        }),
    }
}

#[tokio::test]
async fn coalesce_uses_first_productive_of_three_arms_per_input_occurrence() {
    let plan = GraphPlan::new(
        GraphPlanPolicy::gremlin(),
        Node::GraphReturn {
            fields: vec!["x".into(), "answer".into()],
            result_form: ResultForm::RowSet,
            input: Box::new(Node::GraphCoalesce {
                success: CoalesceSuccess::FirstNonEmpty,
                output: "answer".into(),
                correlation: vec!["x".into()],
                arm_outputs: Vec::new(),
                input: Box::new(Node::GraphValues {
                    bindings: vec!["x".into()],
                    rows: vec![1, 2, 2, 3]
                        .into_iter()
                        .map(|n| vec![Value::Int(n)])
                        .collect(),
                    bulk: None,
                }),
                arms: vec![
                    coalesce_arm(1, &[10, 11]),
                    coalesce_arm(2, &[20]),
                    coalesce_arm(3, &[30]),
                ],
            }),
        },
    );
    let rows = execute(plan).await.unwrap();
    let expected = [
        vec![SqlValue::Int(1), SqlValue::Int(10)],
        vec![SqlValue::Int(1), SqlValue::Int(11)],
        vec![SqlValue::Int(2), SqlValue::Int(20)],
        vec![SqlValue::Int(2), SqlValue::Int(20)],
        vec![SqlValue::Int(3), SqlValue::Int(30)],
    ];
    assert_eq!(rows.len(), expected.len());
    for row in expected {
        assert_eq!(
            rows.iter().filter(|candidate| **candidate == row).count(),
            if row == vec![SqlValue::Int(2), SqlValue::Int(20)] {
                2
            } else {
                1
            }
        );
    }
}
