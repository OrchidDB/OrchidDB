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

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Cypher `null` / SPARQL unbound. Distinct from `Unproductive`.
    Null,
    Bool(bool),
    Byte(i8),
    Short(i16),
    Int(i64),
    Long(i64),
    Float32(f32),
    Float(f64),
    /// Arbitrary-precision integer — Gremlin `BigInteger` / `GType.BIGINT`.
    /// Promoted from `Int` when an `asNumber(GType.BIGINT)` cast hits.
    BigInt(BigInt),
    /// Arbitrary-precision decimal — Gremlin `BigDecimal` /
    /// `GType.BIGDECIMAL`. Carries the type identity so
    /// `P.typeOf(GType.BIGDECIMAL)` matches and the harness's
    /// `d[N].m` rendering kicks in.
    BigDecimal(BigDecimal),
    DateTime(String),
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
            Self::Short(_) => "short",
            Self::Int(_) => "int",
            Self::Long(_) => "long",
            Self::Float32(_) => "float",
            Self::Float(_) => "float",
            Self::BigInt(_) => "bigint",
            Self::BigDecimal(_) => "bigdecimal",
            Self::DateTime(_) => "datetime",
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
            Self::Short(value) => Some(*value as i64),
            Self::Int(value) => Some(*value),
            Self::Long(value) => Some(*value),
            Self::Float32(value) => Some(*value as i64),
            Self::Float(value) => Some(*value as i64),
            Self::BigInt(value) => value.to_i64(),
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
            (Self::String(a), Self::String(b)) => a == b,
            (Self::DateTime(a), Self::DateTime(b)) => a == b,
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
            (Self::List(a), Self::List(b)) => a == b,
            (Self::Map(a), Self::Map(b)) => a == b,
            (Self::Path(a), Self::Path(b)) => a == b,
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
            (Self::String(a), Self::String(b)) => a.cmp(b),
            (Self::DateTime(a), Self::DateTime(b)) => a.cmp(b),
            (Self::Bool(a), Self::Bool(b)) => a.cmp(b),
            _ => return None,
        })
    }
}
