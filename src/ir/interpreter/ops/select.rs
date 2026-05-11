//! GraphSelect — bring labelled bindings into scope.
//!
//! Extracted from `interpreter.rs` lines 1806..1821.

use std::collections::BTreeMap;

use crate::ir::value::Value;

use super::super::{IrResult, Row};

pub(crate) fn select_op(
    labels: &[String],
    outputs: &[String],
    rows: Vec<Row>,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut new_row = Row {
            bindings: BTreeMap::new(),
            bulk: row.bulk,
        };
        for (label, output) in labels.iter().zip(outputs.iter()) {
            if let Some(value) = row.bindings.get(label) {
                new_row.bindings.insert(output.clone(), value.clone());
            }
        }
        out.push(new_row);
    }
    Ok(out)
}
