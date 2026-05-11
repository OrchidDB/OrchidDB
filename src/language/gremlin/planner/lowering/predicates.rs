//! Lower a Gremlin `Predicate` into an `IrExpr` boolean condition.
//!
//! The `target` argument is the IR expression the predicate is applied
//! to (a property access, a binding, an id projection, …). The result
//! is always a boolean expression, and the predicate may compose
//! recursively via `And`/`Or`/`Not`.

use super::helpers::or_chain;
use super::literals::gvalue_to_expr;
use crate::ir::expr::{BinaryOp, IrExpr, StringOp};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::{CompareOp, GValue, Predicate, TextKind};

fn predicate_value_expr_with(
    value: &GValue,
    resolve_binding: &dyn Fn(&str) -> Option<IrExpr>,
) -> GremlinPlanResult<IrExpr> {
    match value {
        GValue::String(label) => {
            let binding_expr =
                resolve_binding(label).unwrap_or_else(|| IrExpr::Binding(label.clone()));
            Ok(IrExpr::Case {
                arms: vec![(IrExpr::IsBound(label.clone()), binding_expr)],
                otherwise: Some(Box::new(gvalue_to_expr(value)?)),
            })
        }
        _ => gvalue_to_expr(value),
    }
}

/// Flatten the value list of `P.within(...)` / `P.without(...)`. Gremlin
/// accepts either a varargs spread or a single list argument; the parser
/// preserves whichever shape the user wrote, so we normalize here. List
/// values are inlined recursively; null values are kept (callers map
/// them to `IS NULL` / `IS NOT NULL` checks via the literal pipeline).
fn flatten_predicate_values(values: &[GValue]) -> Vec<GValue> {
    let mut out = Vec::with_capacity(values.len());
    for value in values {
        match value {
            GValue::List(items) => out.extend(flatten_predicate_values(items)),
            other => out.push(other.clone()),
        }
    }
    out
}

pub(super) fn predicate_to_expr(
    target: IrExpr,
    predicate: &Predicate,
) -> GremlinPlanResult<IrExpr> {
    predicate_to_expr_with_bindings(target, predicate, &|_| None)
}

pub(super) fn predicate_to_expr_with_bindings(
    target: IrExpr,
    predicate: &Predicate,
    resolve_binding: &dyn Fn(&str) -> Option<IrExpr>,
) -> GremlinPlanResult<IrExpr> {
    match predicate {
        Predicate::Compare { op, value } => {
            if matches!(value, GValue::Null) {
                return Ok(match op {
                    CompareOp::Eq => IrExpr::IsNull(Box::new(target)),
                    CompareOp::Neq => IrExpr::IsNotNull(Box::new(target)),
                    CompareOp::Lte | CompareOp::Gte => IrExpr::IsNull(Box::new(target)),
                    CompareOp::Lt | CompareOp::Gt => IrExpr::lit_bool(false),
                });
            }
            let rhs = predicate_value_expr_with(value, resolve_binding)?;
            let bin = match op {
                CompareOp::Eq => BinaryOp::Eq,
                CompareOp::Neq => BinaryOp::Neq,
                CompareOp::Lt => BinaryOp::Lt,
                CompareOp::Lte => BinaryOp::Lte,
                CompareOp::Gt => BinaryOp::Gt,
                CompareOp::Gte => BinaryOp::Gte,
            };
            Ok(IrExpr::Binary {
                op: bin,
                lhs: Box::new(target),
                rhs: Box::new(rhs),
            })
        }
        Predicate::Within(values) => {
            let flattened = flatten_predicate_values(values);
            // `P.within()` with no candidates matches nothing.
            if flattened.is_empty() {
                return Ok(IrExpr::lit_bool(false));
            }
            let mut parts = Vec::with_capacity(flattened.len());
            for v in &flattened {
                if matches!(v, GValue::Null) {
                    parts.push(IrExpr::IsNull(Box::new(target.clone())));
                } else {
                    parts.push(IrExpr::Binary {
                        op: BinaryOp::Eq,
                        lhs: Box::new(target.clone()),
                        rhs: Box::new(predicate_value_expr_with(v, resolve_binding)?),
                    });
                }
            }
            Ok(or_chain(parts))
        }
        Predicate::Without(values) => {
            let flattened = flatten_predicate_values(values);
            // `P.without()` with no candidates matches everything.
            if flattened.is_empty() {
                return Ok(IrExpr::lit_bool(true));
            }
            let mut parts = Vec::with_capacity(flattened.len());
            for v in &flattened {
                if matches!(v, GValue::Null) {
                    parts.push(IrExpr::IsNotNull(Box::new(target.clone())));
                } else {
                    parts.push(IrExpr::Binary {
                        op: BinaryOp::Neq,
                        lhs: Box::new(target.clone()),
                        rhs: Box::new(predicate_value_expr_with(v, resolve_binding)?),
                    });
                }
            }
            Ok(IrExpr::and(parts))
        }
        Predicate::Range {
            lo,
            hi,
            inclusive_lo,
            inclusive_hi,
        } => {
            let lo_op = if *inclusive_lo {
                BinaryOp::Gte
            } else {
                BinaryOp::Gt
            };
            let hi_op = if *inclusive_hi {
                BinaryOp::Lte
            } else {
                BinaryOp::Lt
            };
            let lo_expr = IrExpr::Binary {
                op: lo_op,
                lhs: Box::new(target.clone()),
                rhs: Box::new(gvalue_to_expr(lo)?),
            };
            let hi_expr = IrExpr::Binary {
                op: hi_op,
                lhs: Box::new(target),
                rhs: Box::new(gvalue_to_expr(hi)?),
            };
            Ok(IrExpr::and(vec![lo_expr, hi_expr]))
        }
        Predicate::Outside { lo, hi } => {
            let lt = IrExpr::Binary {
                op: BinaryOp::Lt,
                lhs: Box::new(target.clone()),
                rhs: Box::new(gvalue_to_expr(lo)?),
            };
            let gt = IrExpr::Binary {
                op: BinaryOp::Gt,
                lhs: Box::new(target),
                rhs: Box::new(gvalue_to_expr(hi)?),
            };
            Ok(IrExpr::Binary {
                op: BinaryOp::Or,
                lhs: Box::new(lt),
                rhs: Box::new(gt),
            })
        }
        Predicate::TextLike { pattern, kind } => {
            let op = match kind {
                TextKind::Containing => StringOp::Contains,
                TextKind::StartingWith => StringOp::StartsWith,
                TextKind::EndingWith => StringOp::EndsWith,
            };
            Ok(IrExpr::StringPredicate {
                op,
                target: Box::new(target),
                pattern: Box::new(IrExpr::lit_str(pattern.clone())),
            })
        }
        Predicate::Regex(pattern) => Ok(regex_to_expr(target, pattern)),
        Predicate::TypeOf(name) => Ok(IrExpr::Call {
            name: "typeof_matches".into(),
            args: vec![target, IrExpr::lit_str(name.clone())],
        }),
        Predicate::And(a, b) => {
            let lhs = predicate_to_expr_with_bindings(target.clone(), a, resolve_binding)?;
            let rhs = predicate_to_expr_with_bindings(target, b, resolve_binding)?;
            Ok(IrExpr::and(vec![lhs, rhs]))
        }
        Predicate::Or(a, b) => {
            let lhs = predicate_to_expr_with_bindings(target.clone(), a, resolve_binding)?;
            let rhs = predicate_to_expr_with_bindings(target, b, resolve_binding)?;
            Ok(IrExpr::Binary {
                op: BinaryOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            })
        }
        Predicate::Not(inner) => {
            let inner = predicate_to_expr_with_bindings(target, inner, resolve_binding)?;
            Ok(IrExpr::Not(Box::new(inner)))
        }
    }
}

fn regex_to_expr(target: IrExpr, pattern: &str) -> IrExpr {
    let metas = [
        '.', '*', '+', '?', '|', '[', ']', '(', ')', '{', '}', '\\', '^', '$',
    ];
    if let Some(suffix) = pattern.strip_prefix('^') {
        if !suffix.chars().any(|c| metas.contains(&c)) {
            return IrExpr::StringPredicate {
                op: StringOp::StartsWith,
                target: Box::new(target),
                pattern: Box::new(IrExpr::lit_str(suffix.to_string())),
            };
        }
    }
    if let Some(prefix) = pattern.strip_suffix('$') {
        if !prefix.chars().any(|c| metas.contains(&c)) {
            return IrExpr::StringPredicate {
                op: StringOp::EndsWith,
                target: Box::new(target),
                pattern: Box::new(IrExpr::lit_str(prefix.to_string())),
            };
        }
    }
    let literal_chunks = pattern
        .split(".*")
        .filter(|part| !part.is_empty())
        .filter(|part| !part.chars().any(|c| metas.contains(&c)))
        .collect::<Vec<_>>();
    if !literal_chunks.is_empty() {
        return IrExpr::and(
            literal_chunks
                .into_iter()
                .map(|part| IrExpr::StringPredicate {
                    op: StringOp::Contains,
                    target: Box::new(target.clone()),
                    pattern: Box::new(IrExpr::lit_str(part.to_string())),
                })
                .collect(),
        );
    }
    if !pattern.chars().any(|c| metas.contains(&c)) {
        return IrExpr::StringPredicate {
            op: StringOp::Contains,
            target: Box::new(target),
            pattern: Box::new(IrExpr::lit_str(pattern.to_string())),
        };
    }
    IrExpr::Call {
        name: "regex_match".into(),
        args: vec![target, IrExpr::lit_str(pattern.to_string())],
    }
}
