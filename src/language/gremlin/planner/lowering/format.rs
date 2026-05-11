//! `format(template)` — substitute `{N}` / `%s` / `%{name}` placeholders
//! with the current scalar (or a named binding for `%{name}`). The
//! lowering builds a single concat call against an interleaved list of
//! literal strings and resolved scalars.

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::helpers::{apply_by_spec, consume_by};
use super::literals::gvalue_to_expr;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{Node, ProjectErrorPolicy, ProjectMode, ProjectionItem};
use crate::ir::policy::PropertyMissing;
use crate::language::gremlin::ast::{FormatPart, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) fn lower_format<'a, I>(
    input: Node,
    parts: &[FormatPart],
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

    let mut input = input;
    let format_source = if by_specs.is_empty() {
        CURRENT.to_string()
    } else {
        let source = lo.fresh("format_source");
        input = Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: source.clone(),
                expr: IrExpr::Binding(CURRENT.into()),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: input.boxed(),
        };
        source
    };
    let mut placeholder_index = 0usize;
    let mut pieces = Vec::with_capacity(parts.len());
    for part in parts {
        let piece = match part {
            FormatPart::Literal(text) => IrExpr::lit_str(text.clone()),
            FormatPart::Placeholder { key } => match key {
                Some(name) if !name.is_empty() && name != "_" => IrExpr::Call {
                    name: "format_placeholder".into(),
                    args: vec![
                        IrExpr::Binding(format_source.clone()),
                        IrExpr::Binding(name.clone()),
                        IrExpr::lit_str(name.clone()),
                    ],
                },
                _ if !by_specs.is_empty() => {
                    let spec = &by_specs[placeholder_index % by_specs.len()];
                    placeholder_index += 1;
                    if let Some(value_expr) = direct_by_expr(spec, &format_source)? {
                        value_expr
                    } else {
                        let (next_input, value_expr) = apply_by_spec(input, spec, lo, ctx)?;
                        input = next_input;
                        value_expr
                    }
                }
                _ => IrExpr::Binding(CURRENT.into()),
            },
        };
        pieces.push(piece);
    }

    Ok(Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "format_concat".into(),
            args: pieces,
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    })
}

fn direct_by_expr(
    spec: &crate::language::gremlin::ast::BySpec,
    source_binding: &str,
) -> GremlinPlanResult<Option<IrExpr>> {
    if let Some(key) = &spec.key {
        return Ok(Some(token_or_property_expr(source_binding, key)));
    }
    let Some(traversal) = &spec.traversal else {
        return Ok(Some(IrExpr::Binding(CURRENT.into())));
    };
    match traversal.as_slice() {
        [Step::Values(keys)] if keys.len() == 1 => {
            Ok(Some(token_or_property_expr(source_binding, &keys[0])))
        }
        [Step::Id] => Ok(Some(IrExpr::Id(source_binding.to_string()))),
        [Step::Label] => Ok(Some(IrExpr::Label(source_binding.to_string()))),
        [Step::Constant(value)] => Ok(Some(gvalue_to_expr(value)?)),
        [Step::Identity] => Ok(Some(IrExpr::Binding(source_binding.to_string()))),
        _ => Ok(None),
    }
}

fn token_or_property_expr(binding: &str, key: &str) -> IrExpr {
    match key {
        "id" => IrExpr::Id(binding.to_string()),
        "label" => IrExpr::Label(binding.to_string()),
        other => IrExpr::property(
            binding,
            other.to_string(),
            PropertyMissing::DropUnproductive,
        ),
    }
}
