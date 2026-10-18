//! Closed quantifiers can be evaluated before their AST becomes
//! relational operators. Reuse runtime scalar and quantifier rules;
//! Resolve only proven constant outer aliases; never speculate about function purity.
use super::{BinaryOp, Expr, IrExpr, Lit, Literal, Lowerer, Node, ProjectMode, QuantifierKind, UnaryOp};
use super::expression::{lower_cypher_binary_expr, lower_expr, lower_quantifier_kind};
use crate::ir::{catalog::PropertyGraph, runtime::{Row, eval}, value::Value};
use std::collections::BTreeSet;

fn closed(expr: &Expr, locals: &BTreeSet<String>, lowerer: &mut Lowerer) -> bool {
    match expr {
        // Check even literals in an empty quantifier's predicate: folding must
        // not hide invalid numeric literals that normally fail at planning.
        Expr::Literal(_) => lower_expr(lowerer, expr).is_ok(),
        Expr::Variable(name) => locals.contains(name),
        Expr::List(items) => items.iter().all(|expr| closed(expr, locals, lowerer)),
        Expr::Map(items) => items.iter().all(|(_, expr)| closed(expr, locals, lowerer)),
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => closed(expr, locals, lowerer),
        Expr::Binary { lhs, rhs, .. } => closed(lhs, locals, lowerer) && closed(rhs, locals, lowerer),
        Expr::Function { name, distinct: false, args } if name.eq_ignore_ascii_case("mod") && args.len() == 2 =>
            args.iter().all(|expr| closed(expr, locals, lowerer)),
        Expr::Function { name, distinct: false, args } if name.eq_ignore_ascii_case("abs") && args.len() == 1 =>
            args.iter().all(|expr| closed(expr, locals, lowerer)),
        Expr::Quantifier { variable, collection, predicate, .. } => {
            if !closed(collection, locals, lowerer) { return false; }
            let mut nested = locals.clone();
            nested.insert(variable.clone());
            closed(predicate, &nested, lowerer)
        }
        _ => false,
    }
}

/// Arithmetic is folded only inside the ordinary finite i64/f64 domain.
/// Shared evaluation may promote overflowing integers or soften division
/// errors in other language contexts; do not use that to erase Cypher errors.
fn safe_arithmetic(op: crate::ir::expr::BinaryOp, modulo: bool, left: &Value, right: &Value) -> bool {
    use crate::ir::expr::BinaryOp as Op;
    match (left,right) {
        (Value::Int(left),Value::Int(right)) => {
            if modulo { return left.checked_rem(*right).is_some(); }
            match op {
                Op::Add => left.checked_add(*right).is_some(),
                Op::Sub => left.checked_sub(*right).is_some(),
                Op::Mul => left.checked_mul(*right).is_some(),
                Op::Div => left.checked_div(*right).is_some(),
                _ => true,
            }
        }
        _ => {
            let numeric = |value: &Value| match value { Value::Int(value) => Some(*value as f64), Value::Float(value) if value.is_finite() => Some(*value), _ => None };
            let (Some(left),Some(right)) = (numeric(left),numeric(right)) else { return false; };
            if (modulo || op == Op::Div) && right == 0.0 { return false; }
            let result = if modulo { left % right } else { match op {
                Op::Add => left + right, Op::Sub => left - right, Op::Mul => left * right, Op::Div => left / right, _ => return true,
            }};
            result.is_finite()
        }
    }
}

struct Evaluator<'a> {
    lowerer: &'a mut Lowerer,
    graph: PropertyGraph,
    remaining: usize,
}
impl Evaluator<'_> {
    fn expression(&mut self, expr: &Expr, scope: &Row) -> Option<Value> {
        self.remaining = self.remaining.checked_sub(1)?;
        match expr {
            Expr::Literal(_) => eval(&lower_expr(self.lowerer, expr).ok()?, scope, &self.graph).ok(),
            Expr::Variable(name) => scope.bindings.get(name).cloned(),
            Expr::List(items) => Some(Value::List(items.iter().map(|item| self.expression(item, scope)).collect::<Option<_>>()?)),
            Expr::Map(items) => Some(Value::Map(items.iter().map(|(name, item)| Some((name.clone(),self.expression(item,scope)?))).collect::<Option<_>>()?)),
            Expr::Unary { expr: inner, .. } | Expr::IsNull(inner) | Expr::IsNotNull(inner) => {
                let value = self.expression(inner, scope)?;
                if matches!(expr, Expr::Unary { op: UnaryOp::Neg, .. })
                    && !safe_arithmetic(crate::ir::expr::BinaryOp::Sub,false,&Value::Int(0),&value) { return None; }
                let binding = Box::new(IrExpr::Binding("value".into()));
                let operation = match expr {
                    Expr::Unary { op: UnaryOp::Not, .. } => IrExpr::Not(binding),
                    Expr::Unary { op: UnaryOp::Neg, .. } => IrExpr::Binary {
                        op: crate::ir::expr::BinaryOp::Sub, lhs: Box::new(IrExpr::Lit(Lit::Int(0))), rhs: binding,
                    },
                    Expr::IsNull(_) => IrExpr::IsNull(binding),
                    Expr::IsNotNull(_) => IrExpr::IsNotNull(binding),
                    _ => unreachable!(),
                };
                let mut row = Row::new();
                row.bindings.insert("value".into(),value);
                eval(&operation,&row,&self.graph).ok()
            }
            Expr::Binary { op, lhs, rhs } => {
                let lhs = self.expression(lhs, scope)?;
                let rhs = self.expression(rhs, scope)?;
                if matches!(op,BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div)
                    && !safe_arithmetic(super::expression::lower_binary_op(*op),false,&lhs,&rhs) { return None; }
                let mut row = Row::new();
                row.bindings.insert("left".into(),lhs);
                row.bindings.insert("right".into(),rhs);
                eval(&lower_cypher_binary_expr(*op, IrExpr::Binding("left".into()), IrExpr::Binding("right".into())), &row, &self.graph).ok()
            }
            Expr::Function { name, distinct: false, args } if name.eq_ignore_ascii_case("mod") && args.len() == 2 => {
                let left = self.expression(&args[0], scope)?;
                let right = self.expression(&args[1], scope)?;
                if !safe_arithmetic(crate::ir::expr::BinaryOp::Div,true,&left,&right) { return None; }
                let mut row = Row::new();
                row.bindings.insert("left".into(),left);
                row.bindings.insert("right".into(),right);
                eval(&IrExpr::Call { name: "mod".into(), args: vec![IrExpr::Binding("left".into()),IrExpr::Binding("right".into())] }, &row, &self.graph).ok()
            }
            Expr::Function { name, distinct: false, args } if name.eq_ignore_ascii_case("abs") && args.len() == 1 => {
                let value = self.expression(&args[0], scope)?;
                // Do not invoke the native signed abs on MIN, or fold a
                // type/arity error into a constant predicate result.
                match &value {
                    Value::Int(value) if value.checked_abs().is_some() => {},
                    Value::Float(value) if value.is_finite() => {},
                    Value::Null => {},
                    _ => return None,
                }
                let mut row = Row::new();
                row.bindings.insert("value".into(), value);
                eval(&IrExpr::Call { name: "abs".into(), args: vec![IrExpr::Binding("value".into())] }, &row, &self.graph).ok()
            }
            Expr::Quantifier { kind, variable, collection, predicate } => {
                let source = self.expression(collection,scope)?;
                let predicates = match source {
                    Value::Null => Value::Null,
                    Value::List(items) => {
                        let mut predicates = Vec::with_capacity(items.len());
                        for item in items {
                            let mut nested = scope.clone();
                            nested.bindings.insert(variable.clone(),item);
                            let value = self.expression(predicate,&nested)?;
                            if !matches!(value,Value::Bool(_) | Value::Null) { return None; }
                            predicates.push(value);
                        }
                        Value::List(predicates)
                    }
                    _ => return None,
                };
                // Feeding evaluated truth values through the ordinary operator
                // preserves all/any/none/single's empty and three-valued rules.
                let mut row = Row::new();
                row.bindings.insert("predicates".into(),predicates);
                let rows = crate::ir::runtime::ops::quantifier::quantifier_op(
                    lower_quantifier_kind(*kind),"item",&IrExpr::Binding("predicates".into()),
                    &IrExpr::Binding("item".into()),"result",vec![row],&self.graph,
                ).ok()?;
                rows.first()?.bindings.get("result").cloned()
            }
            _ => None,
        }
    }
}

/// Discover values shared by every surviving row without executing the input
/// plan. In particular an UNWIND rebinding invalidates the old constant, and
/// every projection expression observes the same original input scope.
fn input_constants(input: &Node, graph: &PropertyGraph, remaining: &mut usize) -> Option<Row> {
    *remaining = remaining.checked_sub(1)?;
    match input {
        Node::GraphProject { mode, items, input, .. } => {
            let original = input_constants(input, graph, remaining)?;
            let mut output = if *mode == ProjectMode::PreserveVisible { original.clone() } else { Row::new() };
            for item in items {
                // Rebinding to a nonconstant expression must remove an older
                // proven value even in PreserveVisible mode.
                output.bindings.remove(&item.alias);
                if constant_ir(&item.expr, &original, remaining) {
                    if let Ok(value) = eval(&item.expr, &original, graph) {
                        output.bindings.insert(item.alias.clone(),value);
                    }
                }
            }
            Some(output)
        }
        Node::GraphFilter { input, .. } | Node::GraphSort { input, .. }
        | Node::GraphSlice { input, .. } | Node::GraphSliceExpr { input, .. }
        | Node::GraphDistinct { input, .. } => input_constants(input,graph,remaining),
        Node::GraphUnwind { input, bind, .. } => {
            let mut constants = input_constants(input,graph,remaining)?;
            constants.bindings.remove(bind);
            Some(constants)
        }
        Node::GraphQuantifier { input, output, .. } => {
            let mut constants = input_constants(input,graph,remaining)?;
            constants.bindings.remove(output);
            Some(constants)
        }
        _ => Some(Row::new()),
    }
}

fn constant_ir(expr: &IrExpr, scope: &Row, remaining: &mut usize) -> bool {
    let Some(next) = remaining.checked_sub(1) else { return false; };
    *remaining = next;
    match expr {
        IrExpr::Lit(_) => true,
        IrExpr::Binding(name) => scope.bindings.contains_key(name),
        IrExpr::List(items) => items.iter().all(|expr| constant_ir(expr,scope,remaining)),
        // Alias discovery needs only literal containers and renames. Keep
        // computed WITH bindings opaque rather than moving error validation.
        _ => false,
    }
}

pub(super) fn fold(lowerer: &mut Lowerer, input: &Node, kind: QuantifierKind, variable: &str, collection: &Expr, predicate: &Expr) -> Option<Expr> {
    if !crate::ir::diagnostics::constant_folding_enabled() { return None; }
    let expression = Expr::Quantifier {
        kind, variable: variable.to_owned(), collection: Box::new(collection.clone()), predicate: Box::new(predicate.clone()),
    };
    let graph = PropertyGraph::new();
    let mut remaining = 4096;
    let scope = input_constants(input,&graph,&mut remaining)?;
    if !closed(&expression,&scope.bindings.keys().cloned().collect(),lowerer) { return None; }
    let value = Evaluator { lowerer, graph, remaining }.expression(&expression,&scope)?;
    Some(Expr::Literal(match value { Value::Bool(value) => Literal::Bool(value), Value::Null => Literal::Null, _ => return None }))
}
