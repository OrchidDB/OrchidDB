//! `group()` and `groupCount()` and their `by(...)` modulators.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::helpers::{apply_by_spec, apply_project_by_spec, consume_by};
use super::literals::gvalue_to_expr;
use super::reduce::agg_kind;
use crate::ir::expr::{AggCall, AggKind, IrExpr};
use crate::ir::plan::{GroupValue, Node};
use crate::ir::policy::PropertyMissing;
use crate::language::gremlin::ast::{BySpec, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;

const GROUP_FLATTEN_VALUE_ALIAS: &str = "__group_flatten_value";
const GROUP_UNWRAP_VALUE_ALIAS: &str = "__group_unwrap_value";

pub(super) fn lower_group_step<'a, I>(
    input: Node,
    steps: &mut Peekable<I>,
    just_count: bool,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let key_by = consume_by(steps);
    let value_by = if just_count { None } else { consume_by(steps) };
    lower_group(input, key_by, value_by, just_count, lo, ctx)
}

pub(super) fn lower_group(
    input: Node,
    key_by: Option<BySpec>,
    value_by: Option<BySpec>,
    just_count: bool,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let (input, key_expr) = match key_by {
        Some(spec) => apply_by_spec(input, &spec, lo, ctx)?,
        None => (input, IrExpr::Binding(CURRENT.into())),
    };
    let value_alias = "value".to_string();
    let (input, value) = if just_count {
        (input, GroupValue::CountBulk)
    } else {
        let grouped_agg = match value_by.as_ref() {
            Some(spec) => group_value_aggregate(spec)?,
            None => None,
        };
        match grouped_agg {
            Some(agg) => (input, GroupValue::Aggregate(agg)),
            None => {
                // Value modulators that are real traversals apply to the
                // *group's* traverser stream (TinkerPop semantics), not per
                // input row: `by(bothE().values("weight").dedup().fold())`
                // dedups across the whole group.
                if let Some(value_steps) = value_by
                    .as_ref()
                    .and_then(|spec| spec.traversal.clone())
                    .filter(|steps| !steps.is_empty())
                    .filter(|steps| !references_outer_labels(steps))
                {
                    return lower_group_stream_value(input, key_expr, &value_steps, lo, ctx);
                }
                let (kind, flatten_lists, unwrap_single) =
                    value_by_applied_aggregate(value_by.as_ref())?;
                let (input, arg) = match value_by {
                    Some(spec) if flatten_lists => apply_project_by_spec(input, &spec, lo, ctx)?,
                    Some(spec) => apply_by_spec(input, &spec, lo, ctx)?,
                    None => (input, IrExpr::Binding(CURRENT.into())),
                };
                (
                    input,
                    GroupValue::Aggregate(AggCall {
                        kind,
                        alias: if flatten_lists {
                            GROUP_FLATTEN_VALUE_ALIAS.to_string()
                        } else if unwrap_single {
                            GROUP_UNWRAP_VALUE_ALIAS.to_string()
                        } else {
                            value_alias.clone()
                        },
                        arg: Some(arg),
                        distinct: false,
                    }),
                )
            }
        }
    };
    Ok(Node::GraphGroupMap {
        key: key_expr,
        value,
        output: CURRENT.into(),
        input: input.boxed(),
    })
}

/// True when the value traversal reads outer per-traverser state
/// (`select(label)`, `sack()`, `loops()`, cross-binding `where`): the
/// group-stream rebuild materializes members as bare values, so those
/// references would dangle. Such traversals stay on the per-row path.
fn references_outer_labels(steps: &[Step]) -> bool {
    let rendered = format!("{steps:?}");
    rendered.contains("Select(")
        || rendered.contains("SelectMulti(")
        || rendered.contains("Sack")
        || rendered.contains("Loops(")
        || rendered.contains("WhereString")
}

/// `group().by(K).by(<traversal>)` with a traversal-valued modulator:
/// collect the group members first, then run the value traversal over each
/// group's member stream and rebuild the map.
fn lower_group_stream_value(
    input: Node,
    key_expr: IrExpr,
    value_steps: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    use crate::ir::plan::{ApplyKind, ProjectErrorPolicy, ProjectMode, ProjectionItem};
    use crate::ir::policy::OptionalMissing;

    // 1. key → [members] map.
    let grouped = Node::GraphGroupMap {
        key: key_expr,
        value: GroupValue::Aggregate(AggCall {
            kind: AggKind::CollectTraversers,
            alias: "value".to_string(),
            arg: Some(IrExpr::Binding(CURRENT.into())),
            distinct: false,
        }),
        output: CURRENT.into(),
        input: input.boxed(),
    };
    // 2. one row per (key, members) entry.
    let entries = Node::GraphUnwind {
        input_expr: IrExpr::Binding(CURRENT.into()),
        bind: CURRENT.into(),
        outer: false,
        input: grouped.boxed(),
    };
    // 3. run the value traversal over each entry's member stream.
    let value_alias = lo.fresh("group_stream_value");
    let child_ctx = lo.child_context(ctx, super::context::ChildTraversalKind::ByModulator);
    let right = lo.enter_context(child_ctx, |lo, cctx| -> GremlinPlanResult<Node> {
        let mut node: Node = Node::GraphCorrelate {
            bindings: vec![CURRENT.to_string()],
        };
        node = Node::GraphProject {
            mode: ProjectMode::ReplaceCurrent,
            items: vec![ProjectionItem {
                alias: CURRENT.into(),
                expr: IrExpr::property(
                    CURRENT,
                    "value".to_string(),
                    PropertyMissing::NullOnMissing,
                ),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: node.boxed(),
        };
        node = Node::GraphUnwind {
            input_expr: IrExpr::Binding(CURRENT.into()),
            bind: CURRENT.into(),
            outer: false,
            input: node.boxed(),
        };
        let mut iter = value_steps.iter().peekable();
        while let Some(step) = iter.next() {
            node = super::dispatch::lower_step_with_context(node, step, &mut iter, lo, cctx)?;
        }
        Ok(node)
    })?;
    let right = Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: value_alias.clone(),
            expr: IrExpr::Binding(CURRENT.into()),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: Node::GraphSlice {
            slice: crate::ir::plan::Slice {
                offset: 0,
                fetch: Some(1),
                tail: None,
            },
            input: right.boxed(),
        }
        .boxed(),
    };
    let with_value = Node::GraphApply {
        kind: ApplyKind::Scalar,
        correlation: vec![CURRENT.to_string()],
        outputs: vec![value_alias.clone()],
        optional_missing: OptionalMissing::Null,
        left: entries.boxed(),
        right: right.boxed(),
    };
    // Entries whose value traversal produced nothing are omitted from the
    // final map (e.g. `by(outE().values("weight").sum())` for a vertex
    // with no out-edges).
    let with_value = Node::GraphFilter {
        condition: IrExpr::IsNotNull(Box::new(IrExpr::Binding(value_alias.clone()))),
        input: with_value.boxed(),
    };
    // 4. rebuild the map keyed by the entry key.
    Ok(Node::GraphGroupMap {
        key: IrExpr::property(CURRENT, "key".to_string(), PropertyMissing::NullOnMissing),
        value: GroupValue::Aggregate(AggCall {
            kind: AggKind::CollectTraversers,
            alias: GROUP_UNWRAP_VALUE_ALIAS.to_string(),
            arg: Some(IrExpr::Binding(value_alias)),
            distinct: false,
        }),
        output: CURRENT.into(),
        input: with_value.boxed(),
    })
}

fn group_value_aggregate(spec: &BySpec) -> GremlinPlanResult<Option<AggCall>> {
    let value_alias = "value".to_string();
    let current = || IrExpr::Binding(CURRENT.into());
    let property =
        |key: &String| IrExpr::property(CURRENT, key.clone(), PropertyMissing::DropUnproductive);
    let Some(steps) = spec.traversal.as_deref() else {
        if let Some(key) = &spec.key {
            return Ok(Some(AggCall {
                kind: AggKind::CollectTraversers,
                alias: value_alias.clone(),
                arg: Some(property(key)),
                distinct: false,
            }));
        }
        return Ok(None);
    };
    let agg = match steps {
        [Step::Count] => AggCall {
            kind: AggKind::CountBulk,
            alias: value_alias,
            arg: None,
            distinct: false,
        },
        [Step::Label, Step::Count] => AggCall {
            kind: AggKind::CountBulk,
            alias: value_alias,
            arg: None,
            distinct: false,
        },
        [Step::Values(keys), Step::Count] if keys.len() == 1 => AggCall {
            kind: AggKind::CountRows,
            alias: value_alias,
            arg: Some(property(&keys[0])),
            distinct: false,
        },
        [Step::Fold] => AggCall {
            kind: AggKind::CollectTraversers,
            alias: value_alias,
            arg: Some(current()),
            distinct: false,
        },
        [Step::Values(keys), Step::Fold] if keys.len() == 1 => AggCall {
            kind: AggKind::CollectTraversers,
            alias: value_alias,
            arg: Some(property(&keys[0])),
            distinct: false,
        },
        [Step::Constant(value)] => AggCall {
            kind: AggKind::Min,
            alias: value_alias,
            arg: Some(gvalue_to_expr(value)?),
            distinct: false,
        },
        [Step::Aggregate(kind)] => AggCall {
            kind: agg_kind(*kind)?,
            alias: value_alias,
            arg: Some(current()),
            distinct: false,
        },
        [Step::Values(keys), Step::Aggregate(kind)] if keys.len() == 1 => AggCall {
            kind: agg_kind(*kind)?,
            alias: value_alias,
            arg: Some(property(&keys[0])),
            distinct: false,
        },
        _ => return Ok(None),
    };
    Ok(Some(agg))
}

fn value_by_applied_aggregate(spec: Option<&BySpec>) -> GremlinPlanResult<(AggKind, bool, bool)> {
    let Some(steps) = spec.and_then(|spec| spec.traversal.as_deref()) else {
        return Ok((AggKind::CollectTraversers, false, false));
    };
    let Some(last) = steps.last() else {
        return Ok((AggKind::CollectTraversers, false, false));
    };
    match last {
        Step::Count => Ok((AggKind::Sum, false, false)),
        Step::Aggregate(kind) => Ok((agg_kind(*kind)?, false, false)),
        Step::Fold => Ok((AggKind::CollectTraversers, true, false)),
        _ => Ok((AggKind::CollectTraversers, false, true)),
    }
}
