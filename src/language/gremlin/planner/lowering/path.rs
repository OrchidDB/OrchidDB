//! `path()` / `path().from(label)` / `path().to(label)` projections.
//!
//! The expand operators already accumulate the visited objects under
//! the synthetic `__path` binding (see `interpreter::expand_op`). So
//! `path()` lowers to a `GraphCurrentProject` that pulls that binding
//! into `current`. When the chain has no expansions yet, `__path` is
//! absent — we fall back to a singleton list containing `current`.
//!
//! `pathFrom`/`pathTo` slice the path between labelled bindings; we
//! lower them by passing the label name to a runtime helper that
//! locates the binding inside the path. Cases the helper can't satisfy
//! degrade to the full path (closer to right than empty).

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, PATH};
use crate::ir::expr::IrExpr;
use crate::ir::plan::Node;
use crate::language::gremlin::ast::{Step, StringOp};

pub(super) fn lower_path<'a, I>(input: Node, steps: &mut Peekable<I>, lo: &Lowerer) -> Node
where
    I: Iterator<Item = &'a Step>,
{
    let mut slices = Vec::new();
    while let Some(Step::PathFrom(_) | Step::PathTo(_)) = steps.peek() {
        match steps.next() {
            Some(Step::PathFrom(label)) => slices.push(("path_from", label.clone())),
            Some(Step::PathTo(label)) => slices.push(("path_to", label.clone())),
            _ => break,
        }
    }
    let mut by_keys = Vec::new();
    while let Some(Step::By(_)) = steps.peek() {
        if let Some(Step::By(spec)) = steps.next() {
            by_keys.push(path_by_token(spec));
        }
    }
    let path = Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "path_or_self".into(),
            args: vec![
                IrExpr::Binding(PATH.into()),
                IrExpr::Binding(CURRENT.into()),
            ],
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    };
    let path = slices.into_iter().fold(path, |node, (helper, label)| {
        slice_path(node, helper, &label)
    });
    let keys = by_keys
        .into_iter()
        .map(|key| {
            key.map(IrExpr::lit_str)
                .unwrap_or(IrExpr::Lit(crate::ir::expr::Lit::Null))
        })
        .collect::<Vec<_>>();
    if keys.is_empty() {
        path
    } else {
        Node::GraphCurrentProject {
            expr: IrExpr::Call {
                name: if lo.productive_by {
                    "path_by_keys_keep_nulls".into()
                } else {
                    "path_by_keys".into()
                },
                args: vec![IrExpr::Binding(CURRENT.into()), IrExpr::List(keys)],
            },
            fields: vec![CURRENT.to_string()],
            input: path.boxed(),
        }
    }
}

fn path_by_token(spec: &crate::language::gremlin::ast::BySpec) -> Option<String> {
    if let Some(key) = &spec.key {
        if key == "label" {
            return Some("__label".to_string());
        }
        return Some(key.clone());
    }
    match spec.traversal.as_deref()? {
        [Step::Values(keys)] if keys.len() == 1 => Some(keys[0].clone()),
        [Step::Values(keys), Step::StringOp(StringOp::ToUpper)] if keys.len() == 1 => {
            Some(format!("__upper:{}", keys[0]))
        }
        [Step::Values(keys), Step::StringOp(StringOp::ToLower)] if keys.len() == 1 => {
            Some(format!("__lower:{}", keys[0]))
        }
        [Step::Label] => Some("__label".to_string()),
        [Step::Id] => Some("__id".to_string()),
        _ => None,
    }
}

pub(super) fn lower_path_from(input: Node, label: &str) -> Node {
    slice_path(input, "path_from", label)
}

pub(super) fn lower_path_to(input: Node, label: &str) -> Node {
    slice_path(input, "path_to", label)
}

fn slice_path(input: Node, helper: &str, label: &str) -> Node {
    Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: helper.into(),
            args: vec![
                IrExpr::Binding(CURRENT.into()),
                IrExpr::lit_str(label.to_string()),
                IrExpr::Binding(label.to_string()),
            ],
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}
