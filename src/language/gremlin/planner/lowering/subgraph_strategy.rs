//! `g.withStrategies(SubgraphStrategy(...))` post-filter wrappers.
//!
//! Every vertex / edge producer in the lowering goes through one of
//! these helpers. When no strategy is configured (the common case)
//! they're a pass-through; when a filter sub-traversal is supplied they
//! wrap the producer in a `GraphApply Semi` whose RHS evaluates the
//! filter against the produced binding.

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext};
use super::sub_traversal::lower_child_traversal;
use crate::ir::plan::{ApplyKind, Node, UnionAlign};
use crate::ir::policy::OptionalMissing;
use crate::language::gremlin::ast::Step;
use crate::language::gremlin::planner::error::{GremlinPlanError, GremlinPlanResult};
use crate::language::gremlin::semantics::Direction as SemDirection;

/// Wrap `node` in a SubgraphStrategy `vertices: __.<sub>` post-filter
/// when one is configured. `node` must produce a vertex bound to
/// `current`.
pub(super) fn apply_vertex_subgraph(
    node: Node,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    apply_subgraph_filter(node, lo, ctx, /* vertex */ true)
}

/// Wrap `node` in a SubgraphStrategy `edges: __.<sub>` post-filter when
/// one is configured. `node` must produce an edge bound to `current`.
pub(super) fn apply_edge_subgraph(
    node: Node,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let edge_filtered = apply_subgraph_filter(node, lo, ctx, /* vertex */ false)?;
    apply_edge_endpoint_vertex_filters(edge_filtered, lo, ctx)
}

/// Apply only the configured `edges:` predicate. Vertex-producing
/// expansions bind the traversed relationship transiently so `out()` /
/// `in()` / `both()` can be constrained by the same edge strategy without
/// also treating the final vertex as an edge traverser.
pub(super) fn apply_edge_predicate_subgraph(
    node: Node,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    apply_subgraph_filter(node, lo, ctx, /* vertex */ false)
}

fn apply_subgraph_filter(
    node: Node,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
    vertex: bool,
) -> GremlinPlanResult<Node> {
    if lo.in_subgraph_filter_eval {
        return Ok(node);
    }
    let filter_steps = if vertex {
        lo.subgraph_vertex_filter.clone()
    } else {
        lo.subgraph_edge_filter.clone()
    };
    let Some(filter_steps) = filter_steps else {
        return Ok(node);
    };
    if filter_steps.is_empty() {
        return Ok(node);
    }
    // `__.or(t1, t2, ...)` lowers to `WhereTraversal(Union(...))` at parse
    // time. Inside a SubgraphStrategy edge filter that aliasing is the
    // typical shape, but `WhereTraversal` is a no-op when *we ourselves*
    // are inside a where-traversal predicate context. So unwrap the outer
    // `WhereTraversal(Union(...))` shape if present and treat each
    // alternative as an OR'd Semi apply.
    let alternatives: Vec<Vec<Step>> = match filter_steps.as_slice() {
        [Step::WhereTraversal(sub)] => match sub.as_slice() {
            [Step::Union(alts)] => alts.clone(),
            _ => vec![sub.clone()],
        },
        _ => vec![filter_steps.clone()],
    };

    // Recursion guard: when lowering the filter sub-traversals, any
    // V/E/expand step inside them must NOT re-trigger the same strategy
    // wrapping — otherwise the filter recursively spawns more copies of
    // itself forever.
    lo.in_subgraph_filter_eval = true;
    let result: GremlinPlanResult<Node> = (|| {
        let mut iter = alternatives.into_iter();
        let first = iter
            .next()
            .ok_or_else(|| GremlinPlanError::Plan("empty subgraph filter".to_string()))?;
        let mut probe = lower_child_traversal(&first, lo, ctx, ChildTraversalKind::SubgraphFilter)?;
        for alt in iter {
            let next = lower_child_traversal(&alt, lo, ctx, ChildTraversalKind::SubgraphFilter)?;
            probe = Node::GraphUnion {
                all: true,
                align: UnionAlign::ByPosition,
                left: probe.boxed(),
                right: next.boxed(),
            };
        }
        Ok(Node::GraphApply {
            kind: ApplyKind::Semi,
            correlation: vec![CURRENT.into()],
            outputs: Vec::new(),
            optional_missing: OptionalMissing::Null,
            left: node.boxed(),
            right: probe.boxed(),
        })
    })();
    lo.in_subgraph_filter_eval = false;
    result
}

fn apply_edge_endpoint_vertex_filters(
    node: Node,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    if lo.in_subgraph_filter_eval {
        return Ok(node);
    }
    if !lo.subgraph_check_adjacent_vertices {
        return Ok(node);
    }
    let Some(vertex_filter) = lo.subgraph_vertex_filter.clone() else {
        return Ok(node);
    };
    if vertex_filter.is_empty() {
        return Ok(node);
    }
    let node = apply_edge_endpoint_vertex_filter(node, &vertex_filter, SemDirection::Out, lo, ctx)?;
    apply_edge_endpoint_vertex_filter(node, &vertex_filter, SemDirection::In, lo, ctx)
}

fn apply_edge_endpoint_vertex_filter(
    node: Node,
    vertex_filter: &[Step],
    direction: SemDirection,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let mut endpoint_steps = Vec::with_capacity(vertex_filter.len() + 1);
    endpoint_steps.push(Step::EndpointVertex { direction });
    endpoint_steps.extend(vertex_filter.iter().cloned());

    lo.in_subgraph_filter_eval = true;
    let result =
        lower_child_traversal(&endpoint_steps, lo, ctx, ChildTraversalKind::SubgraphFilter).map(
            |probe| Node::GraphApply {
                kind: ApplyKind::Semi,
                correlation: vec![CURRENT.into()],
                outputs: Vec::new(),
                optional_missing: OptionalMissing::Null,
                left: node.boxed(),
                right: probe.boxed(),
            },
        );
    lo.in_subgraph_filter_eval = false;
    result
}
