//! Prove bounded integer arithmetic safe without evaluating quantifier results.
//! Unknown/rebound inputs retain the general materialization path.
use super::{BinaryOp, Expr, IrExpr, Lit, Literal, Node, ProjectMode, UnaryOp};
use std::collections::BTreeMap;

#[derive(Clone)]
enum Domain {
    Integer(i128, i128),
    String,
    Null,
    List(Box<Domain>),
}
type Scope = BTreeMap<String, Domain>;

fn integer(lo: i128, hi: i128) -> Option<Domain> {
    (lo >= i64::MIN as i128 && hi <= i64::MAX as i128).then_some(Domain::Integer(lo, hi))
}
fn merge(left: Domain, right: Domain) -> Option<Domain> {
    match (left, right) {
        (Domain::Null, domain) | (domain, Domain::Null) => Some(domain),
        (Domain::String, Domain::String) => Some(Domain::String),
        (Domain::Integer(a, b), Domain::Integer(c, d)) => integer(a.min(c), b.max(d)),
        (Domain::List(a), Domain::List(b)) => Some(Domain::List(Box::new(merge(*a, *b)?))),
        _ => None,
    }
}
fn list(items: impl Iterator<Item = Option<Domain>>) -> Option<Domain> {
    let mut items = items;
    let mut domain = items.next()??;
    for item in items {
        domain = merge(domain, item?)?;
    }
    Some(Domain::List(Box::new(domain)))
}
fn ir_domain(expr: &IrExpr, scope: &Scope) -> Option<Domain> {
    match expr {
        IrExpr::Lit(Lit::String(_)) => Some(Domain::String),
        IrExpr::Lit(Lit::Null) => Some(Domain::Null),
        IrExpr::Lit(Lit::Int(value)) => integer(*value as i128, *value as i128),
        IrExpr::Binding(name) => scope.get(name).cloned(),
        IrExpr::List(items) => list(items.iter().map(|item| ir_domain(item, scope))),
        _ => None,
    }
}
fn input_scope(node: &Node) -> Scope {
    match node {
        Node::GraphProject {
            input, mode, items, ..
        } => {
            let original = input_scope(input);
            let mut result = if *mode == ProjectMode::PreserveVisible {
                original.clone()
            } else {
                Scope::new()
            };
            for item in items {
                result.remove(&item.alias);
                if let Some(domain) = ir_domain(&item.expr, &original) {
                    result.insert(item.alias.clone(), domain);
                }
            }
            result
        }
        Node::GraphFilter { input, .. }
        | Node::GraphSort { input, .. }
        | Node::GraphSlice { input, .. }
        | Node::GraphSliceExpr { input, .. }
        | Node::GraphDistinct { input, .. } => input_scope(input),
        Node::GraphUnwind { input, bind, .. } => {
            let mut scope = input_scope(input);
            scope.remove(bind);
            scope
        }
        Node::GraphQuantifier { input, output, .. } => {
            let mut scope = input_scope(input);
            scope.remove(output);
            scope
        }
        _ => Scope::new(),
    }
}
fn bounds(expr: &Expr, scope: &Scope) -> Option<(i128, i128)> {
    match domain(expr, scope)? {
        Domain::Integer(lo, hi) => Some((lo, hi)),
        _ => None,
    }
}
fn domain(expr: &Expr, scope: &Scope) -> Option<Domain> {
    match expr {
        Expr::Literal(Literal::String(_)) => Some(Domain::String),
        Expr::Literal(Literal::Null) => Some(Domain::Null),
        Expr::Literal(Literal::Integer(text)) => {
            let value = text.replace('_', "").parse::<i64>().ok()? as i128;
            integer(value, value)
        }
        Expr::Variable(name) => scope.get(name).cloned(),
        Expr::List(items) => list(items.iter().map(|item| domain(item, scope))),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => {
            let (lo, hi) = bounds(expr, scope)?;
            integer(-hi, -lo)
        }
        Expr::Binary { op, lhs, rhs } => {
            let (a, b) = bounds(lhs, scope)?;
            let (c, d) = bounds(rhs, scope)?;
            match op {
                BinaryOp::Add => integer(a + c, b + d),
                BinaryOp::Sub => integer(a - d, b - c),
                BinaryOp::Mul => {
                    let values = [a * c, a * d, b * c, b * d];
                    integer(*values.iter().min()?, *values.iter().max()?)
                }
                _ => None,
            }
        }
        Expr::Function {
            name,
            distinct: false,
            args,
        } if name.eq_ignore_ascii_case("abs") && args.len() == 1 => {
            let (lo, hi) = bounds(&args[0], scope)?;
            integer(
                if lo <= 0 && hi >= 0 {
                    0
                } else {
                    lo.abs().min(hi.abs())
                },
                lo.abs().max(hi.abs()),
            )
        }
        Expr::Function {
            name,
            distinct: false,
            args,
        } if name.eq_ignore_ascii_case("mod") && args.len() == 2 => {
            let (lo, hi) = bounds(&args[0], scope)?;
            let (divisor_lo, divisor_hi) = bounds(&args[1], scope)?;
            if divisor_lo <= 0 {
                return None;
            }
            integer(
                if lo >= 0 { 0 } else { -(divisor_hi - 1) },
                if hi <= 0 { 0 } else { divisor_hi - 1 },
            )
        }
        _ => None,
    }
}
fn collection(expr: &Expr) -> bool {
    match expr {
        Expr::Variable(_) | Expr::Literal(_) => true,
        Expr::List(items) => items.iter().all(collection),
        _ => false,
    }
}
fn predicate(expr: &Expr, scope: &Scope) -> bool {
    match expr {
        Expr::Variable(_) | Expr::Literal(_) => true,
        Expr::Quantifier {
            variable,
            collection: source,
            predicate: body,
            ..
        } => {
            if !collection(source) {
                return false;
            }
            let mut nested = scope.clone();
            nested.remove(variable);
            if let Some(Domain::List(element)) = domain(source, scope) {
                nested.insert(variable.clone(), *element);
            }
            predicate(body, &nested)
        }
        Expr::Unary {
            op: UnaryOp::Not,
            expr,
        }
        | Expr::IsNull(expr)
        | Expr::IsNotNull(expr) => predicate(expr, scope),
        Expr::Binary { op, lhs, rhs }
            if matches!(
                op,
                BinaryOp::Eq
                    | BinaryOp::Neq
                    | BinaryOp::Lt
                    | BinaryOp::Lte
                    | BinaryOp::Gt
                    | BinaryOp::Gte
                    | BinaryOp::And
                    | BinaryOp::Or
            ) =>
        {
            predicate(lhs, scope) && predicate(rhs, scope)
        }
        Expr::Function {
            name,
            distinct: false,
            args,
        } if name.eq_ignore_ascii_case("size") && args.len() == 1 => matches!(
            domain(&args[0], scope),
            Some(Domain::String | Domain::List(_) | Domain::Null)
        ),
        _ => bounds(expr, scope).is_some(),
    }
}

pub(super) fn direct(input: &Node, variable: &str, source: &Expr, body: &Expr) -> bool {
    // Prove the expressions actually consumed by the quantifier. Unrelated
    // volatile output columns stay in the upstream projection; they do not
    // require occurrence joins or windows inside this scalar collection body.
    let mut scope = input_scope(input);
    let element = domain(source, &scope);
    scope.remove(variable);
    if let Some(Domain::List(element)) = element {
        scope.insert(variable.to_owned(), *element);
    }
    predicate(body, &scope)
}

#[cfg(test)]
#[path = "../../../../../../tests/unit/extracted/language__cypher__planner__lowering__project__quantifier_domains__tests.rs"]
mod tests;
