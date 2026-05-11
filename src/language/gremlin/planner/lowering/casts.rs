//! Scalar casts: `asNumber`, `asString`, `asBool`, `asDate`.
//!
//! Each lowers to a `GraphCurrentProject` whose expression is an
//! `IrExpr::Call` against a runtime cast helper. The interpreter
//! dispatches the call name to a per-target conversion. Complex inputs
//! (List/Map/Node/Edge) follow Gremlin's "best effort" rule: numeric/
//! bool casts of a non-scalar produce `null`; string casts of a non-
//! scalar produce the value's display form.

use super::context::CURRENT;
use super::literals::gvalue_to_expr;
use crate::ir::expr::IrExpr;
use crate::ir::plan::Node;
use crate::language::gremlin::ast::{CastTarget, NumericCast};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::GValue;

pub(super) fn lower_cast_scalar(input: Node, target: CastTarget) -> Node {
    let name = match target {
        CastTarget::Number => "cast_number",
        CastTarget::Numeric(num) => match num {
            NumericCast::Byte => "cast_byte",
            NumericCast::Short => "cast_short",
            NumericCast::Int => "cast_int",
            NumericCast::Long => "cast_long",
            NumericCast::BigInt => "cast_bigint",
            NumericCast::Float => "cast_float",
            NumericCast::Double => "cast_double",
            NumericCast::BigDecimal => "cast_bigdecimal",
        },
        CastTarget::String => "cast_string",
        CastTarget::Bool => "cast_bool",
        CastTarget::Date => "cast_date",
    };
    Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: name.into(),
            args: vec![IrExpr::Binding(CURRENT.into())],
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}

pub(super) fn lower_date_add(input: Node, unit: &str, amount: i64) -> Node {
    Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "date_add".into(),
            args: vec![
                IrExpr::Binding(CURRENT.into()),
                IrExpr::Lit(crate::ir::expr::Lit::String(unit.to_ascii_lowercase())),
                IrExpr::Lit(crate::ir::expr::Lit::Int(amount)),
            ],
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}

pub(super) fn lower_date_diff(input: Node, rhs: &GValue) -> GremlinPlanResult<Node> {
    Ok(Node::GraphCurrentProject {
        expr: IrExpr::Call {
            name: "date_diff".into(),
            args: vec![IrExpr::Binding(CURRENT.into()), gvalue_to_expr(rhs)?],
        },
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    })
}
