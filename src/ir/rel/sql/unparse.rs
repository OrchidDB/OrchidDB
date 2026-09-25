//! Unparse for SQL execution.

use super::*;

/// Unparse a lowered plan to dialect-specific SQL text. Constructs the
/// unparser cannot express surface as [`SqlError::Unsupported`].
pub fn unparse(lowered: &LoweredPlan, dialect: SqlDialect) -> SqlResult<String> {
    let plan = strip_constant_sorts(lowered.plan.clone())?;
    let plan = strip_identity_projections(plan)?;
    let plan = encode_unprintable_literals(plan, dialect)?;
    let plan = strip_column_qualifiers(plan)
        .map_err(|err| SqlError::Unsupported(format!("qualifier strip: {err}")))?;
    let repairs = identifier_quote_repairs(&plan, dialect)?;
    let sql = recursive::unparse_plan(plan, dialect)?;
    Ok(apply_identifier_repairs(sql, &repairs))
}

/// Rewrite literals whose unparsed text would not mean the same value.
///
/// * NUL cannot appear in statement text (DuckDB only; Postgres text cannot
///   hold it at all and fails at setup).
/// * sqlparser's quote escaper treats an existing `''` or `\'` as *already
///   escaped* and copies it through verbatim, so the string `it''s` would be
///   read back by the engine as `it's`, and `a\'b` would end the literal
///   early. Such strings are rebuilt from pieces around `chr(39)`.
/// * Non-finite floats unparse as bare `inf` / `NaN`, which engines read as
///   column references. They become a cast from their standard spelling.
pub(super) fn encode_unprintable_literals(
    plan: LogicalPlan,
    dialect: SqlDialect,
) -> SqlResult<LogicalPlan> {
    let transformed = plan.transform_up_with_subqueries(|node| {
        node.map_expressions(|expr| encode_expression_literals(expr, dialect))
    })?;
    Ok(transformed.data)
}

/// The expression-level half of [`encode_unprintable_literals`], shared by
/// whole-query emission and catalog binding. Metadata and aliases survive.
pub(super) fn encode_expression_literals(
    expr: Expr,
    dialect: SqlDialect,
) -> Result<Transformed<Expr>, DataFusionError> {
    expr.transform_up(|inner| {
        match &inner {
            Expr::Literal(value, _) if value.is_null() && matches!(value.data_type(),
                DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)) => {
                // The upstream unparser reads list offsets without checking
                // validity, rendering a typed null list as []. Preserve both
                // the null value and the type at the SQL boundary.
                return Ok(Transformed::yes(Expr::Cast(datafusion::logical_expr::Cast::new(
                    Box::new(lit(ScalarValue::Null)), value.data_type(),
                ))));
            }
            Expr::Literal(ScalarValue::Float64(Some(value)), _) if !value.is_finite() => {
                return Ok(Transformed::yes(non_finite_float_expr(
                    *value,
                    DataType::Float64,
                )));
            }
            Expr::Literal(ScalarValue::Float32(Some(value)), _) if !value.is_finite() => {
                return Ok(Transformed::yes(non_finite_float_expr(
                    f64::from(*value),
                    DataType::Float32,
                )));
            }
            _ => {}
        }
        let Expr::Literal(ScalarValue::Utf8(Some(value)), metadata) = &inner else {
            return Ok(Transformed::no(inner));
        };
        let encode_nul = dialect == SqlDialect::DuckDb && value.contains('\0');
        let encode_quote = value.contains("''") || value.contains("\\'");
        if !encode_nul && !encode_quote {
            return Ok(Transformed::no(inner));
        }
        let mut parts = Vec::new();
        let mut current = String::new();
        let flush = |current: &mut String, parts: &mut Vec<Expr>| {
            if !current.is_empty() {
                parts.push(Expr::Literal(
                    ScalarValue::Utf8(Some(std::mem::take(current))),
                    metadata.clone(),
                ));
            }
        };
        for ch in value.chars() {
            let code = match ch {
                '\0' if encode_nul => 0_i64,
                '\'' if encode_quote => 39_i64,
                other => {
                    current.push(other);
                    continue;
                }
            };
            flush(&mut current, &mut parts);
            parts.push(df_string::chr(lit(code)));
        }
        flush(&mut current, &mut parts);
        if parts.is_empty() {
            parts.push(lit(""));
        }
        Ok(Transformed::yes(df_string::concat(parts)))
    })
}

pub(super) fn non_finite_float_expr(value: f64, data_type: DataType) -> Expr {
    let spelling = if value.is_nan() {
        "NaN"
    } else if value > 0.0 {
        "Infinity"
    } else {
        "-Infinity"
    };
    Expr::Cast(datafusion::logical_expr::Cast::new(
        Box::new(lit(spelling)),
        data_type,
    ))
}

/// Column names the unparser would quote incorrectly, paired with the text
/// it emits and the correct quoted form.
///
/// sqlparser's identifier escaper copies an existing `""` or `\"` inside a
/// name through verbatim (it assumes the pair is already escaped), so an
/// output alias taken from query text such as `cast("[ab\"cd]" as ...)`
/// terminates the identifier early. Names are collected from every plan
/// schema; each distinct misquoted rendering is rewritten in the SQL text.
pub(super) fn identifier_quote_repairs(
    plan: &LogicalPlan,
    dialect: SqlDialect,
) -> SqlResult<Vec<(String, String)>> {
    let mut names = BTreeSet::new();
    plan.apply_with_subqueries(|node| {
        for field in node.schema().fields() {
            if field.name().contains('"') {
                names.insert(field.name().clone());
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    let mut repairs = Vec::new();
    for name in names {
        let emitted = format!(
            "\"{}\"",
            datafusion::sql::sqlparser::ast::escape_quoted_string(&name, '"')
        );
        let correct = dialect.quote_ident(&name);
        if emitted != correct {
            repairs.push((emitted, correct));
        }
    }
    // Longest first, so a name that contains another is repaired whole.
    repairs.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.0.cmp(&b.0)));
    Ok(repairs)
}

pub(super) fn apply_identifier_repairs(mut sql: String, repairs: &[(String, String)]) -> String {
    for (emitted, correct) in repairs {
        if sql.contains(emitted.as_str()) {
            sql = sql.replace(emitted.as_str(), correct);
        }
    }
    sql
}

/// Re-attach aggregate `ORDER BY` clauses the unparser drops.
///
/// `array_agg(x ORDER BY k)` is valid in every dialect here, but DataFusion's
/// unparser only emits an aggregate's ordering as `WITHIN GROUP`, and only
/// for functions that accept that clause. For `array_agg` the ordering simply
/// vanishes, so the emitted SQL means something different from the plan —
/// collection order becomes whatever the engine happens to produce.
///
/// The call text is regenerated with the same unparser that produced the
/// statement, so it matches the emitted text exactly, and the ordering is
/// spliced in before the closing parenthesis. Anything ambiguous (a call that
/// does not appear exactly once) is reported as unsupported rather than
/// patched on a guess — a wrong splice would be silently wrong SQL, which is
/// precisely what this is fixing.
pub(super) fn restore_aggregate_ordering(
    plan: &LogicalPlan,
    unparser: &Unparser<'_>,
    dialect: SqlDialect,
    sql: String,
) -> SqlResult<String> {
    let mut ordered: Vec<datafusion::logical_expr::expr::AggregateFunction> = Vec::new();
    plan.apply_with_subqueries(|node| {
        node.apply_expressions(|expr| {
            expr.apply(|inner| {
                if let Expr::AggregateFunction(agg) = inner
                    && !agg.params.order_by.is_empty()
                    && !agg.func.supports_within_group_clause()
                {
                    ordered.push(agg.clone());
                }
                Ok(TreeNodeRecursion::Continue)
            })
        })?;
        Ok(TreeNodeRecursion::Continue)
    })?;

    let mut sql = sql;
    for agg in ordered {
        let mut plain = agg.clone();
        plain.params.order_by = Vec::new();
        let mut call = unparser
            .expr_to_sql(&Expr::AggregateFunction(plain))
            .map_err(|err| SqlError::Unsupported(format!("aggregate call: {err}")))?;
        functions::prepare_ast(&mut call, dialect)?;
        let call = call.to_string();
        if sql.matches(call.as_str()).count() != 1 {
            return Err(SqlError::Unsupported(format!(
                "cannot place ORDER BY on `{call}` unambiguously"
            )));
        }
        let mut keys = Vec::with_capacity(agg.params.order_by.len());
        for sort in &agg.params.order_by {
            let mut key = unparser
                .expr_to_sql(&sort.expr)
                .map_err(|err| SqlError::Unsupported(format!("order key: {err}")))?;
            functions::prepare_ast(&mut key, dialect)?;
            keys.push(format!(
                "{key} {} NULLS {}",
                if sort.asc { "ASC" } else { "DESC" },
                if sort.nulls_first { "FIRST" } else { "LAST" }
            ));
        }
        let Some(close) = matching_call_close(&call) else {
            return Err(SqlError::Unsupported(format!(
                "unrecognized aggregate call text `{call}`"
            )));
        };
        let ordered_call = format!(
            "{} ORDER BY {}{}",
            &call[..close],
            keys.join(", "),
            &call[close..]
        );
        sql = sql.replace(call.as_str(), &ordered_call);
    }
    Ok(sql)
}

pub(super) fn matching_call_close(call: &str) -> Option<usize> {
    let open = call.find('(')?;
    let mut depth = 0_u32;
    let mut quoted = false;
    for (offset, ch) in call[open..].char_indices() {
        match ch {
            '\'' => quoted = !quoted,
            '(' if !quoted => depth += 1,
            ')' if !quoted => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

/// Remove projections that select every input column unchanged. The unparser
/// otherwise collapses `Projection -> Sort -> Projection` stacks into a
/// single SELECT and loses the inner aliases, producing SQL that references
/// columns which no longer exist.
pub(super) fn strip_identity_projections(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    let transformed = plan.transform_up(|node| {
        if let LogicalPlan::Projection(projection) = &node
            && is_identity_projection(projection)
        {
            return Ok(Transformed::yes(projection.input.as_ref().clone()));
        }
        Ok(Transformed::no(node))
    })?;
    Ok(transformed.data)
}

/// Rewrite column references to be unqualified wherever that stays
/// unambiguous. DataFusion plans keep base-table qualifiers alive across
/// projection boundaries, which the unparser turns into references like
/// `"table"."col"` outside the derived table that hides `"table"` — invalid
/// SQL. Lowered plans use globally binding-prefixed column names (`a__id`,
/// `e1__src_id`, ...), so unqualified references stay unambiguous — except
/// inside user-supplied subplans ("bring your own schema" views/queries),
/// where the same column name can appear on both sides of a join. There the
/// qualifier is required and is kept: it names a `SubqueryAlias`/table that
/// exists as a FROM alias in the emitted SQL.
pub(super) fn strip_column_qualifiers(plan: LogicalPlan) -> Result<LogicalPlan, DataFusionError> {
    let transformed = plan.transform_up_with_subqueries(|node| {
        // Count how many input fields share each unqualified name in this
        // node's scope; only unique names can safely lose their qualifier.
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        {
            let inputs = node.inputs();
            let scope_schemas: Vec<_> = if inputs.is_empty() {
                vec![node.schema()]
            } else {
                inputs.iter().map(|input| input.schema()).collect()
            };
            for schema in scope_schemas {
                for (_, field) in schema.iter() {
                    *counts.entry(field.name().clone()).or_default() += 1;
                }
            }
        }
        let unique = |name: &str| counts.get(name).copied().unwrap_or(0) <= 1;
        let rewritten = node.map_expressions(|expr| {
            expr.transform_up(|inner| {
                if let Expr::Column(column) = &inner
                    && column.relation.is_some()
                    && unique(&column.name)
                {
                    return Ok(Transformed::yes(Expr::Column(Column::new_unqualified(
                        column.name.clone(),
                    ))));
                }
                Ok(Transformed::no(inner))
            })
        })?;
        if rewritten.transformed {
            Ok(Transformed::yes(rewritten.data.recompute_schema()?))
        } else {
            Ok(rewritten)
        }
    })?;
    Ok(transformed.data)
}

pub(super) fn is_identity_projection(projection: &datafusion::logical_expr::Projection) -> bool {
    let input_schema = projection.input.schema();
    if projection.expr.len() != input_schema.fields().len()
        || projection.schema.fields().len() != input_schema.fields().len()
    {
        return false;
    }
    let columns_only = projection.expr.iter().zip(input_schema.iter()).all(
        |(expr, (qualifier, field))| match expr {
            Expr::Column(column) => {
                column.name == *field.name()
                    && (column.relation.is_none() || column.relation.as_ref() == qualifier)
            }
            _ => false,
        },
    );
    // Removing the projection must not change the observable schema
    // (names and qualifiers) the parent plan sees.
    columns_only
        && projection.schema.iter().zip(input_schema.iter()).all(
            |((out_qualifier, out_field), (in_qualifier, in_field))| {
                out_qualifier == in_qualifier && out_field.name() == in_field.name()
            },
        )
}


/// A literal sort key has no ordering effect. DuckDB rejects ORDER BY NULL;
/// retain a pushed-down fetch as a limit when every key is constant.
fn strip_constant_sorts(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Sort(mut sort) = node else { return Ok(Transformed::no(node)); };
        let count = sort.expr.len();
        sort.expr.retain(|key| !matches!(key.expr, Expr::Literal(_, _)));
        if sort.expr.len() == count { return Ok(Transformed::no(LogicalPlan::Sort(sort))); }
        if !sort.expr.is_empty() { return Ok(Transformed::yes(LogicalPlan::Sort(sort))); }
        let input = sort.input.as_ref().clone();
        let result = if sort.fetch.is_some() {
            datafusion::logical_expr::LogicalPlanBuilder::from(input).limit(0, sort.fetch)?.build()?
        } else { input };
        Ok(Transformed::yes(result))
    })?.data)
}
