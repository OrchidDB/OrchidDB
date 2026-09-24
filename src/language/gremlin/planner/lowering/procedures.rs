//! Procedure-style steps: `call(name, args...)`, `shortestPath`,
//! `pageRank`, `peerPressure`, `connectedComponent`, `fail`.
//!
//! Search is a graph-backed start service. Degree centrality and shortest
//! path use graph execution helpers; other algorithm/procedure names retain
//! their existing unsupported/placeholder behavior.

use super::context::{CURRENT, ChildTraversalKind, Lowerer, TraversalContext};
use super::literals::gvalue_to_expr;
use super::sub_traversal::lower_child_traversal;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{
    ApplyKind, Direction, LabelExpr, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice,
};
use crate::ir::policy::OptionalMissing;
use crate::ir::value::Value;
use crate::language::gremlin::ast::{CallArg, Step};
use crate::language::gremlin::planner::error::{GremlinPlanError, GremlinPlanResult};
use crate::language::gremlin::semantics::{
    CompareOp, Direction as GremlinDirection, GValue, Predicate,
};

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
    if key
        .rsplit('.')
        .next()
        .is_some_and(|suffix| suffix == "edges")
    {
        if let Some(direction) = shortest_path_edges_direction(value) {
            return Ok(Some(apply_shortest_path_direction(input, direction)));
        }
    }
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

fn shortest_path_edges_direction(value: Option<&GValue>) -> Option<Direction> {
    let Some(GValue::String(value)) = value else {
        return None;
    };
    let value = value
        .rsplit('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    if value.contains("OUT") {
        Some(Direction::Out)
    } else if value.contains("IN") {
        Some(Direction::In)
    } else if value.contains("BOTH") {
        Some(Direction::Both)
    } else {
        None
    }
}

fn apply_shortest_path_direction(input: Node, direction: Direction) -> Node {
    match input {
        Node::GraphShortestPath {
            source,
            target,
            rel_types,
            max_distance,
            include_edges,
            output,
            all_paths,
            input,
            ..
        } => Node::GraphShortestPath {
            source,
            target,
            direction,
            rel_types,
            max_distance,
            include_edges,
            output,
            all_paths,
            input,
        },
        other => other,
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
    input: Node,
    name: &'static str,
    options: &[CallOption],
) -> GremlinPlanResult<Node> {
    if name == "shortestPath" {
        let mut direction = Direction::Both;
        let mut rel_types = LabelExpr::Any;
        let mut max_distance = None;
        let mut include_edges = false;
        let mut weighted_distance = false;
        let mut target_condition = None;
        for option in options {
            let suffix = option.key.rsplit('.').next().unwrap_or(&option.key);
            match suffix {
                "edges" => {
                    if let Some(steps) = option.traversal.as_deref() {
                        if let Some((next_direction, next_rel_types)) =
                            shortest_path_edges_traversal(steps)
                        {
                            direction = next_direction;
                            rel_types = next_rel_types;
                            continue;
                        }
                    }
                    if let Some(next_direction) =
                        shortest_path_edges_direction(option.value.as_ref())
                    {
                        direction = next_direction;
                    }
                    if let Some(next_rel_types) = shortest_path_edge_labels(option.value.as_ref()) {
                        rel_types = next_rel_types;
                    }
                }
                "maxDistance" => {
                    if !weighted_distance {
                        max_distance = shortest_path_distance(option.value.as_ref());
                    }
                }
                "includeEdges" => include_edges = true,
                "distance" => {
                    weighted_distance = true;
                    max_distance = None;
                }
                "target" => {
                    if target_condition.is_none() {
                        target_condition = option
                            .traversal
                            .as_deref()
                            .and_then(shortest_path_target_condition);
                    }
                }
                _ => {}
            }
        }
        let shortest = Node::GraphShortestPath {
            source: CURRENT.into(),
            target: None,
            direction,
            rel_types,
            max_distance,
            include_edges,
            output: CURRENT.into(),
            all_paths: true,
            input: input.boxed(),
        };
        return Ok(match target_condition {
            Some(condition) => Node::GraphFilter {
                condition,
                input: shortest.boxed(),
            },
            None => shortest,
        });
    }

    Ok(input)
}

fn shortest_path_edges_traversal(steps: &[Step]) -> Option<(Direction, LabelExpr)> {
    let [
        Step::ExpandEdge {
            direction,
            edge_labels,
        },
    ] = steps
    else {
        return None;
    };
    let direction = match direction {
        GremlinDirection::Out => Direction::Out,
        GremlinDirection::In => Direction::In,
        GremlinDirection::Both => Direction::Both,
    };
    let labels = if edge_labels.is_empty() {
        LabelExpr::Any
    } else {
        LabelExpr::AnyOf(edge_labels.to_vec())
    };
    Some((direction, labels))
}

fn shortest_path_target_condition(steps: &[Step]) -> Option<IrExpr> {
    match steps {
        [Step::Has { key, predicate }] => {
            let value = predicate_eq_value(predicate)?;
            Some(IrExpr::Call {
                name: "path_last_property_eq".into(),
                args: vec![
                    IrExpr::Binding(CURRENT.into()),
                    IrExpr::lit_str(key.clone()),
                    gvalue_to_expr(value).ok()?,
                ],
            })
        }
        [Step::HasLabel(labels)] if labels.len() == 1 => Some(IrExpr::Call {
            name: "path_last_label_eq".into(),
            args: vec![
                IrExpr::Binding(CURRENT.into()),
                IrExpr::lit_str(labels[0].clone()),
            ],
        }),
        [Step::Values(keys), Step::Is { predicate }] if keys.len() == 1 => {
            let value = predicate_eq_value(predicate)?;
            Some(IrExpr::Call {
                name: "path_last_property_eq".into(),
                args: vec![
                    IrExpr::Binding(CURRENT.into()),
                    IrExpr::lit_str(keys[0].clone()),
                    gvalue_to_expr(value).ok()?,
                ],
            })
        }
        _ => None,
    }
}

fn predicate_eq_value(predicate: &Predicate) -> Option<&GValue> {
    match predicate {
        Predicate::Compare {
            op: CompareOp::Eq,
            value,
        } => Some(value),
        _ => None,
    }
}

fn shortest_path_distance(value: Option<&GValue>) -> Option<f64> {
    match value {
        Some(GValue::Int(value) | GValue::Long(value)) => Some(*value as f64),
        Some(GValue::Byte(value)) => Some(*value as f64),
        Some(GValue::Short(value)) => Some(*value as f64),
        Some(GValue::Float32(value)) => Some(*value as f64),
        Some(GValue::Float(value)) => Some(*value),
        _ => None,
    }
}

fn shortest_path_edge_labels(value: Option<&GValue>) -> Option<LabelExpr> {
    let Some(GValue::String(value)) = value else {
        return None;
    };
    if !value.contains("edge_labels") {
        return None;
    }
    let labels = quoted_strings(value);
    (!labels.is_empty()).then_some(LabelExpr::AnyOf(labels))
}

fn quoted_strings(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find('"') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('"') else {
            break;
        };
        out.push(after_start[..end].to_string());
        rest = &after_start[end + 1..];
    }
    out
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
