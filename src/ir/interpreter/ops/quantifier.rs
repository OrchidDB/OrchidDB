//! GraphQuantifier — all/any/none over list traversers.
//!
//! Extracted from `interpreter.rs` lines 1079..1118.

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::plan::QuantifierKind;
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{IrResult, Row};

pub(crate) fn quantifier_op(
    kind: QuantifierKind,
    item_binding: &str,
    input_expr: &IrExpr,
    predicate: &IrExpr,
    output: &str,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let input_value = eval(input_expr, &row, graph)?;
        let result = match input_value {
            Value::List(items) => {
                let mut matches_count = 0usize;
                let mut null_count = 0usize;
                let total = items.len();
                for item in items {
                    let mut item_row = row.clone();
                    item_row.bindings.insert(item_binding.to_string(), item);
                    match eval(predicate, &item_row, graph)? {
                        Value::Bool(true) => matches_count += 1,
                        Value::Null => null_count += 1,
                        _ => {}
                    }
                }
                let false_count = total.saturating_sub(matches_count + null_count);
                match (kind, matches_count, null_count, false_count) {
                    (QuantifierKind::All, _, _, false_count) if false_count > 0 => {
                        Value::Bool(false)
                    }
                    (QuantifierKind::All, matches_count, 0, _) if matches_count == total => {
                        Value::Bool(true)
                    }
                    (QuantifierKind::All, _, _, _) => Value::Null,

                    (QuantifierKind::Any, matches_count, _, _) if matches_count > 0 => {
                        Value::Bool(true)
                    }
                    (QuantifierKind::Any, 0, 0, _) => Value::Bool(false),
                    (QuantifierKind::Any, _, _, _) => Value::Null,

                    (QuantifierKind::None, matches_count, _, _) if matches_count > 0 => {
                        Value::Bool(false)
                    }
                    (QuantifierKind::None, 0, 0, _) => Value::Bool(true),
                    (QuantifierKind::None, _, _, _) => Value::Null,

                    (QuantifierKind::Single, matches_count, _, _) if matches_count > 1 => {
                        Value::Bool(false)
                    }
                    (QuantifierKind::Single, 1, 0, _) => Value::Bool(true),
                    (QuantifierKind::Single, 0, 0, _) => Value::Bool(false),
                    (QuantifierKind::Single, _, _, _) => Value::Null,
                }
            }
            Value::Null => Value::Null,
            _ => Value::Null,
        };
        let mut new_row = row;
        new_row.bindings.insert(output.to_string(), result);
        out.push(new_row);
    }
    Ok(out)
}
