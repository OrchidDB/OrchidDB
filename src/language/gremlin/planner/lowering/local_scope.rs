//! `LocalScoped(<inner>)` — Gremlin's `Scope.local` family. The wrapped
//! step operates against the *current list traverser's elements* rather
//! than against the global row stream. Common shapes:
//!
//! - `tail(local, n)` — last n elements of the list traverser.
//! - `order(local).by(...)` — sort the list traverser's elements.
//! - `sum/min/max/mean(local)` — reduce the list traverser to a scalar.
//! - `<string-op>(local)` — apply a string op to each element.
//!
//! All of these read/write the current value as a `List`. The lowering
//! emits an `IrExpr::Call` against a runtime helper that accepts the
//! current value (a list) plus per-step parameters; the helper does the
//! per-element work and returns the rewritten list/scalar.
//!
//! Steps we don't yet have a runtime helper for surface a clean
//! `Unsupported` so the failure dump shows exactly what's missing.

use super::context::CURRENT;
use super::reduce::agg_kind;
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::Node;
use crate::language::gremlin::ast::{
    AggKind as AstAggKind, CastTarget, NumericCast, Step, StringOp as AstStringOp,
};
use crate::language::gremlin::planner::error::{GremlinPlanError, GremlinPlanResult};

pub(super) fn lower_local_scoped(input: Node, inner: &Step) -> GremlinPlanResult<Node> {
    let helper_call: IrExpr = match inner {
        Step::Tail(n) => call("local_tail", vec![cur(), int(*n as i64)]),
        Step::Limit(n) | Step::Sample(n) => call("local_limit", vec![cur(), int(*n as i64)]),
        Step::Skip(n) => call("local_skip", vec![cur(), int(*n as i64)]),
        Step::Range { low, high } => call(
            "local_range",
            vec![cur(), int(*low as i64), int(*high as i64)],
        ),
        Step::Order => call("local_order", vec![cur()]),
        Step::Dedup => call("local_dedup", vec![cur()]),
        Step::Count => call("local_count", vec![cur()]),
        Step::CastScalar(target) => call(local_cast_helper(*target), vec![cur()]),
        Step::Aggregate(kind) => {
            let helper = match agg_kind(*kind)? {
                crate::ir::expr::AggKind::Sum => "local_sum",
                crate::ir::expr::AggKind::Min => "local_min",
                crate::ir::expr::AggKind::Max => "local_max",
                crate::ir::expr::AggKind::Avg => "local_mean",
                _ => {
                    return Err(GremlinPlanError::Unsupported(format!(
                        "LocalScoped({inner:?}) aggregate is not yet lowered"
                    )));
                }
            };
            call(helper, vec![cur()])
        }
        Step::StringOp(op) => match op {
            AstStringOp::ToLower => call("local_lcase", vec![cur()]),
            AstStringOp::ToUpper => call("local_ucase", vec![cur()]),
            AstStringOp::Length => call("local_length", vec![cur()]),
            AstStringOp::Trim => call("local_trim", vec![cur()]),
            AstStringOp::LTrim => call("local_ltrim", vec![cur()]),
            AstStringOp::RTrim => call("local_rtrim", vec![cur()]),
            AstStringOp::Reverse => call("local_reverse_strings", vec![cur()]),
            AstStringOp::Substring { start, end } => {
                let mut args = vec![cur(), int(*start)];
                if let Some(e) = end {
                    args.push(int(*e));
                }
                call("local_substring", args)
            }
            AstStringOp::Replace { old, new } => call(
                "local_replace",
                vec![
                    cur(),
                    IrExpr::Lit(Lit::String(old.clone())),
                    IrExpr::Lit(Lit::String(new.clone())),
                ],
            ),
            AstStringOp::Split(delim) => call(
                "local_split",
                vec![
                    cur(),
                    delim
                        .clone()
                        .map(IrExpr::lit_str)
                        .unwrap_or(IrExpr::Lit(Lit::Null)),
                ],
            ),
            AstStringOp::Concat(suffix) => call(
                "local_concat",
                vec![cur(), IrExpr::Lit(Lit::String(suffix.clone()))],
            ),
            AstStringOp::Conjoin(delim) => call(
                "local_conjoin",
                vec![cur(), IrExpr::Lit(Lit::String(delim.clone()))],
            ),
            AstStringOp::ConcatTraversal(_) => {
                return Err(GremlinPlanError::Unsupported(
                    "LocalScoped(StringOp(ConcatTraversal)) is not yet lowered".to_string(),
                ));
            }
        },
        other => {
            return Err(GremlinPlanError::Unsupported(format!(
                "LocalScoped({other:?}) is not yet lowered"
            )));
        }
    };
    Ok(Node::GraphCurrentProject {
        expr: helper_call,
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    })
}

fn cur() -> IrExpr {
    IrExpr::Binding(CURRENT.into())
}

fn int(n: i64) -> IrExpr {
    IrExpr::Lit(Lit::Int(n))
}

fn call(name: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call {
        name: name.to_string(),
        args,
    }
}

fn local_cast_helper(target: CastTarget) -> &'static str {
    match target {
        CastTarget::Number => "local_cast_number",
        CastTarget::Numeric(num) => match num {
            NumericCast::Byte => "local_cast_byte",
            NumericCast::Short => "local_cast_short",
            NumericCast::Int => "local_cast_int",
            NumericCast::Long => "local_cast_long",
            NumericCast::BigInt => "local_cast_bigint",
            NumericCast::Float => "local_cast_float",
            NumericCast::Double => "local_cast_double",
            NumericCast::BigDecimal => "local_cast_bigdecimal",
        },
        CastTarget::String => "local_cast_string",
        CastTarget::Bool => "local_cast_bool",
        CastTarget::Date => "local_cast_date",
    }
}

// Required so the `agg_kind` helper isn't reported unused when only one
// branch references it.
#[allow(dead_code)]
fn _force_agg_kind_visibility(k: AstAggKind) -> Option<crate::ir::expr::AggKind> {
    agg_kind(k).ok()
}
