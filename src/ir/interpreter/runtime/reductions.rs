//! List reductions (sum/min/max/mean), fold_reduce, sack ops.
//!
//! Extracted from `interpreter.rs` lines 2892..2996.

use crate::ir::value::Value;

/// `min(local)` / `max(local)` over any Comparable values (TinkerPop
/// orderability): numbers reduce numerically; anything else (strings,
/// dates, ...) reduces via the cross-type total order. Nulls are ignored;
/// an empty / all-null list yields Null.
pub(crate) fn reduce_list_orderable(items: &[Value], op: &str) -> Value {
    let non_null: Vec<&Value> = items
        .iter()
        .filter(|v| !matches!(v, Value::Null))
        .collect();
    if non_null.is_empty() {
        return Value::Null;
    }
    let all_numeric = non_null.iter().all(|v| {
        matches!(
            v,
            Value::Byte(_)
                | Value::Short(_)
                | Value::Int(_)
                | Value::Long(_)
                | Value::Float32(_)
                | Value::Float(_)
        )
    });
    if all_numeric {
        return reduce_list_numeric(items, op);
    }
    use crate::ir::interpreter::expr::compare_values;
    let mut best = non_null[0];
    for candidate in &non_null[1..] {
        let ord = compare_values(candidate, best);
        let better = match op {
            "min" => ord == std::cmp::Ordering::Less,
            _ => ord == std::cmp::Ordering::Greater,
        };
        if better {
            best = candidate;
        }
    }
    best.clone()
}

pub(crate) fn reduce_list_numeric(items: &[Value], op: &str) -> Value {
    if items.is_empty() {
        return Value::Null;
    }
    let mut floats = true;
    let mut acc_int: Option<i64> = None;
    let mut acc_f: Option<f64> = None;
    let mut count: i64 = 0;
    for item in items {
        match item {
            Value::Byte(n) => {
                let n = *n as i64;
                count += 1;
                if let Some(prev) = acc_f {
                    acc_f = Some(combine_numeric(prev, n as f64, op));
                } else if let Some(prev) = acc_int {
                    acc_int = Some(combine_int(prev, n, op));
                } else {
                    acc_int = Some(n);
                    floats = false;
                }
            }
            Value::Short(n) => {
                let n = *n as i64;
                count += 1;
                if let Some(prev) = acc_f {
                    acc_f = Some(combine_numeric(prev, n as f64, op));
                } else if let Some(prev) = acc_int {
                    acc_int = Some(combine_int(prev, n, op));
                } else {
                    acc_int = Some(n);
                    floats = false;
                }
            }
            Value::Int(n) | Value::Long(n) => {
                count += 1;
                if let Some(prev) = acc_f {
                    acc_f = Some(combine_numeric(prev, *n as f64, op));
                } else if let Some(prev) = acc_int {
                    acc_int = Some(combine_int(prev, *n, op));
                } else {
                    acc_int = Some(*n);
                    floats = false;
                }
            }
            Value::Float32(f) => {
                count += 1;
                let prev = acc_f.unwrap_or_else(|| acc_int.map(|n| n as f64).unwrap_or(0.0));
                acc_f = Some(combine_numeric(prev, *f as f64, op));
                acc_int = None;
                floats = true;
            }
            Value::Float(f) => {
                count += 1;
                let prev = acc_f.unwrap_or_else(|| acc_int.map(|n| n as f64).unwrap_or(0.0));
                acc_f = Some(combine_numeric(prev, *f, op));
                acc_int = None;
                floats = true;
            }
            _ => continue,
        }
    }
    let result = if let Some(f) = acc_f {
        if op == "mean" && count > 0 {
            Value::Float(f / count as f64)
        } else {
            Value::Float(f)
        }
    } else if let Some(n) = acc_int {
        if op == "mean" && count > 0 {
            Value::Float(n as f64 / count as f64)
        } else {
            Value::Int(n)
        }
    } else {
        Value::Null
    };
    let _ = floats;
    result
}

pub(crate) fn combine_int(prev: i64, next: i64, op: &str) -> i64 {
    match op {
        "sum" | "mean" => prev + next,
        "min" => prev.min(next),
        "max" => prev.max(next),
        _ => prev,
    }
}

pub(crate) fn combine_numeric(prev: f64, next: f64, op: &str) -> f64 {
    match op {
        "sum" | "mean" => prev + next,
        "min" => prev.min(next),
        "max" => prev.max(next),
        _ => prev,
    }
}

/// `fold(seed, op)` reducer-fold runtime. Folds left over `items`
/// applying the named operator, starting from `seed`.
pub(crate) fn fold_reduce_op(items: &[Value], seed: &Value, op: &str) -> Value {
    let mut acc = seed.clone();
    for item in items {
        acc = apply_sack_op(&acc, item, op);
    }
    acc
}

pub(crate) fn apply_sack_op(lhs: &Value, rhs: &Value, op: &str) -> Value {
    use Value::*;
    match (op, lhs, rhs) {
        ("sum", Int(a), Int(b)) => Int(a + b),
        ("sum", Long(a), Long(b)) => Long(a + b),
        ("sum", Float(a), Float(b)) => Float(a + b),
        ("sum", Float32(a), Float32(b)) => Float32(a + b),
        ("sum", Int(a), Float(b)) | ("sum", Float(b), Int(a)) => Float(*a as f64 + b),
        ("sum", Long(a), Float(b)) | ("sum", Float(b), Long(a)) => Float(*a as f64 + b),
        ("minus", Int(a), Int(b)) => Int(a - b),
        ("minus", Long(a), Long(b)) => Long(a - b),
        ("minus", Float(a), Float(b)) => Float(a - b),
        ("minus", Int(a), Float(b)) => Float(*a as f64 - b),
        ("minus", Float(a), Int(b)) => Float(a - *b as f64),
        ("mult", Int(a), Int(b)) => Int(a * b),
        ("mult", Long(a), Long(b)) => Long(a * b),
        ("mult", Float(a), Float(b)) => Float(a * b),
        ("mult", Int(a), Float(b)) | ("mult", Float(b), Int(a)) => Float(*a as f64 * b),
        ("div", Int(a), Int(b)) if *b != 0 => Int(a / b),
        ("div", Long(a), Long(b)) if *b != 0 => Long(a / b),
        ("div", Float(a), Float(b)) if *b != 0.0 => Float(a / b),
        ("div", Int(a), Float(b)) if *b != 0.0 => Float(*a as f64 / b),
        ("div", Float(a), Int(b)) if *b != 0 => Float(a / *b as f64),
        ("min", Int(a), Int(b)) => Int(*a.min(b)),
        ("min", Long(a), Long(b)) => Long(*a.min(b)),
        ("min", Float(a), Float(b)) => Float(a.min(*b)),
        ("min", Int(a), Float(b)) | ("min", Float(b), Int(a)) => Float((*a as f64).min(*b)),
        ("max", Int(a), Int(b)) => Int(*a.max(b)),
        ("max", Long(a), Long(b)) => Long(*a.max(b)),
        ("max", Float(a), Float(b)) => Float(a.max(*b)),
        ("max", Int(a), Float(b)) | ("max", Float(b), Int(a)) => Float((*a as f64).max(*b)),
        (op @ ("sum" | "minus" | "mult" | "div" | "min" | "max"), a, b)
            if numeric_f64(a).is_some() && numeric_f64(b).is_some() =>
        {
            apply_numeric_sack_op(a, b, op)
        }
        ("assign", _, b) => b.clone(),
        ("and", Bool(a), Bool(b)) => Bool(*a && *b),
        ("or", Bool(a), Bool(b)) => Bool(*a || *b),
        ("addAll", List(a), List(b)) => {
            let mut out = a.clone();
            out.extend(b.iter().cloned());
            List(out)
        }
        ("addAll", Map(a), Map(b)) => {
            let mut out = a.clone();
            for (k, v) in b {
                out.insert(k.clone(), v.clone());
            }
            Map(out)
        }
        ("addAll", Map(a), List(b)) => {
            // Folding a stream of maps into a map seed: each list item is
            // itself a map to merge in.
            let mut out = a.clone();
            for item in b {
                if let Map(m) = item {
                    for (k, v) in m {
                        out.insert(k.clone(), v.clone());
                    }
                }
            }
            Map(out)
        }
        ("addAll", List(a), item) => {
            let mut out = a.clone();
            out.push(item.clone());
            List(out)
        }
        _ => Null,
    }
}

fn apply_numeric_sack_op(lhs: &Value, rhs: &Value, op: &str) -> Value {
    let Some(a) = numeric_f64(lhs) else {
        return Value::Null;
    };
    let Some(b) = numeric_f64(rhs) else {
        return Value::Null;
    };
    if op == "div" && b == 0.0 {
        return Value::Null;
    }
    let out = match op {
        "sum" => a + b,
        "minus" => a - b,
        "mult" => a * b,
        "div" => a / b,
        "min" => a.min(b),
        "max" => a.max(b),
        _ => return Value::Null,
    };
    if matches!(lhs, Value::Float(_)) || matches!(rhs, Value::Float(_)) {
        Value::Float(out)
    } else if matches!(lhs, Value::Float32(_)) || matches!(rhs, Value::Float32(_)) {
        Value::Float32(out as f32)
    } else {
        Value::Int(out as i64)
    }
}

fn numeric_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Byte(n) => Some(*n as f64),
        Value::Short(n) => Some(*n as f64),
        Value::Int(n) | Value::Long(n) => Some(*n as f64),
        Value::Float32(n) => Some(*n as f64),
        Value::Float(n) => Some(*n),
        _ => None,
    }
}
