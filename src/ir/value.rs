//! Dynamic graph values produced and consumed by the interpreter.
//!
//! The interpreter materializes the full result of every binding as a
//! `Value`. We use a typed enum (rather than Arrow scalars) so that node and
//! edge identifiers, property maps, and lists can flow through expression
//! evaluation cleanly. Conversion to Arrow record batches happens at
//! `GraphReturn` boundaries.

use std::collections::BTreeMap;

use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use num_traits::FromPrimitive;

pub const STRUCT_ORDER_KEY: &str = "__new_graph_struct_order";
pub const STRUCT_TYPES_KEY: &str = "__new_graph_struct_types";

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Cypher `null` / SPARQL unbound. Distinct from `Unproductive`.
    Null,
    Bool(bool),
    Byte(i8),
    UInt8(u8),
    Short(i16),
    UInt16(u16),
    Int(i64),
    UInt32(u32),
    Long(i64),
    UInt64(u64),
    Float32(f32),
    Float(f64),
    /// Arbitrary-precision integer — Gremlin `BigInteger` / `GType.BIGINT`.
    /// Promoted from `Int` when an `asNumber(GType.BIGINT)` cast hits.
    BigInt(BigInt),
    UInt128(BigInt),
    /// Arbitrary-precision decimal — Gremlin `BigDecimal` /
    /// `GType.BIGDECIMAL`. Carries the type identity so
    /// `P.typeOf(GType.BIGDECIMAL)` matches and the harness's
    /// `d[N].m` rendering kicks in.
    BigDecimal(BigDecimal),
    DateTime(String),
    /// Kuzu/Cypher internal id: `table_id:offset`.
    InternalId {
        table: i64,
        offset: i64,
    },
    String(String),
    /// A property-graph node. The id is the catalog row id within the
    /// `nodes(<label>)` relation.
    Node {
        label: String,
        id: i64,
    },
    /// A property-graph edge. Carries source and target node identifiers so
    /// that `r.src` / `r.dst` and `EndpointVertex` work without a re-scan.
    Edge {
        rel_type: String,
        id: i64,
        src_label: String,
        src_id: i64,
        dst_label: String,
        dst_id: i64,
        /// Optional Cypher recursive-relationship projection keys. `None`
        /// means render the edge's full catalog property bag.
        projected_properties: Option<Vec<String>>,
    },
    List(Vec<Value>),
    Map(BTreeMap<String, Value>),
    /// Path objects produced by `pathMaterialization=NodesAndRelationships`.
    /// The first element is always a node; nodes and edges alternate.
    Path(Vec<Value>),
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::Byte(_) => "byte",
            Self::UInt8(_) => "uint8",
            Self::Short(_) => "short",
            Self::UInt16(_) => "uint16",
            Self::Int(_) => "int",
            Self::UInt32(_) => "uint32",
            Self::Long(_) => "long",
            Self::UInt64(_) => "uint64",
            Self::Float32(_) => "float",
            Self::Float(_) => "float",
            Self::BigInt(_) => "bigint",
            Self::UInt128(_) => "uint128",
            Self::BigDecimal(_) => "bigdecimal",
            Self::DateTime(_) => "datetime",
            Self::InternalId { .. } => "internal_id",
            Self::String(_) => "string",
            Self::Node { .. } => "node",
            Self::Edge { .. } => "edge",
            Self::List(_) => "list",
            Self::Map(_) => "map",
            Self::Path(_) => "path",
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        use num_traits::ToPrimitive;
        match self {
            Self::Byte(value) => Some(*value as i64),
            Self::UInt8(value) => Some(*value as i64),
            Self::Short(value) => Some(*value as i64),
            Self::UInt16(value) => Some(*value as i64),
            Self::Int(value) => Some(*value),
            Self::UInt32(value) => Some(*value as i64),
            Self::Long(value) => Some(*value),
            Self::UInt64(value) => i64::try_from(*value).ok(),
            Self::Float32(value) => Some(*value as i64),
            Self::Float(value) => Some(*value as i64),
            Self::BigInt(value) => value.to_i64(),
            Self::UInt128(value) => value.to_i64(),
            Self::BigDecimal(value) => value.to_i64(),
            _ => None,
        }
    }

    pub fn truthy(&self) -> bool {
        matches!(self, Self::Bool(true))
    }

    /// Equality with SQL-style three-valued semantics.
    /// `null = anything` is `null`. `Unproductive` propagates as `null`
    /// through the interpreter (we only ever store `null` here — unproductive
    /// is realized by dropping the row entirely, never by producing a value).
    pub fn three_valued_eq(&self, other: &Self) -> Option<bool> {
        fn numeric_decimal(value: &Value) -> Option<BigDecimal> {
            use bigdecimal::FromPrimitive;
            match value {
                Value::Byte(n) => Some(BigDecimal::from(*n)),
                Value::UInt8(n) => Some(BigDecimal::from(*n)),
                Value::Short(n) => Some(BigDecimal::from(*n)),
                Value::UInt16(n) => Some(BigDecimal::from(*n)),
                Value::Int(n) => Some(BigDecimal::from(*n)),
                Value::UInt32(n) => Some(BigDecimal::from(*n)),
                Value::Long(n) => Some(BigDecimal::from(*n)),
                Value::UInt64(n) => Some(BigDecimal::from(*n)),
                Value::Float32(n) => BigDecimal::from_f32(*n),
                Value::Float(n) => BigDecimal::from_f64(*n),
                Value::BigInt(n) => Some(BigDecimal::from(n.clone())),
                Value::UInt128(n) => Some(BigDecimal::from(n.clone())),
                Value::BigDecimal(n) => Some(n.clone()),
                _ => None,
            }
        }
        // TinkerPop equality: `null == null` is true (so `P.eq(null)` matches
        // a null traverser); `null == anything-else` is unknown (None).
        match (self, other) {
            (Self::Null, Self::Null) => return Some(true),
            (Self::Null, _) | (_, Self::Null) => return None,
            _ => {}
        }
        Some(match (self, other) {
            (a, b) if numeric_decimal(a).is_some() || numeric_decimal(b).is_some() => {
                match (numeric_decimal(a), numeric_decimal(b)) {
                    (Some(a), Some(b)) => a == b,
                    _ => false,
                }
            }
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Int(a), Self::Float(b)) | (Self::Float(b), Self::Int(a)) => (*a as f64) == *b,
            // Arbitrary-precision numerics: cross-promote to BigDecimal
            // so `BIGDECIMAL == 29` and `BIGINT == 29` etc. behave like
            // Java's mixed-numeric `==`.
            (Self::BigInt(a), Self::BigInt(b)) => a == b,
            (Self::BigInt(a), Self::Int(b)) | (Self::Int(b), Self::BigInt(a)) => {
                a == &BigInt::from(*b)
            }
            (Self::BigDecimal(a), Self::BigDecimal(b)) => a == b,
            (Self::BigDecimal(a), Self::Int(b)) | (Self::Int(b), Self::BigDecimal(a)) => {
                a == &BigDecimal::from(*b)
            }
            (Self::BigDecimal(a), Self::Float(b)) | (Self::Float(b), Self::BigDecimal(a)) => {
                BigDecimal::from_f64(*b).map(|d| a == &d).unwrap_or(false)
            }
            (Self::BigDecimal(a), Self::BigInt(b)) | (Self::BigInt(b), Self::BigDecimal(a)) => {
                a == &BigDecimal::from(b.clone())
            }
            (Self::BigInt(a), Self::Float(b)) | (Self::Float(b), Self::BigInt(a)) => {
                BigDecimal::from_f64(*b)
                    .map(|d| BigDecimal::from(a.clone()) == d)
                    .unwrap_or(false)
            }
            (Self::String(a), Self::String(b)) => {
                if let (Some(a), Some(b)) = (temporal_sort_key(a), temporal_sort_key(b)) {
                    a == b
                } else if let (Some(a), Some(b)) = (interval_sort_key(a), interval_sort_key(b)) {
                    a == b
                } else {
                    a == b
                }
            }
            (Self::DateTime(a), Self::DateTime(b)) => {
                if let (Some(a), Some(b)) = (temporal_sort_key(a), temporal_sort_key(b)) {
                    a == b
                } else {
                    a == b
                }
            }
            // The Cypher loader stores DATE / TIMESTAMP columns as
            // String values in Arrow (no native date type yet); cross-
            // compare DateTime literals against those String columns so
            // `a.birthdate = date('1900-1-1')` resolves correctly.
            (Self::DateTime(a), Self::String(b)) | (Self::String(b), Self::DateTime(a)) => {
                if let (Some(a), Some(b)) = (temporal_sort_key(a), temporal_sort_key(b)) {
                    a == b
                } else {
                    a == b
                }
            }
            (
                Self::InternalId {
                    table: ta,
                    offset: oa,
                },
                Self::InternalId {
                    table: tb,
                    offset: ob,
                },
            ) => ta == tb && oa == ob,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Node { label: la, id: ia }, Self::Node { label: lb, id: ib }) => {
                la == lb && ia == ib
            }
            (
                Self::Edge {
                    rel_type: ta,
                    id: ia,
                    ..
                },
                Self::Edge {
                    rel_type: tb,
                    id: ib,
                    ..
                },
            ) => ta == tb && ia == ib,
            (Self::List(a), Self::List(b)) | (Self::Path(a), Self::Path(b)) => {
                semantic_slice_eq(a, b)
            }
            (Self::Map(a), Self::Map(b)) => semantic_map_eq(a, b),
            _ => false,
        })
    }

    pub fn three_valued_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        fn numeric_decimal(value: &Value) -> Option<BigDecimal> {
            use bigdecimal::FromPrimitive;
            match value {
                Value::Byte(n) => Some(BigDecimal::from(*n)),
                Value::Short(n) => Some(BigDecimal::from(*n)),
                Value::Int(n) => Some(BigDecimal::from(*n)),
                Value::Long(n) => Some(BigDecimal::from(*n)),
                Value::Float32(n) => BigDecimal::from_f32(*n),
                Value::Float(n) => BigDecimal::from_f64(*n),
                Value::BigInt(n) => Some(BigDecimal::from(n.clone())),
                Value::BigDecimal(n) => Some(n.clone()),
                _ => None,
            }
        }
        // TinkerPop comparability: `null` is comparable only with `null`,
        // and the result is `Equal` (so `P.gte(null)` / `P.lte(null)` match
        // a null traverser, but `P.gt(null)` / `P.lt(null)` do not).
        match (self, other) {
            (Self::Null, Self::Null) => return Some(std::cmp::Ordering::Equal),
            (Self::Null, _) | (_, Self::Null) => return None,
            _ => {}
        }
        Some(match (self, other) {
            (a, b) if numeric_decimal(a).is_some() || numeric_decimal(b).is_some() => {
                match (numeric_decimal(a), numeric_decimal(b)) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    _ => return None,
                }
            }
            (Self::Int(a), Self::Int(b)) => a.cmp(b),
            (Self::Float(a), Self::Float(b)) => a.partial_cmp(b)?,
            (Self::Int(a), Self::Float(b)) => (*a as f64).partial_cmp(b)?,
            (Self::Float(a), Self::Int(b)) => a.partial_cmp(&(*b as f64))?,
            // Arbitrary-precision numerics promote up.
            (Self::BigInt(a), Self::BigInt(b)) => a.cmp(b),
            (Self::BigInt(a), Self::Int(b)) => a.cmp(&BigInt::from(*b)),
            (Self::Int(a), Self::BigInt(b)) => BigInt::from(*a).cmp(b),
            (Self::BigDecimal(a), Self::BigDecimal(b)) => a.cmp(b),
            (Self::BigDecimal(a), Self::Int(b)) => a.cmp(&BigDecimal::from(*b)),
            (Self::Int(a), Self::BigDecimal(b)) => BigDecimal::from(*a).cmp(b),
            (Self::BigDecimal(a), Self::Float(b)) => match BigDecimal::from_f64(*b) {
                Some(d) => a.cmp(&d),
                None => return None,
            },
            (Self::Float(a), Self::BigDecimal(b)) => match BigDecimal::from_f64(*a) {
                Some(d) => d.cmp(b),
                None => return None,
            },
            (Self::BigDecimal(a), Self::BigInt(b)) => a.cmp(&BigDecimal::from(b.clone())),
            (Self::BigInt(a), Self::BigDecimal(b)) => BigDecimal::from(a.clone()).cmp(b),
            (Self::BigInt(a), Self::Float(b)) => match BigDecimal::from_f64(*b) {
                Some(d) => BigDecimal::from(a.clone()).cmp(&d),
                None => return None,
            },
            (Self::Float(a), Self::BigInt(b)) => match BigDecimal::from_f64(*a) {
                Some(d) => d.cmp(&BigDecimal::from(b.clone())),
                None => return None,
            },
            (Self::String(a), Self::String(b)) => {
                if let (Some(a), Some(b)) = (temporal_sort_key(a), temporal_sort_key(b)) {
                    a.cmp(&b)
                } else if let (Some(a), Some(b)) = (interval_sort_key(a), interval_sort_key(b)) {
                    a.cmp(&b)
                } else if let Some(ordering) = blob_string_ordering(a, b) {
                    ordering
                } else {
                    a.cmp(b)
                }
            }
            (Self::DateTime(a), Self::DateTime(b)) => {
                if let (Some(a), Some(b)) = (temporal_sort_key(a), temporal_sort_key(b)) {
                    a.cmp(&b)
                } else {
                    a.cmp(b)
                }
            }
            (Self::DateTime(a), Self::String(b)) | (Self::String(a), Self::DateTime(b)) => {
                match (temporal_sort_key(a), temporal_sort_key(b)) {
                    (Some(a), Some(b)) => a.cmp(&b),
                    _ => return None,
                }
            }
            (
                Self::InternalId {
                    table: ta,
                    offset: oa,
                },
                Self::InternalId {
                    table: tb,
                    offset: ob,
                },
            ) => (ta, oa).cmp(&(tb, ob)),
            (Self::Bool(a), Self::Bool(b)) => a.cmp(b),
            (Self::List(a), Self::List(b)) | (Self::Path(a), Self::Path(b)) => {
                nested_slice_cmp(a, b)?
            }
            (Self::Map(a), Self::Map(b)) => nested_map_cmp(a, b)?,
            _ => return None,
        })
    }
}

fn temporal_sort_key(value: &str) -> Option<(i64, u32, u32, i64, i64, i64, i64)> {
    let inner = value
        .trim()
        .strip_prefix("dt[")
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(value.trim());
    let (date, time) = if let Some((date, time)) = inner.split_once('T') {
        (date, time)
    } else if let Some((date, time)) = inner.split_once(' ') {
        if time.eq_ignore_ascii_case("(BC)") {
            return None;
        }
        (date, time)
    } else {
        (inner, "00:00:00")
    };
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse().ok()?;
    let month = date_parts.next()?.parse().ok()?;
    let day = date_parts.next()?.parse().ok()?;
    if date_parts.next().is_some() {
        return None;
    }
    let time = strip_temporal_timezone(time);
    let mut time_parts = time.split(':');
    let hour = time_parts.next().unwrap_or("0").parse().ok()?;
    let minute = time_parts.next().unwrap_or("0").parse().ok()?;
    let second_part = time_parts.next().unwrap_or("0");
    if time_parts.next().is_some() {
        return None;
    }
    let (second_text, fraction) = second_part.split_once('.').unwrap_or((second_part, ""));
    let second = second_text.parse().ok()?;
    let micros = parse_fraction_micros(fraction)?;
    Some((year, month, day, hour, minute, second, micros))
}

fn interval_sort_key(value: &str) -> Option<(i64, i64, i128)> {
    let mut months = 0i64;
    let mut days = 0i64;
    let mut micros = 0i128;
    let parts = value.trim().split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    let mut index = 0;
    while index < parts.len() {
        if let Some(time_micros) = parse_interval_time(parts[index]) {
            micros = micros.checked_add(time_micros)?;
            index += 1;
            continue;
        }
        let amount = parts[index].parse::<i64>().ok()?;
        let unit = parts.get(index + 1)?.to_ascii_lowercase();
        match unit.as_str() {
            "year" | "years" | "y" | "yr" | "yrs" => {
                months = months.checked_add(amount.checked_mul(12)?)?;
            }
            "month" | "months" | "mon" | "mons" => months = months.checked_add(amount)?,
            "week" | "weeks" => days = days.checked_add(amount.checked_mul(7)?)?,
            "day" | "days" | "d" => days = days.checked_add(amount)?,
            "hour" | "hours" | "h" | "hr" | "hrs" => {
                micros = micros.checked_add((amount as i128).checked_mul(3_600_000_000)?)?;
            }
            "minute" | "minutes" | "m" | "min" | "mins" => {
                micros = micros.checked_add((amount as i128).checked_mul(60_000_000)?)?;
            }
            "second" | "seconds" | "s" | "sec" | "secs" => {
                micros = micros.checked_add((amount as i128).checked_mul(1_000_000)?)?;
            }
            "millisecond" | "milliseconds" | "ms" => {
                micros = micros.checked_add((amount as i128).checked_mul(1_000)?)?;
            }
            "microsecond" | "microseconds" | "us" | "µs" => {
                micros = micros.checked_add(amount as i128)?;
            }
            _ => return None,
        }
        index += 2;
    }
    let days = days.checked_add(months.checked_mul(30)?)?;
    Some((0, days, micros))
}

fn strip_temporal_timezone(time: &str) -> &str {
    if let Some(time) = time.strip_suffix('Z') {
        return time;
    }
    time.char_indices()
        .rev()
        .find_map(|(idx, ch)| (idx > 0 && (ch == '+' || ch == '-')).then_some(&time[..idx]))
        .unwrap_or(time)
}

fn parse_fraction_micros(fraction: &str) -> Option<i64> {
    let digits = fraction
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .take(6)
        .collect::<String>();
    if digits.is_empty() {
        return Some(0);
    }
    format!("{digits:0<6}").parse().ok()
}

fn parse_interval_time(value: &str) -> Option<i128> {
    let mut sign = 1i128;
    let mut text = value;
    if let Some(rest) = text.strip_prefix('-') {
        sign = -1;
        text = rest;
    }
    let parts = text.split(':').collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    let hours = parts[0].parse::<i128>().ok()?;
    let minutes = parts[1].parse::<i128>().ok()?;
    let (seconds_text, fraction) = parts[2].split_once('.').unwrap_or((parts[2], ""));
    let seconds = seconds_text.parse::<i128>().ok()?;
    let micros = parse_fraction_micros(fraction)? as i128;
    sign.checked_mul(
        hours
            .checked_mul(3_600_000_000)?
            .checked_add(minutes.checked_mul(60_000_000)?)?
            .checked_add(seconds.checked_mul(1_000_000)?)?
            .checked_add(micros)?,
    )
}

fn semantic_slice_eq(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| left.three_valued_eq(right) == Some(true))
}

fn semantic_map_eq(
    left: &std::collections::BTreeMap<String, Value>,
    right: &std::collections::BTreeMap<String, Value>,
) -> bool {
    visible_map_len(left) == visible_map_len(right)
        && left
            .iter()
            .filter(|(key, _)| is_visible_map_key(key))
            .all(|(key, left)| {
                right
                    .get(key)
                    .is_some_and(|right| left.three_valued_eq(right) == Some(true))
            })
}

fn nested_slice_cmp(left: &[Value], right: &[Value]) -> Option<std::cmp::Ordering> {
    for (left, right) in left.iter().zip(right.iter()) {
        let cmp = nested_value_cmp(left, right)?;
        if cmp != std::cmp::Ordering::Equal {
            return Some(cmp);
        }
    }
    Some(left.len().cmp(&right.len()))
}

fn nested_map_cmp(
    left: &std::collections::BTreeMap<String, Value>,
    right: &std::collections::BTreeMap<String, Value>,
) -> Option<std::cmp::Ordering> {
    if let Some(order) = map_field_order(left).or_else(|| map_field_order(right)) {
        for key in &order {
            match (left.get(key), right.get(key)) {
                (Some(left), Some(right)) => {
                    let cmp = nested_value_cmp(left, right)?;
                    if cmp != std::cmp::Ordering::Equal {
                        return Some(cmp);
                    }
                }
                (Some(_), None) => return Some(std::cmp::Ordering::Greater),
                (None, Some(_)) => return Some(std::cmp::Ordering::Less),
                (None, None) => {}
            }
        }
        let left_extra = visible_map_keys(left)
            .into_iter()
            .filter(|key| !order.iter().any(|ordered| ordered == key))
            .collect::<Vec<_>>();
        let right_extra = visible_map_keys(right)
            .into_iter()
            .filter(|key| !order.iter().any(|ordered| ordered == key))
            .collect::<Vec<_>>();
        return nested_map_entries_cmp(left, right, &left_extra, &right_extra);
    }
    let left_keys = visible_map_keys(left);
    let right_keys = visible_map_keys(right);
    nested_map_entries_cmp(left, right, &left_keys, &right_keys)
}

fn nested_map_entries_cmp(
    left: &std::collections::BTreeMap<String, Value>,
    right: &std::collections::BTreeMap<String, Value>,
    left_keys: &[String],
    right_keys: &[String],
) -> Option<std::cmp::Ordering> {
    for (left_key, right_key) in left_keys.iter().zip(right_keys.iter()) {
        let key_cmp = left_key.cmp(right_key);
        if key_cmp != std::cmp::Ordering::Equal {
            return Some(key_cmp);
        }
        let value_cmp = nested_value_cmp(left.get(left_key)?, right.get(right_key)?)?;
        if value_cmp != std::cmp::Ordering::Equal {
            return Some(value_cmp);
        }
    }
    Some(left_keys.len().cmp(&right_keys.len()))
}

fn nested_value_cmp(left: &Value, right: &Value) -> Option<std::cmp::Ordering> {
    match (left, right) {
        (Value::Null, Value::Null) => Some(std::cmp::Ordering::Equal),
        (Value::Null, _) => Some(std::cmp::Ordering::Greater),
        (_, Value::Null) => Some(std::cmp::Ordering::Less),
        (Value::List(left), Value::List(right)) | (Value::Path(left), Value::Path(right)) => {
            nested_slice_cmp(left, right)
        }
        (Value::Map(left), Value::Map(right)) => nested_map_cmp(left, right),
        _ => left.three_valued_cmp(right),
    }
}

fn blob_string_ordering(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    if !left.contains("\\x") && !right.contains("\\x") {
        return None;
    }
    Some(blob_sort_bytes(left)?.cmp(&blob_sort_bytes(right)?))
}

fn blob_sort_bytes(text: &str) -> Option<Vec<u8>> {
    let chars = text.chars().collect::<Vec<_>>();
    let mut bytes = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if !ch.is_ascii() {
            return None;
        }
        if ch == '\\' && matches!(chars.get(index + 1), Some('x' | 'X')) {
            let first = chars.get(index + 2).copied()?;
            let second = chars.get(index + 3).copied()?;
            if !first.is_ascii_hexdigit() || !second.is_ascii_hexdigit() {
                return None;
            }
            let hex = format!("{first}{second}");
            bytes.push(u8::from_str_radix(&hex, 16).ok()?);
            index += 4;
            continue;
        }
        bytes.push(ch as u8);
        index += 1;
    }
    Some(bytes)
}

fn map_field_order(map: &std::collections::BTreeMap<String, Value>) -> Option<Vec<String>> {
    let Value::List(items) = map.get(STRUCT_ORDER_KEY)? else {
        return None;
    };
    Some(
        items
            .iter()
            .filter_map(|item| match item {
                Value::String(key) if map.contains_key(key) => Some(key.clone()),
                _ => None,
            })
            .collect(),
    )
}

fn visible_map_keys(map: &std::collections::BTreeMap<String, Value>) -> Vec<String> {
    map.keys()
        .filter(|key| is_visible_map_key(key))
        .cloned()
        .collect()
}

fn visible_map_len(map: &std::collections::BTreeMap<String, Value>) -> usize {
    map.keys().filter(|key| is_visible_map_key(key)).count()
}

fn is_visible_map_key(key: &str) -> bool {
    key != STRUCT_ORDER_KEY && key != STRUCT_TYPES_KEY
}
