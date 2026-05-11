//! GraphUnwind — list fan-out.
//!
//! Extracted from `interpreter.rs` lines 1678..1728.

use std::collections::BTreeMap;

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{IrResult, Row};

pub(crate) fn unwind_op(
    expr: &IrExpr,
    bind: &str,
    outer_flag: bool,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::new();
    for row in rows {
        let value = eval(expr, &row, graph)?;
        match value {
            Value::List(items) if items.is_empty() => {
                if outer_flag {
                    let mut new_row = row.clone();
                    new_row.bindings.insert(bind.to_string(), Value::Null);
                    out.push(new_row);
                }
            }
            Value::List(items) => {
                for item in items {
                    let mut new_row = row.clone();
                    new_row.bindings.insert(bind.to_string(), item);
                    out.push(new_row);
                }
            }
            Value::Map(items) => {
                for (k, v) in items {
                    let mut new_row = row.clone();
                    let mut entry = BTreeMap::new();
                    entry.insert("key".into(), Value::String(k));
                    entry.insert("value".into(), v);
                    new_row.bindings.insert(bind.to_string(), Value::Map(entry));
                    out.push(new_row);
                }
            }
            Value::Null => {
                if outer_flag {
                    let mut new_row = row.clone();
                    new_row.bindings.insert(bind.to_string(), Value::Null);
                    out.push(new_row);
                }
            }
            other => {
                let mut new_row = row.clone();
                new_row.bindings.insert(bind.to_string(), other);
                out.push(new_row);
            }
        }
    }
    Ok(out)
}
