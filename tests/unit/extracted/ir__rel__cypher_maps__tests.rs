
    use super::*;

    #[test]
    fn substitution_never_captures_a_constructor_binding() {
        let values = BTreeMap::from([("xs".into(), IrExpr::List(vec![IrExpr::Binding("a".into())]))]);
        let expr = IrExpr::ListTransform {
            list: Box::new(IrExpr::List(vec![IrExpr::Lit(Lit::Int(1))])),
            item: "a".into(),
            map: Box::new(IrExpr::Binding("xs".into())),
        };
        assert!(substitute(&expr, &values).is_none());
    }

    #[test]
    fn selection_does_not_drop_observable_constructor_evaluation() {
        let expr = IrExpr::Call { name: "cypher_subscript".into(), args: vec![
            IrExpr::List(vec![IrExpr::Lit(Lit::Int(1)), IrExpr::Call { name: "rand".into(), args: vec![] }]),
            IrExpr::Lit(Lit::Int(0)),
        ] };
        assert_eq!(select_constructor(expr.clone()), expr);
    }

    #[test]
    fn literal_collection_eligibility_excludes_evaluation_effects() {
        for expression in [
            IrExpr::Call { name: "rand".into(), args: vec![] },
            IrExpr::Binary { op: BinaryOp::Div,
                lhs: Box::new(IrExpr::Lit(Lit::Int(1))),
                rhs: Box::new(IrExpr::Lit(Lit::Int(0))) },
            IrExpr::Binding("outer".into()),
        ] {
            assert!(!literal_tree(&IrExpr::List(vec![IrExpr::Lit(Lit::Int(1)), expression])));
        }
    }
