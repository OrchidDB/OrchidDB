//! Labelled-binding steps: `as(label)`, `select(label)`, `select(labels...)`,
//! `select(Column.keys|values)`.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::helpers::{apply_by_spec, consume_by};
use super::side_effects::lower_side_effect_bag_as_list;
use crate::ir::expr::{BinaryOp, IrExpr, Lit};
use crate::ir::plan::{BindKind, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
use crate::language::gremlin::ast::{BySpec, MapColumn, Pop, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) fn lower_as(input: Node, label: &str) -> Node {
    let input = Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: select_history_binding(label),
            expr: IrExpr::Call {
                name: "select_history_append".into(),
                args: vec![
                    IrExpr::Binding(select_history_binding(label)),
                    IrExpr::Binding(CURRENT.into()),
                ],
            },
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    };
    Node::GraphBind {
        bind: label.to_string(),
        kind: BindKind::Node,
        expr: Some(IrExpr::Binding(CURRENT.into())),
        input: input.boxed(),
    }
}

pub(super) fn lower_select_label<'a, I>(
    input: Node,
    label: &str,
    pop: Pop,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    if let Some(input) = lower_side_effect_bag_as_list(input.clone(), label, lo) {
        return Ok(input);
    }
    // `select("a")` only produces a row when "a" is reachable: either as
    // a labelled binding upstream or as a key on the current map-shaped
    // traverser. Filter out rows where neither holds — Gremlin drops
    // them rather than emitting a null traverser.
    let input = filter_label_present(input, label);
    let mut input = replace_current(input, select_expr(CURRENT, label, pop));
    if let Some(spec) = consume_by(steps) {
        if let Some(value_expr) = direct_by_expr(&spec) {
            input = replace_current(input, value_expr);
        } else {
            let (next_input, value_expr) = apply_by_spec(input, &spec, lo, ctx)?;
            input = replace_current(next_input, value_expr);
        }
    }
    Ok(input)
}

pub(super) fn lower_select_multi<'a, I>(
    input: Node,
    labels: &[String],
    pop: Pop,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let mut by_specs = Vec::new();
    while let Some(spec) = consume_by(steps) {
        by_specs.push(spec);
    }

    // Drop rows that can't satisfy *every* requested label — Gremlin
    // multi-`select` filters rather than emitting null entries.
    let mut input = input;
    for label in labels {
        input = filter_label_present(input, label);
    }
    let select_source = lo.fresh("select_source");
    let mut input = Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: select_source.clone(),
            expr: IrExpr::Binding(CURRENT.into()),
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    };
    let mut entries = Vec::with_capacity(labels.len() * 2);
    for (index, label) in labels.iter().enumerate() {
        input = replace_current(input, select_expr(&select_source, label, pop));
        let value_expr = if by_specs.is_empty() {
            IrExpr::Binding(CURRENT.into())
        } else {
            let spec = by_spec_for_label(&by_specs, index);
            let (next_input, value_expr) = apply_by_spec(input, spec, lo, ctx)?;
            input = next_input;
            value_expr
        };
        let value_alias = lo.fresh("select_by");
        input = Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: value_alias.clone(),
                expr: value_expr,
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: input.boxed(),
        };
        entries.push(IrExpr::Lit(Lit::String(label.clone())));
        entries.push(IrExpr::Binding(value_alias));
    }

    Ok(Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "make_map".into(),
            args: entries,
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    })
}

/// `select(Column.keys)` / `select(Column.values)` — extract the
/// keys-or-values list from a map-shaped traverser. We lower as a
/// `CurrentProject` against a runtime helper that reads the current
/// value as a `Value::Map` and emits a `Value::List` of the requested
/// half. Non-map inputs degrade to an empty list.
pub(super) fn lower_select_column(input: Node, column: MapColumn) -> Node {
    let helper = match column {
        MapColumn::Keys => "map_keys",
        MapColumn::Values => "map_values",
    };
    Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: helper.into(),
            args: vec![IrExpr::Binding(CURRENT.into())],
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}

/// Drop rows where `label` resolves to neither an upstream binding nor
/// a key in the current map traverser. Gremlin's `select(label)` filters
/// such rows; without this guard our runtime emits null traversers.
fn filter_label_present(input: Node, label: &str) -> Node {
    let bound = IrExpr::IsBound(label.to_string());
    let history = IrExpr::IsBound(select_history_binding(label));
    let map_has = IrExpr::Call {
        name: "map_has_key".into(),
        args: vec![
            IrExpr::Binding(CURRENT.into()),
            IrExpr::lit_str(label.to_string()),
        ],
    };
    Node::GraphFilter {
        condition: IrExpr::Binary {
            op: BinaryOp::Or,
            lhs: Box::new(IrExpr::Binary {
                op: BinaryOp::Or,
                lhs: Box::new(bound),
                rhs: Box::new(history),
            }),
            rhs: Box::new(map_has),
        },
        input: input.boxed(),
    }
}

fn replace_current(input: Node, expr: IrExpr) -> Node {
    Node::GraphProject {
        mode: ProjectMode::ReplaceCurrent,
        items: vec![ProjectionItem {
            alias: CURRENT.into(),
            expr,
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    }
}

fn select_expr(source_binding: &str, label: &str, pop: Pop) -> IrExpr {
    let pop = match pop {
        Pop::First => "first",
        Pop::Last => "last",
        Pop::All => "all",
        Pop::Mixed => "mixed",
    };
    IrExpr::Call {
        name: "select_key_or_binding_pop".into(),
        args: vec![
            IrExpr::Binding(source_binding.into()),
            IrExpr::Binding(label.to_string()),
            IrExpr::Binding(select_history_binding(label)),
            IrExpr::lit_str(label.to_string()),
            IrExpr::lit_str(pop.to_string()),
        ],
    }
}

fn by_spec_for_label<'a>(specs: &'a [BySpec], index: usize) -> &'a BySpec {
    &specs[index % specs.len()]
}

fn direct_by_expr(spec: &BySpec) -> Option<IrExpr> {
    if spec.traversal.is_some() {
        return None;
    }
    let key = spec.key.as_deref()?;
    let key = key.strip_prefix("T.").unwrap_or(key);
    Some(match key.to_ascii_lowercase().as_str() {
        "id" => IrExpr::Id(CURRENT.into()),
        "label" => IrExpr::Label(CURRENT.into()),
        _ => IrExpr::property(
            CURRENT,
            key.to_string(),
            crate::ir::policy::PropertyMissing::DropUnproductive,
        ),
    })
}

fn select_history_binding(label: &str) -> String {
    format!("__gremlin_select_history_{label}")
}
