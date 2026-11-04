use super::*;
use crate::ir::{
    plan::ProcedureArg,
    policy::GraphPlanPolicy,
    procedures::{ProcedureField, ProcedureSignature, TableProcedure},
};

fn context(graph: &PropertyGraph) -> LoweringContext<'_> {
    LoweringContext {
        island_memo: None,
        graph,
        options: RelBackendOptions {
            native_values: true,
            ..Default::default()
        },
        policy: GraphPlanPolicy::cypher(),
        language: Language::Cypher,
        scan_counter: 0,
        correlate_plan: None,
        rdf_typed_terms_used: false,
        scan_logical_labels: false,
        gremlin_label_binds: Default::default(),
        unordered_collects: Default::default(),
        unordered_distincts: Default::default(),
    }
}
fn fixture(outputs: Vec<ProcedureField>, rows: Vec<Vec<Value>>) -> PropertyGraph {
    let mut graph = PropertyGraph::new();
    graph
        .register_table_procedure(
            "test.lookup".into(),
            TableProcedure {
                signature: ProcedureSignature {
                    inputs: vec![ProcedureField {
                        name: "input".into(),
                        type_name: "INTEGER".into(),
                        nullable: true,
                    }],
                    outputs,
                },
                rows,
            },
        )
        .unwrap();
    graph
}
fn output() -> ProcedureField {
    ProcedureField {
        name: "out".into(),
        type_name: "STRING".into(),
        nullable: true,
    }
}
fn argument(value: IrExpr) -> Vec<ProcedureArg> {
    vec![ProcedureArg { name: None, value }]
}
fn source(values: Vec<Value>) -> Node {
    Node::GraphValues {
        bindings: vec!["arg".into()],
        rows: values.into_iter().map(|v| vec![v]).collect(),
        bulk: None,
    }
}

#[tokio::test]
async fn dynamic_lookup_keeps_null_matches_duplicates_and_empty_results() {
    for empty in [false, true] {
        let rows = if empty {
            vec![]
        } else {
            vec![
                vec![Value::Int(7), Value::String("a".into())],
                vec![Value::Int(7), Value::String("a".into())],
                vec![Value::Null, Value::String("null".into())],
                vec![Value::Int(8), Value::String("unmatched".into())],
            ]
        };
        let graph = fixture(vec![output()], rows);
        let mut ctx = context(&graph);
        let input = source(vec![Value::Int(7), Value::Null, Value::Int(99)]);
        let lowered = ctx
            .native_procedure(
                "test.lookup",
                &argument(IrExpr::binding("arg")),
                &["out".into()],
                Some(&input),
            )
            .unwrap();
        let plan = LogicalPlanBuilder::from(lowered.plan)
            .project(vec![col_exact("out")])
            .unwrap()
            .build()
            .unwrap();
        let batches = super::super::execute_test_plan(plan).await.unwrap();
        let mut actual = Vec::new();
        for batch in batches {
            for row in decode_rows(&batch, &["out".into()], &graph).unwrap() {
                let Value::String(value) = row.get("out") else {
                    panic!("lookup returned non-string")
                };
                actual.push(value);
            }
        }
        actual.sort();
        assert_eq!(
            actual,
            if empty {
                Vec::<String>::new()
            } else {
                vec!["a".into(), "a".into(), "null".into()]
            }
        );
    }
}

#[tokio::test]
async fn void_procedures_validate_dynamic_arguments_without_dropping_rows() {
    let graph = fixture(vec![], vec![]);
    let mut ctx = context(&graph);
    let input = source(vec![Value::Int(7), Value::Null, Value::Int(7)]);
    let lowered = ctx
        .native_procedure(
            "test.lookup",
            &argument(IrExpr::binding("arg")),
            &[],
            Some(&input),
        )
        .unwrap();
    let batches = super::super::execute_test_plan(lowered.plan).await.unwrap();
    assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 3);
    for empty in [false, true] {
        let mut ctx = context(&graph);
        let input = source(if empty { vec![] } else { vec![Value::Int(7)] });
        let lowered = ctx
            .native_procedure(
                "test.lookup",
                &argument(IrExpr::Lit(Lit::String("bad".into()))),
                &[],
                Some(&input),
            )
            .unwrap();
        let result = super::super::execute_test_plan(lowered.plan).await;
        if empty {
            assert_eq!(
                result
                    .unwrap()
                    .iter()
                    .map(RecordBatch::num_rows)
                    .sum::<usize>(),
                0
            );
        } else {
            assert!(
                result
                    .unwrap_err()
                    .to_string()
                    .contains("Invalid procedure argument")
            );
        }
    }
}

#[test]
fn typed_procedure_lowering_rejects_unknown_outputs_arity_and_unregistered_names() {
    let graph = fixture(vec![output()], vec![]);
    let mut ctx = context(&graph);
    for (name, args, yields, expected) in [
        (
            "test.missing",
            argument(IrExpr::Lit(Lit::Int(7))),
            vec!["out".into()],
            "Unregistered host procedure",
        ),
        (
            "test.lookup",
            vec![],
            vec!["out".into()],
            "Invalid procedure arity",
        ),
        (
            "test.lookup",
            argument(IrExpr::Lit(Lit::Int(7))),
            vec!["missing".into()],
            "Unknown procedure output",
        ),
    ] {
        let error = ctx
            .native_procedure(name, &args, &yields, None)
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}
