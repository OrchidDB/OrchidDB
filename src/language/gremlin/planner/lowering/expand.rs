//! Expansion steps: `out`/`in`/`both` (vertex → vertex), `outE`/`inE`/
//! `bothE` (vertex → edge), `outV`/`inV`/`bothV` (edge → endpoint),
//! `otherV` (edge → opposite endpoint).
//!
//! Vertex- and edge-producers are post-filtered through the subgraph
//! strategy when one is configured.

use super::context::{CURRENT, Lowerer, PATH, TraversalContext};
use super::helpers::{map_direction, rel_types_label};
use super::sources::project_rel_as_current;
use super::subgraph_strategy::{
    apply_edge_predicate_subgraph, apply_edge_subgraph, apply_vertex_subgraph,
};
use crate::ir::expr::IrExpr;
use crate::ir::plan::{
    LabelExpr, Length, Node, PathMaterialization, PathUpdate, ProjectErrorPolicy, ProjectMode,
    ProjectionItem, TargetMode,
};
use crate::ir::policy::{MatchMode, PathMode};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::Direction as SemDirection;

const EDGE_OTHER: &str = "__edge_other";

pub(super) fn lower_expand_vertex(
    input: Node,
    direction: SemDirection,
    edge_labels: &[String],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let target = lo.fresh("v");
    let rel = lo.fresh("e");
    let expand = Node::GraphExpand {
        graph: "default".into(),
        source: CURRENT.into(),
        target: target.clone(),
        target_mode: TargetMode::BindNew,
        target_labels: LabelExpr::Any,
        rel_binding: Some(rel.clone()),
        rel_types: rel_types_label(edge_labels),
        dir: map_direction(direction),
        length: Length::ONE,
        history: None,
        path: Some(ctx.path_binding().into()),
        path_mode: PathMode::None,
        match_mode: MatchMode::ProviderDefined,
        path_materialization: PathMaterialization::None,
        path_update: PathUpdate::None,
        input: input.boxed(),
    };
    let edge_current = project_rel_as_current(rel, expand);
    let edge_filtered = apply_edge_predicate_subgraph(edge_current, lo, ctx)?;
    let vertex_current = Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![
            ProjectionItem {
                alias: CURRENT.into(),
                expr: IrExpr::Binding(target.clone()),
            },
            ProjectionItem {
                alias: PATH.into(),
                expr: IrExpr::Call {
                    name: "path_append".into(),
                    args: vec![IrExpr::Binding(PATH.into()), IrExpr::Binding(target)],
                },
            },
        ],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: edge_filtered.boxed(),
    };
    apply_vertex_subgraph(vertex_current, lo, ctx)
}

pub(super) fn lower_expand_edge(
    input: Node,
    direction: SemDirection,
    edge_labels: &[String],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    // outE/inE/bothE: the `current` binding becomes the edge.
    let rel = lo.fresh("e");
    let expand = Node::GraphExpand {
        graph: "default".into(),
        source: CURRENT.into(),
        target: EDGE_OTHER.into(),
        target_mode: TargetMode::BindNew,
        target_labels: LabelExpr::Any,
        rel_binding: Some(rel.clone()),
        rel_types: rel_types_label(edge_labels),
        dir: map_direction(direction),
        length: Length::ONE,
        history: None,
        path: Some(ctx.path_binding().into()),
        path_mode: PathMode::None,
        match_mode: MatchMode::ProviderDefined,
        path_materialization: PathMaterialization::None,
        path_update: PathUpdate::None,
        input: input.boxed(),
    };
    let projected = Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![
            ProjectionItem {
                alias: CURRENT.into(),
                expr: IrExpr::Binding(rel.clone()),
            },
            ProjectionItem {
                alias: PATH.into(),
                expr: IrExpr::Call {
                    name: "path_append".into(),
                    args: vec![IrExpr::Binding(PATH.into()), IrExpr::Binding(rel)],
                },
            },
        ],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: expand.boxed(),
    };
    apply_edge_subgraph(projected, lo, ctx)
}

/// `outV()`/`inV()`/`bothV()` — project the endpoint vertex of the
/// edge bound to `current`. We lower as a `CurrentProject` whose
/// expression calls a runtime helper that reads `Edge.{src,dst}_label`
/// + id and rebuilds a `Node` value. `bothV()` flattens to a list and
/// then unwinds (semantics: produce both endpoints as separate rows).
pub(super) fn lower_endpoint_vertex(
    input: Node,
    direction: SemDirection,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let node = match direction {
        SemDirection::Out => endpoint_call(input, "edge_src"),
        SemDirection::In => endpoint_call(input, "edge_dst"),
        SemDirection::Both => {
            let project = Node::GraphCurrentProject {
                expr: IrExpr::Call {
                    name: "edge_both".into(),
                    args: vec![IrExpr::Binding(CURRENT.into())],
                },
                fields: vec![CURRENT.to_string()],
                input: input.boxed(),
            };
            // bothV emits one row per endpoint; unwind the 2-element list.
            Node::GraphUnwind {
                input_expr: IrExpr::Binding(CURRENT.into()),
                bind: CURRENT.into(),
                outer: false,
                input: project.boxed(),
            }
        }
    };
    apply_vertex_subgraph(node, lo, ctx)
}

fn endpoint_call(input: Node, helper: &str) -> Node {
    let endpoint = IrExpr::Call {
        name: helper.into(),
        args: vec![IrExpr::Binding(CURRENT.into())],
    };
    Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![
            ProjectionItem {
                alias: CURRENT.into(),
                expr: endpoint.clone(),
            },
            ProjectionItem {
                alias: PATH.into(),
                expr: IrExpr::Call {
                    name: "path_append".into(),
                    args: vec![IrExpr::Binding(PATH.into()), endpoint],
                },
            },
        ],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    }
}

/// `otherV()` — endpoint of the current edge opposite the side we
/// arrived from. Without per-traverser arrival-side tracking we can't
/// know which side was the source. As a pragmatic best-effort, project
/// the *destination* (treats the current edge as if traversed in `out`
/// direction). This is right for the common `out().outE().otherV()`
/// pattern; it's wrong for `in().inE().otherV()` (would need src
/// instead). Cases that actually traverse `inE/bothE` produce the
/// wrong endpoint, but the chain still runs.
pub(super) fn lower_other_vertex(
    input: Node,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let node = Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![
            ProjectionItem {
                alias: CURRENT.into(),
                expr: other_vertex_expr(),
            },
            ProjectionItem {
                alias: PATH.into(),
                expr: IrExpr::Call {
                    name: "path_append".into(),
                    args: vec![IrExpr::Binding(PATH.into()), other_vertex_expr()],
                },
            },
        ],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    };
    apply_vertex_subgraph(node, lo, ctx)
}

fn other_vertex_expr() -> IrExpr {
    IrExpr::Case {
        arms: vec![(
            IrExpr::IsBound(EDGE_OTHER.into()),
            IrExpr::Binding(EDGE_OTHER.into()),
        )],
        otherwise: Some(Box::new(IrExpr::Call {
            name: "edge_dst".into(),
            args: vec![IrExpr::Binding(CURRENT.into())],
        })),
    }
}
