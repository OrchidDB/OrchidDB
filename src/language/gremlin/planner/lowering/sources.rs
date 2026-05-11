//! Traversal sources: `g.V() / g.V(ids) / g.E() / g.E(ids) / g.inject(...)`
//! plus the `g.union(...)` sourceless start.

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext, TraversalScopeKind};
use super::helpers::filter_by_ids;
use super::literals::{gvalue_to_expr, gvalue_to_value};
use super::procedures::lower_call_source;
use super::side_effects::SACK;
use super::sub_traversal::lower_child_traversal;
use super::subgraph_strategy::{apply_edge_subgraph, apply_vertex_subgraph};
use crate::ir::expr::IrExpr;
use crate::ir::plan::{
    BindKind, Direction, LabelExpr, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem,
    UnionAlign,
};
use crate::language::gremlin::ast::Step;
use crate::language::gremlin::planner::error::{GremlinPlanError, GremlinPlanResult};

pub(super) fn source_node(
    step: &Step,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let node = match step {
        Step::V { ids } if ids.is_empty() => {
            let scan = vertex_scan();
            apply_vertex_subgraph(scan, lo, ctx)
        }
        Step::V { ids } => {
            let scan = vertex_scan();
            apply_vertex_subgraph(filter_by_ids(scan, ids), lo, ctx)
        }
        Step::E { ids } if ids.is_empty() => {
            let scan = edge_scan();
            apply_edge_subgraph(scan, lo, ctx)
        }
        Step::E { ids } => {
            let scan = edge_scan();
            apply_edge_subgraph(filter_by_ids(scan, ids), lo, ctx)
        }
        Step::Inject(values) => Ok(Node::GraphValues {
            bindings: vec![CURRENT.into()],
            rows: values.iter().map(|v| vec![gvalue_to_value(v)]).collect(),
            bulk: None,
        }),
        Step::Call(name, args) => lower_call_source(name, args)?.ok_or_else(|| {
            GremlinPlanError::Unsupported(format!("unsupported source call `{name}`"))
        }),
        Step::Union(branches) => lower_source_union(branches, lo, ctx),
        other => Err(GremlinPlanError::Unsupported(format!(
            "traversal must start with V(), E(), inject(), or union(): found {other:?}"
        ))),
    }?;
    with_initial_sack(node, lo, ctx)
}

fn with_initial_sack(
    node: Node,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    if !matches!(ctx.scope(), TraversalScopeKind::Root) {
        return Ok(node);
    }
    let Some(initial) = lo.sack_initial.as_ref() else {
        return Ok(node);
    };
    Ok(Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: SACK.into(),
            expr: gvalue_to_expr(initial)?,
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: node.boxed(),
    })
}

fn vertex_scan() -> Node {
    Node::GraphBind {
        bind: CURRENT.into(),
        kind: BindKind::Node,
        expr: None,
        input: Node::GraphNodeScan {
            graph: "default".into(),
            binding: CURRENT.into(),
            labels: LabelExpr::Any,
        }
        .boxed(),
    }
}

fn edge_scan() -> Node {
    Node::GraphBind {
        bind: CURRENT.into(),
        kind: BindKind::Edge,
        expr: None,
        input: Node::GraphRelScan {
            graph: "default".into(),
            binding: CURRENT.into(),
            types: LabelExpr::Any,
            dir: Direction::Out,
        }
        .boxed(),
    }
}

/// `g.union(t1, t2, ...)` — sourceless start. Each branch is a source-rooted
/// child traversal that contributes rows; this stays inside the active
/// lowering context instead of recursively creating a fresh `Lowerer`.
fn lower_source_union(
    branches: &[Vec<Step>],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    // `g.union()` with no arms — emit a no-row source. We model that as
    // a `GraphValues` with zero rows so the rest of the chain has
    // something to chain against.
    let mut iter = branches.iter();
    let Some(first) = iter.next() else {
        return Ok(Node::GraphValues {
            bindings: vec![CURRENT.into()],
            rows: Vec::new(),
            bulk: None,
        });
    };
    let mut acc = lower_child_traversal(first, lo, ctx, ChildTraversalKind::SourceUnionArm)?;
    for next in iter {
        let next_node = lower_child_traversal(next, lo, ctx, ChildTraversalKind::SourceUnionArm)?;
        acc = Node::GraphUnion {
            all: true,
            align: UnionAlign::ByPosition,
            left: acc.boxed(),
            right: next_node.boxed(),
        };
    }
    Ok(acc)
}

/// Mid-traversal spawn (`g.V(...)` / `g.E(...)` / `g.inject(...)` not
/// at position 0). Each input row gets paired with each result of the
/// spawn via an `Apply Inner` so the spawned binding *replaces* the
/// outer `current`. This matches the TinkerPop semantics for queries
/// like `g.union(__.V(vid1), __.V(vid4))` reached through arbitrary
/// nesting.
pub(super) fn lower_mid_traversal_spawn(
    input: Node,
    spawn: &Step,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    use crate::ir::plan::ApplyKind;
    use crate::ir::policy::OptionalMissing;

    let spawn_node = source_node(spawn, lo, ctx)?;
    Ok(Node::GraphApply {
        kind: ApplyKind::Inner,
        correlation: vec![CURRENT.into()],
        outputs: vec![CURRENT.into()],
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: spawn_node.boxed(),
    })
}

/// Mid-traversal `inject(values...)` — concat the literal stream onto
/// the upstream row stream. This is *not* a cartesian rebind like
/// `V(...)`/`E(...)`; the inject values become additional traversers in
/// the same stream. We model that as `GraphUnion(all=true, ByPosition)`
/// with the upstream on the left and a `GraphValues` on the right.
pub(super) fn lower_mid_traversal_inject(
    input: Node,
    spawn: &Step,
    _lo: &mut Lowerer,
    _ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let values = match spawn {
        Step::Inject(vs) => vs,
        other => {
            return Err(GremlinPlanError::Unsupported(format!(
                "lower_mid_traversal_inject called with non-Inject step: {other:?}"
            )));
        }
    };
    let injected = Node::GraphValues {
        bindings: vec![CURRENT.into()],
        rows: values.iter().map(|v| vec![gvalue_to_value(v)]).collect(),
        bulk: None,
    };
    Ok(Node::GraphUnion {
        all: true,
        align: UnionAlign::ByPosition,
        left: input.boxed(),
        right: injected.boxed(),
    })
}

/// Wrap an edge traverser produced by `outE/inE/bothE` so the edge
/// becomes the new `current` binding. Used by both the spawn-source
/// path (`g.E()`) and the mid-traversal expand path; kept here next to
/// the scans for visibility.
pub(super) fn project_rel_as_current(rel: String, expand: Node) -> Node {
    Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![ProjectionItem {
            alias: CURRENT.into(),
            expr: IrExpr::Binding(rel),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: expand.boxed(),
    }
}
