//! GraphSort + per-key compare.
//!
//! Extracted from `interpreter.rs` lines 1476..1529.

use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{NullsOrder, SortDir, SortKey};
use crate::ir::value::Value;

use super::super::expr::{compare_values, eval};
use super::super::{IrResult, Row};

pub(crate) fn sort_op(
    keys: &[SortKey],
    mut rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut decorated: Vec<(Vec<Value>, Row)> = rows
        .drain(..)
        .map(|row| {
            let key_values = keys
                .iter()
                .map(|k| eval(&k.expr, &row, graph))
                .collect::<IrResult<Vec<_>>>()?;
            Ok((key_values, row))
        })
        .collect::<IrResult<_>>()?;
    decorated.sort_by(|a, b| {
        for (idx, key) in keys.iter().enumerate() {
            let lhs = &a.0[idx];
            let rhs = &b.0[idx];
            let cmp = compare_for_sort(lhs, rhs, key);
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
        }
        std::cmp::Ordering::Equal
    });
    Ok(decorated.into_iter().map(|(_, row)| row).collect())
}

pub(crate) fn compare_for_sort(lhs: &Value, rhs: &Value, key: &SortKey) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let lhs_null = matches!(lhs, Value::Null);
    let rhs_null = matches!(rhs, Value::Null);
    if lhs_null && rhs_null {
        return Ordering::Equal;
    }
    if lhs_null {
        return match key.nulls {
            NullsOrder::First => Ordering::Less,
            NullsOrder::Last => Ordering::Greater,
            NullsOrder::ProviderDefined => match key.dir {
                SortDir::Asc => Ordering::Less,
                SortDir::Desc => Ordering::Greater,
            },
        };
    }
    if rhs_null {
        return match key.nulls {
            NullsOrder::First => Ordering::Greater,
            NullsOrder::Last => Ordering::Less,
            NullsOrder::ProviderDefined => match key.dir {
                SortDir::Asc => Ordering::Greater,
                SortDir::Desc => Ordering::Less,
            },
        };
    }
    let cmp = lhs
        .three_valued_cmp(rhs)
        .unwrap_or_else(|| compare_values(lhs, rhs));
    match key.dir {
        SortDir::Asc => cmp,
        SortDir::Desc => cmp.reverse(),
    }
}
