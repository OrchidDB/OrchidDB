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

/// Build a native Set, retaining encounter order and typed member identity.
pub fn gremlin_set(items: Vec<Value>) -> Value {
    let mut seen = std::collections::BTreeSet::new();
    let mut unique = Vec::new();
    for item in items {
        if seen.insert(set_member_key(&item)) {
            unique.push(item);
        }
    }
    Value::Set(unique)
}

/// Return native Set members; ordinary maps never represent Sets.
pub fn as_gremlin_set(value: &Value) -> Option<&[Value]> {
    match value {
        Value::Set(items) => Some(items),
        _ => None,
    }
}

fn set_eq(left: &[Value], right: &[Value]) -> bool {
    if left.len() != right.len() { return false; }
    let mut left = left.iter().map(set_member_key).collect::<Vec<_>>();
    let mut right = right.iter().map(set_member_key).collect::<Vec<_>>();
    left.sort();
    right.sort();
    left == right
}

/// Exact native identity within a Gremlin Set. This deliberately differs from
/// numeric predicates and shared scalar equality: boxed NaNs are equal,
/// signed zeros and numeric widths differ, and BigDecimal scale matters.
/// Equality, hashing and distinct all use this recursive key so nested native
/// containers cannot disagree about member identity. This is not a wire codec.
pub(crate) fn set_member_key(value: &Value) -> Vec<u8> {
    fn framed(tag: u8, parts: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
        let mut out = vec![tag];
        for part in parts {
            out.extend_from_slice(&(part.len() as u64).to_be_bytes());
            out.extend(part);
        }
        out
    }
    fn map_key(entries: impl IntoIterator<Item = (Value, Value)>) -> Vec<u8> {
        let mut pairs = entries.into_iter().map(|(key, value)| {
            framed(27, [set_member_key(&key), set_member_key(&value)])
        }).collect::<Vec<_>>();
        pairs.sort();
        framed(8, pairs)
    }
    let scalar = |tag, bytes| framed(tag, [bytes]);
    match value {
        Value::Null => vec![0],
        Value::Bool(v) => scalar(1, vec![u8::from(*v)]),
        Value::Byte(v) => scalar(12, v.to_be_bytes().to_vec()),
        Value::UInt8(v) => scalar(18, v.to_be_bytes().to_vec()),
        Value::Short(v) => scalar(13, v.to_be_bytes().to_vec()),
        Value::UInt16(v) => scalar(19, v.to_be_bytes().to_vec()),
        Value::Int(v) => scalar(2, v.to_be_bytes().to_vec()),
        Value::UInt32(v) => scalar(20, v.to_be_bytes().to_vec()),
        Value::Long(v) => scalar(14, v.to_be_bytes().to_vec()),
        Value::UInt64(v) => scalar(21, v.to_be_bytes().to_vec()),
        Value::Float32(v) => scalar(15, (if v.is_nan() { f32::NAN } else { *v }).to_be_bytes().to_vec()),
        Value::Float(v) => scalar(3, (if v.is_nan() { f64::NAN } else { *v }).to_be_bytes().to_vec()),
        Value::BigInt(v) => scalar(10, v.to_signed_bytes_be()),
        Value::UInt128(v) => scalar(22, v.to_signed_bytes_be()),
        Value::BigDecimal(v) => {
            let (integer, scale) = v.as_bigint_and_exponent();
            framed(11, [integer.to_signed_bytes_be(), scale.to_be_bytes().to_vec()])
        }
        Value::String(v) => scalar(4, v.as_bytes().to_vec()),
        Value::DateTime(v) => scalar(16, v.as_bytes().to_vec()),
        Value::Temporal(v) => scalar(30, v.encode().into_bytes()),
        Value::Token(v) => scalar(24, v.as_bytes().to_vec()),
        Value::Direction(v) => scalar(25, v.as_bytes().to_vec()),
        Value::InternalId { table, offset } => framed(17, [table.to_be_bytes().to_vec(), offset.to_be_bytes().to_vec()]),
        Value::Node { label, id } => framed(5, [label.as_bytes().to_vec(), id.to_be_bytes().to_vec()]),
        Value::Edge { rel_type, id, .. } => framed(6, [rel_type.as_bytes().to_vec(), id.to_be_bytes().to_vec()]),
        Value::VertexProperty { id, .. } => scalar(0x40, id.to_be_bytes().to_vec()),
        Value::Property { key, value, .. } => framed(0x41, [key.as_bytes().to_vec(), set_member_key(value)]),
        Value::List(items) => framed(7, items.iter().map(set_member_key)),
        Value::Path(items) => framed(9, items.iter().map(set_member_key)),
        Value::Set(items) | Value::BulkSet(items) => {
            let mut parts = items.iter().map(set_member_key).collect::<Vec<_>>();
            parts.sort();
            framed(if matches!(value, Value::Set(_)) { 28 } else { 26 }, parts)
        }
        Value::CardinalityValue { cardinality, value } => framed(29, [cardinality.as_bytes().to_vec(), set_member_key(value)]),
        Value::MapEntry(pair) => framed(27, [set_member_key(&pair.0), set_member_key(&pair.1)]),
        Value::Map(map) => map_key(map.iter().map(|(key, value)| (Value::String(key.clone()), value.clone()))),
        Value::TypedMap(entries) => map_key(entries.iter().cloned()),
    }
}

#[cfg(test)]
mod native_set_tests {
    use super::*;
    #[test]
    fn native_set_identity_is_unordered_typed_and_not_a_marker_map() {
        let a = gremlin_set(vec![Value::Int(1), Value::Long(1), Value::Int(1)]);
        let b = gremlin_set(vec![Value::Long(1), Value::Int(1)]);
        assert_eq!(a, b);
        assert_eq!(a.three_valued_eq(&b), Some(true));
        assert_eq!(as_gremlin_set(&a).unwrap().len(), 2);
        assert_ne!(a, Value::List(vec![Value::Int(1), Value::Long(1)]));
        assert_ne!(gremlin_set(vec![Value::Int(1)]), gremlin_set(vec![Value::Long(1)]));
        let map = Value::Map(BTreeMap::from([("__gremlin_set".into(), Value::List(vec![Value::Int(1)]))]));
        assert!(as_gremlin_set(&map).is_none());
        assert_ne!(map, gremlin_set(vec![Value::Int(1)]));
        assert_eq!(gremlin_set(vec![a.clone(), b]), Value::Set(vec![a]));
    }

    #[test]
    fn native_set_dedup_retains_java_floating_identity() {
        let value = gremlin_set(vec![Value::Float(f64::NAN), Value::Float(f64::NAN), Value::Float(0.0), Value::Float(-0.0)]);
        assert_eq!(as_gremlin_set(&value).unwrap().len(), 3);
        assert_eq!(value, value.clone());
    }

    #[test]
    fn native_set_nested_nan_identity_uses_one_canonical_key() {
        let left = Value::Float(f64::from_bits(0x7ff8_0000_0000_0001));
        let right = Value::Float(f64::from_bits(0x7ff8_0000_0000_0002));
        assert_ne!(left, left.clone()); // Shared scalar equality is unchanged.
        let containers = |nan: Value| vec![
            nan.clone(),
            Value::List(vec![Value::List(vec![nan.clone()])]),
            Value::Path(vec![nan.clone()]),
            Value::Map(BTreeMap::from([("n".into(), nan.clone())])),
            Value::TypedMap(vec![(nan.clone(), Value::List(vec![nan.clone()]))]),
            Value::MapEntry(Box::new((nan.clone(), nan.clone()))),
            Value::BulkSet(vec![nan.clone(), nan]),
        ];
        for (a, b) in containers(left).into_iter().zip(containers(right)) {
            assert_eq!(set_member_key(&a), set_member_key(&b));
            assert_eq!(as_gremlin_set(&gremlin_set(vec![a.clone(), b.clone()])).unwrap().len(), 1);
            assert_eq!(gremlin_set(vec![a]), gremlin_set(vec![b]));
        }
        assert_eq!(set_member_key(&Value::Float32(f32::from_bits(0x7fc0_0001))), set_member_key(&Value::Float32(f32::from_bits(0x7fc0_0002))));
    }

    #[test]
    fn native_set_members_preserve_java_map_entry_and_decimal_identity() {
        let a = Value::Map(BTreeMap::from([("x".into(), Value::Int(1)), ("y".into(), Value::Long(2))]));
        let b = Value::TypedMap(vec![(Value::String("y".into()), Value::Long(2)), (Value::String("x".into()), Value::Int(1))]);
        assert_eq!(set_member_key(&a), set_member_key(&b));
        assert_eq!(as_gremlin_set(&gremlin_set(vec![a.clone(), b])).unwrap().len(), 1);
        assert_ne!(set_member_key(&a), set_member_key(&Value::MapEntry(Box::new((Value::String("x".into()), Value::Int(1))))));
        let decimal = |s: &str| Value::BigDecimal(s.parse().unwrap());
        assert_eq!(decimal("1.0"), decimal("1.00")); // Existing shared equality.
        assert_ne!(set_member_key(&decimal("1.0")), set_member_key(&decimal("1.00")));
        assert_eq!(as_gremlin_set(&gremlin_set(vec![decimal("1.0"), decimal("1.00")])).unwrap().len(), 2);
    }
}

#[derive(Debug, Clone)]
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
    Temporal(crate::ir::temporal::TemporalValue),
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
    /// Persisted vertex property; identity is independent of owner/key/value.
    VertexProperty { id: i64, owner: Box<Value>, key: String, value: Box<Value> },
    /// Edge or meta-property. Owner can itself be a VertexProperty.
    Property { owner: Box<Value>, key: String, value: Box<Value> },
    List(Vec<Value>),
    /// Native Gremlin Set; member identity is typed and iteration retains encounter order.
    Set(Vec<Value>),
    Map(BTreeMap<String, Value>),
    /// Gremlin maps may use graph objects, numbers, and tokens as keys.
    TypedMap(Vec<(Value, Value)>),
    /// Native Gremlin Map.Entry, distinct from a map with key/value properties.
    MapEntry(Box<(Value, Value)>),
    /// Typed Gremlin cardinality instruction, distinct from its payload or a map.
    CardinalityValue { cardinality: String, value: Box<Value> },
    /// Gremlin multiset, preserving repeated values and a distinct runtime type.
    BulkSet(Vec<Value>),
    Token(String),
    Direction(String),
    /// Path objects produced by `pathMaterialization=NodesAndRelationships`.
    /// The first element is always a node; nodes and edges alternate.
    Path(Vec<Value>),
}

// Keep exact runtime equality for existing types while making Set identity
// independent of insertion order, including Sets nested in lists and maps.
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::Temporal(a), Self::Temporal(b)) => a == b,
            (Self::Byte(a), Self::Byte(b)) => a == b,
            (Self::UInt8(a), Self::UInt8(b)) => a == b,
            (Self::Short(a), Self::Short(b)) => a == b,
            (Self::UInt16(a), Self::UInt16(b)) => a == b,
            (Self::Int(a), Self::Int(b)) | (Self::Long(a), Self::Long(b)) => a == b,
            (Self::UInt32(a), Self::UInt32(b)) => a == b,
            (Self::UInt64(a), Self::UInt64(b)) => a == b,
            (Self::Float32(a), Self::Float32(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::BigInt(a), Self::BigInt(b)) | (Self::UInt128(a), Self::UInt128(b)) => a == b,
            (Self::BigDecimal(a), Self::BigDecimal(b)) => a == b,
            (Self::String(a), Self::String(b)) | (Self::DateTime(a), Self::DateTime(b))
            | (Self::Token(a), Self::Token(b)) | (Self::Direction(a), Self::Direction(b)) => a == b,
            (Self::InternalId { table: a, offset: x }, Self::InternalId { table: b, offset: y }) => (a,x) == (b,y),
            (Self::Node { label: a, id: x }, Self::Node { label: b, id: y }) => (a,x) == (b,y),
            (Self::VertexProperty { id: a, .. }, Self::VertexProperty { id: b, .. }) => a == b,
            (Self::Property { key: a, value: x, .. }, Self::Property { key: b, value: y, .. }) =>
                a == b && set_member_key(x) == set_member_key(y),
            (Self::Edge { rel_type:a,id:b,src_label:c,src_id:d,dst_label:e,dst_id:f,projected_properties:g },
             Self::Edge { rel_type:h,id:i,src_label:j,src_id:k,dst_label:l,dst_id:m,projected_properties:n }) =>
                (a,b,c,d,e,f,g) == (h,i,j,k,l,m,n),
            (Self::List(a), Self::List(b)) | (Self::Path(a), Self::Path(b))
            | (Self::BulkSet(a), Self::BulkSet(b)) => a == b,
            (Self::Set(a), Self::Set(b)) => set_eq(a,b),
            (Self::Map(a), Self::Map(b)) => a == b,
            (Self::TypedMap(a), Self::TypedMap(b)) => a == b,
            (Self::MapEntry(a), Self::MapEntry(b)) => a == b,
            (Self::CardinalityValue {cardinality:a,value:x}, Self::CardinalityValue {cardinality:b,value:y}) => a == b && set_member_key(x) == set_member_key(y),
            _ => false,
        }
    }
}

impl Value {
    /// Cardinality wrappers are traversal arguments, including when nested in
    /// a collection. Graph property mutation boundaries reject them explicitly.
    pub fn contains_cardinality_value(&self) -> bool {
        match self {
            Self::CardinalityValue { .. } => true,
            Self::List(items) | Self::Set(items) | Self::BulkSet(items) | Self::Path(items) => items.iter().any(Self::contains_cardinality_value),
            Self::Map(map) => map.values().any(Self::contains_cardinality_value),
            Self::TypedMap(entries) => entries.iter().any(|(key,value)| key.contains_cardinality_value() || value.contains_cardinality_value()),
            Self::MapEntry(entry) => entry.0.contains_cardinality_value() || entry.1.contains_cardinality_value(),
            Self::VertexProperty { value, .. } | Self::Property { value, .. } => value.contains_cardinality_value(),
            _ => false,
        }
    }

    pub fn map_from_entries(entries: Vec<(Value, Value)>) -> Self {
        if entries
            .iter()
            .all(|(key, _)| matches!(key, Self::String(_)))
        {
            Self::Map(
                entries
                    .into_iter()
                    .map(|(key, value)| {
                        let Self::String(key) = key else {
                            unreachable!()
                        };
                        (key, value)
                    })
                    .collect(),
            )
        } else {
            Self::TypedMap(entries)
        }
    }
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
            Self::Temporal(v) => v.kind(),
            Self::InternalId { .. } => "internal_id",
            Self::String(_) => "string",
            Self::Node { .. } => "node",
            Self::Edge { .. } => "edge",
            Self::VertexProperty { .. } => "vertex property",
            Self::Property { .. } => "property",
            Self::List(_) => "list",
            Self::Set(_) => "set",
            Self::Map(_) => "map",
            Self::TypedMap(_) => "map",
            Self::MapEntry(_) => "map entry",
            Self::CardinalityValue { .. } => "cardinality value",
            Self::BulkSet(_) => "bulkset",
            Self::Token(_) => "token",
            Self::Direction(_) => "direction",
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
            (Self::Temporal(a), Self::Temporal(b)) => a == b,
            (Self::VertexProperty{id:a,..},Self::VertexProperty{id:b,..})=>a==b,
            (Self::Property{key:a,value:x,..},Self::Property{key:b,value:y,..})=>a==b && set_member_key(x)==set_member_key(y),

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
            (Self::Set(a), Self::Set(b)) => set_eq(a, b),
            (Self::BulkSet(a), Self::BulkSet(b)) => {
                let mut remaining = b.iter().collect::<Vec<_>>();
                a.len() == b.len()
                    && a.iter().all(|item| {
                        if let Some(index) = remaining.iter().position(|other| item == *other) {
                            remaining.remove(index);
                            true
                        } else {
                            false
                        }
                    })
            }
            (Self::Map(a), Self::Map(b)) => semantic_map_eq(a, b),
            (Self::Token(a), Self::Token(b)) | (Self::Direction(a), Self::Direction(b)) => a == b,
            (Self::CardinalityValue {cardinality:a,value:x}, Self::CardinalityValue {cardinality:b,value:y}) => a == b && set_member_key(x) == set_member_key(y),
            (Self::MapEntry(a), Self::MapEntry(b)) => a.0.three_valued_eq(&b.0) == Some(true) && a.1.three_valued_eq(&b.1) == Some(true),
            (Self::TypedMap(a), Self::TypedMap(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(key, value)| {
                        b.iter().any(|(other_key, other_value)| {
                            key == other_key && value.three_valued_eq(other_value) == Some(true)
                        })
                    })
            }
            (Self::TypedMap(a), Self::Map(b)) | (Self::Map(b), Self::TypedMap(a)) => {
                a.len() == b.len()
                    && a.iter().all(|(key, value)| match key {
                        Self::String(key) => b
                            .get(key)
                            .is_some_and(|other| value.three_valued_eq(other) == Some(true)),
                        _ => false,
                    })
            }
            _ => false,
        })
    }

    pub fn three_valued_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        if let (Self::Temporal(a), Self::Temporal(b)) = (self, other) { return a.compare(b); }
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
    key != STRUCT_ORDER_KEY && key != STRUCT_TYPES_KEY && !key.starts_with("__")
}

#[cfg(test)]
mod cardinality_value_tests {
    use super::*;

    #[test]
    fn cardinality_value_identity_is_typed_and_never_an_ordinary_map() {
        let wrap=|kind:&str,value| Value::CardinalityValue {cardinality:kind.into(),value:Box::new(value)};
        let a=wrap("list",Value::Int(1));
        assert_eq!(a,a.clone());
        assert_eq!(a.three_valued_eq(&a),Some(true));
        assert_ne!(a,wrap("single",Value::Int(1)));
        assert_ne!(a,wrap("list",Value::Long(1)));
        assert_ne!(a,Value::Int(1));
        let map=Value::Map(BTreeMap::from([("cardinality".into(),Value::String("list".into())),("value".into(),Value::Int(1))]));
        assert_ne!(a,map);
        assert!(!map.contains_cardinality_value());
        assert!(Value::List(vec![Value::TypedMap(vec![(Value::Int(1),a.clone())])]).contains_cardinality_value());
        assert_eq!(as_gremlin_set(&gremlin_set(vec![a.clone(),a,wrap("list",Value::Long(1))])).unwrap().len(),2);
        let nan=wrap("set",Value::Float(f64::NAN));
        assert_eq!(nan,nan.clone());
        assert_eq!(nan.three_valued_eq(&nan),Some(true));
    }
}
