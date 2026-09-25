//! Expressions for typed SPARQL lowering.

use super::*;

impl Lowerer<'_, '_> {
    // -- expressions -------------------------------------------------------------

    /// Evaluate a compound operand once into fresh columns, so larger
    /// expressions reference it without duplicating its SQL.
    pub(super) fn materialize(&mut self, env: &mut Env, expr: &IrExpr) -> RelResult<Term> {
        let term = self.expr(env, expr)?;
        let simple = |expr: &Expr| matches!(expr, Expr::Column(_) | Expr::Literal(..));
        if matches!(expr, IrExpr::Binding(_) | IrExpr::Lit(_))
            || [&term.value, &term.kind, &term.dt, &term.lang]
                .into_iter()
                .all(simple)
        {
            return Ok(term);
        }
        // Stage the raw term, then normalize it (a NULL value means unbound)
        // over plain column references.
        let existing: Vec<Expr> = env
            .plan
            .schema()
            .fields()
            .iter()
            .map(|field| col_exact(field.name()))
            .collect();
        let raw = self.fresh("raw");
        let mut columns = existing.clone();
        columns.extend(term.aliased(&raw));
        env.plan = self.project(env.plan.clone(), columns)?;
        let name = self.fresh("t");
        let mut columns = existing;
        columns.extend(guarded(Term::columns(&raw)).aliased(&name));
        env.plan = self.project(env.plan.clone(), columns)?;
        env.temps.push(name.clone());
        Ok(Term::columns(&name))
    }

    /// Project a scalar into a fresh column and return a reference to it.
    pub(super) fn column(&mut self, env: &mut Env, expr: Expr) -> RelResult<Expr> {
        if matches!(expr, Expr::Column(_) | Expr::Literal(..)) {
            return Ok(expr);
        }
        let name = self.fresh("c");
        let mut columns: Vec<Expr> = env
            .plan
            .schema()
            .fields()
            .iter()
            .map(|field| col_exact(field.name()))
            .collect();
        columns.push(expr.alias(&name));
        env.plan = self.project(env.plan.clone(), columns)?;
        env.temps.push(name.clone());
        Ok(col_exact(name))
    }

    /// Effective boolean value; NULL is a SPARQL error.
    pub(super) fn ebv(&mut self, env: &mut Env, expr: &IrExpr) -> RelResult<Expr> {
        if let Some(boolean) = self.boolean(env, expr)? {
            return Ok(boolean);
        }
        let term = self.materialize(env, expr)?;
        Ok(term_ebv(&term))
    }

    pub(super) fn boolean(&mut self, env: &mut Env, expr: &IrExpr) -> RelResult<Option<Expr>> {
        Ok(Some(match expr {
            IrExpr::Binary {
                op: BinaryOp::And,
                lhs,
                rhs,
            } => self.ebv(env, lhs)?.and(self.ebv(env, rhs)?),
            IrExpr::Binary {
                op: BinaryOp::Or,
                lhs,
                rhs,
            } => self.ebv(env, lhs)?.or(self.ebv(env, rhs)?),
            IrExpr::Not(inner) => self.ebv(env, inner)?.not(),
            IrExpr::IsBound(name) => env.term(name).bound(),
            IrExpr::Binary { op, lhs, rhs }
                if matches!(
                    op,
                    BinaryOp::Eq
                        | BinaryOp::Neq
                        | BinaryOp::Lt
                        | BinaryOp::Lte
                        | BinaryOp::Gt
                        | BinaryOp::Gte
                ) =>
            {
                let a = self.materialize(env, lhs)?;
                let b = self.materialize(env, rhs)?;
                let rank = self.column(env, promoted_rank(&a, &b))?;
                compare(*op, &a, &b, rank)
            }
            IrExpr::Call { name, args } => match (name.as_str(), args.as_slice()) {
                ("sparql_same_term", [a, b]) => {
                    let a = self.materialize(env, a)?;
                    let b = self.materialize(env, b)?;
                    case(
                        vec![(
                            a.kind.clone().is_null().or(b.kind.clone().is_null()),
                            null_bool(),
                        )],
                        Some(a.same_term(&b)),
                    )
                }
                ("sparql_in", [value, choices @ ..]) => {
                    let value = self.materialize(env, value)?;
                    let mut parts = Vec::new();
                    for choice in choices {
                        let choice = self.materialize(env, choice)?;
                        let rank = self.column(env, promoted_rank(&value, &choice))?;
                        parts.push(compare(BinaryOp::Eq, &value, &choice, rank));
                    }
                    or_all(parts)
                }
                ("isiri" | "isuri", [a]) => {
                    let a = self.materialize(env, a)?;
                    guard_bound(&a, a.kind.clone().eq(s(KIND_IRI)))
                }
                ("isblank", [a]) => {
                    let a = self.materialize(env, a)?;
                    guard_bound(&a, a.kind.clone().eq(s(KIND_BLANK)))
                }
                ("isliteral", [a]) => {
                    let a = self.materialize(env, a)?;
                    guard_bound(&a, a.is_literal())
                }
                ("isnumeric", [a]) => {
                    let a = self.materialize(env, a)?;
                    guard_bound(
                        &a,
                        a.is_numeric()
                            .and(try_cast(a.value.clone(), DataType::Float64).is_not_null()),
                    )
                }
                ("regex", [text, pattern, rest @ ..]) => {
                    let text = self.materialize(env, text)?;
                    let pattern = self.materialize(env, pattern)?;
                    let flags = match rest {
                        [] => None,
                        [flags] => Some(self.materialize(env, flags)?),
                        _ => return unsupported("REGEX takes two or three arguments"),
                    };
                    let mut valid = vec![text.is_string(), pattern.is_simple()];
                    let mut options = s("");
                    if let Some(flags) = &flags {
                        valid.push(flags.is_simple());
                        options = flags.value.clone();
                    }
                    case(
                        vec![(
                            and_all(valid),
                            duck_str("__crabgraph_sparql_scalar",
                                vec![s("regex"), text.value, pattern.value, s(""), options]).eq(s("true")),
                        )],
                        None,
                    )
                }
                ("contains" | "strstarts" | "strends", [a, b]) => {
                    let a = self.materialize(env, a)?;
                    let b = self.materialize(env, b)?;
                    let function = match name.as_str() {
                        "contains" => "contains",
                        "strstarts" => "starts_with",
                        _ => "ends_with",
                    };
                    case(
                        vec![(
                            string_args_compatible(&a, &b),
                            duck(function, vec![a.value, b.value], DataType::Boolean),
                        )],
                        None,
                    )
                }
                ("langmatches", [tag, range]) => {
                    let tag = self.materialize(env, tag)?;
                    let range = self.materialize(env, range)?;
                    let lower_tag = duck_str("lower", vec![tag.value.clone()]);
                    let lower_range = duck_str("lower", vec![range.value.clone()]);
                    let matches =
                        case(
                            vec![(
                                range.value.clone().eq(s("*")),
                                tag.value.clone().not_eq(s("")),
                            )],
                            Some(lower_tag.clone().eq(lower_range.clone()).or(
                                lower_tag.like(duck_str("concat", vec![lower_range, s("-%")])),
                            )),
                        );
                    case(
                        vec![(tag.is_simple().and(range.is_simple()), matches)],
                        None,
                    )
                }
                (name, _) if name == crate::language::sparql::SPARQL_NESTED_EXISTS_CALL => {
                    return unsupported(
                        "EXISTS nested inside a larger expression is not lowered yet; only top-level FILTER (NOT) EXISTS conjuncts are",
                    );
                }
                _ => return Ok(None),
            },
            _ => return Ok(None),
        }))
    }

    pub(super) fn expr(&mut self, env: &mut Env, expr: &IrExpr) -> RelResult<Term> {
        use crate::language::sparql::{
            SPARQL_IRI_CALL, SPARQL_LANG_LITERAL_CALL, SPARQL_LITERAL_CALL,
        };
        if let Some(boolean) = self.boolean(env, expr)? {
            return Ok(Term::boolean(boolean));
        }
        Ok(match expr {
            IrExpr::Binding(name) => env.term(name),
            IrExpr::Lit(value) => match value {
                Lit::Null => Term::error(),
                other => {
                    let (lexical, datatype) = literal_identity(other)?;
                    Term::literal(s(&lexical), &datatype)
                }
            },
            IrExpr::Binary { op, lhs, rhs } => {
                let a = self.materialize(env, lhs)?;
                let b = self.materialize(env, rhs)?;
                let rank = self.column(env, promoted_rank(&a, &b))?;
                arithmetic(*op, &a, &b, rank)?
            }
            IrExpr::Case { arms, otherwise } => {
                // IF(condition, yes, no): errors in the condition propagate;
                // only the selected branch's value (or error) is returned.
                let [(condition, yes)] = arms.as_slice() else {
                    return unsupported("multi-arm CASE is not SPARQL IF");
                };
                let condition = self.ebv(env, condition)?;
                let yes = self.materialize(env, yes)?;
                let no = match otherwise {
                    Some(no) => self.materialize(env, no)?,
                    None => Term::error(),
                };
                let pick = |a: Expr, b: Expr| {
                    case(
                        vec![
                            (condition.clone().is_true(), a),
                            (condition.clone().is_false(), b),
                        ],
                        None,
                    )
                };
                Term {
                    value: pick(yes.value, no.value),
                    kind: pick(yes.kind, no.kind),
                    dt: pick(yes.dt, no.dt),
                    lang: pick(yes.lang, no.lang),
                }
            }
            IrExpr::Call { name, args } => match (name.as_str(), args.as_slice()) {
                (name, [IrExpr::Lit(Lit::String(iri))]) if name == SPARQL_IRI_CALL => {
                    Term::iri(s(iri))
                }
                (
                    name,
                    [
                        IrExpr::Lit(Lit::String(lexical)),
                        IrExpr::Lit(Lit::String(datatype)),
                    ],
                ) if name == SPARQL_LITERAL_CALL => Term::literal(s(lexical), datatype),
                (
                    name,
                    [
                        IrExpr::Lit(Lit::String(lexical)),
                        IrExpr::Lit(Lit::String(lang)),
                    ],
                ) if name == SPARQL_LANG_LITERAL_CALL => Term {
                    value: s(lexical),
                    kind: s(KIND_LITERAL),
                    dt: s(RDF_LANG_STRING),
                    lang: s(&lang.to_ascii_lowercase()),
                },
                ("sparql_coalesce", values) => {
                    let mut terms = Vec::new();
                    for value in values {
                        terms.push(self.materialize(env, value)?);
                    }
                    let pick = |part: fn(&Term) -> Expr| {
                        case(
                            terms
                                .iter()
                                .map(|term| (term.bound(), part(term)))
                                .collect(),
                            None,
                        )
                    };
                    if terms.is_empty() {
                        Term::error()
                    } else {
                        Term {
                            value: pick(|t| t.value.clone()),
                            kind: pick(|t| t.kind.clone()),
                            dt: pick(|t| t.dt.clone()),
                            lang: pick(|t| t.lang.clone()),
                        }
                    }
                }
                ("sparql_unary_plus", [a]) => {
                    let a = self.materialize(env, a)?;
                    a.clone().only_if(a.is_numeric())
                }
                ("sparql_unary_minus", [a]) => {
                    let a = self.materialize(env, a)?;
                    let zero = Term::integer(lit(0_i64));
                    let rank = self.column(env, promoted_rank(&zero, &a))?;
                    arithmetic(BinaryOp::Sub, &zero, &a, rank)?
                }
                (name, args) => self.function(env, name, args)?,
            },
            other => return unsupported(format!("expression {other:?}")),
        })
    }

    pub(super) fn function(&mut self, env: &mut Env, name: &str, args: &[IrExpr]) -> RelResult<Term> {
        let mut terms = Vec::new();
        for arg in args {
            terms.push(self.materialize(env, arg)?);
        }
        let arity = |n: usize| -> RelResult<()> {
            if terms.len() == n {
                Ok(())
            } else {
                unsupported(format!("{name} expects {n} argument(s)"))
            }
        };
        Ok(match name {
            "bnode" => {
                let value = match terms.as_slice() {
                    [] => duck_str("concat", vec![s("b"), cast(duck_str("uuid", vec![]), DataType::Utf8)]),
                    [term] => {
                        if !env.plan.schema().has_column_with_unqualified_name(BNODE_SCOPE) {
                            let mut columns: Vec<_> = env.plan.schema().fields().iter()
                                .map(|field| col_exact(field.name())).collect();
                            columns.push(cast(duck_str("uuid", vec![]), DataType::Utf8).alias(BNODE_SCOPE));
                            let plan = self.project(env.plan.clone(), columns)?;
                            env.plan = self.cte(plan)?;
                        }
                        duck_str("concat", vec![s("b"), col_exact(BNODE_SCOPE), s("_"),
                            duck_str("hex", vec![term.value.clone()])])
                    }
                    _ => return unsupported("BNODE takes zero or one argument"),
                };
                let term = Term { value, kind: s(KIND_BLANK), dt: null_str(), lang: null_str() };
                if terms.is_empty() { term } else { term.only_if(terms[0].is_simple()) }
            }
            "str" => {
                arity(1)?;
                let a = &terms[0];
                Term::string(a.value.clone()).only_if(
                    a.kind
                        .clone()
                        .in_list(vec![s(KIND_IRI), s(KIND_LITERAL)], false),
                )
            }
            "lang" => {
                arity(1)?;
                let a = &terms[0];
                Term::string(duck_str("coalesce", vec![a.lang.clone(), s("")]))
                    .only_if(a.is_literal())
            }
            "datatype" => {
                arity(1)?;
                let a = &terms[0];
                Term::iri(a.dt.clone()).only_if(a.is_literal())
            }
            "sparql_resolve_iri" => {
                arity(2)?;
                let a = &terms[0];
                Term::iri(duck_str("__crabgraph_sparql_scalar", vec![
                    s("resolve_iri"), a.value.clone(), terms[1].value.clone(), s(""), s("")
                ])).only_if(a.kind.clone().eq(s(KIND_IRI)).or(a.is_simple()))
            }
            "iri" | "uri" => {
                arity(1)?;
                let a = &terms[0];
                Term::iri(a.value.clone()).only_if(a.kind.clone().eq(s(KIND_IRI)).or(a.is_simple()))
            }
            "strlen" => {
                arity(1)?;
                let a = &terms[0];
                Term::integer(duck("length", vec![a.value.clone()], DataType::Int64))
                    .only_if(a.is_string())
            }
            "ucase" | "lcase" => {
                arity(1)?;
                let a = &terms[0];
                let function = if name == "ucase" { "upper" } else { "lower" };
                Term {
                    value: duck_str(function, vec![a.value.clone()]),
                    ..a.clone()
                }
                .only_if(a.is_string())
            }
            "strlang" => {
                arity(2)?;
                let (a, b) = (&terms[0], &terms[1]);
                Term {
                    value: a.value.clone(),
                    kind: s(KIND_LITERAL),
                    dt: s(RDF_LANG_STRING),
                    lang: duck_str("lower", vec![b.value.clone()]),
                }
                .only_if(
                    a.is_simple()
                        .and(b.is_simple())
                        .and(b.value.clone().not_eq(s(""))),
                )
            }
            "strdt" => {
                arity(2)?;
                let (a, b) = (&terms[0], &terms[1]);
                Term {
                    value: a.value.clone(),
                    kind: s(KIND_LITERAL),
                    dt: b.value.clone(),
                    lang: null_str(),
                }
                .only_if(a.is_simple().and(b.kind.clone().eq(s(KIND_IRI))))
            }
            "concat" => {
                if terms.is_empty() {
                    return Ok(Term::string(s("")));
                }
                let valid = and_all(terms.iter().map(Term::is_string).collect());
                let first_lang = terms[0].lang.clone();
                let same_lang = and_all(
                    terms
                        .iter()
                        .map(|term| {
                            term.lang
                                .clone()
                                .is_not_null()
                                .and(term.lang.clone().eq(first_lang.clone()))
                        })
                        .collect(),
                );
                let value = duck_str(
                    "concat",
                    terms.iter().map(|term| term.value.clone()).collect(),
                );
                Term {
                    value,
                    kind: s(KIND_LITERAL),
                    dt: case(
                        vec![(same_lang.clone(), s(RDF_LANG_STRING))],
                        Some(s(&xsd("string"))),
                    ),
                    lang: case(vec![(same_lang, first_lang)], None),
                }
                .only_if(valid)
            }
            "substr" => {
                if !(2..=3).contains(&terms.len()) {
                    return unsupported("SUBSTR expects two or three arguments");
                }
                let text = &terms[0];
                let start = duck(
                    "round",
                    vec![try_cast(terms[1].value.clone(), DataType::Float64)],
                    DataType::Float64,
                );
                let mut valid = vec![text.is_string(), terms[1].is_numeric()];
                // Characters at positions p with start <= p < start + length.
                let first = case(
                    vec![(start.clone().lt(lit(1.0_f64)), lit(1.0_f64))],
                    Some(start.clone()),
                );
                let value = if let Some(length) = terms.get(2) {
                    valid.push(length.is_numeric());
                    let length = duck(
                        "round",
                        vec![try_cast(length.value.clone(), DataType::Float64)],
                        DataType::Float64,
                    );
                    let end = start + length;
                    let count = end - first.clone();
                    let count = case(
                        vec![(count.clone().lt(lit(0.0_f64)), lit(0.0_f64))],
                        Some(count),
                    );
                    duck_str(
                        "substring",
                        vec![
                            text.value.clone(),
                            cast(first, DataType::Int64),
                            cast(count, DataType::Int64),
                        ],
                    )
                } else {
                    duck_str(
                        "substring",
                        vec![text.value.clone(), cast(first, DataType::Int64)],
                    )
                };
                Term {
                    value,
                    ..text.clone()
                }
                .only_if(and_all(valid))
            }
            "strbefore" | "strafter" => {
                arity(2)?;
                let (a, b) = (&terms[0], &terms[1]);
                let position = duck(
                    "strpos",
                    vec![a.value.clone(), b.value.clone()],
                    DataType::Int64,
                );
                let found = position.clone().gt(lit(0_i64));
                let value = if name == "strbefore" {
                    duck_str(
                        "substring",
                        vec![a.value.clone(), lit(1_i64), position.clone() - lit(1_i64)],
                    )
                } else {
                    duck_str(
                        "substring",
                        vec![
                            a.value.clone(),
                            position.clone()
                                + duck("length", vec![b.value.clone()], DataType::Int64),
                        ],
                    )
                };
                // A miss yields the empty simple literal; a hit keeps the
                // language tag of the first argument.
                Term {
                    value: case(vec![(found.clone(), value)], Some(s(""))),
                    kind: s(KIND_LITERAL),
                    dt: case(vec![(found.clone(), a.dt.clone())], Some(s(&xsd("string")))),
                    lang: case(vec![(found, a.lang.clone())], None),
                }
                .only_if(string_args_compatible(a, b))
            }
            "replace" => {
                if !(3..=4).contains(&terms.len()) {
                    return unsupported("REPLACE expects three or four arguments");
                }
                let text = &terms[0];
                let mut options = s("");
                let mut valid = vec![text.is_string(), terms[1].is_simple(), terms[2].is_simple()];
                if let Some(flags) = terms.get(3) {
                    valid.push(flags.is_simple());
                    options = flags.value.clone();
                }
                Term {
                    value: duck_str(
                        "__crabgraph_sparql_scalar",
                        vec![
                            s("replace"),
                            text.value.clone(),
                            terms[1].value.clone(),
                            terms[2].value.clone(),
                            options,
                        ],
                    ),
                    ..text.clone()
                }
                .only_if(and_all(valid))
            }
            "abs" | "ceil" | "floor" | "round" => {
                arity(1)?;
                let a = &terms[0];
                let rank = a.numeric_rank();
                let apply = |value: Expr| match name {
                    "abs" => duck("abs", vec![value.clone()], DataType::Float64),
                    "ceil" => duck("ceil", vec![value.clone()], DataType::Float64),
                    "floor" => duck("floor", vec![value.clone()], DataType::Float64),
                    // fn:round rounds halves toward positive infinity.
                    _ => duck("floor", vec![value + lit(0.5_f64)], DataType::Float64),
                };
                let value = case(
                    vec![
                        (
                            rank.clone().eq(lit(1_i64)),
                            cast(
                                cast(apply(try_cast(a.value.clone(), INT)), INT),
                                DataType::Utf8,
                            ),
                        ),
                        (
                            rank.clone().eq(lit(2_i64)),
                            decimal_lexical(apply(try_cast(a.value.clone(), DEC))),
                        ),
                    ],
                    Some(double_lexical(apply(try_cast(
                        a.value.clone(),
                        DataType::Float64,
                    )))),
                );
                Term { value, ..a.clone() }.only_if(a.is_numeric())
            }
            "md5" | "sha1" | "sha256" => {
                arity(1)?;
                let a = &terms[0];
                Term::string(duck_str(name, vec![a.value.clone()])).only_if(a.is_simple())
            }
            "sha384" | "sha512" | "encode_for_uri" => {
                arity(1)?;
                let a = &terms[0];
                Term::string(duck_str("__crabgraph_sparql_scalar",
                    vec![s(name), a.value.clone(), s(""), s(""), s("")]))
                    .only_if(if name == "encode_for_uri" { a.is_string() } else { a.is_simple() })
            }
            "timezone" => {
                arity(1)?;
                let a = &terms[0];
                Term::literal(duck_str("__crabgraph_sparql_scalar",
                    vec![s(name), a.value.clone(), s(""), s(""), s("")]), &xsd("dayTimeDuration"))
                    .only_if(a.has_datatype(&xsd("dateTime")))
            }
            "now" => {
                arity(0)?;
                Term::literal(duck_str("strftime", vec![
                    duck("make_timestamp", vec![duck("epoch_us", vec![
                        duck("now", vec![], DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, Some("UTC".into())))
                    ], DataType::Int64)], DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond, None)),
                    s("%Y-%m-%dT%H:%M:%S.%fZ")]), &xsd("dateTime"))
            }
            "year" | "month" | "day" | "hours" | "minutes" | "seconds" | "tz" => {
                arity(1)?;
                let a = &terms[0];
                let group = match name {
                    "year" => 1,
                    "month" => 2,
                    "day" => 3,
                    "hours" => 4,
                    "minutes" => 5,
                    "seconds" => 6,
                    _ => 7,
                };
                let pattern = r"^(-?[0-9]{4,})-([0-9]{2})-([0-9]{2})T([0-9]{2}):([0-9]{2}):([0-9]{2}(?:\.[0-9]+)?)(Z|[+-][0-9]{2}:[0-9]{2})?$";
                let part = duck_str(
                    "regexp_extract",
                    vec![a.value.clone(), s(pattern), lit(group as i64)],
                );
                let valid = a.has_datatype(&xsd("dateTime")).and(duck(
                    "regexp_full_match",
                    vec![a.value.clone(), s(&pattern[1..pattern.len() - 1])],
                    DataType::Boolean,
                ));
                let term = match name {
                    "tz" => Term::string(part),
                    "seconds" => {
                        Term::literal(decimal_lexical(try_cast(part, DEC)), &xsd("decimal"))
                    }
                    _ => Term::literal(
                        cast(try_cast(part, DataType::Int64), DataType::Utf8),
                        &xsd("integer"),
                    ),
                };
                term.only_if(valid)
            }
            "uuid" => {
                arity(0)?;
                Term::iri(duck_str(
                    "concat",
                    vec![
                        s("urn:uuid:"),
                        cast(duck("uuid", vec![], DataType::Utf8), DataType::Utf8),
                    ],
                ))
            }
            "struuid" => {
                arity(0)?;
                Term::string(cast(duck("uuid", vec![], DataType::Utf8), DataType::Utf8))
            }
            "rand" => {
                arity(0)?;
                Term::literal(
                    double_lexical(duck("random", vec![], DataType::Float64)),
                    &xsd("double"),
                )
            }
            iri if iri.starts_with(XSD) => {
                arity(1)?;
                xsd_cast(&iri[XSD.len()..], &terms[0])?
            }
            other => return unsupported(format!("function `{other}`")),
        })
    }
}
