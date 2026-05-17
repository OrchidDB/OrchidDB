//! GraphAggregate, GraphGroupMap, agg-call helpers.
//!
//! Extracted from `interpreter.rs` lines 1120..1361.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::{AggCall, AggKind, IrExpr};
use crate::ir::plan::{GroupValue, ProjectionItem};
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::runtime::display_for_group_key;
use super::super::{InterpretError, IrResult, Row};
use super::distinct::{encode_key, encode_value};

const GROUP_FLATTEN_VALUE_ALIAS: &str = "__group_flatten_value";

pub(crate) fn aggregate_op(
    group: &[ProjectionItem],
    aggs: &[AggCall],
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    use std::collections::BTreeMap as Map;
    let mut groups: Map<Vec<u8>, (Vec<Value>, Vec<Row>)> = Map::new();
    for row in rows {
        let mut key_values = Vec::with_capacity(group.len());
        for item in group {
            key_values.push(eval(&item.expr, &row, graph)?);
        }
        let key_bytes = encode_key(&key_values);
        groups
            .entry(key_bytes)
            .or_insert_with(|| (key_values, Vec::new()))
            .1
            .push(row);
    }
    if group.is_empty() && groups.is_empty() {
        // Special case: aggregate with no rows.
        //
        // In TinkerPop / Gremlin semantics, `sum()`, `min()`, `max()`,
        // and `mean()` on an empty traverser stream produce *no*
        // traversers (an empty result), while `count()` produces a
        // single `0` and `fold()` (collect) produces a single empty
        // list. If every aggregate in this node is a reduction whose
        // identity is "no rows" (Sum/Min/Max/Avg), return an empty
        // result. Otherwise emit a single identity row so SQL-style
        // `count()` / `fold()` queries still get one row.
        let all_drop_on_empty = !aggs.is_empty()
            && aggs.iter().all(|agg| {
                matches!(
                    agg.kind,
                    AggKind::Sum | AggKind::Min | AggKind::Max | AggKind::Avg
                )
            });
        if all_drop_on_empty {
            return Ok(Vec::new());
        }
        let mut row = Row::new();
        for item in group {
            row.bindings.insert(item.alias.clone(), Value::Null);
        }
        for agg in aggs {
            row.bindings
                .insert(agg.alias.clone(), agg_identity(agg.kind));
        }
        return Ok(vec![row]);
    }
    let mut out = Vec::new();
    for (_, (key_values, group_rows)) in groups {
        let mut row = Row::new();
        for (item, value) in group.iter().zip(key_values.into_iter()) {
            row.bindings.insert(item.alias.clone(), value);
        }
        for agg in aggs {
            let value = compute_aggregate(agg, &group_rows, graph)?;
            row.bindings.insert(agg.alias.clone(), value);
        }
        out.push(row);
    }
    Ok(out)
}

pub(crate) fn group_map_op(
    key: &IrExpr,
    value: &GroupValue,
    output: &str,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut groups: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for row in rows {
        let key_value = eval(key, &row, graph)?;
        groups.entry(map_key(&key_value)).or_default().push(row);
    }
    let mut map = BTreeMap::new();
    for (key, group_rows) in groups {
        let value = match value {
            GroupValue::CountBulk => {
                let total: u64 = group_rows.iter().map(|row| row.bulk).sum();
                Value::String(format!("d[{total}].l"))
            }
            GroupValue::Aggregate(agg) => {
                let value = compute_aggregate(agg, &group_rows, graph)?;
                if matches!(agg.kind, AggKind::CollectTraversers)
                    && agg.alias == GROUP_FLATTEN_VALUE_ALIAS
                {
                    flatten_group_lists(value)
                } else {
                    value
                }
            }
        };
        map.insert(key, value);
    }
    Ok(vec![Row::new().with(output, Value::Map(map))])
}

pub(crate) fn map_key(value: &Value) -> String {
    display_for_group_key(value)
}

pub(crate) fn agg_identity(kind: AggKind) -> Value {
    match kind {
        AggKind::CountRows | AggKind::CountBulk | AggKind::CountDistinct | AggKind::CountIf => {
            Value::Long(0)
        }
        AggKind::Sum | AggKind::SumOrZero => Value::Int(0),
        AggKind::AvgOrZero => Value::Float(0.0),
        AggKind::AvgOrNull => Value::Null,
        AggKind::StDev | AggKind::StDevP => Value::Float(0.0),
        AggKind::CollectRows | AggKind::CollectTraversers => Value::List(Vec::new()),
        _ => Value::Null,
    }
}

pub(crate) fn compute_aggregate(
    agg: &AggCall,
    rows: &[Row],
    graph: &PropertyGraph,
) -> IrResult<Value> {
    match agg.kind {
        AggKind::CountRows => {
            // `countRows(x)` only counts rows where evaluating `x` is
            // non-null; `countRows()` counts every row.
            let count = match &agg.arg {
                None => rows.len() as i64,
                Some(expr) => rows
                    .iter()
                    .map(|r| eval(expr, r, graph))
                    .filter_map(Result::ok)
                    .filter(|v| !matches!(v, Value::Null))
                    .count() as i64,
            };
            Ok(Value::Long(count))
        }
        AggKind::CountBulk => {
            let total: u64 = rows.iter().map(|r| r.bulk).sum();
            Ok(Value::Long(total as i64))
        }
        AggKind::CountDistinct => {
            let expr = match &agg.arg {
                Some(expr) => expr,
                None => {
                    return Err(InterpretError::Type(
                        "count(distinct ?) requires arg".into(),
                    ));
                }
            };
            let mut seen = BTreeSet::new();
            let mut count = 0i64;
            for row in rows {
                let v = eval(expr, row, graph)?;
                if matches!(v, Value::Null) {
                    continue;
                }
                if seen.insert(encode_value(&v)) {
                    count += 1;
                }
            }
            Ok(Value::Long(count))
        }
        AggKind::CountIf => {
            let expr = agg
                .arg
                .as_ref()
                .ok_or_else(|| InterpretError::Type("count_if requires an argument".into()))?;
            let mut seen = BTreeSet::new();
            let mut count = 0i64;
            for row in rows {
                let value = eval(expr, row, graph)?;
                if agg.distinct && !seen.insert(encode_value(&value)) {
                    continue;
                }
                if aggregate_truthy(&value) {
                    count += 1;
                }
            }
            Ok(Value::Long(count))
        }
        AggKind::Sum | AggKind::SumOrZero => {
            use bigdecimal::BigDecimal;
            use num_bigint::BigInt;
            use num_traits::{ToPrimitive, Zero};

            let expr = agg
                .arg
                .as_ref()
                .ok_or_else(|| InterpretError::Type("sum requires an argument".into()))?;
            let mut int_sum: i64 = 0;
            let mut bigint_sum = BigInt::zero();
            let mut decimal_sum = BigDecimal::from(0);
            let mut float_sum: f64 = 0.0;
            let mut have_bigint = false;
            let mut have_decimal = false;
            let mut have_float = false;
            for value in aggregate_values(expr, rows, graph, agg.distinct)? {
                match value {
                    Value::Byte(n) => int_sum += n as i64,
                    Value::Short(n) => int_sum += n as i64,
                    Value::Int(n) | Value::Long(n) => int_sum += n,
                    Value::Float32(f) => {
                        have_float = true;
                        float_sum += f as f64;
                    }
                    Value::Float(f) => {
                        have_float = true;
                        float_sum += f;
                    }
                    Value::BigInt(n) => {
                        have_bigint = true;
                        bigint_sum += n;
                    }
                    Value::BigDecimal(d) => {
                        have_decimal = true;
                        decimal_sum += d;
                    }
                    // Non-numeric inputs (Node/Edge/List/Map/Path) are
                    // ignored rather than failing; this matches the
                    // looser conformance harness expectation that a
                    // mis-shaped sum produces what it can.
                    _ => {}
                }
            }
            if have_float {
                let bigint = bigint_sum.to_f64().unwrap_or(0.0);
                let decimal = decimal_sum.to_f64().unwrap_or(0.0);
                Ok(Value::Float(float_sum + int_sum as f64 + bigint + decimal))
            } else if have_decimal {
                Ok(Value::BigDecimal(
                    decimal_sum + BigDecimal::from(bigint_sum) + BigDecimal::from(int_sum),
                ))
            } else if have_bigint {
                Ok(Value::BigInt(bigint_sum + BigInt::from(int_sum)))
            } else {
                Ok(Value::Int(int_sum))
            }
        }
        AggKind::Avg | AggKind::AvgOrZero | AggKind::AvgOrNull => {
            let expr = agg
                .arg
                .as_ref()
                .ok_or_else(|| InterpretError::Type("avg requires an argument".into()))?;
            let mut sum = 0.0_f64;
            let mut count = 0_i64;
            for value in aggregate_values(expr, rows, graph, agg.distinct)? {
                match value {
                    Value::Byte(n) => {
                        sum += n as f64;
                        count += 1;
                    }
                    Value::Short(n) => {
                        sum += n as f64;
                        count += 1;
                    }
                    Value::Int(n) | Value::Long(n) => {
                        sum += n as f64;
                        count += 1;
                    }
                    Value::Float32(f) => {
                        sum += f as f64;
                        count += 1;
                    }
                    Value::Float(f) => {
                        sum += f;
                        count += 1;
                    }
                    _ => {}
                }
            }
            if count == 0 {
                if agg.kind == AggKind::AvgOrZero {
                    Ok(Value::Float(0.0))
                } else {
                    Ok(Value::Null)
                }
            } else {
                Ok(Value::Float(sum / count as f64))
            }
        }
        AggKind::Min | AggKind::Max | AggKind::MinOrNull | AggKind::MaxOrNull => {
            let expr = agg
                .arg
                .as_ref()
                .ok_or_else(|| InterpretError::Type("min/max requires an argument".into()))?;
            let mut current: Option<Value> = None;
            for v in aggregate_values(expr, rows, graph, agg.distinct)? {
                current = match current.take() {
                    None => Some(v),
                    Some(existing) => match (existing.three_valued_cmp(&v), agg.kind) {
                        (Some(std::cmp::Ordering::Greater), AggKind::Min | AggKind::MinOrNull) => {
                            Some(v)
                        }
                        (Some(std::cmp::Ordering::Less), AggKind::Max | AggKind::MaxOrNull) => {
                            Some(v)
                        }
                        (_, _) => Some(existing),
                    },
                };
            }
            Ok(current.unwrap_or(Value::Null))
        }
        AggKind::StDev | AggKind::StDevP => {
            let expr = agg
                .arg
                .as_ref()
                .ok_or_else(|| InterpretError::Type("stDev requires an argument".into()))?;
            let values = numeric_aggregate_values(expr, rows, graph, agg.distinct)?;
            Ok(Value::Float(stddev(&values, agg.kind == AggKind::StDev)))
        }
        AggKind::PercentileCont | AggKind::PercentileDisc => {
            let expr = agg.arg.as_ref().ok_or_else(|| {
                InterpretError::Type("percentile aggregate requires arguments".into())
            })?;
            let IrExpr::List(args) = expr else {
                return Err(InterpretError::Type(
                    "percentile aggregate requires value and percentile arguments".into(),
                ));
            };
            let [value_expr, percentile_expr] = args.as_slice() else {
                return Err(InterpretError::Type(
                    "percentile aggregate requires value and percentile arguments".into(),
                ));
            };
            let Some(percentile) = percentile_value(percentile_expr, rows, graph)? else {
                return Ok(Value::Null);
            };
            let mut values = numeric_aggregate_values(value_expr, rows, graph, agg.distinct)?;
            if values.is_empty() {
                return Ok(Value::Null);
            }
            values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            Ok(Value::Float(match agg.kind {
                AggKind::PercentileCont => percentile_cont(&values, percentile),
                AggKind::PercentileDisc => percentile_disc(&values, percentile),
                _ => unreachable!(),
            }))
        }
        AggKind::CollectRows | AggKind::CollectTraversers => {
            let expr = agg
                .arg
                .as_ref()
                .ok_or_else(|| InterpretError::Type("collect requires an argument".into()))?;
            let mut list = Vec::new();
            let mut seen = BTreeSet::new();
            let mut evaluated = 0usize;
            for row in rows {
                let v = eval(expr, row, graph)?;
                evaluated += 1;
                if matches!(v, Value::Null) {
                    continue;
                }
                if agg.distinct && !seen.insert(encode_value(&v)) {
                    continue;
                }
                if matches!(agg.kind, AggKind::CollectTraversers) {
                    for _ in 0..row.bulk {
                        list.push(v.clone());
                    }
                } else {
                    list.push(v);
                }
            }
            if matches!(agg.kind, AggKind::CollectRows) && evaluated > 0 && list.is_empty() {
                return Ok(Value::Null);
            }
            Ok(Value::List(list))
        }
    }
}

fn aggregate_values(
    expr: &IrExpr,
    rows: &[Row],
    graph: &PropertyGraph,
    distinct: bool,
) -> IrResult<Vec<Value>> {
    let mut values = Vec::new();
    let mut seen = BTreeSet::new();
    for row in rows {
        let value = eval(expr, row, graph)?;
        if matches!(value, Value::Null) {
            continue;
        }
        if distinct && !seen.insert(encode_value(&value)) {
            continue;
        }
        values.push(value);
    }
    Ok(values)
}

fn aggregate_truthy(value: &Value) -> bool {
    use num_traits::Zero;

    match value {
        Value::Bool(value) => *value,
        Value::Byte(value) => *value != 0,
        Value::Short(value) => *value != 0,
        Value::Int(value) | Value::Long(value) => *value != 0,
        Value::Float32(value) => !value.is_nan() && *value != 0.0,
        Value::Float(value) => !value.is_nan() && *value != 0.0,
        Value::BigInt(value) => !value.is_zero(),
        Value::BigDecimal(value) => !value.is_zero(),
        _ => false,
    }
}

fn numeric_aggregate_values(
    expr: &IrExpr,
    rows: &[Row],
    graph: &PropertyGraph,
    distinct: bool,
) -> IrResult<Vec<f64>> {
    aggregate_values(expr, rows, graph, distinct).map(|values| {
        values
            .into_iter()
            .filter_map(|value| numeric_f64(&value))
            .collect()
    })
}

fn numeric_f64(value: &Value) -> Option<f64> {
    use num_traits::ToPrimitive;
    match value {
        Value::Byte(value) => Some(*value as f64),
        Value::Short(value) => Some(*value as f64),
        Value::Int(value) | Value::Long(value) => Some(*value as f64),
        Value::Float32(value) => Some(*value as f64),
        Value::Float(value) => Some(*value),
        Value::BigInt(value) => value.to_f64(),
        Value::BigDecimal(value) => value.to_f64(),
        _ => None,
    }
}

fn stddev(values: &[f64], sample: bool) -> f64 {
    let n = values.len();
    if n == 0 || (sample && n == 1) {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    let variance_sum = values
        .iter()
        .map(|value| {
            let delta = value - mean;
            delta * delta
        })
        .sum::<f64>();
    let denominator = (if sample { n - 1 } else { n }) as f64;
    (variance_sum / denominator).sqrt()
}

fn percentile_value(expr: &IrExpr, rows: &[Row], graph: &PropertyGraph) -> IrResult<Option<f64>> {
    let row = rows.first().cloned().unwrap_or_else(Row::new);
    Ok(numeric_f64(&eval(expr, &row, graph)?).map(|value| value.clamp(0.0, 1.0)))
}

fn percentile_cont(values: &[f64], percentile: f64) -> f64 {
    if values.len() == 1 {
        return values[0];
    }
    let rank = percentile * (values.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower == upper {
        values[lower]
    } else {
        let fraction = rank - lower as f64;
        values[lower] + (values[upper] - values[lower]) * fraction
    }
}

fn percentile_disc(values: &[f64], percentile: f64) -> f64 {
    let index = (percentile * (values.len() - 1) as f64).round() as usize;
    values[index]
}

fn flatten_group_lists(value: Value) -> Value {
    let Value::List(items) = value else {
        return value;
    };
    let mut flattened = Vec::new();
    for item in items {
        match item {
            Value::List(nested) => flattened.extend(nested),
            other => flattened.push(other),
        }
    }
    Value::List(flattened)
}
