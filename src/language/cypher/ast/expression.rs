use super::{PatternPart, Query};

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Star,
    Variable(String),
    Property {
        target: Box<Expr>,
        key: String,
    },
    LabelPredicate {
        target: Box<Expr>,
        labels: Vec<String>,
    },
    Parameter(String),
    Literal(Literal),
    List(Vec<Expr>),
    Map(Vec<(String, Expr)>),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    IsNull(Box<Expr>),
    IsNotNull(Box<Expr>),
    StringPredicate {
        op: StringPredicateOp,
        target: Box<Expr>,
        pattern: Box<Expr>,
    },
    Function {
        name: String,
        distinct: bool,
        args: Vec<Expr>,
    },
    Case {
        case: Option<Box<Expr>>,
        arms: Vec<(Expr, Expr)>,
        otherwise: Option<Box<Expr>>,
    },
    Exists(ExistsSubquery),
    PatternPredicate(Vec<PatternPart>),
    ListComprehension {
        variable: String,
        collection: Box<Expr>,
        predicate: Option<Box<Expr>>,
        map: Box<Expr>,
    },
    ListReduce {
        accumulator: String,
        variable: String,
        collection: Box<Expr>,
        map: Box<Expr>,
    },
    /// Kuzu `list_transform(list, x -> expr)`.
    ListTransform {
        variable: String,
        collection: Box<Expr>,
        map: Box<Expr>,
    },
    /// Kuzu `list_filter(list, x -> predicate)`.
    ListFilter {
        variable: String,
        collection: Box<Expr>,
        predicate: Box<Expr>,
    },
    PatternComprehension {
        variable: Option<String>,
        pattern: Box<PatternPart>,
        predicate: Option<Box<Expr>>,
        map: Box<Expr>,
    },
    Quantifier {
        kind: QuantifierKind,
        variable: String,
        collection: Box<Expr>,
        predicate: Box<Expr>,
    },
    CountStar,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExistsSubquery {
    pub query: Option<Box<Query>>,
    pub patterns: Vec<PatternPart>,
    pub predicate: Option<Box<Expr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Null,
    Bool(bool),
    Integer(String),
    Float(f64),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringPredicateOp {
    StartsWith,
    EndsWith,
    Contains,
    Regex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantifierKind {
    All,
    Any,
    None,
    Single,
}

impl Expr {
    pub fn variable_name(&self) -> Option<&str> {
        match self {
            Self::Variable(name) => Some(name),
            _ => None,
        }
    }
}
