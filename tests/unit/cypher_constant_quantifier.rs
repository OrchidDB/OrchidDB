use super::*;
use crate::ir::expr::BinaryOp as Op;

#[test]
fn folding_arithmetic_preserves_overflow_and_invalid_operand_errors() {
    let cases = [
        (Op::Add, false, Value::Int(i64::MAX), Value::Int(1), false),
        (Op::Sub, false, Value::Int(i64::MIN), Value::Int(1), false),
        (Op::Mul, false, Value::Int(i64::MAX), Value::Int(2), false),
        (Op::Div, false, Value::Int(i64::MIN), Value::Int(-1), false),
        (Op::Div, true, Value::Int(i64::MIN), Value::Int(-1), false),
        (Op::Div, true, Value::Int(2), Value::Int(0), false),
        (Op::Add, false, Value::Float(f64::MAX), Value::Float(f64::MAX), false),
        (Op::Sub, false, Value::Float(-f64::MAX), Value::Float(f64::MAX), false),
        (Op::Mul, false, Value::Float(f64::MAX), Value::Int(2), false),
        (Op::Div, false, Value::Float(1.), Value::Float(-0.), false),
        (Op::Div, true, Value::Float(1.), Value::Float(0.), false),
        (Op::Add, false, Value::Float(f64::NAN), Value::Int(1), false),
        (Op::Add, false, Value::Int(1), Value::Float(f64::INFINITY), false),
        (Op::Add, false, Value::Null, Value::Int(1), false),
        (Op::Add, false, Value::Int(1), Value::Bool(true), false),
        (Op::Sub, false, Value::Float(5.5), Value::Int(2), true),
        (Op::Mul, false, Value::Int(2), Value::Float(5.5), true),
        (Op::Div, false, Value::Float(5.5), Value::Int(2), true),
        (Op::Div, true, Value::Int(5), Value::Float(2.), true),
    ];
    for (operation, modulo, left, right, expected) in cases {
        assert_eq!(safe_arithmetic(operation, modulo, &left, &right), expected,
            "{operation:?} modulo={modulo}: {left:?}, {right:?}");
    }
}

#[test]
fn constant_discovery_budget_cannot_be_reused_after_exhaustion() {
    let scope = Row::new();
    let expression = IrExpr::List(vec![IrExpr::Lit(Lit::Int(1)), IrExpr::List(vec![IrExpr::Lit(Lit::Int(2))])]);
    let mut remaining = 3;
    assert!(!constant_ir(&expression, &scope, &mut remaining));
    assert_eq!(remaining, 0);
    assert!(!constant_ir(&IrExpr::Lit(Lit::Int(1)), &scope, &mut remaining));
    assert_eq!(remaining, 0);
    let mut remaining = 4;
    assert!(constant_ir(&expression, &scope, &mut remaining));
    assert_eq!(remaining, 0);
}

#[test]
fn evaluator_budget_and_unbound_variables_decline_folding() {
    let mut lowerer = Lowerer::new();
    let mut evaluator = Evaluator { lowerer: &mut lowerer, graph: PropertyGraph::new(), remaining: 0 };
    assert!(evaluator.expression(&Expr::Literal(Literal::Bool(true)), &Row::new()).is_none());
    evaluator.remaining = 10;
    assert!(evaluator.expression(&Expr::Variable("missing".into()), &Row::new()).is_none());
    let expression = Expr::Map(vec![("a".into(), Expr::Variable("missing".into()))]);
    assert!(evaluator.expression(&expression, &Row::new()).is_none());
}

#[test]
fn closedness_checks_nested_bindings_and_function_arity_before_folding() {
    let mut lowerer = Lowerer::new();
    let locals = BTreeSet::from(["outer".into()]);
    let variable = Expr::Variable("outer".into());
    let call = |name: &str, distinct, args| Expr::Function { name: name.into(), distinct, args };
    for expression in [
        call("abs", false, vec![]),
        call("abs", false, vec![variable.clone(), variable.clone()]),
        call("abs", true, vec![variable.clone()]),
        call("mod", false, vec![variable.clone()]),
        call("rand", false, vec![]),
        Expr::Variable("unknown".into()),
    ] {
        assert!(!closed(&expression, &locals, &mut lowerer), "{expression:?}");
    }
    for expression in [
        call("ABS", false, vec![variable.clone()]),
        call("MOD", false, vec![variable.clone(), variable.clone()]),
        Expr::Map(vec![("x".into(), variable)]),
    ] {
        assert!(closed(&expression, &locals, &mut lowerer), "{expression:?}");
    }
}

#[test]
fn scalar_abs_folding_preserves_null_and_rejects_invalid_values() {
    let call = Expr::Function { name: "abs".into(), distinct: false,
        args: vec![Expr::Variable("input".into())] };
    let mut lowerer = Lowerer::new();
    for (input, expected) in [
        (Value::Int(-7), Some(Value::Long(7))),
        (Value::Float(-1.5), Some(Value::Float(1.5))),
        (Value::Null, Some(Value::Null)),
        (Value::Int(i64::MIN), None),
        (Value::Float(f64::INFINITY), None),
        (Value::Bool(true), None),
    ] {
        let mut scope = Row::new();
        scope.bindings.insert("input".into(), input);
        let mut evaluator = Evaluator { lowerer: &mut lowerer, graph: PropertyGraph::new(), remaining: 10 };
        assert_eq!(evaluator.expression(&call, &scope), expected);
    }
}

fn project(input: Node, mode: ProjectMode, items: Vec<(&str, IrExpr)>) -> Node {
    Node::GraphProject {
        mode,
        items: items.into_iter().map(|(alias, expr)| crate::ir::plan::ProjectionItem { alias: alias.into(), expr }).collect(),
        error_policy: crate::ir::plan::ProjectErrorPolicy::PropagateError,
        input: Box::new(input),
    }
}

#[test]
fn alias_discovery_observes_original_scope_and_invalidates_rebound_constants() {
    let initial = project(Node::GraphOneRow, ProjectMode::ReplaceScope,
        vec![("x", IrExpr::Lit(Lit::Int(1))), ("keep", IrExpr::Lit(Lit::Bool(true)))]);
    let changed = project(initial.clone(), ProjectMode::PreserveVisible,
        vec![("x", IrExpr::Binding("unknown".into())), ("original", IrExpr::Binding("x".into()))]);
    let graph = PropertyGraph::new();
    let constants = input_constants(&changed, &graph, &mut 100).unwrap();
    assert!(!constants.bindings.contains_key("x"));
    assert_eq!(constants.bindings.get("original"), Some(&Value::Int(1)));
    assert_eq!(constants.bindings.get("keep"), Some(&Value::Bool(true)));
    let replaced = project(initial.clone(), ProjectMode::ReplaceScope,
        vec![("renamed", IrExpr::Binding("x".into()))]);
    let constants = input_constants(&replaced, &graph, &mut 100).unwrap();
    assert_eq!(constants.bindings.len(), 1);
    assert_eq!(constants.bindings.get("renamed"), Some(&Value::Int(1)));
    let unwind = Node::GraphUnwind { input_expr: IrExpr::List(vec![]), bind: "x".into(), outer: false, input: Box::new(initial.clone()) };
    let quantified = Node::GraphQuantifier {
        kind: crate::ir::plan::QuantifierKind::All, item_binding: "item".into(),
        input_expr: IrExpr::List(vec![]), predicate: IrExpr::Lit(Lit::Bool(true)),
        output: "x".into(), input: Box::new(initial),
    };
    for node in [unwind, quantified] {
        let constants = input_constants(&node, &graph, &mut 100).unwrap();
        assert!(!constants.bindings.contains_key("x"));
        assert_eq!(constants.bindings.get("keep"), Some(&Value::Bool(true)));
    }
    assert!(input_constants(&changed, &graph, &mut 0).is_none());
}
