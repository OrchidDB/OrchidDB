//! Value-level expressions used inside Graph IR nodes.
//!
//! These are **not** the IR nodes themselves; they appear as children of
//! `GraphFilter`, `GraphProject`, `GraphAggregate.aggs`, etc.

use crate::ir::policy::PropertyMissing;

/// Reference to a logical binding produced by upstream operators.
pub type BindingId = String;

#[derive(Debug, Clone, PartialEq)]
pub enum Lit {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
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
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StringOp {
    StartsWith,
    EndsWith,
    Contains,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IrExpr {
    Lit(Lit),
    /// Refers to a column / binding in the current row.
    Binding(BindingId),
    /// `property(b, "name", policy=...)`.
    Property {
        binding: BindingId,
        name: String,
        policy: PropertyMissing,
    },
    /// `id(b)` — element identifier projection.
    Id(BindingId),
    /// `label(b)` — label / type projection.
    Label(BindingId),
    /// `b.kind == Node && labels(b) ∋ <label>`.
    HasLabel {
        binding: BindingId,
        label: String,
    },
    Binary {
        op: BinaryOp,
        lhs: Box<IrExpr>,
        rhs: Box<IrExpr>,
    },
    Not(Box<IrExpr>),
    StringPredicate {
        op: StringOp,
        target: Box<IrExpr>,
        pattern: Box<IrExpr>,
    },
    IsNull(Box<IrExpr>),
    IsNotNull(Box<IrExpr>),
    /// `is_bound(?x)` — SPARQL bound predicate. We model unbound as `Null`.
    IsBound(BindingId),
    /// `simple_path(path)`-style path predicates evaluated against a binding
    /// of kind `Path`.
    SimplePath(BindingId),
    List(Vec<IrExpr>),
    /// Function call. Used for `lcase`, `distance`, `point`, custom UDFs etc.
    Call {
        name: String,
        args: Vec<IrExpr>,
    },
    /// `case` expression: each (cond, value); else branch falls back.
    Case {
        arms: Vec<(IrExpr, IrExpr)>,
        otherwise: Option<Box<IrExpr>>,
    },
}

impl IrExpr {
    pub fn lit_int(value: i64) -> Self {
        Self::Lit(Lit::Int(value))
    }

    pub fn lit_str(value: impl Into<String>) -> Self {
        Self::Lit(Lit::String(value.into()))
    }

    pub fn lit_bool(value: bool) -> Self {
        Self::Lit(Lit::Bool(value))
    }

    pub fn binding(name: impl Into<BindingId>) -> Self {
        Self::Binding(name.into())
    }

    pub fn property(
        binding: impl Into<BindingId>,
        name: impl Into<String>,
        policy: PropertyMissing,
    ) -> Self {
        Self::Property {
            binding: binding.into(),
            name: name.into(),
            policy,
        }
    }

    pub fn eq(lhs: IrExpr, rhs: IrExpr) -> Self {
        Self::Binary {
            op: BinaryOp::Eq,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    pub fn and(parts: Vec<IrExpr>) -> Self {
        let mut iter = parts.into_iter();
        let mut acc = match iter.next() {
            Some(first) => first,
            None => return Self::Lit(Lit::Bool(true)),
        };
        for next in iter {
            acc = Self::Binary {
                op: BinaryOp::And,
                lhs: Box::new(acc),
                rhs: Box::new(next),
            };
        }
        acc
    }
}

/// Aggregate function instance used in `GraphAggregate.aggs`.
#[derive(Debug, Clone, PartialEq)]
pub struct AggCall {
    pub kind: AggKind,
    /// Output binding name.
    pub alias: BindingId,
    /// `None` for `count(*) / countRows()`.
    pub arg: Option<IrExpr>,
    /// `count(DISTINCT x)` etc.
    pub distinct: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggKind {
    /// `countRows()` — counts visible rows. Bag semantics. Argument may
    /// scope which rows participate (`countRows(f)` only counts rows where
    /// `f` is non-null).
    CountRows,
    /// `countBulk()` — sums Gremlin traverser bulk metadata.
    CountBulk,
    /// `count(DISTINCT x)` style.
    CountDistinct,
    Sum,
    Avg,
    Min,
    Max,
    /// Cypher-style aggregate variants emit one identity/null row for an
    /// empty ungrouped input; Gremlin reductions using Sum/Avg/Min/Max keep
    /// their existing empty-stream drop behavior.
    SumOrZero,
    /// Legacy identity-producing average variant for non-Cypher callers that
    /// need an empty input to produce `0.0`.
    AvgOrZero,
    /// Cypher `avg()`: emit one row on empty input, with `null` when there
    /// are no non-null numeric values.
    AvgOrNull,
    MinOrNull,
    MaxOrNull,
    StDev,
    StDevP,
    PercentileCont,
    PercentileDisc,
    /// `collectRows(x)` — gather row values into a list.
    CollectRows,
    /// `collectTraversers(current)` — Gremlin fold semantics.
    CollectTraversers,
}
