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
        BarrierBulkPolicy::PreserveAndMerge => merge_equal_traversers(&mut out),
        BarrierBulkPolicy::ProviderDefined => {}
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

fn merge_equal_traversers(rows: &mut Vec<Row>) {
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
                rows[prev].bulk = rows[prev].bulk.saturating_add(bulk);
                keep[idx] = false;
            }
            None => {
                sigs.insert(sig, idx);
            }
        }
    }
    let mut iter = keep.into_iter();
    rows.retain(|_| iter.next().unwrap_or(true));
}

/// Helper used when the planner emits a barrier with everything default
/// — i.e. it acts as an explicit "materialise here" boundary. The
/// stream is unchanged.
pub(crate) fn passthrough_barrier(rows: Vec<Row>) -> Vec<Row> {
    rows
}
