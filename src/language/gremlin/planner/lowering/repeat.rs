//! `repeat(body)` and its `times`/`emit`/`until` modulators.

use std::iter::Peekable;

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext};
use super::sub_traversal::lower_child_traversal;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{ApplyKind, EmitMode, Node, PathObjects, UnionAlign};
use crate::ir::policy::OptionalMissing;
use crate::language::gremlin::ast::Step;
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) fn lower_repeat<'a, I>(
    input: Node,
    name: Option<&str>,
    body: &[Step],
    prefix_emit: Option<Option<Vec<Step>>>,
    prefix_until: Option<Vec<Step>>,
    prefix_times: Option<u64>,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let prefix_times_set = prefix_times.is_some();
    let mut emit: Option<Option<Vec<Step>>> = prefix_emit.clone();
    let mut until_first = prefix_until.is_some();
    let mut until: Option<Vec<Step>> = prefix_until;
    let mut times: Option<u64> = prefix_times;
    let mut times_is_postfix = false;
    while let Some(next) = steps.peek() {
        match next {
            Step::Times(n) => {
                times = Some(*n);
                if !prefix_times_set {
                    times_is_postfix = true;
                }
                steps.next();
            }
            Step::Emit(p) => {
                emit = Some(p.clone());
                steps.next();
            }
            Step::Until(p) => {
                until = Some(p.clone());
                until_first = false;
                steps.next();
            }
            _ => break,
        }
    }
    let repeat_times: Option<u32> = match times {
        // Postfix `.times(0)` is equivalent to `until(loops>=0)`, which
        // (per TinkerPop semantics) runs the body exactly once before
        // the post-iteration check fires.
        Some(0) if times_is_postfix => Some(1),
        Some(n) => Some(n as u32),
        None => None,
    };
    let body_node = lower_child_traversal(body, lo, ctx, ChildTraversalKind::RepeatBody)?;

    // Postfix `repeat(...).emit(...)`. None inner = emit() always, Some
    // inner = emit(P) or emit(<traversal>) — try a row-level expression
    // first, fall back to a sub-traversal probe.
    let emit_mode = match emit {
        None => EmitMode::AfterLoop,
        Some(None) => EmitMode::AfterEachIteration,
        Some(Some(emit_steps)) => match try_until_steps_to_expr(&emit_steps)? {
            Some(expr) => EmitMode::AfterEachIfPredicate(expr),
            None => {
                let probe = lower_child_traversal(
                    &emit_steps,
                    lo,
                    ctx,
                    ChildTraversalKind::WherePredicate,
                )?;
                EmitMode::AfterEachIfTraversal(probe.boxed())
            }
        },
    };

    // Prefix `emit(...).repeat(...)` — applied to the seed before the
    // first iteration.
    let (prefix_predicate, prefix_traversal) = match prefix_emit {
        None => (None, None),
        Some(None) => (Some(crate::ir::expr::IrExpr::lit_bool(true)), None),
        Some(Some(seed_steps)) => match try_until_steps_to_expr(&seed_steps)? {
            Some(expr) => (Some(expr), None),
            None => {
                let probe = lower_child_traversal(
                    &seed_steps,
                    lo,
                    ctx,
                    ChildTraversalKind::WherePredicate,
                )?;
                (None, Some(probe.boxed()))
            }
        },
    };

    let (until_expr, until_traversal) = match until {
        Some(steps) => match try_until_steps_to_expr(&steps)? {
            Some(expr) => (Some(expr), None),
            None => {
                let probe =
                    lower_child_traversal(&steps, lo, ctx, ChildTraversalKind::WherePredicate)?;
                (None, Some(probe.boxed()))
            }
        },
        None => (None, None),
    };
    // `until(p).repeat(body)` is while-do: a seed traverser that already
    // satisfies `p` leaves the loop without running the body (TinkerPop's
    // `untilFirst`), and is never emit-split. Every later check happens at
    // the same loop count as the IR's post-step `until`, so only the seed
    // needs splitting: exiting seeds are emitted directly, the rest loop.
    let seed_matches =
        |kind: ApplyKind, negate: bool, seed: Node| match (&until_expr, &until_traversal) {
            (Some(expr), _) => Node::GraphFilter {
                condition: if negate {
                    IrExpr::Case {
                        arms: vec![(expr.clone(), IrExpr::lit_bool(false))],
                        otherwise: Some(Box::new(IrExpr::lit_bool(true))),
                    }
                } else {
                    expr.clone()
                },
                input: seed.boxed(),
            },
            (None, Some(probe)) => Node::GraphApply {
                kind,
                correlation: vec![CURRENT.into()],
                outputs: Vec::new(),
                optional_missing: OptionalMissing::Null,
                left: seed.boxed(),
                right: probe.clone(),
            },
            (None, None) => seed,
        };
    let (exiting, seed) = if until_first && (until_expr.is_some() || until_traversal.is_some()) {
        (
            Some(seed_matches(ApplyKind::Semi, false, input.clone())),
            seed_matches(ApplyKind::Anti, true, input),
        )
    } else {
        (None, input)
    };
    let repeat = Node::GraphRepeat {
        loop_name: name.map(str::to_string),
        times: repeat_times,
        emit: emit_mode,
        until: until_expr,
        until_traversal,
        path: None,
        path_objects: PathObjects::VerticesOnly,
        prefix_predicate,
        prefix_traversal,
        seed: seed.boxed(),
        body: body_node.boxed(),
    };
    Ok(match exiting {
        Some(exiting) => Node::GraphUnion {
            all: true,
            align: UnionAlign::ByVariableName,
            left: exiting.boxed(),
            right: repeat.boxed(),
        },
        None => repeat,
    })
}

/// Recognize a small subset of `until(...)` / `emit(...)` sub-
/// traversals as a row-level boolean expression. Returns `None` when
/// the sub-traversal can't be reduced — callers can then fall back to
/// a sub-traversal probe (for emit) or `lit(false)` (for until).
fn try_until_steps_to_expr(steps: &[Step]) -> GremlinPlanResult<Option<crate::ir::expr::IrExpr>> {
    use super::predicates::predicate_to_expr;
    use crate::ir::expr::IrExpr;
    use crate::ir::policy::PropertyMissing;
    let cur = || IrExpr::Binding("current".into());
    Ok(Some(match steps {
        [Step::Has { key, predicate }] => predicate_to_expr(
            IrExpr::property("current", key.clone(), PropertyMissing::DropUnproductive),
            predicate,
        )?,
        [Step::HasNot { key }] => IrExpr::IsNull(Box::new(IrExpr::property(
            "current",
            key.clone(),
            PropertyMissing::NullOnMissing,
        ))),
        [Step::HasId { ids }] => {
            let mut parts = Vec::with_capacity(ids.len());
            for v in ids {
                match v {
                    crate::language::gremlin::semantics::GValue::String(s) => {
                        parts.push(
                            super::helpers::element_token_filter("current", s).unwrap_or_else(
                                || IrExpr::Binary {
                                    op: crate::ir::expr::BinaryOp::Eq,
                                    lhs: Box::new(IrExpr::Id("current".into())),
                                    rhs: Box::new(IrExpr::lit_str(s.clone())),
                                },
                            ),
                        );
                    }
                    _ => {
                        if let Ok(lit) = super::literals::gvalue_to_lit(v) {
                            parts.push(IrExpr::Binary {
                                op: crate::ir::expr::BinaryOp::Eq,
                                lhs: Box::new(IrExpr::Id("current".into())),
                                rhs: Box::new(IrExpr::Lit(lit)),
                            });
                        }
                    }
                }
            }
            super::helpers::or_chain(parts)
        }
        [Step::HasIdPredicate { predicate }] => {
            predicate_to_expr(IrExpr::Id("current".into()), predicate)?
        }
        [Step::HasLabel(labels)] => super::helpers::any_label("current", labels),
        [Step::Is { predicate }] => predicate_to_expr(cur(), predicate)?,
        [Step::Loops(name), Step::Is { predicate }] => predicate_to_expr(
            IrExpr::Binding(super::dispatch::loop_binding_name(name)),
            predicate,
        )?,
        _ => return Ok(None),
    }))
}
