//! Per-element string operations applied to the current scalar
//! projection. Each variant maps to an IR `Call` against a runtime
//! UDF (see `ir::interpreter::eval_call`).

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext};
use super::sub_traversal::lower_child_traversal;
use crate::ir::expr::{IrExpr, Lit};
use crate::ir::plan::{ApplyKind, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice};
use crate::ir::policy::OptionalMissing;
use crate::language::gremlin::ast::StringOp as AstStringOp;
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) fn lower_string_op(
    input: Node,
    op: &AstStringOp,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let arg = || IrExpr::Binding(CURRENT.into());
    match op {
        AstStringOp::Length => Ok(project_call(input, "length", vec![arg()])),
        AstStringOp::ToLower => Ok(project_call(input, "lcase", vec![arg()])),
        AstStringOp::ToUpper => Ok(project_call(input, "ucase", vec![arg()])),
        AstStringOp::Trim => Ok(project_call(input, "trim", vec![arg()])),
        AstStringOp::LTrim => Ok(project_call(input, "ltrim", vec![arg()])),
        AstStringOp::RTrim => Ok(project_call(input, "rtrim", vec![arg()])),
        AstStringOp::Reverse => Ok(project_call(input, "reverse", vec![arg()])),
        AstStringOp::Substring { start, end } => {
            let mut args = vec![arg(), IrExpr::Lit(Lit::Int(*start))];
            if let Some(end) = end {
                args.push(IrExpr::Lit(Lit::Int(*end)));
            }
            Ok(project_call(input, "gremlin_substring", args))
        }
        AstStringOp::Replace { old, new } => Ok(project_call(
            input,
            "replace",
            vec![
                arg(),
                IrExpr::lit_str(old.clone()),
                IrExpr::lit_str(new.clone()),
            ],
        )),
        AstStringOp::Concat(suffix) => {
            let suffix_expr = IrExpr::lit_str(suffix.clone());
            let expr = if suffix.is_empty() {
                IrExpr::Call {
                    name: "concat".into(),
                    args: vec![arg(), suffix_expr],
                }
            } else {
                IrExpr::Case {
                    arms: vec![(IrExpr::IsNull(Box::new(arg())), suffix_expr.clone())],
                    otherwise: Some(Box::new(IrExpr::Call {
                        name: "concat".into(),
                        args: vec![arg(), suffix_expr],
                    })),
                }
            };
            Ok(project_expr(input, expr))
        }
        AstStringOp::Conjoin(delim) => Ok(project_call(
            input,
            "conjoin",
            vec![arg(), IrExpr::lit_str(delim.clone())],
        )),
        AstStringOp::Split(delim) => Ok(project_call(
            input,
            "split",
            vec![
                arg(),
                delim
                    .clone()
                    .map(IrExpr::lit_str)
                    .unwrap_or(IrExpr::Lit(Lit::Null)),
            ],
        )),
        AstStringOp::ConcatTraversal(sub) => lower_concat_traversal(input, sub, lo, ctx),
    }
}

fn project_call(input: Node, name: &str, args: Vec<IrExpr>) -> Node {
    project_expr(
        input,
        IrExpr::Call {
            name: name.to_string(),
            args,
        },
    )
}

fn project_expr(input: Node, expr: IrExpr) -> Node {
    Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![ProjectionItem {
            alias: CURRENT.to_string(),
            expr,
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    }
}

/// `concat(__.t)` — run the sub-traversal per input row, project the
/// concatenation `current ++ sub_result` into `current`.
///
/// Plan shape:
///   Apply Inner( left, project(probe := first sub_result) )
///   then  CurrentProject( concat(current, probe) )
///
/// We then drop `probe` by collapsing back into `current`.
fn lower_concat_traversal(
    input: Node,
    sub: &[crate::language::gremlin::ast::Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let probe = lo.fresh("concat_rhs");
    let sub_node = Node::GraphSlice {
        slice: Slice {
            offset: 0,
            fetch: Some(1),
            tail: None,
        },
        input: lower_child_traversal(sub, lo, ctx, ChildTraversalKind::StringRhs)?.boxed(),
    };
    let projected_probe = Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: probe.clone(),
            expr: IrExpr::Binding(CURRENT.into()),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: sub_node.boxed(),
    };
    let applied = Node::GraphApply {
        kind: ApplyKind::Inner,
        correlation: Vec::new(),
        outputs: vec![probe.clone()],
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: projected_probe.boxed(),
    };
    Ok(Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![ProjectionItem {
            alias: CURRENT.to_string(),
            expr: IrExpr::Call {
                name: "concat".into(),
                args: vec![IrExpr::Binding(CURRENT.into()), IrExpr::Binding(probe)],
            },
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: applied.boxed(),
    })
}
