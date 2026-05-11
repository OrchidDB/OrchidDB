//! `group()` and `groupCount()` and their `by(...)` modulators.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::helpers::{apply_by_spec, apply_project_by_spec, consume_by};
use super::reduce::agg_kind;
use crate::ir::expr::{AggCall, AggKind, IrExpr};
use crate::ir::plan::{GroupValue, Node};
use crate::ir::policy::PropertyMissing;
use crate::language::gremlin::ast::{BySpec, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;

const GROUP_FLATTEN_VALUE_ALIAS: &str = "__group_flatten_value";

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
                let (kind, flatten_lists) = value_by_applied_aggregate(value_by.as_ref())?;
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

fn value_by_applied_aggregate(spec: Option<&BySpec>) -> GremlinPlanResult<(AggKind, bool)> {
    let Some(steps) = spec.and_then(|spec| spec.traversal.as_deref()) else {
        return Ok((AggKind::CollectTraversers, false));
    };
    let Some(last) = steps.last() else {
        return Ok((AggKind::CollectTraversers, false));
    };
    match last {
        Step::Count => Ok((AggKind::Sum, false)),
        Step::Aggregate(kind) => Ok((agg_kind(*kind)?, false)),
        Step::Fold => Ok((AggKind::CollectTraversers, true)),
        _ => Ok((AggKind::CollectTraversers, false)),
    }
}
