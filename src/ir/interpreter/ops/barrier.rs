//! GraphBarrier — partitioned order + slice + bulk policy.
//!
//! Spec §6.6 / §6.7 / §10.4. The barrier semantically materialises the
//! upstream stream, partitions it by `partition` keys, sorts each
//! partition by `order`, applies `slice` per partition, and finally
//! collapses the partitions back into a single stream. With an empty
//! `partition` the operator behaves as a global sort+slice.

use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{BarrierBulkPolicy, Slice, SortKey};
use crate::ir::value::Value;

use super::super::{IrResult, Row};
use super::distinct::encode_value;
use super::slice::slice_op;
use super::sort::sort_op;

pub(crate) fn barrier_op(
    partition: &[String],
    order: &[SortKey],
    slice: &Slice,
    materialize: bool,
    bulk_policy: BarrierBulkPolicy,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let _ = materialize; // single-process interpreter — always materialised.

    let partitioned: Vec<Vec<Row>> = if partition.is_empty() {
        vec![rows]
    } else {
        partition_rows(partition, rows)
    };

    let mut out = Vec::new();
    for group in partitioned {
        let ordered = if order.is_empty() {
            group
        } else {
            sort_op(order, group, graph)?
        };
        let sliced = slice_op(slice, ordered)?;
        out.extend(sliced);
    }

    match bulk_policy {
        BarrierBulkPolicy::ResetToOne => {
            for row in &mut out {
                row.bulk = 1;
            }
        }
        BarrierBulkPolicy::PreserveAndMerge => merge_equal_traversers(&mut out)?,
        BarrierBulkPolicy::ProviderDefined => {}
        BarrierBulkPolicy::Gremlin { normalize_sack } => {
            out = gremlin_barrier(out, normalize_sack, graph)?;
        }
    }
    Ok(out)
}

fn partition_rows(partition: &[String], rows: Vec<Row>) -> Vec<Vec<Row>> {
    use std::collections::BTreeMap;
    // BTreeMap keeps partitions in deterministic key order.
    let mut groups: BTreeMap<Vec<u8>, Vec<Row>> = BTreeMap::new();
    let mut order: Vec<Vec<u8>> = Vec::new();
    for row in rows {
        let mut key = Vec::new();
        for binding in partition {
            let value = row.bindings.get(binding).cloned().unwrap_or(Value::Null);
            key.extend(encode_value(&value));
            key.push(0);
        }
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(row);
    }
    order
        .into_iter()
        .filter_map(|k| groups.remove(&k))
        .collect()
}

fn merge_equal_traversers(rows: &mut Vec<Row>) -> IrResult<()> {
    use std::collections::BTreeMap;
    let signatures: Vec<Vec<u8>> = rows
        .iter()
        .map(|row| {
            let mut sig = Vec::new();
            for (k, v) in &row.bindings {
                sig.extend_from_slice(k.as_bytes());
                sig.push(0);
                sig.extend(encode_value(v));
                sig.push(0xff);
            }
            sig
        })
        .collect();
    let mut sigs: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    let mut keep: Vec<bool> = vec![true; rows.len()];
    for (idx, sig) in signatures.into_iter().enumerate() {
        match sigs.get(&sig).copied() {
            Some(prev) => {
                let bulk = rows[idx].bulk;
                rows[prev].bulk = rows[prev].bulk.checked_add(bulk).ok_or_else(|| {
                    super::super::InterpretError::ExecutionLimit("traverser bulk overflow".into())
                })?;
                keep[idx] = false;
            }
            None => {
                sigs.insert(sig, idx);
            }
        }
    }
    let mut iter = keep.into_iter();
    rows.retain(|_| iter.next().unwrap_or(true));
    Ok(())
}

/// Helper used when the planner emits a barrier with everything default
/// — i.e. it acts as an explicit "materialise here" boundary. The
/// stream is unchanged.
pub(crate) fn passthrough_barrier(rows: Vec<Row>) -> Vec<Row> {
    rows
}

/// Remove compiler temporaries once a complete Gremlin step has finished.
/// Actual labels are identified by their select-history register. Path elision
/// is permitted only by the planner's whole-traversal proof.
pub(crate) fn compact_gremlin_row(row: &mut Row) {
    let labels = row
        .bindings
        .keys()
        .filter_map(|key| {
            key.strip_prefix("__gremlin_select_history_")
                .map(str::to_owned)
        })
        .collect::<std::collections::BTreeSet<_>>();
    let keep_path = row.bindings.get("__gremlin_bulk_safe") != Some(&Value::Bool(true));
    let on_edge = matches!(row.bindings.get("current"), Some(Value::Edge { .. }));
    row.bindings.retain(|key, _| {
        key == "current"
            || labels.contains(key)
            || (key.starts_with("__")
                && (keep_path || (key != "__path" && key != "__path_labels"))
                && (on_edge || key != "__edge_other"))
    });
}

pub(crate) fn compact_repeat_frontier(mut rows: Vec<Row>) -> IrResult<Vec<Row>> {
    if rows.first().is_none_or(|row| {
        row.bindings.get("__gremlin_bulk_safe") != Some(&Value::Bool(true))
            || row.bindings.get("__bulk_enabled") == Some(&Value::Bool(false))
            || row.bindings.contains_key("__sack")
    }) {
        return Ok(rows);
    }
    for row in &mut rows {
        compact_gremlin_row(row);
    }
    merge_equal_traversers(&mut rows)?;
    Ok(rows)
}

fn gremlin_barrier(rows: Vec<Row>, normalize: bool, graph: &PropertyGraph) -> IrResult<Vec<Row>> {
    use super::super::runtime::eval_call;
    let mut out: Vec<Row> = Vec::new();
    let mut seen = std::collections::BTreeMap::new();
    for mut row in rows {
        compact_gremlin_row(&mut row);
        let sack = row.bindings.get("__sack").cloned();
        let merge = match row.bindings.get("__sack_merge") {
            Some(Value::String(op)) => Some(op.clone()),
            _ => None,
        };
        // Without a sack merger, sacks prohibit bulking even when equal.
        if merge.is_none()
            && (sack.is_some() || row.bindings.get("__bulk_enabled") == Some(&Value::Bool(false)))
        {
            out.push(row);
            continue;
        }
        let mut key_row = row.clone();
        if merge.is_some() {
            key_row.bindings.remove("__sack");
        }
        let key = super::distinct::row_signature(&key_row);
        if let Some(&index) = seen.get(&key) {
            let previous: &mut Row = &mut out[index];
            if let (Some(sack), Some(op)) = (sack, merge) {
                let old = previous
                    .bindings
                    .get("__sack")
                    .cloned()
                    .unwrap_or(Value::Null);
                previous.bindings.insert(
                    "__sack".into(),
                    eval_call("sack_apply", vec![old, sack, Value::String(op)], graph)?,
                );
            }
            if row.bindings.get("__bulk_enabled") != Some(&Value::Bool(false)) {
                previous.bulk = previous.bulk.checked_add(row.bulk).ok_or_else(|| {
                    super::super::InterpretError::ExecutionLimit("traverser bulk overflow".into())
                })?;
            }
        } else {
            seen.insert(key, out.len());
            out.push(row);
        }
    }
    if normalize {
        let mut total = Value::Float(0.0);
        for row in &out {
            let sack = row.bindings.get("__sack").cloned().unwrap_or(Value::Null);
            let weighted = eval_call(
                "sack_apply",
                vec![
                    sack,
                    Value::Long(row.bulk as i64),
                    Value::String("mult".into()),
                ],
                graph,
            )?;
            total = eval_call(
                "sack_apply",
                vec![total, weighted, Value::String("sum".into())],
                graph,
            )?;
        }
        for row in &mut out {
            let sack = row.bindings.get("__sack").cloned().unwrap_or(Value::Null);
            let weighted = eval_call(
                "sack_apply",
                vec![
                    sack,
                    Value::Long(row.bulk as i64),
                    Value::String("mult".into()),
                ],
                graph,
            )?;
            let sack = eval_call(
                "sack_apply",
                vec![weighted, total.clone(), Value::String("div".into())],
                graph,
            )?;
            row.bindings.insert("__sack".into(), sack);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod bulk_tests {
    use super::*;

    #[test]
    fn compaction_rejects_bulk_overflow() {
        let mut row = Row::new()
            .with("current", Value::Int(1))
            .with("__gremlin_bulk_safe", Value::Bool(true));
        row.bulk = u64::MAX;
        let mut other = row.clone();
        other.bulk = 1;
        assert!(matches!(
            compact_repeat_frontier(vec![row, other]),
            Err(super::super::super::InterpretError::ExecutionLimit(_))
        ));
    }
}
