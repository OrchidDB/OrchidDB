
    use super::*;

    #[test]
    fn nested_null_types_unify_without_numeric_coercion() {
        let values = Value::List(vec![
            Value::List(vec![]),
            Value::List(vec![Value::Null]),
            Value::List(vec![Value::Long(i64::MAX)]),
            Value::Null,
        ]);
        let kind = collections::value_arrow_type(&values).unwrap();
        let scalar = collections::value_scalar(&values, &kind).unwrap();
        assert_eq!(scalar.data_type(), kind);
        assert!(literal(&values).is_ok());
        let temporal = crate::ir::temporal::parse("date", "2024-01-02").unwrap();
        let temporal_list = Value::List(vec![Value::List(vec![]), Value::List(vec![Value::Temporal(temporal)])]);
        let ty = collections::value_arrow_type(&temporal_list).unwrap();
        assert!(crate::ir::temporal::transport::contains_type(&ty));
        assert!(literal(&temporal_list).is_ok());
        assert!(literal(&Value::List(vec![Value::Int(1), Value::Float(1.0)])).is_err());
        assert!(literal(&Value::List(vec![Value::Int(1), Value::List(vec![])])).is_err());
        assert!(collections::uniform_list_type([&Value::List(vec![])].into_iter()).is_some());
    }

    #[test]
    fn quantifier_does_not_erase_function_evaluation() {
        let call = IrExpr::Call { name: "rand".into(), args: vec![] };
        assert!(literal_quantifier(QuantifierKind::All,
            &IrExpr::List(vec![call]), &IrExpr::Lit(Lit::Bool(false))).is_none());
        for size in 0..3 {
            let list = IrExpr::List(vec![IrExpr::Lit(Lit::Int(1)); size]);
            for kind in [QuantifierKind::All, QuantifierKind::Any, QuantifierKind::None, QuantifierKind::Single] {
                for predicate in [Lit::Null, Lit::Bool(false), Lit::Bool(true)] {
                    assert!(literal_quantifier(kind, &list, &IrExpr::Lit(predicate)).is_some());
                }
            }
        }
    }
    #[test]
    fn negative_float_literals_are_pure_but_arithmetic_is_not() {
        let subtract = |lhs, rhs| IrExpr::Binary {
            op: BinaryOp::Sub, lhs: Box::new(lhs), rhs: Box::new(rhs),
        };
        for value in [0.0, -0.0, 2.1, f64::MAX] {
            let expr = subtract(IrExpr::Lit(Lit::Int(0)), IrExpr::Lit(Lit::Float(value)));
            assert!(pure_literal(&expr));
            for kind in [QuantifierKind::All, QuantifierKind::Any, QuantifierKind::None, QuantifierKind::Single] {
                assert!(literal_quantifier(kind, &IrExpr::List(vec![expr.clone()]),
                    &IrExpr::Lit(Lit::Bool(true))).is_some());
            }
        }
        for expr in [
            subtract(IrExpr::Lit(Lit::Int(0)), IrExpr::Lit(Lit::Int(i64::MIN))),
            subtract(IrExpr::Lit(Lit::Int(1)), IrExpr::Lit(Lit::Float(2.1))),
            subtract(IrExpr::Lit(Lit::Int(0)), IrExpr::Lit(Lit::Float(f64::INFINITY))),
            subtract(IrExpr::Lit(Lit::Int(0)), IrExpr::Call { name: "rand".into(), args: vec![] }),
            IrExpr::Binary { op: BinaryOp::Div, lhs: Box::new(IrExpr::Lit(Lit::Int(1))),
                rhs: Box::new(IrExpr::Lit(Lit::Int(0))) },
        ] {
            assert!(!pure_literal(&expr));
            assert!(literal_quantifier(QuantifierKind::All, &IrExpr::List(vec![expr]),
                &IrExpr::Lit(Lit::Bool(true))).is_none());
        }
    }

