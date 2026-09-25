//! Aggregate for typed SPARQL lowering.

use super::*;

impl Lowerer<'_, '_> {
    pub(super) fn aggregate(
        &mut self,
        sol: Sol,
        group: &[ProjectionItem],
        aggs: &[AggCall],
    ) -> RelResult<Sol> {
        let mut env = Env {
            plan: sol.plan.clone(),
            vars: sol.vars.clone(),
            temps: Vec::new(),
        };
        let mut group_vars = Vec::new();
        for item in group {
            match &item.expr {
                IrExpr::Binding(name) if name == &item.alias => group_vars.push(name.clone()),
                _ => {
                    return unsupported("GROUP BY keys must be variables after algebra translation");
                }
            }
        }
        // Pre-project every aggregate argument into fresh term columns.
        let mut pre = Vec::new();
        for var in &group_vars {
            pre.extend(env.term(var).aliased(var));
        }
        pre.extend(sol.keys.iter().map(col_exact));
        let mut inputs = Vec::new();
        for (index, agg) in aggs.iter().enumerate() {
            let base = format!("__sq_agg{index}");
            let (arg, separator) = match (&agg.kind, &agg.arg) {
                (AggKind::CollectRows, Some(IrExpr::Call { name, args }))
                    if name == crate::language::sparql::SPARQL_GROUP_CONCAT_CALL =>
                {
                    match args.as_slice() {
                        [arg, IrExpr::Lit(Lit::String(separator))] => {
                            (Some(arg.clone()), Some(separator.clone()))
                        }
                        _ => return unsupported("malformed GROUP_CONCAT"),
                    }
                }
                (AggKind::CollectRows | AggKind::CollectTraversers, _) => {
                    return unsupported("collection aggregates are not SPARQL aggregates");
                }
                (_, arg) => (arg.clone(), None),
            };
            match &arg {
                Some(arg) => {
                    let term = guarded(self.materialize(&mut env, arg)?);
                    pre.extend(term.aliased(&base));
                }
                None if agg.kind == AggKind::CountDistinct => {
                    // COUNT(DISTINCT *): one text key over every variable.
                    let mut parts = Vec::new();
                    for var in sol.vars.keys() {
                        let term = Term::columns(var);
                        parts.extend([
                            duck_str("coalesce", vec![term.kind, s("U")]),
                            duck_str("coalesce", vec![term.dt, s("")]),
                            duck_str("coalesce", vec![term.lang, s("")]),
                            duck_str("coalesce", vec![term.value, s("")]),
                        ]);
                    }
                    let key = if parts.is_empty() {
                        s("")
                    } else {
                        duck_str(
                            "concat_ws",
                            std::iter::once(s("\u{1}")).chain(parts).collect(),
                        )
                    };
                    pre.extend(Term::string(key).aliased(&base));
                }
                None => {}
            }
            inputs.push((
                base,
                arg.is_some() || agg.kind == AggKind::CountDistinct,
                separator,
            ));
        }
        if pre.is_empty() {
            pre.push(lit(1_i64).alias(self.fresh("row")));
        }
        let pre_plan = self.project(env.plan, pre)?;
        let pre_plan = self.cte(pre_plan)?;
        let mut group_exprs = Vec::new();
        for var in &group_vars {
            group_exprs.extend(var_columns(var).into_iter().map(col_exact));
        }
        group_exprs.extend(sol.keys.iter().map(col_exact));
        let mut agg_exprs = Vec::new();
        let mut finals: Vec<(String, Term, bool)> = Vec::new();
        for ((agg, (base, has_arg, separator)), index) in aggs.iter().zip(inputs).zip(0..) {
            let term = Term::columns(&base);
            let name = |part: &str| format!("__sq_agg{index}_{part}");
            let distinct = |expr: Expr| -> RelResult<Expr> {
                Ok(if agg.distinct {
                    expr.distinct().build()?
                } else {
                    expr
                })
            };
            match agg.kind {
                AggKind::CountRows | AggKind::CountBulk | AggKind::CountDistinct => {
                    let counted = if !has_arg {
                        count_all()
                    } else {
                        let key = case(
                            vec![(term.kind.clone().is_null(), null_str())],
                            Some(duck_str(
                                "concat_ws",
                                vec![
                                    s("\u{1}"),
                                    term.kind.clone(),
                                    duck_str("coalesce", vec![term.dt.clone(), s("")]),
                                    duck_str("coalesce", vec![term.lang.clone(), s("")]),
                                    term.value.clone(),
                                ],
                            )),
                        );
                        distinct(df_count(key))?
                    };
                    agg_exprs.push(counted.alias(name("count")));
                    finals.push((
                        agg.alias.clone(),
                        Term::integer(col_exact(name("count"))),
                        true,
                    ));
                }
                AggKind::Sum
                | AggKind::SumOrZero
                | AggKind::Avg
                | AggKind::AvgOrNull
                | AggKind::AvgOrZero => {
                    let rank = term.numeric_rank();
                    let valid = case(
                        vec![
                            (term.kind.clone().is_null(), lit(ScalarValue::Int64(None))),
                            (
                                rank.clone().is_not_null().and(
                                    try_cast(term.value.clone(), DataType::Float64).is_not_null(),
                                ),
                                lit(1_i64),
                            ),
                        ],
                        Some(lit(0_i64)),
                    );
                    agg_exprs.push(df_min(valid).alias(name("valid")));
                    agg_exprs.push(df_max(rank).alias(name("rank")));
                    agg_exprs.push(
                        distinct(df_count(try_cast(term.value.clone(), DataType::Float64)))?
                            .alias(name("n")),
                    );
                    agg_exprs.push(
                        distinct(df_sum(try_cast(term.value.clone(), INT)))?.alias(name("int")),
                    );
                    agg_exprs.push(
                        distinct(df_sum(try_cast(term.value.clone(), DEC)))?.alias(name("dec")),
                    );
                    agg_exprs.push(
                        distinct(df_sum(try_cast(term.value.clone(), DataType::Float64)))?
                            .alias(name("dbl")),
                    );
                    let rank = col_exact(name("rank"));
                    let n = col_exact(name("n"));
                    let is_avg = matches!(
                        agg.kind,
                        AggKind::Avg | AggKind::AvgOrNull | AggKind::AvgOrZero
                    );
                    let dec_value = if is_avg {
                        duck_str("__crabgraph_sparql_scalar", vec![s("decimal_divide"),
                            cast(col_exact(name("dec")), DataType::Utf8),
                            cast(n.clone(), DataType::Utf8), s(""), s("")])
                    } else {
                        decimal_lexical(col_exact(name("dec")))
                    };
                    let dbl_value = if is_avg {
                        col_exact(name("dbl")) / cast(n.clone(), DataType::Float64)
                    } else {
                        col_exact(name("dbl"))
                    };
                    let int_rank = if is_avg { lit(-1_i64) } else { lit(1_i64) };
                    let value = case(
                        vec![
                            (n.clone().eq(lit(0_i64)), s("0")),
                            (
                                rank.clone().eq(int_rank),
                                integer_lexical(col_exact(name("int"))),
                            ),
                            (rank.clone().lt_eq(lit(2_i64)), dec_value),
                        ],
                        Some(double_lexical(dbl_value)),
                    );
                    let dt = case(
                        vec![
                            (n.clone().eq(lit(0_i64)), s(&xsd("integer"))),
                            (
                                rank.clone().eq(lit(1_i64)),
                                s(&xsd(if is_avg { "decimal" } else { "integer" })),
                            ),
                            (rank.clone().eq(lit(2_i64)), s(&xsd("decimal"))),
                            (rank.clone().eq(lit(3_i64)), s(&xsd("float"))),
                        ],
                        Some(s(&xsd("double"))),
                    );
                    let ok = col_exact(name("valid")).eq(lit(0_i64)).not();
                    let ok = case(
                        vec![(col_exact(name("valid")).is_null(), lit(true))],
                        Some(ok),
                    );
                    finals.push((
                        agg.alias.clone(),
                        Term {
                            value,
                            kind: s(KIND_LITERAL),
                            dt,
                            lang: null_str(),
                        }
                        .only_if(ok),
                        false,
                    ));
                }
                AggKind::Min | AggKind::MinOrNull | AggKind::Max | AggKind::MaxOrNull => {
                    let asc = matches!(agg.kind, AggKind::Min | AggKind::MinOrNull);
                    let mut order = vec![term.kind.clone().is_null().sort(true, false)];
                    order.extend(order_keys(&term).into_iter().map(|key| key.sort(asc, asc)));
                    let parts = [
                        ("v", term.value.clone()),
                        ("k", term.kind.clone()),
                        ("d", term.dt.clone()),
                        ("l", term.lang.clone()),
                    ];
                    for (part, expr) in parts {
                        agg_exprs.push(
                            array_agg(expr)
                                .order_by(order.clone())
                                .build()?
                                .alias(name(part)),
                        );
                    }
                    let first = |part: &str| {
                        duck_str("list_extract", vec![col_exact(name(part)), lit(1_i64)])
                    };
                    finals.push((
                        agg.alias.clone(),
                        Term {
                            value: first("v"),
                            kind: first("k"),
                            dt: first("d"),
                            lang: first("l"),
                        },
                        false,
                    ));
                }
                AggKind::CollectRows => {
                    let separator = separator.unwrap_or_else(|| " ".into());
                    let valid = case(
                        vec![
                            (term.kind.clone().is_null(), lit(ScalarValue::Int64(None))),
                            (term.is_literal(), lit(1_i64)),
                        ],
                        Some(lit(0_i64)),
                    );
                    agg_exprs.push(df_min(valid).alias(name("valid")));
                    let concatenated = datafusion::functions_aggregate::string_agg::string_agg(
                        term.value.clone(),
                        s(&separator),
                    );
                    agg_exprs.push(distinct(concatenated)?.alias(name("text")));
                    let ok = case(
                        vec![(col_exact(name("valid")).is_null(), lit(true))],
                        Some(col_exact(name("valid")).eq(lit(1_i64))),
                    );
                    finals.push((
                        agg.alias.clone(),
                        Term::string(duck_str("coalesce", vec![col_exact(name("text")), s("")]))
                            .only_if(ok),
                        false,
                    ));
                }
                other => return unsupported(format!("aggregate {other:?}")),
            }
        }
        let aggregated = LogicalPlanBuilder::from(pre_plan)
            .aggregate(group_exprs, agg_exprs)?
            .build()?;
        // Keep aggregate calls out of the finishing projection's SQL: the
        // projection references each result several times.
        let aggregated = self.cte(aggregated)?;
        let mut columns = Vec::new();
        let mut vars = BTreeMap::new();
        for var in &group_vars {
            columns.extend(var_columns(var).into_iter().map(col_exact));
            vars.insert(var.clone(), sol.vars.get(var).copied().unwrap_or(false));
        }
        columns.extend(sol.keys.iter().map(col_exact));
        for (alias, term, certain) in finals {
            columns.extend(guarded(term).aliased(&alias));
            vars.insert(alias, certain);
        }
        let plan = self.project(aggregated, columns)?;
        let plan = self.cte(plan)?;
        Ok(Sol {
            plan,
            vars,
            keys: sol.keys,
            ord: None,
        })
    }

    // -- CONSTRUCT -----------------------------------------------------------

}
