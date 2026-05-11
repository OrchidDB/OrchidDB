//! IrExpr router — `eval` traverses a value-level expression tree.
//! Heavy work (arithmetic, comparison, runtime function calls,
//! property-object projection) lives in the sister modules.

mod arithmetic;
mod binary;
mod compare;
mod path_predicate;

pub(crate) use arithmetic::arithmetic;
pub(crate) use binary::eval_binary;
pub(crate) use compare::compare_values;
pub(crate) use path_predicate::is_simple_path;

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::{IrExpr, Lit, StringOp};
use crate::ir::policy::PropertyMissing;
use crate::ir::value::Value;

use super::Row;
use super::runtime::{algorithm_property, eval_call};
use super::{InterpretError, IrResult};

pub fn eval(expr: &IrExpr, row: &Row, graph: &PropertyGraph) -> IrResult<Value> {
    match expr {
        IrExpr::Lit(lit) => Ok(match lit {
            Lit::Null => Value::Null,
            Lit::Bool(b) => Value::Bool(*b),
            Lit::Int(n) => Value::Int(*n),
            Lit::Float(f) => Value::Float(*f),
            Lit::String(s) => Value::String(s.clone()),
        }),
        IrExpr::Binding(name) => Ok(row.get(name)),
        IrExpr::Property {
            binding,
            name,
            policy,
        } => {
            let value = row.bindings.get(binding).cloned().unwrap_or(Value::Null);
            let resolved = match value {
                Value::Node { label, id } => {
                    let stored = graph.node_property(&label, id, name);
                    if matches!(stored, Value::Null) {
                        algorithm_property(graph, &Value::Node { label, id }, name)
                            .unwrap_or(Value::Null)
                    } else {
                        stored
                    }
                }
                Value::Edge { rel_type, id, .. } => graph.edge_property(&rel_type, id, name),
                Value::Map(map) => map.get(name).cloned().unwrap_or(Value::Null),
                Value::Null => Value::Null,
                // Non-element values (Int/Bool/String/...) under a
                // `property(...)` access — return null. This shows up
                // when sub-traversals like `local(values("x"))` re-feed
                // a scalar into a property-projection context.
                _ => Value::Null,
            };
            // `DropUnproductive` is signalled by returning Value::Null; the
            // owning `CurrentProject` operator removes the row. For
            // `NullOnMissing`, null is the answer. For `Error`, we fail.
            match policy {
                PropertyMissing::Error if matches!(resolved, Value::Null) => {
                    Err(InterpretError::Type(format!(
                        "property `{name}` missing for binding `{binding}`"
                    )))
                }
                _ => Ok(resolved),
            }
        }
        IrExpr::Id(binding) => match row.bindings.get(binding) {
            Some(Value::Node { id, .. }) | Some(Value::Edge { id, .. }) => Ok(Value::Int(*id)),
            _ => Ok(Value::Null),
        },
        IrExpr::Label(binding) => match row.bindings.get(binding) {
            Some(Value::Node { label, .. }) => Ok(Value::String(label.clone())),
            Some(Value::Edge { rel_type, .. }) => Ok(Value::String(rel_type.clone())),
            _ => Ok(Value::Null),
        },
        IrExpr::HasLabel { binding, label } => match row.bindings.get(binding) {
            Some(Value::Node { label: l, .. }) => Ok(Value::Bool(l == label)),
            Some(Value::Edge { rel_type, .. }) => Ok(Value::Bool(rel_type == label)),
            _ => Ok(Value::Null),
        },
        IrExpr::Binary { op, lhs, rhs } => eval_binary(*op, lhs, rhs, row, graph),
        IrExpr::Not(inner) => match eval(inner, row, graph)? {
            Value::Bool(b) => Ok(Value::Bool(!b)),
            Value::Null => Ok(Value::Null),
            other => Err(InterpretError::Type(format!(
                "not on {}",
                other.type_name()
            ))),
        },
        IrExpr::StringPredicate {
            op,
            target,
            pattern,
        } => {
            let target = eval(target, row, graph)?;
            let pattern = eval(pattern, row, graph)?;
            match (target, pattern) {
                (Value::String(t), Value::String(p)) => Ok(Value::Bool(match op {
                    StringOp::StartsWith => t.starts_with(&p),
                    StringOp::EndsWith => t.ends_with(&p),
                    StringOp::Contains => t.contains(&p),
                })),
                (Value::Null, _) | (_, Value::Null) => Ok(Value::Null),
                (a, b) => Err(InterpretError::Type(format!(
                    "string predicate on {} / {}",
                    a.type_name(),
                    b.type_name()
                ))),
            }
        }
        IrExpr::IsNull(inner) => Ok(Value::Bool(matches!(eval(inner, row, graph)?, Value::Null))),
        IrExpr::IsNotNull(inner) => Ok(Value::Bool(!matches!(
            eval(inner, row, graph)?,
            Value::Null
        ))),
        IrExpr::IsBound(name) => Ok(Value::Bool(matches!(
            row.bindings.get(name),
            Some(v) if !matches!(v, Value::Null)
        ))),
        IrExpr::SimplePath(name) => match row.bindings.get(name) {
            Some(Value::Path(items)) => Ok(Value::Bool(is_simple_path(items))),
            _ => Ok(Value::Bool(true)),
        },
        IrExpr::List(items) => {
            let evaluated = items
                .iter()
                .map(|item| eval(item, row, graph))
                .collect::<IrResult<Vec<_>>>()?;
            Ok(Value::List(evaluated))
        }
        IrExpr::Call { name, args } => {
            let evaluated = args
                .iter()
                .map(|item| eval(item, row, graph))
                .collect::<IrResult<Vec<_>>>()?;
            eval_call(name, evaluated, graph)
        }
        IrExpr::Case { arms, otherwise } => {
            for (cond, value) in arms {
                if matches!(eval(cond, row, graph)?, Value::Bool(true)) {
                    return eval(value, row, graph);
                }
            }
            match otherwise {
                Some(expr) => eval(expr, row, graph),
                None => Ok(Value::Null),
            }
        }
    }
}
