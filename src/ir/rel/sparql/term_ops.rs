//! Term ops for typed SPARQL lowering.

use super::*;

pub(super) fn guard_bound(term: &Term, condition: Expr) -> Expr {
    case(
        vec![(term.kind.clone().is_null(), null_bool())],
        Some(condition),
    )
}

/// A materialized term with consistent unbound components.
pub(super) fn guarded(term: Term) -> Term {
    let unbound = term.value.clone().is_null().or(term.kind.clone().is_null());
    let guard = |expr: Expr| case(vec![(unbound.clone(), null_str())], Some(expr));
    Term {
        value: guard(term.value),
        kind: guard(term.kind),
        dt: guard(term.dt),
        lang: guard(term.lang),
    }
}

pub(super) fn term_ebv(term: &Term) -> Expr {
    case(
        vec![
            (term.kind.clone().is_null(), null_bool()),
            (term.is_literal().not(), null_bool()),
            (
                term.dt.clone().eq(s(&xsd("boolean"))),
                term.value.clone().in_list(vec![s("true"), s("1")], false),
            ),
            (term.is_string(), term.value.clone().not_eq(s(""))),
            (
                term.is_numeric(),
                duck(
                    "coalesce",
                    vec![
                        try_cast(term.value.clone(), DataType::Float64)
                            .not_eq(lit(0.0_f64))
                            .and(
                                duck(
                                    "isnan",
                                    vec![try_cast(term.value.clone(), DataType::Float64)],
                                    DataType::Boolean,
                                )
                                .not(),
                            ),
                        lit(false),
                    ],
                    DataType::Boolean,
                ),
            ),
        ],
        Some(null_bool()),
    )
}

/// Both arguments are string literals with compatible language tags.
pub(super) fn string_args_compatible(a: &Term, b: &Term) -> Expr {
    a.is_string().and(b.is_string()).and(
        b.lang
            .clone()
            .is_null()
            .or(not_distinct(a.lang.clone(), b.lang.clone())),
    )
}

/// Sort keys implementing SPARQL ORDER BY: unbound, blank nodes, IRIs, then
/// literals; numerics by value, other literals by lexical form.
pub(super) fn order_keys(term: &Term) -> Vec<Expr> {
    let kind_rank = case(
        vec![
            (term.kind.clone().is_null(), lit(0_i64)),
            (term.kind.clone().eq(s(KIND_BLANK)), lit(1_i64)),
            (term.kind.clone().eq(s(KIND_IRI)), lit(2_i64)),
        ],
        Some(lit(3_i64)),
    );
    let numeric = case(vec![(term.is_numeric(), lit(0_i64))], Some(lit(1_i64)));
    let number = case(
        vec![(
            term.is_numeric(),
            try_cast(term.value.clone(), DataType::Float64),
        )],
        None,
    );
    vec![
        kind_rank,
        numeric,
        number,
        term.value.clone(),
        term.dt.clone(),
        term.lang.clone(),
    ]
}

pub(super) fn numeric_operands(op: Option<BinaryOp>, a: &Term, b: &Term, data_type: DataType) -> Expr {
    let (x, y) = (
        try_cast(a.value.clone(), data_type.clone()),
        try_cast(b.value.clone(), data_type),
    );
    match op {
        Some(BinaryOp::Eq) | None => x.eq(y),
        Some(BinaryOp::Neq) => x.not_eq(y),
        Some(BinaryOp::Lt) => x.lt(y),
        Some(BinaryOp::Lte) => x.lt_eq(y),
        Some(BinaryOp::Gt) => x.gt(y),
        Some(BinaryOp::Gte) => x.gt_eq(y),
        Some(BinaryOp::Add) => x + y,
        Some(BinaryOp::Sub) => x - y,
        Some(BinaryOp::Mul) => x * y,
        Some(BinaryOp::Div) => x / y,
        Some(_) => lit(ScalarValue::Null),
    }
}

/// The greater numeric promotion rank of two operands.
/// NULL unless both operands are numeric.
pub(super) fn promoted_rank(a: &Term, b: &Term) -> Expr {
    let (ra, rb) = (a.numeric_rank(), b.numeric_rank());
    case(
        vec![
            (
                ra.clone().is_null().or(rb.clone().is_null()),
                lit(ScalarValue::Int64(None)),
            ),
            (ra.clone().gt_eq(rb.clone()), ra),
        ],
        Some(rb),
    )
}

const KNOWN_DATATYPES: &[&str] = &["string", "boolean", "date", "dateTime"];

pub(super) fn known_datatype(term: &Term) -> Expr {
    term.is_numeric().and(try_cast(term.value.clone(), DataType::Float64).is_not_null()).or(term.dt.clone().in_list(
        KNOWN_DATATYPES
            .iter()
            .map(|local| s(&xsd(local)))
            .chain(std::iter::once(s(RDF_LANG_STRING)))
            .collect(),
        false,
    ))
}

pub(super) fn compare(op: BinaryOp, a: &Term, b: &Term, rank: Expr) -> Expr {
    let numeric = case(
        vec![
            (
                rank.clone().eq(lit(1_i64)),
                numeric_operands(Some(op), a, b, INT),
            ),
            (
                rank.clone().eq(lit(2_i64)),
                numeric_operands(Some(op), a, b, DEC),
            ),
        ],
        Some(numeric_operands(Some(op), a, b, DataType::Float64)),
    );
    let ordered = |x: Expr, y: Expr| match op {
        BinaryOp::Eq => x.eq(y),
        BinaryOp::Neq => x.not_eq(y),
        BinaryOp::Lt => x.lt(y),
        BinaryOp::Lte => x.lt_eq(y),
        BinaryOp::Gt => x.gt(y),
        _ => x.gt_eq(y),
    };
    let numeric = if matches!(op, BinaryOp::Eq | BinaryOp::Neq) {
        // An ill-typed literal has no numeric value, but identical RDF
        // terms still compare equal under RDFterm-equal.
        duck("coalesce", vec![numeric,
            case(vec![(a.same_term(b), lit(op == BinaryOp::Eq))], Some(null_bool()))],
            DataType::Boolean)
    } else { numeric };
    let both_numeric = rank.clone().is_not_null();
    let both_simple = a.is_simple().and(b.is_simple());
    let both_boolean = a
        .has_datatype(&xsd("boolean"))
        .and(b.has_datatype(&xsd("boolean")));
    let both_datetime = a
        .has_datatype(&xsd("dateTime"))
        .and(b.has_datatype(&xsd("dateTime")));
    let temporal = |operation: &str| duck_str("__crabgraph_sparql_scalar", vec![
        s(operation), a.value.clone(), b.value.clone(),
        s(match op { BinaryOp::Eq => "eq", BinaryOp::Neq => "ne",
            BinaryOp::Lt => "lt", BinaryOp::Lte => "le", BinaryOp::Gt => "gt", _ => "ge" }), s("")
    ]).eq(s("true"));
    let mut arms = vec![
        (
            a.kind.clone().is_null().or(b.kind.clone().is_null()),
            null_bool(),
        ),
        (both_numeric, numeric),
        (both_simple, ordered(a.value.clone(), b.value.clone())),
        (both_boolean, ordered(a.boolean_value(), b.boolean_value())),
        (both_datetime, temporal("compare_datetime")),
        (a.has_datatype(&xsd("date")).and(b.has_datatype(&xsd("date"))), temporal("compare_date")),
    ];
    if matches!(op, BinaryOp::Eq | BinaryOp::Neq) {
        let equal = op == BinaryOp::Eq;
        // RDFterm-equal: identical terms are equal; distinct non-literals
        // or literals of distinct known datatypes are unequal; anything
        // else cannot be decided and is an error.
        arms.push((a.same_term(b), lit(equal)));
        arms.push((a.is_literal().not().or(b.is_literal().not()), lit(!equal)));
        arms.push((
            a.dt.clone()
                .eq(s(RDF_LANG_STRING))
                .or(b.dt.clone().eq(s(RDF_LANG_STRING))),
            lit(!equal),
        ));
        arms.push((known_datatype(a).and(known_datatype(b)), lit(!equal)));
    }
    case(arms, Some(null_bool()))
}

pub(super) fn arithmetic(op: BinaryOp, a: &Term, b: &Term, rank: Expr) -> RelResult<Term> {
    if !matches!(
        op,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div
    ) {
        return unsupported(format!("operator {op:?} on RDF terms"));
    }
    let numeric = rank.clone().is_not_null();
    let mut rank = rank;
    if op == BinaryOp::Div {
        // Integer division yields xsd:decimal.
        rank = case(vec![(rank.clone().eq(lit(1_i64)), lit(2_i64))], Some(rank));
    }
    let zero_divisor = try_cast(b.value.clone(), DataType::Float64).eq(lit(0.0_f64));
    let decimal = if op == BinaryOp::Div {
        duck_str("__crabgraph_sparql_scalar", vec![s("decimal_divide"),
            a.value.clone(), b.value.clone(), s(""), s("")])
    } else if op == BinaryOp::Mul {
        decimal_lexical(numeric_operands(Some(op), a, b, DataType::Decimal128(38, 9)))
    } else {
        decimal_lexical(numeric_operands(Some(op), a, b, DEC))
    };
    let mut arms = Vec::new();
    if op == BinaryOp::Div {
        arms.push((rank.clone().eq(lit(2_i64)).and(zero_divisor), null_str()));
    }
    arms.extend([
        (
            rank.clone().eq(lit(1_i64)),
            integer_lexical(numeric_operands(Some(op), a, b, INT)),
        ),
        (rank.clone().eq(lit(2_i64)), decimal),
    ]);
    let value = case(
        arms,
        Some(double_lexical(numeric_operands(
            Some(op),
            a,
            b,
            DataType::Float64,
        ))),
    );
    let dt = case(
        vec![
            (rank.clone().eq(lit(1_i64)), s(&xsd("integer"))),
            (rank.clone().eq(lit(2_i64)), s(&xsd("decimal"))),
            (rank.clone().eq(lit(3_i64)), s(&xsd("float"))),
        ],
        Some(s(&xsd("double"))),
    );
    Ok(Term {
        value,
        kind: s(KIND_LITERAL),
        dt,
        lang: null_str(),
    }
    .only_if(numeric))
}

pub(super) fn xsd_cast(target: &str, a: &Term) -> RelResult<Term> {
    let value = a.value.clone();
    let rank = a.numeric_rank();
    let from_string = a.is_simple();
    let is_bool = a.has_datatype(&xsd("boolean"));
    let trimmed = duck_str("trim", vec![value.clone()]);
    let matches = |pattern: &str| {
        duck(
            "regexp_full_match",
            vec![trimmed.clone(), s(pattern)],
            DataType::Boolean,
        )
    };
    Ok(match target {
        "string" => Term::string(duck_str("__crabgraph_sparql_scalar", vec![
            s("cast_string"), value, duck_str("coalesce", vec![a.dt.clone(), s("")]), s(""), s("")
        ])).only_if(
            a.kind
                .clone()
                .in_list(vec![s(KIND_IRI), s(KIND_LITERAL)], false),
        ),
        "integer" => {
            let lexical = case(
                vec![
                    (
                        from_string.clone().and(matches("[+-]?[0-9]+")),
                        integer_lexical(try_cast(trimmed.clone(), INT)),
                    ),
                    (
                        rank.clone().eq(lit(1_i64)),
                        integer_lexical(try_cast(value.clone(), INT)),
                    ),
                    (
                        rank.clone().eq(lit(2_i64)),
                        integer_lexical(cast(
                            duck("trunc", vec![try_cast(value.clone(), DEC)], DEC),
                            INT,
                        )),
                    ),
                    (
                        rank.clone().gt_eq(lit(3_i64)),
                        integer_lexical(try_cast(
                            duck(
                                "trunc",
                                vec![try_cast(value.clone(), DataType::Float64)],
                                DataType::Float64,
                            ),
                            INT,
                        )),
                    ),
                    (
                        is_bool.clone(),
                        case(vec![(a.boolean_value(), s("1"))], Some(s("0"))),
                    ),
                ],
                None,
            );
            Term::literal(lexical, &xsd("integer"))
        }
        "decimal" => {
            let lexical = case(
                vec![
                    (
                        from_string
                            .clone()
                            .and(matches(r"[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)")),
                        decimal_lexical(try_cast(trimmed.clone(), DEC)),
                    ),
                    (
                        rank.clone().is_not_null(),
                        decimal_lexical(try_cast(value.clone(), DEC)),
                    ),
                    (
                        is_bool.clone(),
                        case(vec![(a.boolean_value(), s("1.0"))], Some(s("0.0"))),
                    ),
                ],
                None,
            );
            Term::literal(lexical, &xsd("decimal"))
        }
        "double" | "float" => {
            let lexical = case(
                vec![
                    (
                        from_string.clone().and(matches(
                            r"[+-]?(([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?|INF|NaN)",
                        )),
                        double_lexical(try_cast(trimmed.clone(), DataType::Float64)),
                    ),
                    (
                        rank.clone().is_not_null(),
                        double_lexical(try_cast(value.clone(), DataType::Float64)),
                    ),
                    (
                        is_bool.clone(),
                        case(vec![(a.boolean_value(), s("1.0E0"))], Some(s("0.0E0"))),
                    ),
                ],
                None,
            );
            Term::literal(lexical, &xsd(target))
        }
        "boolean" => {
            let lexical = case(
                vec![
                    (
                        from_string
                            .clone()
                            .and(trimmed.clone().in_list(vec![s("true"), s("1")], false)),
                        s("true"),
                    ),
                    (
                        from_string
                            .clone()
                            .and(trimmed.clone().in_list(vec![s("false"), s("0")], false)),
                        s("false"),
                    ),
                    (
                        is_bool.clone(),
                        case(vec![(a.boolean_value(), s("true"))], Some(s("false"))),
                    ),
                    (
                        rank.clone().is_not_null(),
                        case(vec![(term_ebv(a).is_true(), s("true"))], Some(s("false"))),
                    ),
                ],
                None,
            );
            Term::literal(lexical, &xsd("boolean"))
        }
        "dateTime" => {
            let valid = from_string
                .or(a.has_datatype(&xsd("dateTime")))
                .and(matches(r"-?[0-9]{4,}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}(\.[0-9]+)?(Z|[+-][0-9]{2}:[0-9]{2})?"));
            Term::literal(trimmed, &xsd("dateTime")).only_if(valid)
        }
        other => return unsupported(format!("cast to xsd:{other}")),
    })
}

/// Lexical form and datatype of a scalar literal constant.
pub(super) fn literal_identity(value: &Lit) -> RelResult<(String, String)> {
    Ok(match value {
        Lit::Null => return unsupported("NULL is not an RDF term"),
        Lit::Bool(value) => (value.to_string(), xsd("boolean")),
        Lit::Int(value) => (value.to_string(), xsd("integer")),
        Lit::Float(value) => (format!("{value:?}"), xsd("double")),
        Lit::String(value) => (value.clone(), xsd("string")),
    })
}

/// Fold constant boolean and string structure produced by term helpers, so
/// constants do not expand into large CASE trees in the generated SQL.
pub(super) fn fold(expr: Expr) -> RelResult<Expr> {
    use datafusion::common::tree_node::{Transformed, TreeNode};
    use datafusion::logical_expr::{BinaryExpr, Operator};

    fn text(expr: &Expr) -> Option<Option<&str>> {
        match expr {
            Expr::Literal(ScalarValue::Utf8(value), _) => Some(value.as_deref()),
            Expr::Literal(ScalarValue::Null, _) => Some(None),
            _ => None,
        }
    }
    fn boolean(expr: &Expr) -> Option<Option<bool>> {
        match expr {
            Expr::Literal(ScalarValue::Boolean(value), _) => Some(*value),
            _ => None,
        }
    }
    let folded = expr.transform_up(|expr| {
        let replacement = match &expr {
            Expr::BinaryExpr(BinaryExpr { left, op, right }) => match op {
                Operator::And => match (boolean(left), boolean(right)) {
                    (Some(Some(false)), _) | (_, Some(Some(false))) => Some(lit(false)),
                    (Some(Some(true)), _) => Some(right.as_ref().clone()),
                    (_, Some(Some(true))) => Some(left.as_ref().clone()),
                    _ => None,
                },
                Operator::Or => match (boolean(left), boolean(right)) {
                    (Some(Some(true)), _) | (_, Some(Some(true))) => Some(lit(true)),
                    (Some(Some(false)), _) => Some(right.as_ref().clone()),
                    (_, Some(Some(false))) => Some(left.as_ref().clone()),
                    _ => None,
                },
                Operator::Eq
                | Operator::NotEq
                | Operator::Lt
                | Operator::LtEq
                | Operator::Gt
                | Operator::GtEq
                    if matches!(left.as_ref(), Expr::Literal(value, _) if value.is_null())
                        || matches!(right.as_ref(), Expr::Literal(value, _) if value.is_null()) =>
                {
                    Some(null_bool())
                }
                Operator::Eq | Operator::NotEq => match (text(left), text(right)) {
                    (Some(Some(a)), Some(Some(b))) => Some(lit((a == b) == (*op == Operator::Eq))),
                    (Some(None), Some(_)) | (Some(_), Some(None)) => Some(null_bool()),
                    _ => None,
                },
                _ => None,
            },
            Expr::Not(inner) => boolean(inner).map(|value| match value {
                Some(value) => lit(!value),
                None => null_bool(),
            }),
            Expr::IsNull(inner) => match inner.as_ref() {
                Expr::Literal(value, _) => Some(lit(value.is_null())),
                _ => None,
            },
            Expr::IsNotNull(inner) => match inner.as_ref() {
                Expr::Literal(value, _) => Some(lit(!value.is_null())),
                _ => None,
            },
            Expr::IsTrue(inner) => boolean(inner).map(|value| lit(value == Some(true))),
            Expr::IsFalse(inner) => boolean(inner).map(|value| lit(value == Some(false))),
            Expr::InList(list) if !list.negated => match text(&list.expr) {
                Some(Some(value)) if list.list.iter().all(|item| text(item).is_some()) => {
                    Some(lit(list
                        .list
                        .iter()
                        .any(|item| text(item) == Some(Some(value)))))
                }
                Some(None) => Some(null_bool()),
                _ => None,
            },
            Expr::Case(case_expr) if case_expr.expr.is_none() => {
                let mut arms = Vec::new();
                let mut otherwise = case_expr.else_expr.clone();
                let mut changed = false;
                let mut dropped = None;
                for (when, then) in &case_expr.when_then_expr {
                    match boolean(when) {
                        Some(Some(true)) => {
                            otherwise = Some(then.clone());
                            changed = true;
                            break;
                        }
                        Some(_) => {
                            changed = true;
                            dropped.get_or_insert_with(|| then.clone());
                        }
                        None => arms.push((when.clone(), then.clone())),
                    }
                }
                if !changed {
                    None
                } else if arms.is_empty() {
                    match otherwise {
                        Some(expr) => Some(*expr),
                        // Keep a typed NULL: `CASE WHEN false THEN x END`.
                        None if case_expr.when_then_expr.len() == 1 => None,
                        None => Some(Expr::Case(Case {
                            expr: None,
                            when_then_expr: vec![(
                                Box::new(lit(false)),
                                dropped.expect("dropped arm"),
                            )],
                            else_expr: None,
                        })),
                    }
                } else {
                    Some(Expr::Case(Case {
                        expr: None,
                        when_then_expr: arms,
                        else_expr: otherwise,
                    }))
                }
            }
            _ => None,
        };
        Ok(match replacement {
            Some(expr) => Transformed::yes(expr),
            None => Transformed::no(expr),
        })
    })?;
    Ok(folded.data)
}
