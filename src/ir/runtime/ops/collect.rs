//! GraphCollect — list-shaped collection projection.
//!
//! Evaluates `value` over every input row, optionally orders and
//! deduplicates the results, and emits a single output row with the
//! aggregated list bound to `alias`.

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::plan::SortKey;
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{IrResult, Row};
use super::distinct::encode_value;
use super::sort::compare_for_sort;

pub(crate) fn collect_op(
    value: &IrExpr,
    distinct: bool,
    order: &[SortKey],
    alias: &str,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    if order.is_empty() {
        let mut list = if distinct {
            Vec::new()
        } else {
            Vec::with_capacity(rows.len())
        };
        let mut seen = std::collections::BTreeSet::new();
        for row in rows {
            let value = eval(value, &row, graph)?;
            if !distinct || seen.insert(encode_value(&value)) {
                list.push(value);
            }
        }
        return Ok(vec![Row::new().with(alias, Value::List(list))]);
    }
    let mut decorated: Vec<(Vec<Value>, Value)> = Vec::with_capacity(rows.len());
    for row in rows {
        let v = eval(value, &row, graph)?;
        let key_values = order
            .iter()
            .map(|k| eval(&k.expr, &row, graph))
            .collect::<IrResult<Vec<_>>>()?;
        decorated.push((key_values, v));
    }
    decorated.sort_by(|a, b| {
        for (idx, key) in order.iter().enumerate() {
            let cmp = compare_for_sort(&a.0[idx], &b.0[idx], key);
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
        }
        std::cmp::Ordering::Equal
    });
    let mut list: Vec<Value> = if distinct {
        Vec::new()
    } else {
        Vec::with_capacity(decorated.len())
    };
    if distinct {
        let mut seen = std::collections::BTreeSet::new();
        for (_, v) in decorated {
            if seen.insert(encode_value(&v)) {
                list.push(v);
            }
        }
    } else {
        list.extend(decorated.into_iter().map(|(_, v)| v));
    }
    Ok(vec![Row::new().with(alias, Value::List(list))])
}
