//! Source operators — values, node scans, rel scans, label resolvers.
//!
//! Extracted from `interpreter.rs` lines 416..535.

use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{Direction, LabelExpr};
use crate::ir::value::Value;

use super::super::{InterpretError, IrResult, Row};

pub(crate) fn values_op(
    bindings: &[String],
    rows: &[Vec<Value>],
    bulk: Option<&[u64]>,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::with_capacity(rows.len());
    for (idx, row_values) in rows.iter().enumerate() {
        if row_values.len() != bindings.len() {
            return Err(InterpretError::Type(format!(
                "GraphValues row width {} != bindings {}",
                row_values.len(),
                bindings.len()
            )));
        }
        let mut row = Row::new();
        for (key, value) in bindings.iter().zip(row_values.iter()) {
            row.bindings.insert(key.clone(), value.clone());
        }
        if let Some(bulks) = bulk {
            if let Some(b) = bulks.get(idx) {
                row.bulk = *b;
            }
        }
        out.push(row);
    }
    Ok(out)
}

pub(crate) fn node_scan(
    binding: &str,
    labels: &LabelExpr,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::new();
    for label in matching_labels(labels, graph) {
        let table = graph.node_table(&label)?;
        for row_id in 0..table.batch.num_rows() {
            // Multi-label `AllOf` filter — stored as a single label in this
            // simple model, so we accept exact label matches for now.
            let mut row = Row::new();
            row.bindings.insert(
                binding.to_string(),
                Value::Node {
                    label: label.clone(),
                    id: row_id as i64,
                },
            );
            out.push(row);
        }
    }
    Ok(out)
}

pub(crate) fn rel_scan(
    binding: &str,
    types: &LabelExpr,
    _dir: Direction,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::new();
    let candidate_types = matching_rel_types(types, graph);
    for rel_type in candidate_types {
        let table = graph.edge_table(&rel_type)?;
        for row_id in 0..table.batch.num_rows() {
            let (src_label, src_id, dst_label, dst_id) = graph
                .edge_endpoints(&rel_type, row_id as i64)
                .expect("endpoints");
            let mut row = Row::new();
            row.bindings.insert(
                binding.to_string(),
                Value::Edge {
                    rel_type: rel_type.clone(),
                    id: row_id as i64,
                    src_label,
                    src_id,
                    dst_label,
                    dst_id,
                },
            );
            out.push(row);
        }
    }
    Ok(out)
}

pub(crate) fn matching_labels(labels: &LabelExpr, graph: &PropertyGraph) -> Vec<String> {
    let all = graph.labels();
    match labels {
        LabelExpr::Any => all,
        LabelExpr::AnyOf(names) => all
            .into_iter()
            .filter(|label| names.iter().any(|n| n == label))
            .collect(),
        LabelExpr::AllOf(names) => {
            // In this simple store every node has a single label, so AllOf
            // with >1 label produces no matches. AllOf with one label is
            // equivalent to AnyOf.
            if names.len() == 1 {
                all.into_iter().filter(|l| l == &names[0]).collect()
            } else {
                Vec::new()
            }
        }
        LabelExpr::Not(inner) => {
            let blocked = matching_labels(inner, graph);
            all.into_iter().filter(|l| !blocked.contains(l)).collect()
        }
    }
}

pub(crate) fn matching_rel_types(types: &LabelExpr, graph: &PropertyGraph) -> Vec<String> {
    let all = graph.rel_types();
    match types {
        LabelExpr::Any => all,
        LabelExpr::AnyOf(names) => all
            .into_iter()
            .filter(|t| names.iter().any(|n| n == t))
            .collect(),
        LabelExpr::AllOf(names) if names.len() == 1 => {
            all.into_iter().filter(|t| t == &names[0]).collect()
        }
        LabelExpr::AllOf(_) => Vec::new(),
        LabelExpr::Not(inner) => {
            let blocked = matching_rel_types(inner, graph);
            all.into_iter().filter(|t| !blocked.contains(t)).collect()
        }
    }
}
