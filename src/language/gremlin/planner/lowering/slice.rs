//! `dedup`, `order`, `range`, `skip`, `tail`, `limit`, `sample` —
//! row-shape adjustments that don't transform the current binding's
//! type.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::helpers::{apply_by_spec, consume_by};
use crate::ir::expr::IrExpr;
use crate::ir::plan::{
    DistinctBulk, DistinctMode, Node, NullsOrder, ProjectErrorPolicy, ProjectMode, ProjectionItem,
    Slice, SortDir, SortKey,
};
use crate::ir::policy::PropertyMissing;
use crate::language::gremlin::ast::{SortDir as AstSortDir, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) fn lower_dedup<'a, I>(
    input: Node,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let by = consume_by(steps);
    let (input, key_expr) = match by {
        Some(spec) => {
            if let Some(expr) = simple_by_key_expr(&spec, lo) {
                apply_unproductive_filter(input, expr, lo)
            } else {
                apply_by_spec(input, &spec, lo, ctx)?
            }
        }
        None => (
            input,
            // Property-object traversers dedup by (key, value) — the
            // owning element does not participate (TinkerPop Property
            // equality). Other values dedup by themselves.
            IrExpr::Call {
                name: "gremlin_dedup_key".into(),
                args: vec![IrExpr::Binding(CURRENT.into())],
            },
        ),
    };
    let dedup_binding = lo.fresh("dedup_key");
    let projected = Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: dedup_binding.clone(),
            expr: key_expr,
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    };
    Ok(Node::GraphDistinct {
        keys: vec![dedup_binding],
        mode: DistinctMode::Traverser,
        bulk: DistinctBulk::ResetToOne,
        input: projected.boxed(),
    })
}

pub(super) fn lower_dedup_labels<'a, I>(
    mut input: Node,
    labels: &[String],
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let mut keys = labels.to_vec();
    if let Some(spec) = consume_by(steps) {
        let saved = lo.fresh("dedup_current");
        input = Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: saved.clone(),
                expr: IrExpr::Binding(CURRENT.into()),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: input.boxed(),
        };
        keys.clear();
        for label in labels {
            input = Node::GraphProject {
                mode: ProjectMode::PreserveVisible,
                items: vec![ProjectionItem {
                    alias: CURRENT.into(),
                    expr: IrExpr::Binding(label.clone()),
                }],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: input.boxed(),
            };
            let (next, expr) = if let Some(expr) = simple_by_key_expr(&spec, lo) {
                apply_unproductive_filter(input, expr, lo)
            } else {
                apply_by_spec(input, &spec, lo, ctx)?
            };
            let key = lo.fresh("dedup_label_key");
            input = Node::GraphProject {
                mode: ProjectMode::PreserveVisible,
                items: vec![ProjectionItem {
                    alias: key.clone(),
                    expr,
                }],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: next.boxed(),
            };
            keys.push(key);
        }
        input = Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: CURRENT.into(),
                expr: IrExpr::Binding(saved),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: input.boxed(),
        };
    }
    Ok(Node::GraphDistinct {
        keys,
        mode: DistinctMode::Traverser,
        bulk: DistinctBulk::ResetToOne,
        input: input.boxed(),
    })
}

pub(super) fn lower_order<'a, I>(
    input: Node,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let mut input = input;
    let mut keys = Vec::new();
    while let Some(spec) = consume_by(steps) {
        let dir = match spec.direction {
            AstSortDir::Asc => SortDir::Asc,
            AstSortDir::Desc => SortDir::Desc,
        };
        let (new_input, expr) = if spec.key.is_none() && spec.traversal.is_none() {
            // Direction-only `by(asc)` / `by(desc)`: order the traverser
            // itself under full orderability semantics.
            (
                input,
                IrExpr::Call {
                    name: "gremlin_order_key".into(),
                    args: vec![IrExpr::Binding(CURRENT.into())],
                },
            )
        } else if let Some(expr) = simple_by_key_expr(&spec, lo) {
            apply_unproductive_filter(input, expr, lo)
        } else {
            apply_by_spec(input, &spec, lo, ctx)?
        };
        input = new_input;
        let expr = match expr {
            IrExpr::Call { ref name, .. } if name == "gremlin_order_key" => expr,
            expr => IrExpr::Call { name: "gremlin_order_key".into(), args: vec![expr] },
        };
        keys.push(SortKey {
            expr,
            dir: dir.clone(),
            nulls: if matches!(dir, SortDir::Desc) {
                NullsOrder::Last
            } else {
                NullsOrder::First
            },
        });
    }
    if keys.is_empty() {
        keys.push(SortKey {
            expr: IrExpr::Call {
                name: "gremlin_order_key".into(),
                args: vec![IrExpr::Binding(CURRENT.into())],
            },
            dir: SortDir::Asc,
            nulls: NullsOrder::First,
        });
    }
    Ok(Node::GraphSort {
        keys,
        input: input.boxed(),
    })
}

pub(super) fn lower_range(input: Node, low: u64, high: u64) -> Node {
    Node::GraphSlice {
        slice: Slice {
            offset: low,
            fetch: Some(high.saturating_sub(low)),
            tail: None,
        },
        input: input.boxed(),
    }
}

pub(super) fn lower_skip(input: Node, n: u64) -> Node {
    Node::GraphSlice {
        slice: Slice {
            offset: n,
            fetch: None,
            tail: None,
        },
        input: input.boxed(),
    }
}

pub(super) fn lower_tail(input: Node, n: u64) -> Node {
    Node::GraphSlice {
        slice: Slice {
            offset: 0,
            fetch: None,
            tail: Some(n),
        },
        input: input.boxed(),
    }
}

pub(super) fn lower_limit_or_sample(input: Node, fetch: u64) -> Node {
    Node::GraphSlice {
        slice: Slice {
            offset: 0,
            fetch: Some(fetch),
            tail: None,
        },
        input: input.boxed(),
    }
}

pub(super) fn lower_sample<'a, I>(
    input: Node, fetch: u64, steps: &mut Peekable<I>, lo: &mut Lowerer, ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let (input, weight) = match consume_by(steps) {
        Some(spec) => {
            let (input, weight) = apply_by_spec(input, &spec, lo, ctx)?;
            (input, Some(weight))
        }
        None => (input, None),
    };
    Ok(Node::GraphSample {
        kind: crate::ir::plan::SampleKind::Global(fetch),
        seed: lo.random_seed,
        step_id: lo.fresh("sample"),
        weight,
        input: input.boxed(),
    })
}

pub(super) fn consume_local_order_by<'a, I>(steps: &mut Peekable<I>)
where
    I: Iterator<Item = &'a Step>,
{
    while consume_by(steps).is_some() {}
}

fn simple_by_key_expr(
    spec: &crate::language::gremlin::ast::BySpec,
    lo: &Lowerer,
) -> Option<IrExpr> {
    if spec.traversal.is_some() {
        return None;
    }
    let key = spec.key.as_ref()?;
    let key = key.strip_prefix("T.").unwrap_or(key).to_string();
    let missing = lo.by_property_missing();
    let expr = match key.as_str() {
        "id" => IrExpr::Id(CURRENT.into()),
        "label" => IrExpr::Label(CURRENT.into()),
        "keys" => IrExpr::property(CURRENT, "key".to_string(), missing),
        "values" => IrExpr::property(CURRENT, "value".to_string(), missing),
        _ => IrExpr::property(CURRENT, key.clone(), missing),
    };
    Some(expr)
}

fn apply_unproductive_filter(input: Node, expr: IrExpr, lo: &Lowerer) -> (Node, IrExpr) {
    let input = if matches!(lo.by_property_missing(), PropertyMissing::DropUnproductive) {
        Node::GraphFilter {
            condition: IrExpr::IsNotNull(Box::new(expr.clone())),
            input: input.boxed(),
        }
    } else {
        input
    };
    (input, expr)
}
