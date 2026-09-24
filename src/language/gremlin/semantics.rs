use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementKind {
    Vertex,
    Edge,
    VertexProperty,
    Property,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Out,
    In,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompareOp {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GValue {
    CardinalityValue { cardinality: String, value: Box<GValue> },
    Token(String),
    DirectionToken(String),
    TypedMap(Vec<(GValue,GValue)>),
    VertexRef { id: Box<GValue>, label: String },
    EdgeRef { id: Box<GValue> },
    Null,
    Bool(bool),
    Int(i64),
    Byte(i8),
    Short(i16),
    Long(i64),
    BigInt(num_bigint::BigInt),
    Float32(f32),
    Float(f64),
    BigDecimal(bigdecimal::BigDecimal),
    DateTime(String),
    String(String),
    List(Vec<GValue>),
    /// Gremlin `{a, b}` set literal — order-preserving, deduplicated.
    /// Carries set identity so `P.typeOf(GType.SET)` and the harness's
    /// `s[...]` rendering can distinguish it from a plain list.
    Set(Vec<GValue>),
    Map(BTreeMap<String, GValue>),
}

impl GValue {
    pub fn as_sql_literal_debug(&self) -> String {
        match self {
            Self::CardinalityValue {cardinality,value} => format!("Cardinality.{cardinality}({})",value.as_sql_literal_debug()),
            Self::Token(token) | Self::DirectionToken(token) => token.clone(),
            Self::TypedMap(_) => "<typed-map>".into(),
            Self::EdgeRef { id } => format!("edge({})", id.as_sql_literal_debug()),
            Self::VertexRef { id, label } => format!("new Vertex({}, {:?})", id.as_sql_literal_debug(), label),
            Self::Null => "NULL".to_owned(),
            Self::Bool(value) => value.to_string(),
            Self::Int(value) | Self::Long(value) => value.to_string(),
            Self::Byte(value) => value.to_string(),
            Self::Short(value) => value.to_string(),
            Self::BigInt(value) => value.to_string(),
            Self::Float32(value) => value.to_string(),
            Self::BigDecimal(value) => value.to_string(),
            Self::Float(value) => value.to_string(),
            Self::DateTime(value) => format!("datetime('{}')", value.replace('\'', "''")),
            Self::String(value) => format!("'{}'", value.replace('\'', "''")),
            Self::List(_) => "<list>".to_owned(),
            Self::Set(_) => "<set>".to_owned(),
            Self::Map(_) => "<map>".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Predicate {
    Compare {
        op: CompareOp,
        value: GValue,
    },
    Within(Vec<GValue>),
    Without(Vec<GValue>),
    TypeOf(String),
    Range {
        lo: GValue,
        hi: GValue,
        inclusive_lo: bool,
        inclusive_hi: bool,
    },
    Outside {
        lo: GValue,
        hi: GValue,
    },
    TextLike {
        pattern: String,
        kind: TextKind,
    },
    Regex(String),
    And(Box<Predicate>, Box<Predicate>),
    Or(Box<Predicate>, Box<Predicate>),
    Not(Box<Predicate>),
}

impl Predicate {
    pub fn eq(value: GValue) -> Self {
        Self::Compare {
            op: CompareOp::Eq,
            value,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKind {
    Containing,
    StartingWith,
    EndingWith,
}
