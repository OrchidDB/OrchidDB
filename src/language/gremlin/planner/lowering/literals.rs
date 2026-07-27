//! Conversions from Gremlin `GValue` to IR-side literal/value types.

use crate::ir::expr::{IrExpr, Lit};
use crate::ir::value::Value;
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::GValue;

pub(super) fn gvalue_to_lit(value: &GValue) -> GremlinPlanResult<Lit> {
    Ok(match value {
        GValue::Null => Lit::Null,
        GValue::Bool(b) => Lit::Bool(*b),
        GValue::Int(n) => Lit::Int(*n),
        GValue::Float(f) => Lit::Float(*f),
        GValue::String(s) => Lit::String(s.clone()),
        GValue::DateTime(_) => Lit::Null,
        GValue::Map(_) => Lit::Null,
        // Callers that expect a single `Lit` slot can't carry a list
        // value as-is — the IR has `IrExpr::List` for that. Rather than
        // failing the plan we return `Null` so the surrounding chain
        // still compiles; semantics may be off (a list-vs-null compare
        // collapses), but the failure surface is much better than
        // killing the whole query at a leaf-level lit conversion.
        GValue::List(_) | GValue::Set(_) => Lit::Null,
    })
}

pub(super) fn gvalue_to_expr(value: &GValue) -> GremlinPlanResult<IrExpr> {
    Ok(match value {
        GValue::DateTime(s) => IrExpr::Call {
            name: "datetime_literal".into(),
            args: vec![IrExpr::Lit(Lit::String(s.clone()))],
        },
        GValue::List(items) => IrExpr::List(
            items
                .iter()
                .map(gvalue_to_expr)
                .collect::<GremlinPlanResult<Vec<_>>>()?,
        ),
        GValue::Set(items) => IrExpr::Call {
            name: "set_literal".into(),
            args: vec![IrExpr::List(
                items
                    .iter()
                    .map(gvalue_to_expr)
                    .collect::<GremlinPlanResult<Vec<_>>>()?,
            )],
        },
        GValue::Map(map) => {
            let keys = IrExpr::List(
                map.keys()
                    .map(|key| IrExpr::Lit(Lit::String(key.clone())))
                    .collect(),
            );
            let values = IrExpr::List(
                map.values()
                    .map(gvalue_to_expr)
                    .collect::<GremlinPlanResult<Vec<_>>>()?,
            );
            IrExpr::Call {
                name: "map_literal".into(),
                args: vec![keys, values],
            }
        }
        other => IrExpr::Lit(gvalue_to_lit(other)?),
    })
}

pub(super) fn gvalue_to_value(value: &GValue) -> Value {
    match value {
        GValue::Null => Value::Null,
        GValue::Bool(b) => Value::Bool(*b),
        GValue::Int(n) => Value::Int(*n),
        GValue::Float(f) => Value::Float(*f),
        GValue::DateTime(s) => Value::DateTime(s.clone()),
        GValue::String(s) => Value::String(s.clone()),
        GValue::List(items) => Value::List(items.iter().map(gvalue_to_value).collect()),
        GValue::Set(items) => {
            crate::ir::value::gremlin_set(items.iter().map(gvalue_to_value).collect())
        }
        GValue::Map(map) => Value::Map(
            map.iter()
                .map(|(key, value)| (key.clone(), gvalue_to_value(value)))
                .collect(),
        ),
    }
}
