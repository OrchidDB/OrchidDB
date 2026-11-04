
    use super::*;
    fn value(value: i64) -> Expr {
        Expr::Literal(Literal::Integer(value.to_string()))
    }
    fn op(op: BinaryOp, lhs: Expr, rhs: Expr) -> Expr {
        Expr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }
    #[test]
    fn bounds_check_every_intermediate_without_folding_the_result() {
        let scope = Scope::from([("x".into(), Domain::Integer(1, 9))]);
        let x = Expr::Variable("x".into());
        assert_eq!(
            bounds(&op(BinaryOp::Mul, x.clone(), x.clone()), &scope),
            Some((1, 81))
        );
        for expr in [
            op(BinaryOp::Add, value(i64::MAX), value(1)),
            op(BinaryOp::Sub, value(i64::MIN), value(1)),
            op(BinaryOp::Mul, value(i64::MAX), value(2)),
            Expr::Function {
                name: "abs".into(),
                distinct: false,
                args: vec![value(i64::MIN)],
            },
            op(
                BinaryOp::Sub,
                op(BinaryOp::Add, value(i64::MAX), value(1)),
                value(1),
            ),
        ] {
            assert!(bounds(&expr, &scope).is_none());
        }
        let modulo = |divisor| Expr::Function {
            name: "mod".into(),
            distinct: false,
            args: vec![x.clone(), divisor],
        };
        assert!(bounds(&modulo(x.clone()), &scope).is_some());
        assert!(bounds(&modulo(op(BinaryOp::Sub, x.clone(), value(5))), &scope).is_none());
    }
    #[test]
    fn nested_iterator_shadowing_does_not_reuse_outer_bounds() {
        let source = Expr::List(vec![value(1), value(9)]);
        let nested = |items| Expr::Quantifier {
            kind: super::super::QuantifierKind::All,
            variable: "x".into(),
            collection: Box::new(Expr::List(items)),
            predicate: Box::new(op(
                BinaryOp::Eq,
                Expr::Function {
                    name: "mod".into(),
                    distinct: false,
                    args: vec![value(1), Expr::Variable("x".into())],
                },
                value(0),
            )),
        };
        assert!(direct(
            &Node::GraphOneRow,
            "x",
            &source,
            &nested(vec![value(2)])
        ));
        assert!(!direct(
            &Node::GraphOneRow,
            "x",
            &source,
            &nested(vec![value(0)])
        ));
        assert!(!direct(&Node::GraphOneRow, "x", &source, &nested(vec![])));
    }

    #[test]
    fn proven_domains_survive_row_operators_but_not_rebinding() {
        use crate::ir::plan::{ProjectionItem, ProjectErrorPolicy, Slice, DistinctMode, DistinctBulk, QuantifierKind};
        let project = |input: Node, mode, name: &str, expr| Node::GraphProject {
            mode, items: vec![ProjectionItem { alias: name.into(), expr }],
            error_policy: ProjectErrorPolicy::PropagateError, input: input.boxed(),
        };
        let source = || project(Node::GraphOneRow, ProjectMode::ReplaceScope, "xs",
            IrExpr::List(vec![IrExpr::Lit(Lit::Int(-3)), IrExpr::Lit(Lit::Null), IrExpr::Lit(Lit::Int(4))]));
        let body = op(BinaryOp::Lt,
            op(BinaryOp::Mul, Expr::Variable("x".into()), Expr::Variable("x".into())), value(17));
        let safe = |input: &Node, name: &str| direct(input, "x", &Expr::Variable(name.into()), &body);
        assert!(safe(&source(), "xs"));
        for node in [
            Node::GraphFilter {condition: IrExpr::Lit(Lit::Bool(true)), input: source().boxed()},
            Node::GraphSort {keys: vec![], input: source().boxed()},
            Node::GraphSlice {slice: Slice::NONE, input: source().boxed()},
            Node::GraphSliceExpr {offset: None, fetch: Some(IrExpr::Lit(Lit::Int(1))), input: source().boxed()},
            Node::GraphDistinct {keys: vec!["xs".into()], mode: DistinctMode::Row, bulk: DistinctBulk::NotApplicable, input: source().boxed()},
            project(source(), ProjectMode::PreserveVisible, "other", IrExpr::Lit(Lit::String("s".into()))),
        ] {
            assert!(safe(&node, "xs"));
        }
        let renamed = project(source(), ProjectMode::ReplaceScope, "ys", IrExpr::Binding("xs".into()));
        assert!(safe(&renamed, "ys"));
        assert!(!safe(&renamed, "xs"));
        for node in [
            project(source(), ProjectMode::PreserveVisible, "xs", IrExpr::Binding("unknown".into())),
            Node::GraphUnwind {input_expr: IrExpr::List(vec![]), bind: "xs".into(), outer: false, input: source().boxed()},
            Node::GraphQuantifier {kind: QuantifierKind::All, item_binding: "item".into(), input_expr: IrExpr::List(vec![]), predicate: IrExpr::Lit(Lit::Bool(true)), output: "xs".into(), input: source().boxed()},
        ] {
            assert!(!safe(&node, "xs"));
        }
    }

    #[test]
    fn arithmetic_bounds_enclose_every_value_in_small_signed_domains() {
        let x = Expr::Variable("x".into());
        let y = Expr::Variable("y".into());
        for (a,b) in [(-4,-1),(-2,3),(0,0),(1,4)] {
            for (c,d) in [(-4,-1),(-2,3),(0,0),(1,4)] {
                let scope = Scope::from([("x".into(),Domain::Integer(a,b)),("y".into(),Domain::Integer(c,d))]);
                for operator in [BinaryOp::Add,BinaryOp::Sub,BinaryOp::Mul] {
                    let (lo,hi) = bounds(&op(operator, x.clone(), y.clone()), &scope).unwrap();
                    for left in a..=b {
                        for right in c..=d {
                            let actual = match operator {BinaryOp::Add => left+right, BinaryOp::Sub => left-right, _ => left*right};
                            assert!((lo..=hi).contains(&actual));
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn null_and_nested_collection_domains_do_not_invent_numeric_bounds() {
        let size = |expr| Expr::Function {name: "size".into(), distinct: false,args:vec![expr]};
        let item=Expr::Variable("x".into());
        for source in [
            Expr::List(vec![Expr::Literal(Literal::String("abc".into())),Expr::Literal(Literal::Null)]),
            Expr::List(vec![Expr::List(vec![value(1)]),Expr::List(vec![value(2),value(3)])]),
            Expr::List(vec![Expr::Literal(Literal::Null)]),
        ] {
            assert!(direct(&Node::GraphOneRow,"x",&source,&size(item.clone())));
            assert!(!direct(&Node::GraphOneRow,"x",&source,&op(BinaryOp::Add,item.clone(),value(1))));
        }
        for source in [Expr::List(vec![]),Expr::List(vec![value(1),Expr::Literal(Literal::String("s".into()))])] {
            assert!(!direct(&Node::GraphOneRow,"x",&source,&size(item.clone())));
        }
        let scope=Scope::new();
        for expression in [Expr::Literal(Literal::Integer("999999999999999999999999".into())),
            Expr::Literal(Literal::Bool(true)),op(BinaryOp::Div,value(1),value(2))] {
            assert!(bounds(&expression,&scope).is_none());
        }
        assert_eq!(bounds(&Expr::Literal(Literal::Integer("1_024".into())),&scope),Some((1024,1024)));
        let nested=Expr::Quantifier {
            kind:super::super::QuantifierKind::Any, variable:"x".into(),
            collection:Box::new(Expr::Function {name:"range".into(),distinct:false,args:vec![value(1),value(3)]}),
            predicate:Box::new(Expr::Literal(Literal::Bool(true))),
        };
        assert!(!predicate(&nested,&scope));
        for expression in [Expr::Unary {op:UnaryOp::Not,expr:Box::new(Expr::Literal(Literal::Bool(false)))},
            Expr::IsNull(Box::new(item.clone())),Expr::IsNotNull(Box::new(item))] {
            assert!(predicate(&expression,&scope));
        }
    }

    #[test]
    fn unary_absolute_and_modulo_bounds_enclose_signed_inputs() {
        let x=Expr::Variable("x".into());
        for (a,b) in [(-8,-2),(-3,7),(0,0),(2,9)] {
            let scope=Scope::from([("x".into(),Domain::Integer(a,b))]);
            let neg=Expr::Unary {op:UnaryOp::Neg,expr:Box::new(x.clone())};
            let abs=Expr::Function {name:"ABS".into(),distinct:false,args:vec![x.clone()]};
            assert_eq!(bounds(&neg,&scope),Some((-b,-a)));
            let (lo,hi)=bounds(&abs,&scope).unwrap();
            for n in a..=b { assert!((lo..=hi).contains(&n.abs())); }
            for divisor in 1..=5 {
                let modulo=Expr::Function {name:"mod".into(),distinct:false,args:vec![x.clone(),value(divisor)]};
                let (lo,hi)=bounds(&modulo,&scope).unwrap();
                for n in a..=b { assert!((lo..=hi).contains(&(n % divisor as i128))); }
            }
        }
        assert!(bounds(&Expr::Unary {op:UnaryOp::Neg,expr:Box::new(value(i64::MIN))},&Scope::new()).is_none());
    }

