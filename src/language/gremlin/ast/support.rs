use super::Step;
use crate::language::gremlin::semantics::{GValue, Predicate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapColumn {
    Keys,
    Values,
}

/// Gremlin's `Operator` enum (ordering mirrors the grammar's `traversalOperator`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SackOp {
    /// `sum` / `Operator.sum` — addition.
    Sum,
    /// `sumLong` / `Operator.sumLong` — long-typed addition (same as Sum).
    SumLong,
    /// `minus` / `Operator.minus` — subtraction (`sack = sack - rhs`).
    Minus,
    /// `mult` / `Operator.mult` — multiplication.
    Mult,
    /// `div` / `Operator.div` — division.
    Div,
    /// `min` / `Operator.min` — least.
    Min,
    /// `max` / `Operator.max` — greatest.
    Max,
    /// `assign` / `Operator.assign` — overwrite (`sack = rhs`).
    Assign,
    /// `and` / `Operator.and` — boolean AND.
    And,
    /// `or` / `Operator.or` — boolean OR.
    Or,
    /// `addAll` / `Operator.addAll` — list/set concat (rarely meaningful in scalar).
    AddAll,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CallArg {
    /// Literal argument: strings, numbers, booleans, lists, or null.
    Value(GValue),
    /// Map argument. We keep the raw top-level map text until the literal
    /// model grows a structured map representation.
    Map(String),
    /// Traversal argument such as `call("x", __.project(...))`.
    Traversal(Vec<Step>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraversalOption {
    pub key: OptionKey,
    pub traversal: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OptionKey {
    Value(GValue),
    Predicate(Predicate),
    /// `option(__.dispatch_traversal, __.action)` — the option key is itself
    /// a traversal that must accept the dispatched value (its truth value
    /// against the current input decides the option).
    Traversal(Vec<Step>),
    PickAny,
    PickNone,
    PickUnproductive,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FormatPart {
    /// Literal text in the format template.
    Literal(String),
    /// A `{0}` or `%s` placeholder — substitutes the current scalar.
    /// The `key` carries the name from `%{name}` syntax (TinkerPop uses this
    /// to resolve property values / `as(...)` bindings); `_` or empty means
    /// "use the matching by(...) modulator / current scalar".
    Placeholder { key: Option<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListOpKind {
    /// `merge(list)` — set union (dedup'd).
    Merge,
    /// `combine(list)` — concat (no dedup).
    Combine,
    /// `intersect(list)` — set intersection.
    Intersect,
    /// `difference(list)` — set difference (a − b).
    Difference,
    /// `disjunct(list)` — symmetric difference ((a∪b) − (a∩b)).
    Disjunct,
    /// `product(list)` — cartesian product (each element of LHS paired with
    /// each element of RHS).
    Product,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MathExpr {
    /// `_ + literal` — current scalar plus a constant.
    Add(f64),
    /// `_ - literal`.
    Sub(f64),
    /// `_ * literal`.
    Mul(f64),
    /// `_ / literal`.
    Div(f64),
    /// `literal - _` — constant minus current scalar (e.g. `0-_` for negation).
    SubFromLit(f64),
    /// `literal / _` — constant divided by current scalar.
    DivByLit(f64),
    /// `_ OP _` — both operands are the current scalar / by(...) modulator.
    /// The planner consumes the trailing by(...) (if any) for the lhs/rhs
    /// expression and applies the op.
    BinSelf(MathOp),
    /// `_ OP name` — current scalar combined with a named binding /
    /// side-effect / property reference resolved at the planner.
    SelfRhsName(MathOp, String),
    /// `name OP _` — named binding combined with the current scalar.
    SelfLhsName(MathOp, String),
    /// `name1 OP name2` — both operands are named bindings.
    BothNamed(MathOp, String, String),
    /// `name OP literal`.
    NameRhsLit(MathOp, String, f64),
    /// `literal OP name`.
    LitRhsName(MathOp, f64, String),
    /// `fn _` — unary math function over the current scalar / by modulator.
    UnaryFn(String),
    /// `fn(_ OP literal)` — common composed form such as
    /// `ceil(_ * 100)`.
    UnaryCurrentOpLit {
        func: String,
        op: MathOp,
        value: f64,
    },
    /// `name` — bare reference to a named binding (no operator).
    Var(String),
    /// Unparseable expression — treated as identity at the planner.
    Identity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MathOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringOp {
    /// `length()` — character count.
    Length,
    /// `toLower()` / `asLower()` style.
    ToLower,
    /// `toUpper()`.
    ToUpper,
    /// `trim()`.
    Trim,
    /// `lTrim()`.
    LTrim,
    /// `rTrim()`.
    RTrim,
    /// `reverse()`.
    Reverse,
    /// `substring(start)` (end omitted) or `substring(start, end)`.
    Substring { start: i64, end: Option<i64> },
    /// `replace(old, new)`.
    Replace { old: String, new: String },
    /// `concat(s)` — append literal string.
    Concat(String),
    /// `concat(__.traversal)` — append the result of evaluating a sub-
    /// traversal. The planner runs the traversal per input row and casts
    /// the result to text before appending.
    ConcatTraversal(Vec<Step>),
    /// `conjoin(delim)` — joins a List traverser with `delim` into a single
    /// string. On a non-list scalar, behaves like `Concat(delim)` (TinkerPop
    /// raises an error for non-iterable inputs but we degrade gracefully).
    Conjoin(String),
    /// `split(delimiter)` — degenerate to identity at the SQL layer (we
    /// can't fan a row out to a list traverser yet).
    Split(Option<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BySpec {
    /// The key to project. `None` means "use the current scalar".
    pub key: Option<String>,
    /// A sub-traversal projection (`by(__.bothE().count())` etc.). When
    /// present, takes precedence over `key`. Older code paths that don't
    /// inspect this field still get the right behaviour as long as they
    /// also fall through on `key.is_none()`.
    pub traversal: Option<Vec<Step>>,
    /// Sort direction (only consumed by `order()`; ignored by everything
    /// else). Defaults to ascending.
    pub direction: SortDir,
}

impl BySpec {
    pub fn key(key: impl Into<String>) -> Self {
        Self {
            key: Some(key.into()),
            traversal: None,
            direction: SortDir::Asc,
        }
    }

    pub fn default() -> Self {
        Self {
            key: None,
            traversal: None,
            direction: SortDir::Asc,
        }
    }

    pub fn traversal(steps: Vec<Step>) -> Self {
        Self {
            key: None,
            traversal: Some(steps),
            direction: SortDir::Asc,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

/// Gremlin `Pop` discriminant for labelled bindings. `select(Pop.first, "v")`
/// picks the earliest `as("v")` recorded along the chain; `Pop.last` picks
/// the latest. `Pop::All` returns a list of every binding under that label
/// in chain order. `Pop::Mixed` returns a list when there are multiple
/// bindings, otherwise the scalar value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pop {
    First,
    Last,
    All,
    Mixed,
}

/// Target type of `asNumber(GType.X)` / `asString` / `asBool` / `asDate`.
///
/// Numeric casts carry a refinement so `asNumber(GType.BIGDECIMAL)`
/// produces a value the type-of predicate can recognise as
/// `bigdecimal`, distinct from a plain `Long`. Refinements that the
/// runtime doesn't model precisely (e.g. arbitrary-precision integers)
/// fall back to `Long`/`Double` semantics with the type tag preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastTarget {
    /// `asNumber()` with no refinement — promote to `Long`/`Double`
    /// the same way TinkerPop's default policy does.
    Number,
    /// Refined numeric cast.
    Numeric(NumericCast),
    String,
    Bool,
    Date,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericCast {
    Byte,
    Short,
    Int,
    Long,
    BigInt,
    Float,
    Double,
    BigDecimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggKind {
    Sum,
    Min,
    Max,
    Mean,
    Product,
}
