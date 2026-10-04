use super::*;
use crate::ir::procedures::{ProcedureField, ProcedureSignature, TableProcedure};

#[test]
fn prepared_yields_preserve_order_and_lazy_validation() {
    let mut graph = PropertyGraph::new();
    let field = |name: &str| ProcedureField {
        name: name.into(),
        type_name: "INTEGER".into(),
        nullable: false,
    };
    graph.procedures = std::sync::Arc::new(BTreeMap::from([(
        "test.rows".into(),
        TableProcedure {
            signature: ProcedureSignature {
                inputs: vec![],
                outputs: vec![field("first"), field("second")],
            },
            rows: vec![
                vec![Value::Int(1), Value::Int(2)],
                vec![Value::Int(3), Value::Int(4)],
            ],
        },
    )]));
    let yields = vec!["second".into(), "first".into()];
    let result = procedure_call_op(
        "test.rows",
        &[],
        &yields,
        vec![Row::new(), Row::new()],
        &graph,
    )
    .unwrap();
    assert_eq!(
        result
            .iter()
            .map(|row| (row.get("second"), row.get("first")))
            .collect::<Vec<_>>(),
        vec![
            (Value::Int(2), Value::Int(1)),
            (Value::Int(4), Value::Int(3)),
            (Value::Int(2), Value::Int(1)),
            (Value::Int(4), Value::Int(3)),
        ]
    );
    let invalid = vec!["missing".into()];
    assert!(
        procedure_call_op("test.rows", &[], &invalid, vec![], &graph)
            .unwrap()
            .is_empty()
    );
    assert!(procedure_call_op("test.rows", &[], &invalid, vec![Row::new()], &graph).is_err());
    std::sync::Arc::make_mut(&mut graph.procedures)
        .get_mut("test.rows")
        .unwrap()
        .rows
        .clear();
    assert!(
        procedure_call_op("test.rows", &[], &invalid, vec![Row::new()], &graph)
            .unwrap()
            .is_empty()
    );
}
