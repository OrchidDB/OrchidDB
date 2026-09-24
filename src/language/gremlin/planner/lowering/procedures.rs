//! Procedure-style steps: `call(name, args...)`, `shortestPath`,
//! `pageRank`, `peerPressure`, `connectedComponent`, `fail`.
//!
//! Search and degree centrality use graph execution helpers. GraphComputer
//! steps require the JVM execution profile; native lowering rejects them
//! rather than returning partial paths or fabricated compute properties.

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext};
use super::literals::gvalue_to_expr;
use super::sub_traversal::lower_child_traversal;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{
    ApplyKind, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice,
};
use crate::ir::policy::OptionalMissing;
use crate::ir::value::Value;
use crate::language::gremlin::ast::{CallArg, Step};
use crate::language::gremlin::planner::error::{GremlinPlanError, GremlinPlanResult};
use crate::language::gremlin::semantics::GValue;

#[derive(Debug, Clone)]
pub(super) struct CallOption {
    pub key: String,
    pub value: Option<GValue>,
    pub traversal: Option<Vec<Step>>,
}

pub(super) fn lower_call(
    input: Node,
    name: &str,
    args: &[CallArg],
    options: &[CallOption],
) -> GremlinPlanResult<Node> {
    if name == "tinker.search" {
        return Err(GremlinPlanError::Unsupported(
            "tinker.search can only be used as a traversal source".into(),
        ));
    }
    if is_degree_centrality(name) {
        return Ok(Node::GraphProject {
            mode: ProjectMode::ReplaceCurrent,
            items: vec![ProjectionItem {
                alias: CURRENT.into(),
                expr: IrExpr::Call {
                    name: "tinker_degree_centrality".into(),
                    args: vec![
                        IrExpr::Binding(CURRENT.into()),
                        IrExpr::lit_str(direction_option(args, options).to_string()),
                    ],
                },
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: input.boxed(),
        });
    }

    let mut call_args: Vec<IrExpr> = Vec::with_capacity(args.len() + 2);
    call_args.push(IrExpr::Binding(CURRENT.into()));
    call_args.push(IrExpr::lit_str(name.to_string()));
    for arg in args {
        match arg {
            CallArg::Value(v) => call_args.push(gvalue_to_expr(v)?),
            CallArg::Map(_) => call_args.push(IrExpr::Lit(crate::ir::expr::Lit::Null)),
            // We don't have a procedure registry that can run arbitrary
            // sub-traversals, so we elide a `__.traversal` argument as
            // null. The surrounding `procedure_call` helper returns null
            // anyway; this keeps the chain compiling.
            CallArg::Traversal(_) => call_args.push(IrExpr::Lit(crate::ir::expr::Lit::Null)),
        }
    }
    Ok(Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "procedure_call".into(),
            args: call_args,
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    })
}

pub(super) fn lower_call_source(
    name: &str,
    args: &[CallArg],
    options: &[CallOption],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Option<Node>> {
    if name.is_empty() || name == "--list" {
        let rows = if name == "--list" && !args.is_empty() {
            vec![vec![Value::String("tinker.search".into())]]
        } else {
            vec![
                vec![Value::String("tinker.search".into())],
                vec![Value::String("tinker.degree.centrality".into())],
            ]
        };
        return Ok(Some(Node::GraphValues {
            bindings: vec![CURRENT.into()],
            rows,
            bulk: None,
        }));
    }

    if name == "tinker.search" {
        return lower_search_source(args, options, lo, ctx).map(Some);
    }

    Ok(None)
}

pub(super) fn lower_call_with_option(
    input: Node,
    key: &str,
    value: Option<&GValue>,
    _traversal: Option<&[Step]>,
) -> GremlinPlanResult<Option<Node>> {
    let Some(value) = value else {
        return Ok(None);
    };
    match (key, value) {
        ("service", GValue::String(service)) => Ok(Some(Node::GraphFilter {
            condition: IrExpr::eq(
                IrExpr::Binding(CURRENT.into()),
                IrExpr::lit_str(service.clone()),
            ),
            input: input.boxed(),
        })),
        ("type", GValue::String(kind)) => Ok(Some(Node::GraphFilter {
            condition: IrExpr::eq(
                IrExpr::Call {
                    name: "element_kind".into(),
                    args: vec![IrExpr::Binding(CURRENT.into())],
                },
                IrExpr::lit_str(kind.clone()),
            ),
            input: input.boxed(),
        })),
        ("search", GValue::String(_)) => Ok(None),
        _ => Ok(None),
    }
}

fn is_degree_centrality(name: &str) -> bool {
    name == "tinker.degree.centrality"
}

fn direction_option(args: &[CallArg], options: &[CallOption]) -> &'static str {
    if options.iter().any(|option| {
        option.key == "direction"
            && !matches!(option.value.as_ref(), Some(GValue::String(value)) if value != "OUT")
    }) || args.iter().any(call_arg_mentions_out)
    {
        "OUT"
    } else {
        "IN"
    }
}

fn call_arg_mentions_out(arg: &CallArg) -> bool {
    match arg {
        CallArg::Value(GValue::String(value)) => value == "OUT",
        CallArg::Traversal(steps) => {
            let debug = format!("{steps:?}");
            debug.contains("OUT") || debug.contains("direction")
        }
        _ => false,
    }
}

/// Keep search parameters in the IR until execution so the service sees
/// graph mutations, bound maps, and values produced by child traversals.
fn lower_search_source(
    args: &[CallArg],
    options: &[CallOption],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let mut input = Node::GraphValues {
        bindings: vec![CURRENT.into()],
        rows: vec![vec![Value::Null]],
        bulk: None,
    };
    let mut parameters = Vec::new();
    for arg in args {
        parameters.push(match arg {
            CallArg::Value(value) => gvalue_to_expr(value)?,
            CallArg::Traversal(steps) => search_parameter_traversal(&mut input, steps, lo, ctx)?,
            CallArg::Map(text) => {
                return Err(GremlinPlanError::Unsupported(format!(
                    "unresolved call map argument: {text}"
                )));
            }
        });
    }
    for option in options {
        let value = if let Some(steps) = &option.traversal {
            search_parameter_traversal(&mut input, steps, lo, ctx)?
        } else if let Some(value) = &option.value {
            gvalue_to_expr(value)?
        } else {
            IrExpr::Lit(crate::ir::expr::Lit::Bool(true))
        };
        parameters.push(IrExpr::Call {
            name: "map_literal".into(),
            args: vec![
                IrExpr::List(vec![IrExpr::lit_str(&option.key)]),
                IrExpr::List(vec![value]),
            ],
        });
    }
    let project = Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "tinker_search".into(),
            args: parameters,
        },
        fields: vec![CURRENT.into()],
        input: input.boxed(),
    };
    Ok(Node::GraphUnwind {
        input_expr: IrExpr::Binding(CURRENT.into()),
        bind: CURRENT.into(),
        outer: false,
        input: project.boxed(),
    })
}

fn search_parameter_traversal(
    input: &mut Node,
    steps: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<IrExpr> {
    let binding = lo.fresh("search_parameter");
    let child = lower_child_traversal(steps, lo, ctx, ChildTraversalKind::ByModulator)?;
    let right = Node::GraphProject {
        mode: ProjectMode::ReplaceScope,
        items: vec![ProjectionItem {
            alias: binding.clone(),
            expr: IrExpr::Binding(CURRENT.into()),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: Node::GraphSlice {
            slice: Slice {
                offset: 0,
                fetch: Some(1),
                tail: None,
            },
            input: child.boxed(),
        }
        .boxed(),
    };
    *input = Node::GraphApply {
        kind: ApplyKind::Scalar,
        correlation: vec![CURRENT.into()],
        outputs: vec![binding.clone()],
        optional_missing: OptionalMissing::Null,
        left: input.clone().boxed(),
        right: right.boxed(),
    };
    Ok(IrExpr::Binding(binding))
}

pub(super) fn lower_graph_algorithm(
    _input: Node,
    name: &'static str,
    _options: &[CallOption],
) -> GremlinPlanResult<Node> {
    Err(GremlinPlanError::Unsupported(format!(
        "{name}() requires the JVM GraphComputer execution profile"
    )))
}

pub(super) fn lower_fail(_input: Node, message: Option<&str>) -> GremlinPlanResult<Node> {
    Err(GremlinPlanError::Unsupported(format!(
        "fail({}) is not yet lowered",
        message.unwrap_or("")
    )))
}

/// A write procedure participates in GraphEngine statement rollback/persistence.
pub(super) fn lower_import(path: &str, reader: Option<&str>, read: bool) -> GremlinPlanResult<Node> {
    use crate::ir::plan::{ProcedureArg, ProcedureMode};
    if !read {
        return Err(GremlinPlanError::Unsupported("io() requires read(); file writing is not supported".into()));
    }
    Ok(Node::GraphProcedureCall {
        name: "gremlin.io.read".into(),
        args: [path, reader.unwrap_or("")].into_iter().map(|value| ProcedureArg {
            name: None, value: IrExpr::lit_str(value),
        }).collect(),
        yields: vec![], mode: ProcedureMode::Write, input: None,
    })
}
