//! Lossless homogeneous collection literals and cardinality-only quantifiers.
//!
//! These helpers deliberately do not establish list comparison semantics.
use super::*;

/// Materialize recursive empty/all-null lists using their concrete siblings'
/// element types. Reject mixed numeric and scalar/list values without coercion.
pub(super) fn literal(value: &Value) -> RelResult<Expr> {
    let kind = collections::value_arrow_type(value).ok_or_else(|| {
        RelError::Unsupported("heterogeneous Cypher constant requires runtime values".into())
    })?;
    Ok(lit(collections::value_scalar(value, &kind)?))
}

/// Only syntax whose evaluation cannot invoke functions, read a binding or
/// throw is eligible. Knowing list length alone must not erase an effect.
fn pure_literal(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Lit(_) => true,
        IrExpr::List(items) => items.iter().all(pure_literal),
        // The frontend represents a negative float literal as 0 - value.
        // This exact finite operation cannot fail; do not broaden to general
        // arithmetic (integer overflow, division and function effects matter).
        IrExpr::Binary { op: BinaryOp::Sub, lhs, rhs } => {
            matches!(lhs.as_ref(), IrExpr::Lit(Lit::Int(0)))
                && matches!(rhs.as_ref(), IrExpr::Lit(Lit::Float(value)) if value.is_finite())
        },
        IrExpr::Call { name, args } if matches!(name.as_str(), "make_map" | "map") => {
            args.len() % 2 == 0 && args.chunks_exact(2).all(|pair| {
                matches!(pair[0], IrExpr::Lit(Lit::String(_))) && pure_literal(&pair[1])
            })
        },
        _ => false,
    }
}

/// Evaluate a literal-predicate quantifier without materializing irrelevant
/// list payloads (which can have genuinely heterogeneous types).
pub(super) fn literal_quantifier(
    kind: QuantifierKind,
    input: &IrExpr,
    predicate: &IrExpr,
) -> Option<Expr> {
    if !crate::ir::diagnostics::constant_folding_enabled() { return None; }
    let IrExpr::List(items) = input else { return None; };
    if !items.iter().all(pure_literal) { return None; }
    let truth = match predicate {
        IrExpr::Lit(Lit::Bool(value)) => Some(*value),
        IrExpr::Lit(Lit::Null) => None,
        _ => return None,
    };
    let result = if items.is_empty() {
        Some(matches!(kind, QuantifierKind::All | QuantifierKind::None))
    } else {
        match (kind, truth) {
            (_, None) => None,
            (QuantifierKind::All | QuantifierKind::Any, Some(value)) => Some(value),
            (QuantifierKind::None, Some(value)) => Some(!value),
            (QuantifierKind::Single, Some(value)) => Some(value && items.len() == 1),
        }
    };
    Some(lit(ScalarValue::Boolean(result)))
}

/// Reuse native quantifier semantics for closed immutable map/list constants.
/// The only nonliteral predicate allowed is direct lookup on the local item;
/// failures decline folding so runtime error timing is not moved earlier.
pub(super) fn closed_quantifier(
    kind: QuantifierKind,
    item: &str,
    input: &IrExpr,
    predicate: &IrExpr,
    graph: &PropertyGraph,
) -> Option<Expr> {
    if !crate::ir::diagnostics::constant_folding_enabled() { return None; }
    if !pure_literal(input) || !matches!(input, IrExpr::List(_)) { return None; }
    fn local_predicate(expr: &IrExpr, item: &str) -> bool {
        match expr {
            IrExpr::Lit(_) => true,
            IrExpr::Binary { op: BinaryOp::Sub, .. } if pure_literal(expr) => true,
            IrExpr::Binding(binding) | IrExpr::Property { binding, .. } => binding == item,
            IrExpr::IsNull(inner) | IrExpr::IsNotNull(inner) | IrExpr::Not(inner) => local_predicate(inner, item),
            IrExpr::Call { name, args } if name == "size" => {
                args.len() == 1 && local_predicate(&args[0], item)
            }
            IrExpr::Call { name, args } if name == "cypher_subscript" => matches!(args.as_slice(),
                [target, IrExpr::Lit(Lit::String(_))] if local_predicate(target, item)),
            IrExpr::Call { name, args } if matches!(name.as_str(), "cypher_eq" | "cypher_neq" | "cypher_lt" | "cypher_lte" | "cypher_gt" | "cypher_gte") => args.iter().all(|arg| local_predicate(arg, item)),
            IrExpr::Binary { op, lhs, rhs } if matches!(op, BinaryOp::Eq | BinaryOp::Neq | BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte | BinaryOp::And | BinaryOp::Or) => local_predicate(lhs, item) && local_predicate(rhs, item),
            _ => false,
        }
    }
    if !local_predicate(predicate, item) {
        return None;
    }
    let output = "__w_folded_quantifier";
    let rows = crate::ir::runtime::ops::quantifier::quantifier_op(
        kind, item, input, predicate, output, vec![KernelRow::new()], graph,
    ).ok()?;
    match rows.first()?.bindings.get(output)? {
        Value::Bool(value) => Some(lit(*value)),
        Value::Null => Some(lit(ScalarValue::Boolean(None))),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/extracted/ir__rel__cypher_lists__tests.rs"]
mod tests;
