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
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    DateTime(String),
    String(String),
    List(Vec<GValue>),
    Map(BTreeMap<String, GValue>),
}

impl GValue {
    pub fn as_sql_literal_debug(&self) -> String {
        match self {
            Self::Null => "NULL".to_owned(),
            Self::Bool(value) => value.to_string(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.to_string(),
            Self::DateTime(value) => format!("datetime('{}')", value.replace('\'', "''")),
            Self::String(value) => format!("'{}'", value.replace('\'', "''")),
            Self::List(_) => "<list>".to_owned(),
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
