//! IrExpr router — `eval` traverses a value-level expression tree.
//! Heavy work (arithmetic, comparison, runtime function calls,
//! property-object projection) lives in the sister modules.

mod arithmetic;
mod binary;
mod compare;
mod path_predicate;

pub(crate) use arithmetic::{arithmetic, modulo};
pub(crate) use binary::eval_binary;
pub(crate) use compare::compare_values;
pub(crate) use path_predicate::is_simple_path;

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::{IrExpr, Lit, StringOp};
use crate::ir::policy::PropertyMissing;
use crate::ir::value::Value;

use super::Row;
use super::runtime::{algorithm_property, eval_call, runtime_list};
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
                Value::Node { .. } | Value::Edge { .. } | Value::InternalId { .. }
                    if matches!(
                        name.as_str(),
                        "_id" | "_ID" | "_label" | "_LABEL" | "_src" | "_SRC" | "_dst" | "_DST"
                    ) =>
                {
                    super::runtime::graph_element_property(graph, &value, name)
                }
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
                // Cypher string predicates on non-string operands yield
                // null (three-valued logic) rather than raising.
                (_, _) => Ok(Value::Null),
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
        IrExpr::ListReduce {
            collection,
            accumulator,
            item,
            map,
        } => {
            let source = eval(collection, row, graph)?;
            let items = match runtime_list(&source) {
                Some(items) => items,
                None if matches!(source, Value::Null) => return Ok(Value::Null),
                None => vec![source],
            };
            let mut iter = items.into_iter();
            let Some(mut acc) = iter.next() else {
                return Err(InterpretError::Type(
                    "Cannot execute list_reduce on an empty list".to_string(),
                ));
            };
            for value in iter {
                let mut scratch = row.clone();
                scratch.bindings.insert(accumulator.clone(), acc);
                scratch.bindings.insert(item.clone(), value);
                acc = eval(map, &scratch, graph)?;
            }
            Ok(acc)
        }
        IrExpr::ListTransform { list, item, map } => {
            let source = eval(list, row, graph)?;
            let items = match runtime_list(&source) {
                Some(items) => items,
                None if matches!(source, Value::Null) => return Ok(Value::Null),
                None => {
                    return Err(InterpretError::Type(format!(
                        "list_transform expects a list, got {}",
                        source.type_name()
                    )));
                }
            };
            let mut out = Vec::with_capacity(items.len());
            for value in items {
                let mut scratch = row.clone();
                scratch.bindings.insert(item.clone(), value);
                out.push(eval(map, &scratch, graph)?);
            }
            Ok(Value::List(out))
        }
        IrExpr::ListFilter {
            list,
            item,
            predicate,
        } => {
            let source = eval(list, row, graph)?;
            let items = match runtime_list(&source) {
                Some(items) => items,
                None if matches!(source, Value::Null) => return Ok(Value::Null),
                None => {
                    return Err(InterpretError::Type(format!(
                        "list_filter expects a list, got {}",
                        source.type_name()
                    )));
                }
            };
            let mut out = Vec::with_capacity(items.len());
            for value in items {
                let mut scratch = row.clone();
                scratch.bindings.insert(item.clone(), value.clone());
                match eval(predicate, &scratch, graph)? {
                    Value::Bool(true) => out.push(value),
                    Value::Bool(false) | Value::Null => {}
                    other => {
                        return Err(InterpretError::Type(format!(
                            "list_filter predicate must be boolean, got {}",
                            other.type_name()
                        )));
                    }
                }
            }
            Ok(Value::List(out))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::expr::BinaryOp;
    use crate::ir::interpreter::Row;

    fn empty_graph() -> PropertyGraph {
        PropertyGraph::new()
    }

    fn list_lit(items: Vec<i64>) -> IrExpr {
        IrExpr::List(items.into_iter().map(IrExpr::lit_int).collect())
    }

    fn add_binding(a: &str, b: i64) -> IrExpr {
        IrExpr::Binary {
            op: BinaryOp::Add,
            lhs: Box::new(IrExpr::Binding(a.to_string())),
            rhs: Box::new(IrExpr::lit_int(b)),
        }
    }

    #[test]
    fn list_transform_evaluates_lazily() {
        let expr = IrExpr::ListTransform {
            list: Box::new(list_lit(vec![1, 2, 3])),
            item: "x".to_string(),
            map: Box::new(add_binding("x", 10)),
        };
        let row = Row::new();
        let graph = empty_graph();
        let value = eval(&expr, &row, &graph).unwrap();
        assert_eq!(
            value,
            Value::List(vec![Value::Int(11), Value::Int(12), Value::Int(13)])
        );
    }

    #[test]
    fn list_transform_returns_null_on_null_input() {
        let expr = IrExpr::ListTransform {
            list: Box::new(IrExpr::Lit(Lit::Null)),
            item: "x".to_string(),
            map: Box::new(IrExpr::Binding("x".to_string())),
        };
        let row = Row::new();
        let graph = empty_graph();
        let value = eval(&expr, &row, &graph).unwrap();
        assert!(matches!(value, Value::Null));
    }

    #[test]
    fn list_filter_drops_null_predicate_results() {
        // predicate: x = NULL -> Value::Null for every row.
        let predicate = IrExpr::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(IrExpr::Binding("x".to_string())),
            rhs: Box::new(IrExpr::Lit(Lit::Null)),
        };
        let expr = IrExpr::ListFilter {
            list: Box::new(list_lit(vec![1, 2, 3])),
            item: "x".to_string(),
            predicate: Box::new(predicate),
        };
        let row = Row::new();
        let graph = empty_graph();
        let value = eval(&expr, &row, &graph).unwrap();
        assert_eq!(value, Value::List(vec![]));
    }

    #[test]
    fn list_filter_returns_null_on_null_input() {
        let expr = IrExpr::ListFilter {
            list: Box::new(IrExpr::Lit(Lit::Null)),
            item: "x".to_string(),
            predicate: Box::new(IrExpr::Lit(Lit::Bool(true))),
        };
        let row = Row::new();
        let graph = empty_graph();
        let value = eval(&expr, &row, &graph).unwrap();
        assert!(matches!(value, Value::Null));
    }

    #[test]
    fn list_transform_captures_outer_bindings() {
        // map = x + outer, where outer is bound in the row.
        let map = IrExpr::Binary {
            op: BinaryOp::Add,
            lhs: Box::new(IrExpr::Binding("x".to_string())),
            rhs: Box::new(IrExpr::Binding("outer".to_string())),
        };
        let expr = IrExpr::ListTransform {
            list: Box::new(list_lit(vec![1, 2, 3])),
            item: "x".to_string(),
            map: Box::new(map),
        };
        let row = Row::new().with("outer", Value::Int(100));
        let graph = empty_graph();
        let value = eval(&expr, &row, &graph).unwrap();
        assert_eq!(
            value,
            Value::List(vec![Value::Int(101), Value::Int(102), Value::Int(103)])
        );
    }

    #[test]
    fn list_transform_nested_lambdas() {
        // list_transform([[1,2],[3,4]], xs -> list_transform(xs, y -> y * 0 + y + 1))
        // Use addition since we don't have a Mul-only here; equivalent: y -> y + 1.
        let inner = IrExpr::ListTransform {
            list: Box::new(IrExpr::Binding("xs".to_string())),
            item: "y".to_string(),
            map: Box::new(add_binding("y", 1)),
        };
        let outer = IrExpr::ListTransform {
            list: Box::new(IrExpr::List(vec![
                list_lit(vec![1, 2]),
                list_lit(vec![3, 4]),
            ])),
            item: "xs".to_string(),
            map: Box::new(inner),
        };
        let row = Row::new();
        let graph = empty_graph();
        let value = eval(&outer, &row, &graph).unwrap();
        assert_eq!(
            value,
            Value::List(vec![
                Value::List(vec![Value::Int(2), Value::Int(3)]),
                Value::List(vec![Value::Int(4), Value::Int(5)]),
            ])
        );
    }

    #[test]
    fn list_filter_keeps_truthy_elements() {
        // Predicate: x = 2
        let predicate = IrExpr::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(IrExpr::Binding("x".to_string())),
            rhs: Box::new(IrExpr::lit_int(2)),
        };
        let expr = IrExpr::ListFilter {
            list: Box::new(list_lit(vec![1, 2, 3, 2])),
            item: "x".to_string(),
            predicate: Box::new(predicate),
        };
        let row = Row::new();
        let graph = empty_graph();
        let value = eval(&expr, &row, &graph).unwrap();
        assert_eq!(value, Value::List(vec![Value::Int(2), Value::Int(2)]));
    }
}
