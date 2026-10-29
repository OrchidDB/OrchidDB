//! Once-per-query SQL slicing for bounds proven valid without evaluating them.
//!
//! SQL audit [L21](../../../tools/diagnostics/sql-lowering-audit.md#l21-sorting-and-slicing): Bounds belong to query or invocation scope, not each result row.
//! Validation must remain observable even when the input produces no rows.
use super::*;

fn bounded_float(expr: &IrExpr) -> Option<(Expr, f64, f64)> {
    match expr {
        IrExpr::Call { name, args }
            if matches!(name.as_str(), "rand" | "random") && args.is_empty() =>
        {
            Some((df_math::random(), 0.0, 1.0))
        }
        IrExpr::Lit(Lit::Int(n)) if (*n as f64) as i128 == *n as i128 => {
            Some((lit(*n), *n as f64, *n as f64))
        }
        IrExpr::Lit(Lit::Float(n)) if n.is_finite() => Some((lit(*n), *n, *n)),
        IrExpr::Binary { op, lhs, rhs }
            if matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul) =>
        {
            // Do not replace integer-only arithmetic (with its overflow
            // behavior) by floating arithmetic. Every accepted computed subtree
            // therefore contains a floating leaf or random().
            if matches!(lhs.as_ref(), IrExpr::Lit(Lit::Int(_)))
                && matches!(rhs.as_ref(), IrExpr::Lit(Lit::Int(_)))
            {
                return None;
            }
            let (a, alo, ahi) = bounded_float(lhs)?;
            let (b, blo, bhi) = bounded_float(rhs)?;
            let (lo, hi) = match op {
                BinaryOp::Add => (alo + blo, ahi + bhi),
                BinaryOp::Sub => (alo - bhi, ahi - blo),
                _ => {
                    let xs = [alo * blo, alo * bhi, ahi * blo, ahi * bhi];
                    (
                        xs.into_iter().fold(f64::INFINITY, f64::min),
                        xs.into_iter().fold(f64::NEG_INFINITY, f64::max),
                    )
                }
            };
            (lo.is_finite() && hi.is_finite()).then(|| (binary(a, *op, b), lo, hi))
        }
        _ => None,
    }
}
pub(super) fn bound(expr: &IrExpr) -> Option<Expr> {
    let IrExpr::Call { name, args } = expr else {
        return None;
    };
    if name != "cypher_convert.tointeger" || args.len() != 1 {
        return None;
    }
    if let IrExpr::Lit(Lit::Int(n)) = &args[0] {
        return (*n >= 0).then(|| lit(*n));
    }
    let (value, lo, hi) = bounded_float(&args[0])?;
    // Bounds outside this range retain native validation and error timing.
    if lo < 0.0 || hi >= 9223372036854775808.0 {
        return None;
    }
    Some(Expr::Cast(Cast::new(
        Box::new(df_math::trunc(vec![value])),
        DataType::Int64,
    )))
}
