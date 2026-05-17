//! Filter steps: `has*`, `where*`, `is`, `all`, `any`, `none(P)`,
//! `simplePath`, `cyclicPath`, `discard`/`none()`, plus the small
//! `lower_quantifier_filter` helper used by all-of / any-of / none-of.

use super::context::{CURRENT, ChildTraversalKind, Lowerer, PATH, TraversalContext};
use std::iter::Peekable;

use super::helpers::{any_label, consume_by, element_token_filter, filter_by_ids, or_chain};
use super::literals::gvalue_to_expr;
use super::predicates::{predicate_to_expr, predicate_to_expr_with_bindings};
use super::sub_traversal::lower_child_traversal;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{ApplyKind, Node, PathFilterScope, QuantifierKind};
use crate::ir::policy::{OptionalMissing, PropertyMissing};
use crate::language::gremlin::ast::{BySpec, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::{GValue, Predicate};

pub(super) fn lower_has_label(input: Node, labels: &[String]) -> GremlinPlanResult<Node> {
    Ok(Node::GraphFilter {
        condition: any_label(CURRENT, labels),
        input: input.boxed(),
    })
}

pub(super) fn lower_has(input: Node, key: &str, predicate: &Predicate) -> GremlinPlanResult<Node> {
    let property = IrExpr::property(CURRENT, key.to_string(), PropertyMissing::DropUnproductive);
    let exists = IrExpr::IsNotNull(Box::new(IrExpr::property(
        CURRENT,
        key.to_string(),
        PropertyMissing::NullOnMissing,
    )));
    let predicate = predicate_to_expr(property, predicate)?;
    Ok(Node::GraphFilter {
        condition: IrExpr::and(vec![exists, predicate]),
        input: input.boxed(),
    })
}

pub(super) fn lower_has_not(input: Node, key: &str) -> GremlinPlanResult<Node> {
    Ok(Node::GraphFilter {
        condition: IrExpr::IsNull(Box::new(IrExpr::property(
            CURRENT,
            key.to_string(),
            PropertyMissing::NullOnMissing,
        ))),
        input: input.boxed(),
    })
}

/// `hasKey(k)` (and the single-arg `has(k)`) keeps rows whose current
/// value carries `k`: either an element with property `k`, or a
/// Property-stream row (the `{key, value}` map produced by
/// `properties()`) whose `"key"` field equals `k`. We OR the two
/// checks so the step works in both contexts.
pub(super) fn lower_has_key(input: Node, key: &str) -> GremlinPlanResult<Node> {
    Ok(Node::GraphFilter {
        condition: has_key_expr(key),
        input: input.boxed(),
    })
}

pub(super) fn lower_has_key_any(input: Node, keys: &[String]) -> GremlinPlanResult<Node> {
    let parts = keys.iter().map(|k| has_key_expr(k)).collect::<Vec<_>>();
    Ok(Node::GraphFilter {
        condition: or_chain(parts),
        input: input.boxed(),
    })
}

fn has_key_expr(key: &str) -> IrExpr {
    let element_match = IrExpr::IsNotNull(Box::new(IrExpr::property(
        CURRENT,
        key.to_string(),
        PropertyMissing::NullOnMissing,
    )));
    let property_map_match = IrExpr::Binary {
        op: crate::ir::expr::BinaryOp::Eq,
        lhs: Box::new(IrExpr::property(
            CURRENT,
            "key".to_string(),
            PropertyMissing::NullOnMissing,
        )),
        rhs: Box::new(IrExpr::lit_str(key.to_string())),
    };
    IrExpr::Binary {
        op: crate::ir::expr::BinaryOp::Or,
        lhs: Box::new(property_map_match),
        rhs: Box::new(element_match),
    }
}

pub(super) fn lower_has_id(input: Node, ids: &[GValue]) -> Node {
    // Flatten any list-shaped ids and drop nulls; if everything is
    // null/empty after flattening, the filter matches nothing.
    fn flatten(values: &[GValue], out: &mut Vec<GValue>) {
        for v in values {
            match v {
                GValue::List(items) => flatten(items, out),
                GValue::Null => {}
                other => out.push(other.clone()),
            }
        }
    }
    if ids.is_empty() {
        return input;
    }
    let mut flat = Vec::new();
    flatten(ids, &mut flat);
    if flat.is_empty() {
        // `hasId(null)` / `hasId(P.eq(null))` / `hasId([])` should
        // discard every row.
        return Node::GraphFilter {
            condition: IrExpr::lit_bool(false),
            input: input.boxed(),
        };
    }
    filter_by_ids(input, &flat)
}

pub(super) fn lower_has_id_predicate(
    input: Node,
    predicate: &Predicate,
) -> GremlinPlanResult<Node> {
    if let Predicate::Compare {
        op,
        value: GValue::String(token),
    } = predicate
    {
        if let Some(condition) = element_token_filter(CURRENT, token) {
            let condition = match op {
                crate::language::gremlin::semantics::CompareOp::Eq => condition,
                crate::language::gremlin::semantics::CompareOp::Neq => {
                    IrExpr::Not(Box::new(condition))
                }
                _ => predicate_to_expr(IrExpr::Id(CURRENT.into()), predicate)?,
            };
            return Ok(Node::GraphFilter {
                condition,
                input: input.boxed(),
            });
        }
    }
    Ok(Node::GraphFilter {
        condition: predicate_to_expr(IrExpr::Id(CURRENT.into()), predicate)?,
        input: input.boxed(),
    })
}

/// `hasValue(P)` — keep elements where ANY property matches the
/// predicate. A correct implementation enumerates the catalog row's
/// properties at runtime; for now we delegate to a runtime helper that
/// inspects the bound element's full property bag.
pub(super) fn lower_has_value(input: Node, predicate: &Predicate) -> GremlinPlanResult<Node> {
    use super::predicates::predicate_to_expr;
    // `hasValue(P)` is a Property-stream filter (after `.properties()`),
    // where current is a `{key, value}` map: apply the predicate to
    // `current["value"]`.
    let cond = predicate_to_expr(
        IrExpr::property(CURRENT, "value".to_string(), PropertyMissing::NullOnMissing),
        predicate,
    )?;
    Ok(Node::GraphFilter {
        condition: cond,
        input: input.boxed(),
    })
}

pub(super) fn lower_is<'a, I>(
    input: Node,
    predicate: &Predicate,
    steps: &mut Peekable<I>,
    lo: &Lowerer,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let by = consume_by(steps);
    let target = binding_by_expr(CURRENT, by.as_ref());
    let condition = predicate_to_expr_with_bindings(target, predicate, &|label| {
        if let Some(by) = by.as_ref() {
            return Some(binding_by_expr(label, Some(by)));
        }
        if let Some(seed) = lo.side_effect_seeds.get(label) {
            return gvalue_to_expr(seed).ok();
        }
        None
    })?;
    Ok(Node::GraphFilter {
        condition,
        input: input.boxed(),
    })
}

pub(super) fn lower_quantifier_filter(
    input: Node,
    kind: QuantifierKind,
    predicate: &Predicate,
    lo: &mut Lowerer,
) -> GremlinPlanResult<Node> {
    let item = lo.fresh("q_item");
    let output = lo.fresh("q_pass");
    let predicate = predicate_to_expr(IrExpr::Binding(item.clone()), predicate)?;
    Ok(Node::GraphFilter {
        condition: IrExpr::Binding(output.clone()),
        input: Node::GraphQuantifier {
            kind,
            item_binding: item,
            input_expr: IrExpr::Binding(CURRENT.into()),
            predicate,
            output,
            input: input.boxed(),
        }
        .boxed(),
    })
}

pub(super) fn lower_simple_path(input: Node) -> Node {
    Node::GraphPathFilter {
        condition: IrExpr::SimplePath(PATH.into()),
        scope: PathFilterScope::FinalPath,
        input: input.boxed(),
    }
}

pub(super) fn lower_cyclic_path(input: Node) -> Node {
    Node::GraphPathFilter {
        condition: IrExpr::Not(Box::new(IrExpr::SimplePath(PATH.into()))),
        scope: PathFilterScope::FinalPath,
        input: input.boxed(),
    }
}

pub(super) fn lower_discard_or_none(input: Node) -> Node {
    Node::GraphFilter {
        condition: IrExpr::lit_bool(false),
        input: input.boxed(),
    }
}

pub(super) fn lower_where_traversal(
    input: Node,
    sub: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    Ok(Node::GraphApply {
        kind: ApplyKind::Semi,
        correlation: vec![CURRENT.into()],
        outputs: Vec::new(),
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: lower_child_traversal(sub, lo, ctx, ChildTraversalKind::WherePredicate)?.boxed(),
    })
}

/// `where("a", P.eq("b"))` / `where("a", P.gt("b"))` — the predicate's
/// "value" side is the *name* of another binding rather than a literal.
/// Replace the literal references inside the predicate tree with
/// `IrExpr::Binding` lookups so the comparison runs against the
/// already-bound row.
pub(super) fn lower_where_string<'a, I>(
    input: Node,
    label: &str,
    predicate: &crate::language::gremlin::semantics::Predicate,
    steps: &mut Peekable<I>,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    use crate::language::gremlin::semantics::{CompareOp, GValue, Predicate};
    fn compare_op(op: CompareOp) -> crate::ir::expr::BinaryOp {
        match op {
            CompareOp::Eq => crate::ir::expr::BinaryOp::Eq,
            CompareOp::Neq => crate::ir::expr::BinaryOp::Neq,
            CompareOp::Lt => crate::ir::expr::BinaryOp::Lt,
            CompareOp::Lte => crate::ir::expr::BinaryOp::Lte,
            CompareOp::Gt => crate::ir::expr::BinaryOp::Gt,
            CompareOp::Gte => crate::ir::expr::BinaryOp::Gte,
        }
    }
    let mut bys = Vec::new();
    while let Some(by) = consume_by(steps) {
        bys.push(by);
    }

    fn compare_bindings(
        label: &str,
        lhs_by: Option<&BySpec>,
        op: CompareOp,
        other: &str,
    ) -> IrExpr {
        IrExpr::Binary {
            op: compare_op(op),
            lhs: Box::new(binding_by_expr(label, lhs_by)),
            rhs: Box::new(binding_by_expr(other, lhs_by)),
        }
    }
    fn rhs_by<'a>(bys: &'a [BySpec], rhs_idx: &mut usize) -> Option<&'a BySpec> {
        match bys.len() {
            0 => None,
            1 => bys.first(),
            _ => {
                let idx = (*rhs_idx).min(bys.len() - 1);
                *rhs_idx += 1;
                bys.get(idx)
            }
        }
    }
    fn rewrite(
        label: &str,
        lhs_by: Option<&BySpec>,
        bys: &[BySpec],
        rhs_idx: &mut usize,
        predicate: &Predicate,
    ) -> GremlinPlanResult<IrExpr> {
        match predicate {
            Predicate::Compare {
                op,
                value: GValue::String(other),
            } => Ok(IrExpr::Binary {
                op: compare_op(*op),
                lhs: Box::new(binding_by_expr(label, lhs_by)),
                rhs: Box::new(binding_by_expr(other, rhs_by(bys, rhs_idx))),
            }),
            Predicate::And(a, b) => Ok(IrExpr::and(vec![
                rewrite(label, lhs_by, bys, rhs_idx, a)?,
                rewrite(label, lhs_by, bys, rhs_idx, b)?,
            ])),
            Predicate::Or(a, b) => Ok(IrExpr::Binary {
                op: crate::ir::expr::BinaryOp::Or,
                lhs: Box::new(rewrite(label, lhs_by, bys, rhs_idx, a)?),
                rhs: Box::new(rewrite(label, lhs_by, bys, rhs_idx, b)?),
            }),
            Predicate::Not(inner) => Ok(IrExpr::Not(Box::new(rewrite(
                label, lhs_by, bys, rhs_idx, inner,
            )?))),
            _ => Ok(IrExpr::lit_bool(true)),
        }
    }
    // `binding(label) cmp binding(other)` — we replace both the lhs
    // *and* rhs of the predicate so the comparison fires per-row. The
    // first arg of `Compare` becomes `binding(label)` here.
    let lhs_by = bys.first();
    let mut rhs_idx = if bys.len() > 1 { 1 } else { 0 };
    let cond = match predicate {
        Predicate::Compare {
            op,
            value: GValue::String(other),
        } => {
            if bys.len() <= 1 {
                compare_bindings(label, lhs_by, *op, other)
            } else {
                IrExpr::Binary {
                    op: compare_op(*op),
                    lhs: Box::new(binding_by_expr(label, lhs_by)),
                    rhs: Box::new(binding_by_expr(other, rhs_by(&bys, &mut rhs_idx))),
                }
            }
        }
        _ => rewrite(label, lhs_by, &bys, &mut rhs_idx, predicate)?,
    };
    Ok(Node::GraphFilter {
        condition: cond,
        input: input.boxed(),
    })
}

fn binding_by_expr(binding: &str, by: Option<&BySpec>) -> IrExpr {
    match by.and_then(|spec| spec.key.as_deref()) {
        Some("id") => IrExpr::Id(binding.to_string()),
        Some("label") => IrExpr::Label(binding.to_string()),
        Some(key) => IrExpr::property(binding, key.to_string(), PropertyMissing::NullOnMissing),
        None => IrExpr::Binding(binding.to_string()),
    }
}

pub(super) fn lower_not_traversal(
    input: Node,
    sub: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    Ok(Node::GraphApply {
        kind: ApplyKind::Anti,
        correlation: vec![CURRENT.into()],
        outputs: Vec::new(),
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: lower_child_traversal(sub, lo, ctx, ChildTraversalKind::NotPredicate)?.boxed(),
    })
}
