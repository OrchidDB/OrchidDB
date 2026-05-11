//! GraphProject and GraphCurrentProject.
//!
//! Extracted from `interpreter.rs` lines 1041..1077.

use std::collections::BTreeMap;

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{ProjectMode, ProjectionItem};
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{IrResult, Row};

pub(crate) fn project_op(
    mode: ProjectMode,
    items: &[ProjectionItem],
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut new_row = match mode {
            ProjectMode::PreserveVisible | ProjectMode::ReplaceCurrent => row.clone(),
            ProjectMode::ReplaceScope => Row {
                bindings: BTreeMap::new(),
                bulk: row.bulk,
            },
        };
        for item in items {
            let value = eval(&item.expr, &row, graph)?;
            new_row.bindings.insert(item.alias.clone(), value);
        }
        out.push(new_row);
    }
    Ok(out)
}

pub(crate) fn current_project_op(
    expr: &IrExpr,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::new();
    for mut row in rows {
        let value = eval(expr, &row, graph)?;
        // Gremlin DropUnproductive: drop the row when the value is null.
        if matches!(value, Value::Null) {
            continue;
        }
        row.bindings.insert("current".to_string(), value);
        out.push(row);
    }
    Ok(out)
}
