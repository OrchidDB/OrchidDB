use super::*;
use crate::ir::plan::{
    JoinKind, NullsOrder, ProjectErrorPolicy, ProjectMode, ProjectionItem, SortDir, SortKey,
};

fn project(input: Node, mode: ProjectMode, names: &[&str]) -> Node {
    Node::GraphProject {
        mode,
        items: names
            .iter()
            .map(|name| ProjectionItem {
                alias: (*name).into(),
                expr: IrExpr::Lit(Lit::Int(1)),
            })
            .collect(),
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    }
}

#[test]
fn singleton_projection_preserves_unique_visible_names_and_replaces_scope() {
    let input = project(Node::GraphOneRow, ProjectMode::ReplaceScope, &["a", "b"]);
    let extended = project(input.clone(), ProjectMode::PreserveVisible, &["b", "c"]);
    assert_eq!(
        singleton_fields(&extended),
        Some(vec!["a".into(), "b".into(), "c".into()])
    );
    assert_eq!(
        singleton_fields(&project(input, ProjectMode::ReplaceScope, &["c"])),
        Some(vec!["c".into()])
    );
    assert_eq!(
        singleton_fields(&project(
            Node::GraphEmpty,
            ProjectMode::PreserveVisible,
            &["c"]
        )),
        None
    );
}

#[test]
fn total_source_proof_rejects_row_dependent_filters_and_unwind_expressions() {
    for condition in [
        IrExpr::Lit(Lit::Null),
        IrExpr::Lit(Lit::Bool(false)),
        IrExpr::Lit(Lit::Bool(true)),
    ] {
        assert!(total_source(&Node::GraphFilter {
            condition,
            input: Node::GraphOneRow.boxed()
        }));
    }
    assert!(!total_source(&Node::GraphFilter {
        condition: IrExpr::binding("x"),
        input: Node::GraphOneRow.boxed()
    }));
    for (items, expected) in [
        (vec![], true),
        (vec![IrExpr::Lit(Lit::Null)], true),
        (vec![IrExpr::binding("x")], false),
    ] {
        let unwind = Node::GraphUnwind {
            input_expr: IrExpr::List(items),
            bind: "x".into(),
            outer: false,
            input: Node::GraphOneRow.boxed(),
        };
        assert_eq!(total_source(&unwind), expected);
        assert_eq!(singleton_fields(&unwind), None);
    }
    let sorted = Node::GraphSort {
        keys: vec![SortKey {
            expr: IrExpr::binding("x"),
            dir: SortDir::Asc,
            nulls: NullsOrder::Last,
        }],
        input: Node::GraphEmpty.boxed(),
    };
    assert!(total_source(&sorted));
    let mut joined = Node::GraphJoin {
        kind: JoinKind::Cross,
        condition: None,
        left: sorted.boxed(),
        right: Node::GraphOneRow.boxed(),
    };
    assert!(total_source(&joined));
    if let Node::GraphJoin { condition, .. } = &mut joined {
        *condition = Some(IrExpr::Lit(Lit::Bool(true)));
    }
    assert!(!total_source(&joined));
}

#[test]
fn aggregate_singleton_proof_requires_total_count_and_no_grouping() {
    use crate::ir::expr::AggCall;
    let base = Node::GraphAggregate {
        group: vec![],
        aggs: vec![AggCall {
            kind: AggKind::CountRows,
            alias: "n".into(),
            arg: None,
            distinct: false,
        }],
        fields: vec!["n".into()],
        input: Node::GraphEmpty.boxed(),
    };
    assert_eq!(singleton_fields(&base), Some(vec!["n".into()]));
    for (kind, distinct, arg, expected) in [
        (AggKind::CountRows, false, Some(IrExpr::binding("x")), true),
        (AggKind::CountRows, true, None, false),
        (AggKind::Sum, false, Some(IrExpr::binding("x")), false),
        (
            AggKind::CountRows,
            false,
            Some(IrExpr::Lit(Lit::Int(1))),
            false,
        ),
    ] {
        let mut aggregate = base.clone();
        if let Node::GraphAggregate { aggs, .. } = &mut aggregate {
            aggs[0].kind = kind;
            aggs[0].distinct = distinct;
            aggs[0].arg = arg;
        }
        assert_eq!(total_source(&aggregate), expected);
        assert_eq!(singleton_fields(&aggregate).is_some(), expected);
    }
    let mut grouped = base;
    if let Node::GraphAggregate { group, .. } = &mut grouped {
        group.push(ProjectionItem {
            alias: "x".into(),
            expr: IrExpr::binding("x"),
        });
    }
    assert!(total_source(&grouped));
    assert_eq!(singleton_fields(&grouped), None);
}

#[test]
fn specialization_avoids_ordinal_collisions_and_preserves_registration_order() {
    use crate::ir::procedures::{ProcedureField, ProcedureSignature, TableProcedure};
    let ordinal = "__procedure_registration_order";
    let mut graph = PropertyGraph::new();
    graph
        .register_table_procedure(
            "test.ordered".into(),
            TableProcedure {
                signature: ProcedureSignature {
                    inputs: vec![],
                    outputs: vec![ProcedureField {
                        name: "value".into(),
                        type_name: "INTEGER".into(),
                        nullable: false,
                    }],
                },
                rows: vec![
                    vec![Value::Int(8)],
                    vec![Value::Int(3)],
                    vec![Value::Int(8)],
                ],
            },
        )
        .unwrap();
    let input = project(
        Node::GraphOneRow,
        ProjectMode::PreserveVisible,
        &[ordinal, "__procedure_registration_order_"],
    );
    let call = Node::GraphProcedureCall {
        name: "test.ordered".into(),
        args: vec![],
        yields: vec!["value".into()],
        mode: ProcedureMode::Read,
        input: Some(input.boxed()),
    };
    let replacement = specialize_call(&call, &graph).expect("constant table call must specialize");
    let Node::GraphProject { items, input, .. } = replacement else {
        panic!("projection missing")
    };
    assert_eq!(
        items
            .iter()
            .map(|item| item.alias.as_str())
            .collect::<Vec<_>>(),
        vec![ordinal, "__procedure_registration_order_", "value"]
    );
    let Node::GraphSort { keys, input } = *input else {
        panic!("registration sort missing")
    };
    assert_eq!(
        keys[0].expr,
        IrExpr::binding("__procedure_registration_order__")
    );
    let Node::GraphJoin { right, .. } = *input else {
        panic!("outer row missing")
    };
    let Node::GraphValues { bindings, rows, .. } = *right else {
        panic!("snapshot missing")
    };
    assert_eq!(bindings, vec!["value", "__procedure_registration_order__"]);
    assert_eq!(
        rows,
        vec![
            vec![Value::Int(8), Value::Int(0)],
            vec![Value::Int(3), Value::Int(1)],
            vec![Value::Int(8), Value::Int(2)]
        ]
    );
    let mut collision = call;
    if let Node::GraphProcedureCall { input, .. } = &mut collision {
        *input = Some(project(Node::GraphOneRow, ProjectMode::PreserveVisible, &["value"]).boxed());
    }
    assert!(specialize_call(&collision, &graph).is_none());
}
