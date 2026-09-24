//! Bounded consumption of row-local pipelines containing lazy side effects.
//!
//! Barriers remain whole-stream operations. Each eligible stage is evaluated
//! with the existing interpreter over one incoming traverser; its outputs are
//! consumed downstream before another upstream traverser is requested.

use super::super::{
    IrResult, Row,
    run::{ExecutionContext, run_with_context},
};
use super::repeat::{run_body_with_frontier, run_with_frontier};
use crate::ir::{
    analysis::{Effect, children, node_effect},
    catalog::PropertyGraph,
    plan::{Node, Slice},
};

pub(crate) fn bounded_lazy_pipeline(
    input: &Node,
    slice: &Slice,
    frontier: Option<&[Row]>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> Option<IrResult<Vec<Row>>> {
    let fetch = slice.fetch.filter(|_| slice.tail.is_none())?;
    let mut source = input.clone();
    let mut stages = Vec::new();
    let mut lazy = false;
    while let Some(upstream) = take_stream_input(&mut source) {
        lazy |= matches!(source, Node::GraphSideEffect { eager: false, .. });
        stages.push(source);
        source = *upstream;
    }
    if !lazy {
        return None;
    }
    stages.reverse();
    Some((|| {
        let rows = match frontier {
            Some(frontier) => run_with_frontier(&source, frontier, graph, ctx)?,
            None => run_with_context(&source, graph, ctx)?,
        };
        // EarlyLimitStrategy moves a range before bound scalar maps and
        // side-effect steps. At a filter/flat-map boundary RangeGlobalStep
        // requests one additional traverser before recognizing its high bound.
        let boundary = stages
            .iter()
            .rposition(|stage| !range_movable(stage))
            .map_or(0, |index| index + 1);
        let (prefix, suffix) = stages.split_at(boundary);
        let high = slice.offset.saturating_add(fetch);
        let mut consumer = Consumer {
            remaining: if fetch == 0 {
                0
            } else {
                high.saturating_add(1)
            },
            position: 0,
            offset: slice.offset,
            high,
            suffix,
            output: Vec::new(),
        };
        for row in rows {
            for _ in 0..row.bulk {
                if consumer.remaining == 0 {
                    break;
                }
                let mut single = row.clone();
                single.bulk = 1;
                consume(single, prefix, &mut consumer, graph, ctx)?;
            }
            if consumer.remaining == 0 {
                break;
            }
        }
        Ok(consumer.output)
    })())
}

struct Consumer<'a> {
    remaining: u64,
    position: u64,
    offset: u64,
    high: u64,
    suffix: &'a [Node],
    output: Vec<Row>,
}

fn consume(
    row: Row,
    stages: &[Node],
    consumer: &mut Consumer<'_>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<()> {
    if consumer.remaining == 0 {
        return Ok(());
    }
    let Some((stage, rest)) = stages.split_first() else {
        let mut row = row;
        row.bulk = row.bulk.min(consumer.remaining);
        consumer.remaining -= row.bulk;
        let position = consumer.position;
        consumer.position = consumer.position.saturating_add(row.bulk);
        let mut selected = super::slice::slice_op(
            &Slice {
                offset: consumer.offset.saturating_sub(position),
                fetch: Some(consumer.high.saturating_sub(position.max(consumer.offset))),
                tail: None,
            },
            vec![row],
        )?;
        for stage in consumer.suffix {
            if selected.is_empty() {
                break;
            }
            selected = run_body_with_frontier(stage, selected, graph, ctx)?;
        }
        consumer.output.extend(selected);
        return Ok(());
    };
    for row in run_body_with_frontier(stage, vec![row], graph, ctx)? {
        consume(row, rest, consumer, graph, ctx)?;
        if consumer.remaining == 0 {
            break;
        }
    }
    Ok(())
}

fn range_movable(node: &Node) -> bool {
    matches!(
        node,
        Node::GraphProject { .. }
            | Node::GraphCurrentProject { .. }
            | Node::GraphBind { .. }
            | Node::GraphSelect { .. }
            | Node::GraphReadSideEffect { .. }
            | Node::GraphSideEffect { eager: false, .. }
            | Node::GraphSetProperty { .. }
            | Node::GraphCreate { .. }
    ) || matches!(node, Node::GraphProcedureCall { name, .. }
        if name == "gremlin.mutation.add_vertex")
}

fn pure(node: &Node) -> bool {
    node_effect(node) == Effect::Pure && children(node).into_iter().all(pure)
}

fn take_stream_input(node: &mut Node) -> Option<Box<Node>> {
    let input = match node {
        Node::GraphFilter { input, .. }
        | Node::GraphProject { input, .. }
        | Node::GraphCurrentProject { input, .. }
        | Node::GraphBind { input, .. }
        | Node::GraphExpand { input, .. }
        | Node::GraphUnwind { input, .. }
        | Node::GraphSelect { input, .. }
        | Node::GraphReadSideEffect { input, .. }
        | Node::GraphSetProperty { input, .. }
        | Node::GraphCreate { input, .. }
        | Node::GraphDelete { input, .. } => input,
        Node::GraphProcedureCall {
            name,
            input: Some(input),
            ..
        } if name.starts_with("gremlin.mutation.") => input,
        // Apply children already have per-traverser scope. Pure children can
        // materialize their own results without executing unconsumed effects.
        Node::GraphApply { left, right, .. } if pure(right) => left,
        Node::GraphChoose {
            input,
            arms,
            default,
            ..
        } if arms
            .iter()
            .all(|arm| pure(&arm.body) && !super::choose::contains_barrier(&arm.body))
            && default
                .as_deref()
                .is_none_or(|body| pure(body) && !super::choose::contains_barrier(body)) =>
        {
            input
        }
        Node::GraphSideEffect {
            input,
            eager: false,
            ..
        } => input,
        _ => return None,
    };
    Some(std::mem::replace(
        input,
        Node::GraphCorrelate {
            bindings: vec!["__gremlin_group_members".into()],
        }
        .boxed(),
    ))
}
