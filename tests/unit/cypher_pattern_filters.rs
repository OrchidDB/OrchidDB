use super::*;

#[test]
fn parameter_property_filter_retains_binding_and_parameter_identity() {
    let mut lowerer = Lowerer::new();
    lowerer.replace_scope(vec!["outside".to_string()]);
    let result = apply_property_filters(
        &mut lowerer,
        Node::GraphOneRow,
        "person",
        &Expr::Parameter("criteria".into()),
        &BTreeSet::new(),
    )
    .unwrap();
    let Node::GraphFilter { condition, input } = result else {
        panic!("missing filter")
    };
    assert_eq!(
        condition,
        IrExpr::Call {
            name: "cypher_properties_match".into(),
            args: vec![
                IrExpr::Binding("person".into()),
                IrExpr::Call {
                    name: "parameter".into(),
                    args: vec![IrExpr::Lit(Lit::String("criteria".into()))]
                }
            ]
        }
    );
    assert!(matches!(*input, Node::GraphOneRow));
    assert_eq!(lowerer.visible_set(), BTreeSet::from(["outside".into()]));
}

#[test]
fn pattern_filter_errors_preserve_the_callers_scope() {
    for recursive in [false, true] {
        for expression in [
            Expr::Variable("missing".into()),
            Expr::Map(vec![("weight".into(), Expr::Variable("missing".into()))]),
        ] {
            let mut lowerer = Lowerer::new();
            let scope = BTreeSet::from(["outside".to_string()]);
            lowerer.replace_scope(scope.iter().cloned());
            let result = if recursive {
                apply_variable_length_property_filters(
                    &mut lowerer,
                    Node::GraphOneRow,
                    "path",
                    "source",
                    &expression,
                    &scope,
                )
            } else {
                apply_property_filters(
                    &mut lowerer,
                    Node::GraphOneRow,
                    "person",
                    &expression,
                    &scope,
                )
            };
            assert!(result.is_err(), "unbound property expression must fail");
            assert!(result.unwrap_err().to_string().contains("missing"));
            assert_eq!(lowerer.visible_set(), scope);
        }
    }
}

#[test]
fn recursive_parameter_filter_quantifies_over_edges_without_leaking_its_local() {
    let mut lowerer = Lowerer::new();
    let scope = BTreeSet::from(["path".to_string(), "source".to_string()]);
    lowerer.replace_scope(scope.iter().cloned());
    let result = apply_variable_length_property_filters(
        &mut lowerer,
        Node::GraphOneRow,
        "path",
        "source",
        &Expr::Parameter("criteria".into()),
        &scope,
    )
    .unwrap();
    assert!(matches!(&result, Node::GraphFilter { .. }));
    let printed = format!("{result:?}");
    assert!(printed.contains("cypher_properties_match"), "{printed}");
    assert!(printed.contains("criteria"), "{printed}");
    assert!(printed.contains("path"), "{printed}");
    assert_eq!(lowerer.visible_set(), scope);
}
