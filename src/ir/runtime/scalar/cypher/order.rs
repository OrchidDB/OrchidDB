//! Cypher's total ordering, including heterogeneous and nested collections.
use std::cmp::Ordering;
use crate::ir::value::Value;
use crate::ir::runtime::expr::compare_values;

pub(crate) fn compare(left: &Value, right: &Value) -> Ordering {
    fn rank(value: &Value) -> u8 {
        match value {
            Value::Map(_) => 0,
            Value::Node { .. } => 1,
            Value::Edge { .. } => 2,
            Value::List(_) => 3,
            Value::Path(_) => 4,
            Value::Temporal(_) | Value::DateTime(_) => 5,
            Value::String(_) => 6,
            Value::Bool(_) => 7,
            Value::Null => 9,
            _ => 8,
        }
    }
    let category = rank(left).cmp(&rank(right));
    if category != Ordering::Equal { return category; }
    match (left, right) {
        (Value::List(a), Value::List(b)) | (Value::Path(a), Value::Path(b)) => {
            for (a,b) in a.iter().zip(b) {
                let order = compare(a,b);
                if order != Ordering::Equal { return order; }
            }
            a.len().cmp(&b.len())
        }
        _ => {
            let nan = |value: &Value| matches!(value, Value::Float(v) if v.is_nan())
                || matches!(value, Value::Float32(v) if v.is_nan());
            match (nan(left),nan(right)) {
                (true,true) => Ordering::Equal,
                (true,false) => Ordering::Greater,
                (false,true) => Ordering::Less,
                _ => left.three_valued_cmp(right).unwrap_or_else(||compare_values(left,right)),
            }
        }
    }
}
