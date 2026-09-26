//! Scalar aggregation kernels and relational group plan analysis.

use std::collections::BTreeSet;

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::{AggCall, AggKind, IrExpr};
use crate::ir::plan::ProjectionItem;
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::scalar::display_for_group_key;
use super::super::{RuntimeError, IrResult, Row};
use super::distinct::{encode_key, encode_value};


pub(crate) fn aggregate_op(
    group: &[ProjectionItem],
    aggs: &[AggCall],
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    if aggs.iter().any(|agg| agg.kind == AggKind::EngineFunction) {
        return Err(RuntimeError::Unsupported(
            "engine aggregate functions require relational execution".into(),
        ));
    }
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

pub(crate) fn map_key(value: &Value) -> String {
    display_for_group_key(value)
}

pub(crate) fn unwrap_single_group_value(value: Value) -> Value {
    match value {
        Value::List(items) if items.len() == 1 => items.into_iter().next().unwrap_or(Value::Null),
        other => other,
    }
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
        AggKind::CollectRows | AggKind::CollectNonNull | AggKind::CollectTraversers => Value::List(Vec::new()),
        _ => Value::Null,
    }
}

pub(crate) fn compute_aggregate(
    agg: &AggCall,
    rows: &[Row],
    graph: &PropertyGraph,
) -> IrResult<Value> {
    match agg.kind {
        AggKind::EngineFunction => Err(RuntimeError::Unsupported(
            "engine aggregate functions require relational execution".into(),
        )),
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
            let total = checked_bulk_total(rows)?;
            Ok(Value::Long(total as i64))
        }
        AggKind::CountDistinct => {
            let expr = match &agg.arg {
                Some(expr) => expr,
                None => {
                    return Err(RuntimeError::Type(
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
                .ok_or_else(|| RuntimeError::Type("count_if requires an argument".into()))?;
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
                .ok_or_else(|| RuntimeError::Type("sum requires an argument".into()))?;
            let mut int_sum: i64 = 0;
            let mut bigint_sum = BigInt::zero();
            let mut decimal_sum = BigDecimal::from(0);
            let mut float_sum: f64 = 0.0;
            let mut have_bigint = false;
            let mut have_decimal = false;
            let mut have_float = false;
            for (value, weight) in aggregate_weighted_values(expr, rows, graph, agg.distinct)? {
                match value {
                    Value::Byte(n) => int_sum = add_weighted_integer(int_sum, n as i64, weight)?,
                    Value::Short(n) => int_sum = add_weighted_integer(int_sum, n as i64, weight)?,
                    Value::Int(n) | Value::Long(n) => {
                        int_sum = add_weighted_integer(int_sum, n, weight)?
                    }
                    Value::Float32(f) => {
                        have_float = true;
                        float_sum += f as f64 * weight as f64;
                    }
                    Value::Float(f) => {
                        have_float = true;
                        float_sum += f * weight as f64;
                    }
                    Value::BigInt(n) => {
                        have_bigint = true;
                        bigint_sum += n * BigInt::from(weight);
                    }
                    Value::BigDecimal(d) => {
                        have_decimal = true;
                        decimal_sum += d * BigDecimal::from(weight);
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
                .ok_or_else(|| RuntimeError::Type("avg requires an argument".into()))?;
            let mut sum = 0.0_f64;
            let mut count = 0_u64;
            for (value, weight) in aggregate_weighted_values(expr, rows, graph, agg.distinct)? {
                match value {
                    Value::Byte(n) => {
                        sum += n as f64 * weight as f64;
                        count = count.checked_add(weight).ok_or_else(|| {
                            RuntimeError::Runtime("aggregate bulk overflow".into())
                        })?;
                    }
                    Value::Short(n) => {
                        sum += n as f64 * weight as f64;
                        count = count.checked_add(weight).ok_or_else(|| {
                            RuntimeError::Runtime("aggregate bulk overflow".into())
                        })?;
                    }
                    Value::Int(n) | Value::Long(n) => {
                        sum += n as f64 * weight as f64;
                        count = count.checked_add(weight).ok_or_else(|| {
                            RuntimeError::Runtime("aggregate bulk overflow".into())
                        })?;
                    }
                    Value::Float32(f) => {
                        sum += f as f64 * weight as f64;
                        count = count.checked_add(weight).ok_or_else(|| {
                            RuntimeError::Runtime("aggregate bulk overflow".into())
                        })?;
                    }
                    Value::Float(f) => {
                        sum += f * weight as f64;
                        count = count.checked_add(weight).ok_or_else(|| {
                            RuntimeError::Runtime("aggregate bulk overflow".into())
                        })?;
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
                .ok_or_else(|| RuntimeError::Type("min/max requires an argument".into()))?;
            // Cross-type fallback follows openCypher orderability
            // (maps < nodes < rels < lists < temporals < strings <
            // booleans < numbers) when values aren't mutually
            // comparable.
            fn orderability_rank(value: &Value) -> u8 {
                match value {
                    Value::Map(_) => 0,
                    Value::Node { .. } => 1,
                    Value::Edge { .. } => 2,
                    Value::List(_) | Value::Path(_) => 3,
                    Value::DateTime(_) => 4,
                    Value::String(_) => 5,
                    Value::Bool(_) => 6,
                    _ => 7,
                }
            }
            let mut current: Option<Value> = None;
            for v in aggregate_values(expr, rows, graph, agg.distinct)? {
                current = match current.take() {
                    None => Some(v),
                    Some(existing) => {
                        let ord = existing.three_valued_cmp(&v).or_else(|| {
                            if matches!(existing, Value::Null) || matches!(v, Value::Null) {
                                None
                            } else {
                                Some(orderability_rank(&existing).cmp(&orderability_rank(&v)))
                            }
                        });
                        match (ord, agg.kind) {
                            (
                                Some(std::cmp::Ordering::Greater),
                                AggKind::Min | AggKind::MinOrNull,
                            ) => Some(v),
                            (Some(std::cmp::Ordering::Less), AggKind::Max | AggKind::MaxOrNull) => {
                                Some(v)
                            }
                            (_, _) => Some(existing),
                        }
                    }
                };
            }
            Ok(current.unwrap_or(Value::Null))
        }
        AggKind::StDev | AggKind::StDevP => {
            let expr = agg
                .arg
                .as_ref()
                .ok_or_else(|| RuntimeError::Type("stDev requires an argument".into()))?;
            let values = numeric_aggregate_values(expr, rows, graph, agg.distinct)?;
            Ok(Value::Float(stddev(&values, agg.kind == AggKind::StDev)))
        }
        AggKind::PercentileCont | AggKind::PercentileDisc => {
            let expr = agg.arg.as_ref().ok_or_else(|| {
                RuntimeError::Type("percentile aggregate requires arguments".into())
            })?;
            let IrExpr::List(args) = expr else {
                return Err(RuntimeError::Type(
                    "percentile aggregate requires value and percentile arguments".into(),
                ));
            };
            let [value_expr, percentile_expr] = args.as_slice() else {
                return Err(RuntimeError::Type(
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
        AggKind::CollectRows | AggKind::CollectNonNull | AggKind::CollectTraversers => {
            let expr = agg
                .arg
                .as_ref()
                .ok_or_else(|| RuntimeError::Type("collect requires an argument".into()))?;
            let mut list = Vec::new();
            let mut seen = BTreeSet::new();
            let mut evaluated = 0usize;
            for row in rows {
                let v = eval(expr, row, graph)?;
                evaluated += 1;
                if matches!(v, Value::Null)
                    && (matches!(agg.kind, AggKind::CollectRows | AggKind::CollectNonNull)
                        || matches!(
                            expr,
                            IrExpr::Property {
                                policy: crate::ir::policy::PropertyMissing::DropUnproductive,
                                ..
                            }
                        ))
                {
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
            // Kuzu renders COLLECT over only-null inputs as NULL (the
            // agg suites assert `size(collect(...))` is NULL for empty
            // groups); openCypher's TCK wants `[]` here, but the corpus
            // ground truth is Kuzu's output.
            if matches!(agg.kind, AggKind::CollectRows) && evaluated > 0 && list.is_empty() {
                return Ok(Value::Null);
            }
            Ok(Value::List(list))
        }
    }
}

pub(crate) fn checked_bulk_total(rows: &[Row]) -> IrResult<u64> {
    rows.iter().try_fold(0u64, |total, row| {
        total.checked_add(row.bulk).ok_or_else(|| {
            RuntimeError::ExecutionLimit("aggregate traverser bulk overflow".into())
        })
    })
}

fn add_weighted_integer(total: i64, value: i64, weight: u64) -> IrResult<i64> {
    i64::try_from(i128::from(total) + i128::from(value) * i128::from(weight))
        .map_err(|_| RuntimeError::Runtime("integer sum overflow".into()))
}

fn aggregate_values(
    expr: &IrExpr,
    rows: &[Row],
    graph: &PropertyGraph,
    distinct: bool,
) -> IrResult<Vec<Value>> {
    Ok(aggregate_weighted_values(expr, rows, graph, distinct)?
        .into_iter()
        .map(|(value, _)| value)
        .collect())
}

/// Keep bulk as a weight rather than expanding potentially huge frontiers.
fn aggregate_weighted_values(
    expr: &IrExpr,
    rows: &[Row],
    graph: &PropertyGraph,
    distinct: bool,
) -> IrResult<Vec<(Value, u64)>> {
    let mut values = Vec::new();
    let mut seen = BTreeSet::new();
    for row in rows {
        if row.bulk == 0 {
            continue;
        }
        let value = eval(expr, row, graph)?;
        if matches!(value, Value::Null) {
            continue;
        }
        if distinct && !seen.insert(encode_value(&value)) {
            continue;
        }
        values.push((value, if distinct { 1 } else { row.bulk }));
    }
    Ok(values)
}

fn aggregate_truthy(value: &Value) -> bool {
    use num_traits::Zero;

    match value {
        Value::Bool(value) => *value,
        Value::Byte(value) => *value != 0,
        Value::UInt8(value) => *value != 0,
        Value::Short(value) => *value != 0,
        Value::UInt16(value) => *value != 0,
        Value::Int(value) | Value::Long(value) => *value != 0,
        Value::UInt32(value) => *value != 0,
        Value::UInt64(value) => *value != 0,
        Value::Float32(value) => !value.is_nan() && *value != 0.0,
        Value::Float(value) => !value.is_nan() && *value != 0.0,
        Value::BigInt(value) => !value.is_zero(),
        Value::UInt128(value) => !value.is_zero(),
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
    // Discrete percentiles select the smallest rank whose cumulative
    // distribution reaches p (nearest-rank), with p=0 selecting the first.
    let index = ((percentile * values.len() as f64).ceil() as usize).saturating_sub(1);
    values[index]
}

pub(crate) fn flatten_group_lists(value: Value) -> Value {
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

#[cfg(test)]
mod tests {
    use num_bigint::BigInt;

    use super::*;

    #[test]
    fn sum_and_mean_weight_bulk_without_expanding_rows() {
        let graph = PropertyGraph::new();
        let mut first = Row::new().with("current", Value::Int(2));
        first.bulk = 1_000_000_000;
        let mut second = Row::new().with("current", Value::Int(8));
        second.bulk = 3_000_000_000;
        let rows = [first, second];
        let mut agg = AggCall {
            kind: AggKind::Sum,
            alias: "sum".into(),
            arg: Some(IrExpr::Binding("current".into())),
            distinct: false,
        };
        assert_eq!(
            compute_aggregate(&agg, &rows, &graph).unwrap(),
            Value::Int(26_000_000_000)
        );
        agg.kind = AggKind::Avg;
        assert_eq!(
            compute_aggregate(&agg, &rows, &graph).unwrap(),
            Value::Float(6.5)
        );
        agg.distinct = true;
        assert_eq!(
            compute_aggregate(&agg, &rows, &graph).unwrap(),
            Value::Float(5.0)
        );
        agg.kind = AggKind::Sum;
        assert_eq!(
            compute_aggregate(&agg, &rows, &graph).unwrap(),
            Value::Int(10)
        );
    }

    #[test]
    fn count_if_truthiness_includes_unsigned_numbers() {
        assert!(aggregate_truthy(&Value::UInt8(1)));
        assert!(aggregate_truthy(&Value::UInt16(1)));
        assert!(aggregate_truthy(&Value::UInt32(1)));
        assert!(aggregate_truthy(&Value::UInt64(1)));
        assert!(aggregate_truthy(&Value::UInt128(BigInt::from(1))));

        assert!(!aggregate_truthy(&Value::UInt8(0)));
        assert!(!aggregate_truthy(&Value::UInt16(0)));
        assert!(!aggregate_truthy(&Value::UInt32(0)));
        assert!(!aggregate_truthy(&Value::UInt64(0)));
        assert!(!aggregate_truthy(&Value::UInt128(BigInt::from(0))));
    }
}

const GROUP_MEMBERS: &str = "__gremlin_group_members";

/// Detach the first supported reducing barrier from its post-processing
/// traversal. Its prefix has already been captured at contribution insertion.
pub(crate) fn split_writer_finalizer(node: &mut crate::ir::plan::Node) -> Option<(crate::ir::plan::Node, AggKind)> {
    use crate::ir::plan::Node;
    if let Node::GraphAggregate { group, aggs, input, .. } = node {
        if group.is_empty() && aggs.len() == 1 && aggs[0].alias == "current"
            && matches!(aggs[0].kind, AggKind::CountBulk | AggKind::CollectTraversers)
            && matches!(input.as_ref(), Node::GraphCorrelate { bindings } if bindings == &[GROUP_MEMBERS])
        {
            let kind = aggs[0].kind;
            return Some((std::mem::replace(node, group_members_source()), kind));
        }
    }
    let input = match node {
        Node::GraphFilter { input, .. } | Node::GraphProject { input, .. }
        | Node::GraphCurrentProject { input, .. } | Node::GraphBind { input, .. }
        | Node::GraphExpand { input, .. } | Node::GraphUnwind { input, .. }
        | Node::GraphSelect { input, .. } | Node::GraphSideEffect { input, .. }
        | Node::GraphReadSideEffect { input, .. } | Node::GraphGroupSideEffect { input, .. }
        | Node::GraphGroupCountSideEffect { input, .. } | Node::GraphChoose { input, .. }
        | Node::GraphCoalesce { input, .. } | Node::GraphPathFilter { input, .. }
        | Node::GraphSetProperty { input, .. } | Node::GraphCreate { input, .. }
        | Node::GraphDelete { input, .. } | Node::GraphAggregate { input, .. }
        | Node::GraphGroupMap { input, .. } | Node::GraphDistinct { input, .. }
        | Node::GraphSort { input, .. } | Node::GraphSlice { input, .. }
        | Node::GraphSliceExpr { input, .. } | Node::GraphBarrier { input, .. } => input,
        Node::GraphApply { left, .. } => left,
        Node::GraphProcedureCall { input: Some(input), .. } => input,
        _ => return None,
    };
    split_writer_finalizer(input)
}

fn group_members_source() -> crate::ir::plan::Node {
    crate::ir::plan::Node::GraphCorrelate {
        bindings: vec![GROUP_MEMBERS.into()],
    }
}

/// Find the first barrier on the main value traversal, excluding barriers
/// in per-traverser child traversals. Replace its input with stored members.
pub(crate) fn split_group_prefix(node: &mut crate::ir::plan::Node) -> Option<crate::ir::plan::Node> {
    use crate::ir::plan::Node;
    let barrier = matches!(
        node,
        Node::GraphAggregate { .. }
            | Node::GraphGroupMap { .. }
            | Node::GraphDistinct { .. }
            | Node::GraphSort { .. }
            | Node::GraphSlice { .. }
            | Node::GraphSliceExpr { .. }
            | Node::GraphBarrier { .. }
            | Node::GraphSample { kind: crate::ir::plan::SampleKind::Global(_), .. }
    );
    let input = match node {
        Node::GraphAggregate { input, .. }
        | Node::GraphGroupMap { input, .. }
        | Node::GraphDistinct { input, .. }
        | Node::GraphSort { input, .. }
        | Node::GraphSample { input, .. }
        | Node::GraphSlice { input, .. }
        | Node::GraphSliceExpr { input, .. }
        | Node::GraphBarrier { input, .. }
        | Node::GraphFilter { input, .. }
        | Node::GraphProject { input, .. }
        | Node::GraphCurrentProject { input, .. }
        | Node::GraphBind { input, .. }
        | Node::GraphExpand { input, .. }
        | Node::GraphUnwind { input, .. }
        | Node::GraphSelect { input, .. }
        | Node::GraphSideEffect { input, .. }
        | Node::GraphReadSideEffect { input, .. }
        | Node::GraphGroupSideEffect { input, .. }
        | Node::GraphGroupCountSideEffect { input, .. }
        | Node::GraphChoose { input, .. }
        | Node::GraphCoalesce { input, .. }
        | Node::GraphPathFilter { input, .. }
        | Node::GraphSetProperty { input, .. }
        | Node::GraphCreate { input, .. }
        | Node::GraphDelete { input, .. } => input,
        Node::GraphApply { left, .. } => left,
        Node::GraphProcedureCall { input: Some(input), .. } => input,
        _ => return None,
    };
    if let Some(prefix) = split_group_prefix(input) {
        return Some(prefix);
    }
    barrier.then(|| *std::mem::replace(input, group_members_source().boxed()))
}
