//! `group()` and `groupCount()` and their `by(...)` modulators.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::helpers::{apply_by_spec, consume_by};
use crate::ir::expr::{AggCall, AggKind, IrExpr};
use crate::ir::plan::{GroupValue, Node};
use crate::ir::policy::PropertyMissing;
use crate::language::gremlin::ast::{BySpec, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;

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
    let value = if just_count {
        GroupValue::CountBulk
    } else if let Some(value_steps) = value_by
        .as_ref()
        .and_then(|spec| spec.traversal.as_deref())
        .filter(|steps| !steps.is_empty())
    {
        // A by-traversal consumes the complete group of traversers, including
        // their label bindings. Reductions must not run separately per member.
        let traversal = super::sub_traversal::lower_child_traversal(
            value_steps,
            lo,
            ctx,
            super::context::ChildTraversalKind::ByModulator,
        )?;
        GroupValue::Traversal {
            traversal: traversal.boxed(),
        }
    } else if let Some(agg) = value_by
        .as_ref()
        .map(group_value_aggregate)
        .transpose()?
        .flatten()
    {
        GroupValue::Aggregate(agg)
    } else {
        GroupValue::Aggregate(AggCall {
            kind: AggKind::CollectTraversers,
            alias: value_alias,
            arg: Some(IrExpr::Binding(CURRENT.into())),
            distinct: false,
        })
    };
    Ok(Node::GraphGroupMap {
        key: key_expr,
        value,
        output: CURRENT.into(),
        input: input.boxed(),
    })
}

fn group_value_aggregate(spec: &BySpec) -> GremlinPlanResult<Option<AggCall>> {
    Ok(spec.key.as_ref().map(|key| AggCall {
        kind: AggKind::CollectTraversers,
        alias: "value".to_string(),
        arg: Some(IrExpr::property(
            CURRENT,
            key.clone(),
            PropertyMissing::DropUnproductive,
        )),
        distinct: false,
    }))
}
