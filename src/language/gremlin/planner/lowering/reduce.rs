//! Whole-stream reductions that fold the result down: `count`, `sum`/
//! `min`/`max`/`mean`, `fold`, `unfold`.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::helpers::{apply_by_spec, consume_by};
use crate::ir::expr::{AggCall, AggKind, IrExpr};
use crate::ir::plan::Node;
use crate::language::gremlin::ast::{AggKind as AstAggKind, Step};
use crate::language::gremlin::planner::error::{GremlinPlanError, GremlinPlanResult};

pub(super) fn lower_count(input: Node) -> Node {
    Node::GraphAggregate {
        group: Vec::new(),
        aggs: vec![AggCall {
            kind: AggKind::CountBulk,
            alias: CURRENT.into(),
            arg: None,
            distinct: false,
        }],
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}

pub(super) fn lower_aggregate<'a, I>(
    input: Node,
    kind: AstAggKind,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let (input, arg) = match consume_by(steps) {
        Some(spec) => apply_by_spec(input, &spec, lo, ctx)?,
        None => (input, IrExpr::Binding(CURRENT.into())),
    };
    Ok(Node::GraphAggregate {
        group: Vec::new(),
        aggs: vec![AggCall {
            kind: agg_kind(kind)?,
            alias: CURRENT.into(),
            arg: Some(arg),
            distinct: false,
        }],
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    })
}

pub(super) fn lower_fold(input: Node) -> Node {
    Node::GraphAggregate {
        group: Vec::new(),
        aggs: vec![AggCall {
            kind: AggKind::CollectTraversers,
            alias: CURRENT.into(),
            arg: Some(IrExpr::Binding(CURRENT.into())),
            distinct: false,
        }],
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}

pub(super) fn lower_unfold(input: Node) -> Node {
    // Route through `gremlin_unfold_items` so null traversers unfold to
    // themselves (TinkerPop semantics) instead of Cypher's UNWIND-null →
    // nothing.
    Node::GraphUnwind {
        input_expr: IrExpr::Call {
            name: "gremlin_unfold_items".into(),
            args: vec![IrExpr::Binding(CURRENT.into())],
        },
        bind: CURRENT.into(),
        outer: false,
        input: input.boxed(),
    }
}

pub(super) fn agg_kind(kind: AstAggKind) -> GremlinPlanResult<AggKind> {
    Ok(match kind {
        AstAggKind::Sum => AggKind::Sum,
        AstAggKind::Min => AggKind::Min,
        AstAggKind::Max => AggKind::Max,
        AstAggKind::Mean => AggKind::Avg,
        AstAggKind::Product => {
            return Err(GremlinPlanError::Unsupported(
                "product() reduction is not yet lowered".to_string(),
            ));
        }
    })
}
