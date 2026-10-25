//! Unparse for SQL execution.

use super::*;

/// Unparse a lowered plan to dialect-specific SQL text. Constructs the
/// unparser cannot express surface as [`SqlError::Unsupported`].
pub fn unparse(lowered: &LoweredPlan, dialect: SqlDialect) -> SqlResult<String> {
    unparse_plan_with_context(lowered.plan.clone(), dialect, &lowered.rule_context)
}

pub(crate) fn unparse_plan(plan: LogicalPlan, dialect: SqlDialect) -> SqlResult<String> {
    unparse_plan_with_context(plan, dialect, &Default::default())
}
pub(crate) fn unparse_plan_with_context(plan: LogicalPlan, dialect: SqlDialect, context: &crate::ir::rules::RuleContext) -> SqlResult<String> {
    stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024, || unparse_plan_inner(plan, dialect, context))
}
fn unparse_plan_inner(plan: LogicalPlan, dialect: SqlDialect, context: &crate::ir::rules::RuleContext) -> SqlResult<String> {
    super::super::pipeline::validate_sql_contract(&plan, dialect, context)?;
    let plan = super::correlation::preserve_scopes(plan)?;
    // Compact emission is an implementation alternative, after the same required
    // semantic and capability validation as the ordinary emitter.
    if let Some(sql) = super::compact_unwind::emit(&plan, dialect)? {
        context.record_selection("adapter.compact_unwind", "selected-language-contract", "eligible typed VALUES/UNNEST shape");
        return Ok(sql);
    }
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.projection_cleanup", false, |p| Ok(super::cleanup::simplify(p)?))?;
    let plan = super::lowering::transform_relations(plan, dialect)?;
    crate::ir::rules::RuleSession::new(context.clone()).invalidate();
    let plan = crate::ir::rel::pipeline::placement(plan, context).map_err(|e| match e {
        crate::ir::rel::RelError::RulePipeline(error) => SqlError::RulePipeline(error),
        error => SqlError::Unsupported(error.to_string()),
    })?.plan;
    validate_portable_calls(&plan, dialect)?;
    plan.apply_with_subqueries(|node| {
        for expr in node.expressions() {
            expr.apply(|expr| {
                if let Some(requirements) = super::capabilities::SubqueryRequirements::from_expr(expr) {
                    dialect.check_subquery(&requirements).map_err(|why| DataFusionError::Plan(why.into()))?;
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    let plan = specialize_portable_calls(plan, dialect)?;
    crate::ir::rules::RuleSession::new(context.clone()).invalidate();
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.output_demand", false, |p| Ok(super::super::optimizer::prune_sql_outputs(p)?))?;
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.predicate_scopes", false, super::predicate_scope::rewrite)?;
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.optional_presence", false, super::optional_filter::rewrite)?;
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.literal_case", false, |p| Ok(super::literal_case::rewrite(p)?))?;
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.projection_cleanup", false, |p| Ok(super::cleanup::simplify(p)?))?;
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.output_demand", false, |p| Ok(super::super::optimizer::prune_sql_outputs(p)?))?;
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.projection_cleanup", false, |p| Ok(super::cleanup::simplify(p)?))?;
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.aggregate_scan", false, |p| Ok(super::aggregate_scan::rewrite(p)?))?;
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.conditional_count", false, |p| Ok(super::count_filter::rewrite(p, dialect)?))?;
    let plan = if dialect == SqlDialect::Postgres { super::postgres_lists::encode(plan)? } else { plan };
    // Generated-statistics providers accept only inexact pushdown: the parent
    // residual is authoritative. Do not emit duplicate scan hints under aliases
    // (DF53 can reintroduce the hidden physical qualifier inside RDF CASEs).
    let plan = plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::TableScan(mut scan) = node else { return Ok(Transformed::no(node)); };
        if crate::ir::rel::statistics::statistics_provider(&datafusion::datasource::source_as_provider(&scan.source)?).is_some() && !scan.filters.is_empty() {
            scan.filters.clear();
            scan.fetch = None;
            return Ok(Transformed::yes(LogicalPlan::TableScan(scan)));
        }
        Ok(Transformed::no(LogicalPlan::TableScan(scan)))
    })?.data;
    let plan = super::super::pipeline::typed_sql(plan, context, "adapter.shared_partition_scans", false, super::shared_scans::rewrite)?;
    let plan = expand_sort_fetch(plan)?;
    let plan = strip_constant_sorts(plan)?;
    let plan = plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Projection(projection) = &node else { return Ok(Transformed::no(node)); };
        if !projection.expr.is_empty() { return Ok(Transformed::no(node)); }
        // SQL has no zero-column rows. A constant preserves row multiplicity,
        // including DISTINCT's one-row result for a nonempty zero-column bag.
        Ok(Transformed::yes(datafusion::logical_expr::LogicalPlanBuilder::from(
            projection.input.as_ref().clone())
            .project(vec![lit(1_i64).alias("__orchid_cardinality")])?.build()?))
    })?.data;
    let plan = strip_identity_projections(plan)?;
    let plan = preserve_postgres_text_extrema(plan, dialect)?;
    let plan = preserve_portable_numeric_types(plan, dialect)?;
    // DF's unparser renders an EmptyRelation as a SELECT of typed NULLs,
    // irrespective of produce_one_row. Preserve zero cardinality explicitly.
    let plan = plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::EmptyRelation(empty) = node else { return Ok(Transformed::no(node)); };
        if empty.produce_one_row { return Ok(Transformed::no(LogicalPlan::EmptyRelation(empty))); }
        let mut columns = empty.schema.iter().map(|(qualifier, field)| {
            Expr::Cast(datafusion::logical_expr::Cast::new(
                Box::new(lit(ScalarValue::Null)), field.data_type().clone()))
                .alias_qualified(qualifier.cloned(), field.name())
        }).collect::<Vec<_>>();
        if columns.is_empty() {
            columns.push(lit(1_i64).alias("__orchid_void"));
        }
        let plan = datafusion::logical_expr::LogicalPlanBuilder::empty(true)
            .project(columns)?.limit(0, Some(0))?.build()?;
        Ok(Transformed::yes(plan))
    })?.data;
    let plan = encode_unprintable_literals(plan, dialect)?;
    let plan = normalize_projection_names(plan)?;
    // A final naming projection must not bury ORDER BY in a derived table:
    // Trino correctly discards ordering that belongs only to a subquery.
    let plan = plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Projection(projection) = &node else { return Ok(Transformed::no(node)); };
        let LogicalPlan::Sort(sort) = projection.input.as_ref() else { return Ok(Transformed::no(node)); };
        let mut columns = std::collections::BTreeMap::new();
        for (expr, output) in projection.expr.iter().zip(projection.schema.columns()) {
            let expr = if let Expr::Alias(a) = expr { a.expr.as_ref() } else { expr };
            let Expr::Column(column) = expr else { return Ok(Transformed::no(node)); };
            columns.insert(sort.input.schema().index_of_column(column)?, output);
        }
        let mut keys = Vec::new();
        for key in &sort.expr {
            let mut complete = true;
            let expr = key.expr.clone().transform_up(|expr| {
                if let Expr::Column(column) = &expr {
                    let index = sort.input.schema().index_of_column(column)?;
                    if let Some(output) = columns.get(&index) { return Ok(Transformed::yes(Expr::Column(output.clone()))); }
                    complete = false;
                }
                Ok(Transformed::no(expr))
            })?.data;
            if !complete { return Ok(Transformed::no(node)); }
            keys.push(expr.sort(key.asc, key.nulls_first));
        }
        let mut projection = projection.clone();
        projection.input = sort.input.clone();
        let mut sort = sort.clone();
        sort.input = Arc::new(LogicalPlan::Projection(projection));
        sort.expr = keys;
        Ok(Transformed::yes(LogicalPlan::Sort(sort)))
    })?.data;
    let plan = preserve_limit_output(plan)?;
    let plan = if !dialect.requires_scope_repair() && !has_rdf_source(&plan) {
        strip_column_qualifiers(plan).map_err(|err| SqlError::Unsupported(format!("qualifier strip: {err}")))?
    } else { plan };
    // DataFusion may decorrelate a scalar aggregate to a conditionless outer
    // join. Unlike CROSS JOIN, an outer join requires an explicit SQL predicate.
    // Add it after optimization, where it cannot be simplified away again.
    let plan = plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Join(mut join) = node else { return Ok(Transformed::no(node)); };
        if matches!(join.join_type, datafusion::logical_expr::JoinType::Left
            | datafusion::logical_expr::JoinType::Right | datafusion::logical_expr::JoinType::Full)
            && join.on.is_empty() && join.filter.is_none()
            && join.join_constraint == datafusion::logical_expr::JoinConstraint::On {
            join.filter = Some(lit(true));
            return Ok(Transformed::yes(LogicalPlan::Join(join)));
        }
        Ok(Transformed::no(LogicalPlan::Join(join)))
    })?.data;
    let plan = super::correlation::preserve_scopes(plan)?;
    let repairs = identifier_quote_repairs(&plan, dialect)?;
    let sql = super::logical_functions::with_plan(&plan, dialect, || recursive::unparse_plan(plan.clone(), dialect))?;
    Ok(apply_identifier_repairs(sql, &repairs))
}

/// SQL names must follow the declared projection schema even when lowering
/// replaces a column by a representation cast. Collapse repeated explicit
/// casts, but retain the inner SQL cast: backend function types (e.g. UUID)
/// can differ from their logical string representation.
fn normalize_projection_names(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Projection(mut projection) = node else { return Ok(Transformed::no(node)); };
        projection.expr = projection.expr.into_iter().zip(projection.schema.iter())
            .map(|(expr, (qualifier, field))| {
                let expr = expr.transform_up(|expr| {
                    if let Expr::Cast(cast) = &expr {
                        if let Expr::Cast(inner) = cast.expr.as_ref()
                            && inner.data_type == cast.data_type {
                            return Ok(Transformed::yes(*cast.expr.clone()));
                        }
                    }
                    Ok(Transformed::no(expr))
                })?.data;
                let expr = match expr { Expr::Alias(alias) => *alias.expr, other => other };
                Ok(expr.alias_qualified(qualifier.cloned(), field.name()))
            }).collect::<datafusion::common::Result<Vec<_>>>()?;
        Ok(Transformed::yes(LogicalPlan::Projection(projection)))
    })?.data)
}

/// Arrow string extrema use byte ordering, independent of database locale.
/// Explicit collation keeps PostgreSQL MIN/MAX consistent with native execution.
fn preserve_postgres_text_extrema(plan: LogicalPlan, dialect: SqlDialect) -> SqlResult<LogicalPlan> {
    use crate::ir::functions::logical::{LogicalFunction, SqlFunctionMapping};
    use datafusion::logical_expr::{ExprSchemable, Volatility, expr_rewriter::NamePreserver};
    if dialect != SqlDialect::Postgres {
        return Ok(plan);
    }
    Ok(plan.transform_up_with_subqueries(|node| {
        let names = NamePreserver::new(&node);
        let mut schema = datafusion::common::DFSchema::empty();
        for input in node.inputs() { schema.merge(input.schema()); }
        schema.merge(node.schema());
        node.map_expressions(|expr| {
            let saved = names.save(&expr);
            Ok(expr.transform_up(|expr| {
                let Expr::AggregateFunction(mut agg) = expr else { return Ok(Transformed::no(expr)); };
                if matches!(agg.func.name(), "min" | "max") && agg.params.args.len() == 1 {
                    let ty = agg.params.args[0].get_type(&schema)?;
                    if matches!(ty, DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View) {
                        let native = datafusion::logical_expr::create_udf(
                            "__postgres_binary_text", vec![ty.clone()], ty, Volatility::Immutable,
                            Arc::new(|args| Ok(args[0].clone())),
                        );
                        let function = LogicalFunction::new("__postgres_binary_text", Arc::new(native), BTreeMap::from([
                            ("postgres".into(), SqlFunctionMapping { value: "(__arg0 COLLATE \"C\")".into(), ordering: None })
                        ])).into_udf();
                        agg.params.args[0] = function.call(vec![agg.params.args[0].clone()]);
                        return Ok(Transformed::yes(Expr::AggregateFunction(agg)));
                    }
                }
                Ok(Transformed::no(Expr::AggregateFunction(agg)))
            })?.map_data(|expr| Ok(saved.restore(expr)))?)
        })
    })?.data)
}

/// Preserve each portable numeric result's declared width before its consumer
/// executes in SQL (not just when a final batch is decoded). For example,
/// character_length(Utf8) is Int32 while DuckDB length returns BIGINT.
fn preserve_portable_numeric_types(
    plan: LogicalPlan,
    dialect: SqlDialect,
) -> SqlResult<LogicalPlan> {
    use datafusion::logical_expr::ExprSchemable;
    use datafusion::logical_expr::expr_rewriter::NamePreserver;
    Ok(plan
        .transform_up_with_subqueries(|node| {
            let names = NamePreserver::new(&node);
            let mut schema = datafusion::common::DFSchema::empty();
            for input in node.inputs() {
                schema.merge(input.schema());
            }
            schema.merge(node.schema());
            node.map_expressions(|expr| {
                let saved = names.save(&expr);
                let transformed = expr.transform_up(|expr| {
                    if let Expr::Cast(cast) = &expr {
                        let source = cast.expr.get_type(&schema)?;
                        if matches!(cast.data_type, DataType::List(_) | DataType::LargeList(_)) {
                            return Ok(Transformed::yes(crate::ir::functions::typed_argument_cast_for_engine(cast.expr.as_ref().clone(), cast.data_type.clone(), dialect.name())?));
                        }

                        if matches!(cast.data_type, DataType::Binary | DataType::LargeBinary | DataType::BinaryView)
                            && matches!(source, DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View)
                            && matches!(dialect, SqlDialect::DuckDb | SqlDialect::Postgres) {
                            // Arrow's text-to-binary coercion copies UTF-8 bytes.
                            // Database casts instead interpret backslash escapes.
                            let (name, mut args, mut types) = (if dialect == SqlDialect::Postgres { "convert_to" } else { "encode" }, vec![cast.expr.as_ref().clone()], vec![source]);
                            if dialect == SqlDialect::Postgres { args.push(lit("UTF8")); types.push(DataType::Utf8); }
                            let udf = datafusion::logical_expr::create_udf(
                                &format!("{}{name}", crate::ir::functions::ENGINE_FUNCTION_PREFIX), types, cast.data_type.clone(), datafusion::logical_expr::Volatility::Immutable,
                                Arc::new(|_| Err(DataFusionError::NotImplemented("SQL-only UTF-8 conversion".into()))),
                            );
                            return Ok(Transformed::yes(udf.call(args)));
                        }
                    }
                    let Expr::ScalarFunction(call) = &expr else {
                        return Ok(Transformed::no(expr));
                    };
                    if !crate::ir::functions::logical::definition(&call.func)
                        .is_some_and(|f| f.portable_builtin)
                    {
                        return Ok(Transformed::no(expr));
                    }
                    let ty = expr.get_type(&schema)?;
                    if ty.is_numeric() {
                        Ok(Transformed::yes(
                            crate::ir::functions::typed_argument_cast_for_engine(
                                expr,
                                ty,
                                dialect.name(),
                            )?,
                        ))
                    } else {
                        Ok(Transformed::no(expr))
                    }
                })?;
                Ok(transformed.map_data(|expr| Ok(saved.restore(expr)))?)
            })
        })?
        .data)
}

/// LIMIT creates a SELECT scope in the upstream unparser. Without an explicit
/// projection, a join's left input projection can become that scope's entire
/// output, silently dropping columns supplied by the right input. Only joins
/// need this repair: wrapping DISTINCT would separate its ORDER BY from LIMIT
/// and let duplicate elimination reorder rows before slicing.
fn preserve_limit_output(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Limit(mut limit) = node else { return Ok(Transformed::no(node)); };
        if matches!(limit.input.as_ref(), LogicalPlan::Join(_)) {
            let columns = limit.input.schema().columns().into_iter().map(Expr::Column).collect();
            limit.input = Arc::new(LogicalPlan::Projection(
                datafusion::logical_expr::Projection::try_new(columns, limit.input)?));
        }
        Ok(Transformed::yes(LogicalPlan::Limit(limit)))
    })?.data)
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
        if let Expr::Cast(cast)=&inner
            && dialect==SqlDialect::DuckDb
            && matches!(cast.data_type,DataType::List(_)|DataType::LargeList(_)|DataType::FixedSizeList(..)) {
            return Ok(Transformed::yes(crate::ir::functions::typed_argument_cast_for_engine(*cast.expr.clone(),cast.data_type.clone(),dialect.name())?));
        }
        if let Expr::Literal(value, _) = &inner {
            // DataFusion's generic list unparser loses nested null-list
            // validity (it renders such elements as empty arrays). Use the
            // adapter's recursive, schema-aware literal codec instead.
            if dialect == SqlDialect::DuckDb && matches!(value, ScalarValue::List(_) | ScalarValue::LargeList(_) | ScalarValue::FixedSizeList(_)) && !value.is_null() {
                return Ok(Transformed::yes(super::structured::literal_expr(value, dialect)
                    .map_err(|e| DataFusionError::Plan(e.to_string()))?));
            }
            if dialect==SqlDialect::DuckDb && value.is_null() && matches!(value.data_type(),DataType::List(_)|DataType::LargeList(_)|DataType::FixedSizeList(..)) {
                return Ok(Transformed::yes(crate::ir::functions::typed_argument_cast_for_engine(lit(ScalarValue::Null),value.data_type(),dialect.name())?));
            }
            if dialect == SqlDialect::DuckDb && value.is_null()
                && matches!(value.data_type(), DataType::Struct(_)) {
                return Ok(Transformed::yes(crate::ir::functions::typed_argument_cast_for_engine(
                    lit(ScalarValue::Null), value.data_type(), dialect.name(),
                )?));
            }
            if super::structured::is_structured(&value.data_type()) && matches!(dialect, SqlDialect::DuckDb | SqlDialect::Postgres) {
                return Ok(Transformed::yes(super::structured::literal_expr(value, dialect).map_err(|e| DataFusionError::Plan(e.to_string()))?));
            }
            if crate::ir::functions::domain::descriptor(&value.data_type()).is_some() {
                let storage = crate::ir::functions::domain::storage(value)?;
                // The domain cast supplies DATE already; passing Date32 to
                // the generic unparser would add a second date cast.
                let storage = if crate::ir::temporal::transport::is_date_type(&value.data_type()) && !storage.is_null() {
                    let array = storage.to_array()?;
                    ScalarValue::Utf8(Some(arrow::util::display::array_value_to_string(array.as_ref(), 0)?))
                } else { storage };
                return Ok(Transformed::yes(crate::ir::functions::typed_argument_cast_for_engine(
                    lit(storage), value.data_type(), dialect.name(),
                )?));
            }
            if matches!(value.data_type(), DataType::Timestamp(_, Some(_)))
                && matches!(dialect, SqlDialect::DuckDb | SqlDialect::Postgres) {
                // DF53 renders zoned timestamp literals as plain TIMESTAMP,
                // which silently reinterprets the instant in the session zone.
                let text = if value.is_null() { None } else {
                    Some(arrow::util::display::array_value_to_string(value.to_array()?.as_ref(), 0)?)
                };
                return Ok(Transformed::yes(crate::ir::functions::typed_argument_cast_for_engine(
                    lit(ScalarValue::Utf8(text)), value.data_type(), dialect.name(),
                )?));
            }
            if dialect == SqlDialect::Postgres && value.is_null() && value.data_type() != DataType::Null {
                return Ok(Transformed::yes(crate::ir::functions::typed_argument_cast_for_engine(
                    lit(ScalarValue::Null), value.data_type(), dialect.name(),
                )?));
            }
        }
        // PostgreSQL composite identities are ROW constructors, with scalar
        // coercions on each component (anonymous structs have no cast type).
        if dialect == SqlDialect::Postgres {
            if let Expr::Cast(cast) = &inner {
                if let DataType::Struct(fields) = &cast.data_type {
                    if let Expr::ScalarFunction(call) = cast.expr.as_ref() {
                        if call.func.name() == "named_struct" && call.args.len() == fields.len() * 2 {
                            let args = call.args.chunks_exact(2).zip(fields.iter()).flat_map(|(pair, field)| [pair[0].clone(), Expr::Cast(datafusion::logical_expr::Cast::new(Box::new(pair[1].clone()), field.data_type().clone()))]).collect();
                            return Ok(Transformed::yes(call.func.call(args)));
                        }
                    }
                }
            }
        }
        // DF53 cannot unparse casts to Arrow structs or binary values. Keep
        // their complete target type through the existing SQL cast adapter.
        {
            let needs_adapter = |kind: &DataType| {
                matches!(kind,
                DataType::Struct(_) | DataType::Binary | DataType::LargeBinary
                | DataType::BinaryView | DataType::FixedSizeBinary(_)
                | DataType::Time32(_) | DataType::Time64(_))
            };
            if let Expr::Cast(cast) = &inner {
                if needs_adapter(&cast.data_type) {
                    return Ok(Transformed::yes(crate::ir::functions::typed_argument_cast_for_engine(
                        *cast.expr.clone(), cast.data_type.clone(),
                            dialect.name(),
                    )?));
                }
            }
            if let Expr::Literal(value, _) = &inner {
                if value.is_null() && needs_adapter(&value.data_type()) {
                    return Ok(Transformed::yes(crate::ir::functions::typed_argument_cast_for_engine(
                        lit(ScalarValue::Null), value.data_type(),
                            dialect.name(),
                    )?));
                }
            }
        }
        if let Expr::BinaryExpr(binary) = &inner {
            use datafusion::logical_expr::{Operator, ScalarUDF, Signature, Volatility, expr_fn::SimpleScalarUDF};
            let name = match binary.op {
                Operator::IsNotDistinctFrom => Some("__orchiddb_is_not_distinct_from"),
                Operator::IsDistinctFrom => Some("__orchiddb_is_distinct_from"),
                _ => None,
            };
            if let Some(name) = name {
                // SQL-only placeholder bypasses DF53's missing operator
                // unparser. The AST adapter restores the exact operator,
                // evaluating each operand once, including volatile operands.
                let function = ScalarUDF::from(SimpleScalarUDF::new_with_signature(name,
                    Signature::any(2, Volatility::Immutable), DataType::Boolean,
                    Arc::new(|_| Err(DataFusionError::Internal("SQL-only comparison placeholder".into())))));
                return Ok(Transformed::yes(function.call(vec![(*binary.left).clone(), (*binary.right).clone()])));
            }
        }
        match &inner {
            Expr::Literal(value, _) if dialect == SqlDialect::Postgres && matches!(value.data_type(), DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) if *f.data_type() != DataType::Null) =>
            {
                // Preserve both empty-array type and null-array validity.
                let expression = if value.is_null() { lit(ScalarValue::Null) } else { inner.clone() };
                return Ok(Transformed::yes(crate::ir::functions::typed_argument_cast_for_engine(
                    expression, value.data_type(), dialect.name(),
                )?));
            }
            Expr::Literal(value, _) if value.is_null() && matches!(value.data_type(),
                DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)) => {
                return Ok(Transformed::yes(Expr::Cast(datafusion::logical_expr::Cast::new(
                    Box::new(lit(ScalarValue::Null)), value.data_type(),
                ))));
            }
            Expr::Literal(ScalarValue::Float64(Some(value)), _) if !value.is_finite() || (*value == 0.0 && value.is_sign_negative()) => {
                return Ok(Transformed::yes(non_finite_float_expr(
                    *value,
                    DataType::Float64,
                )));
            }
            Expr::Literal(ScalarValue::Float32(Some(value)), _) if !value.is_finite() || (*value == 0.0 && value.is_sign_negative()) => {
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
    let spelling = if value == 0.0 { "-0" } else if value.is_nan() {
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

/// Reattach aggregate ordering to syntax nodes, including copies introduced by
/// the unparser when it expands COALESCE or substitutes a projected alias.
/// Equal call syntax is safe to rewrite only when every matching logical call
/// has the same ordering contract (including unordered calls).
pub(super) fn restore_aggregate_ordering(
    plan: &LogicalPlan,
    unparser: &Unparser<'_>,
    dialect: SqlDialect,
    mut statement: datafusion::sql::sqlparser::ast::Statement,
) -> SqlResult<String> {
    use datafusion::sql::sqlparser::ast;
    use std::ops::ControlFlow;
    let mut aggregates = Vec::new();
    plan.apply_with_subqueries(|node| {
        node.apply_expressions(|expr| {
            expr.apply(|inner| {
                if let Expr::AggregateFunction(agg) = inner
                    && !agg.func.supports_within_group_clause()
                {
                    aggregates.push(agg.clone());
                }
                Ok(TreeNodeRecursion::Continue)
            })
        })?;
        Ok(TreeNodeRecursion::Continue)
    })?;
    // Keep AST equality rather than substring matches: call-looking text in a
    // string literal or quoted identifier is data and must never be rewritten.
    let mut repairs: Vec<(ast::Expr, ast::Expr, bool, usize)> = Vec::new();
    for agg in aggregates {
        let mut plain = agg.clone();
        plain.params.order_by.clear();
        let mut call = unparser.expr_to_sql(&Expr::AggregateFunction(plain))?;
        functions::prepare_ast(&mut call, dialect)?;
        let mut ordered = call.clone();
        if !agg.params.order_by.is_empty() {
            let mut keys = Vec::new();
            for sort in &agg.params.order_by {
                let mut expr = unparser.expr_to_sql(&sort.expr)?;
                functions::prepare_ast(&mut expr, dialect)?;
                keys.push(ast::OrderByExpr {
                    expr,
                    options: ast::OrderByOptions {
                        asc: Some(sort.asc), nulls_first: Some(sort.nulls_first),
                    },
                    with_fill: None,
                });
            }
            let ast::Expr::Function(function) = &mut ordered else {
                return Err(SqlError::Unsupported(format!("unrecognized ordered aggregate `{call}`")));
            };
            let ast::FunctionArguments::List(args) = &mut function.args else {
                return Err(SqlError::Unsupported(format!("unrecognized aggregate arguments `{call}`")));
            };
            args.clauses.push(ast::FunctionArgumentClause::OrderBy(keys));
        }
        if let Some((_, existing, _, _)) = repairs.iter().find(|(plain, _, _, _)| plain == &call) {
            if existing != &ordered {
                return Err(SqlError::Unsupported(format!(
                    "cannot place different ORDER BY contracts on indistinguishable aggregate `{call}`"
                )));
            }
        } else {
            repairs.push((call, ordered, !agg.params.order_by.is_empty(), 0));
        }
    }
    let _: ControlFlow<()> = ast::visit_expressions_mut(&mut statement, |expr| {
        if matches!(expr, ast::Expr::Function(_)) {
            if let Some((_, replacement, _, seen)) = repairs.iter_mut().find(|(plain, _, _, _)| plain == expr) {
                *expr = replacement.clone();
                *seen += 1;
            }
        }
        ControlFlow::Continue(())
    });
    for (call, _, ordered, seen) in repairs {
        if ordered && seen == 0 {
            return Err(SqlError::Unsupported(format!("cannot locate ordered aggregate `{call}` in emitted SQL")));
        }
    }
    Ok(statement.to_string())
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
        // Aggregate/projection expression names are referenced by parent nodes.
        // Removing a qualifier inside sum(t.x) must not rename its output.
        let preserve_names = matches!(
            node,
            LogicalPlan::Aggregate(_) | LogicalPlan::Projection(_) | LogicalPlan::Window(_)
        );
        let rewritten = node.map_expressions(|expr| {
            let name = expr.qualified_name().1;
            let rewritten = expr.transform_up(|inner| {
                if let Expr::Column(column) = &inner
                    && column.relation.is_some()
                    && unique(&column.name)
                {
                    return Ok(Transformed::yes(Expr::Column(Column::new_unqualified(
                        column.name.clone(),
                    ))));
                }
                Ok(Transformed::no(inner))
            })?;
            rewritten.map_data(|expr| {
                Ok(if preserve_names && expr.qualified_name().1 != name {
                    expr.alias(name)
                } else {
                    expr
                })
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
        |(expr, (qualifier, field))| {
            // A same-name alias still only forwards the column. The output
            // schema check below rules out renaming or qualifier changes.
            let mut expr = expr;
            while let Expr::Alias(alias) = expr { expr = alias.expr.as_ref(); }
            match expr {
            Expr::Column(column) => {
                column.name == *field.name()
                    && (column.relation.is_none() || column.relation.as_ref() == qualifier)
            }
            _ => false,
        }},
    );
    // Removing the projection must not change the observable schema
    // (names and qualifiers) the parent plan sees.
    columns_only
        && projection.schema.iter().zip(input_schema.iter()).all(
            |((out_qualifier, out_field), (in_qualifier, in_field))| {
                out_qualifier == in_qualifier && out_field == in_field
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

/// The upstream unparser overwrites an enclosing LIMIT with Sort.fetch,
/// which includes the outer OFFSET. Keep TopK as a separate limit scope.
fn sort_scope(input: LogicalPlan, alias: String) -> datafusion::common::Result<LogicalPlan> {
    // A SQL scope changes physical qualifiers, not the output contract seen
    // by the existing Sort and its parents. Rebind by ordinal, never by a
    // potentially ambiguous unqualified name.
    let schema = input.schema().clone();
    let scoped = datafusion::logical_expr::LogicalPlanBuilder::from(input).alias(alias)?.build()?;
    // Unqualified consumers already resolve through the new scope by name.
    // Only existing qualified bindings need an explicit schema-restoring layer.
    if schema.iter().all(|(qualifier, _)| qualifier.is_none()) {
        return Ok(scoped);
    }
    let exprs = scoped.schema().columns().into_iter().zip(schema.iter())
        .map(|(column, (qualifier, field))| Expr::Column(column)
            .alias_qualified(qualifier.cloned(), field.name())).collect();
    Ok(LogicalPlan::Projection(datafusion::logical_expr::Projection::try_new_with_schema(
        exprs, Arc::new(scoped), schema,
    )?))
}

fn expand_sort_fetch(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    let mut boundary = 0;
    Ok(plan.transform_up_with_subqueries(|node| {
        let LogicalPlan::Sort(mut sort) = node else { return Ok(Transformed::no(node)); };
        // SQL ORDER BY expressions can resolve a shadowed name against the
        // projection input instead of its output. Materialize that scope.
        if matches!(sort.input.as_ref(), LogicalPlan::Projection(_)) {
            sort.input = Arc::new(sort_scope(sort.input.as_ref().clone(),
                format!("__w_sql_cte_sort_input_{boundary}"))?);
            boundary += 1;
        }
        let Some(fetch) = sort.fetch.take() else { return Ok(Transformed::yes(LogicalPlan::Sort(sort))); };
        let columns = sort.input.schema().columns().into_iter().map(Expr::Column).collect::<Vec<_>>();
        let keys = sort.expr.clone();
        let limited = datafusion::logical_expr::LogicalPlanBuilder::from(LogicalPlan::Sort(sort))
            .limit(0, Some(fetch))?.project(columns)?.build()?;
        let plan = datafusion::logical_expr::LogicalPlanBuilder::from(sort_scope(limited,
            format!("__w_sql_cte_topk_{boundary}"))?).sort(keys)?.build()?;
        boundary += 1;
        Ok(Transformed::yes(plan))
    })?.data)
}

// Ontology relationship mappings contain joins whose source namespaces must
// survive until the SQL AST exposes the final derived-table scopes.
pub(super) fn has_rdf_source(plan: &LogicalPlan) -> bool {
    let mut found = false;
    let _ = plan.apply_with_subqueries(|node| {
        if let LogicalPlan::SubqueryAlias(alias) = node {
            found |= alias.alias.table().starts_with("__w_sql_cte_rdf_source_");
        }
        Ok(TreeNodeRecursion::Continue)
    });
    found
}

fn validate_portable_calls(plan: &LogicalPlan, dialect: SqlDialect) -> SqlResult<()> {
    plan.apply_with_subqueries(|node| {
        let mut schema = datafusion::common::DFSchema::empty();
        for input in node.inputs() { schema.merge(input.schema()); }
        schema.merge(node.schema());
        node.apply_expressions(|expr| expr.apply(|expr| {
            if let Expr::Literal(value, _) = expr {
                if let Some(reason) = crate::ir::functions::portable::literal_issue(value, dialect.name()) {
                    return Err(DataFusionError::Plan(reason.into()));
                }
            }
            if let Expr::ScalarFunction(call) = expr {
                if let Some(f) = crate::ir::functions::logical::definition(&call.func).filter(|f| f.portable_builtin) {
                    if matches!(dialect, SqlDialect::Custom(adapter) if adapter.function_mapping(f.logical_name()).is_some()) {
                        return Ok(TreeNodeRecursion::Continue);
                    }
                    if !f.has_sql_mapping(dialect.name(), call.args.len()) {
                        return Err(DataFusionError::Plan(format!("{} has no {} mapping for {} arguments: {}", f.logical_name(), dialect.name(), call.args.len(), crate::ir::functions::portable::audit_note(f.logical_name(), dialect.name()))));
                    }
                    crate::ir::functions::portable::validate_call(f.logical_name(), &call.args, &schema, dialect.name())
                        .map_err(|reason| DataFusionError::Plan(format!("{} has no {} mapping for this call: {reason}", f.logical_name(), dialect.name())))?;
                }
            }
            Ok(TreeNodeRecursion::Continue)
        }))?;
        Ok(TreeNodeRecursion::Continue)
    })?;
    Ok(())
}

fn specialize_portable_calls(plan: LogicalPlan, dialect: SqlDialect) -> SqlResult<LogicalPlan> {
    use datafusion::logical_expr::expr_rewriter::NamePreserver;
    Ok(plan.transform_up_with_subqueries(|node| {
        let names = NamePreserver::new(&node);
        let mut schema = datafusion::common::DFSchema::empty();
        for input in node.inputs() { schema.merge(input.schema()); }
        schema.merge(node.schema());
        node.map_expressions(|expr| {
            let saved = names.save(&expr);
            Ok(expr.transform_up(|expr| {
                if let Expr::ScalarFunction(call) = &expr {
                    // Relational lowering creates native struct calls for identities.
                    // Route these through the same JSONB adapter as user-facing
                    // portable calls before the unparser turns fields into identifiers.
                    if dialect == SqlDialect::Postgres
                        && matches!(call.func.name(), "get_field" | "named_struct" | "struct")
                        && crate::ir::functions::logical::definition(&call.func).is_none()
                    {
                        let portable = crate::ir::functions::portable::function(&format!("fn.{}", call.func.name()))
                            .expect("native structured function has a portable adapter");
                        let f = crate::ir::functions::logical::definition(&portable).unwrap();
                        let value = crate::ir::functions::portable::structured::mapping(f.logical_name(), &call.args, &schema, true)?;
                        return Ok(Transformed::yes(f.specialized(dialect.name(), call.args.len(), value).into_udf().call(call.args.clone())));
                    }
                    if let Some(f) = crate::ir::functions::logical::definition(&call.func).filter(|f| f.portable_builtin) {
                        if crate::ir::functions::portable::structured::handles(f.logical_name()) && !matches!(dialect, SqlDialect::Custom(_)) {
                            let value = crate::ir::functions::portable::structured::mapping(f.logical_name(), &call.args, &schema, dialect == SqlDialect::Postgres)?;
                            return Ok(Transformed::yes(f.specialized(dialect.name(), call.args.len(), value).into_udf().call(call.args.clone())));
                        }
                    }
                }
                Ok(Transformed::no(expr))
            })?.map_data(|expr| Ok(saved.restore(expr)))?)
        })
    })?.data)
}

#[cfg(test)]
mod ordering_repair_tests {
    use super::*;
    use datafusion::logical_expr::{ExprFunctionExt, LogicalPlanBuilder};
    use datafusion::prelude::col;

    #[test]
    fn representation_casts_keep_declared_projection_names() {
        use datafusion::logical_expr::{Projection, expr::Cast};
        let input = LogicalPlanBuilder::empty(true).project(vec![lit("IRI").alias("kind")]).unwrap().build().unwrap();
        let schema = input.schema().clone();
        let expression = Expr::Cast(Cast::new(Box::new(col("kind")), DataType::Utf8));
        let plan = LogicalPlan::Projection(Projection::try_new_with_schema(vec![expression], Arc::new(input), schema).unwrap());
        let fixed = normalize_projection_names(plan).unwrap();
        let LogicalPlan::Projection(projection) = fixed else { panic!() };
        assert_eq!(projection.expr[0].to_string(), "CAST(kind AS Utf8) AS kind");
        assert_eq!(projection.schema.field(0).name(), "kind");
    }

    use datafusion::functions_aggregate::expr_fn::array_agg;
    use datafusion::sql::sqlparser::{dialect::GenericDialect, parser::Parser};

    fn plan(aggregates: Vec<Expr>) -> LogicalPlan {
        LogicalPlanBuilder::empty(true)
            .project(vec![lit(1_i64).alias("x"),lit(2_i64).alias("k")]).unwrap()
            .aggregate(Vec::<Expr>::new(), aggregates).unwrap().build().unwrap()
    }
    fn repair(plan: &LogicalPlan, sql: &str) -> SqlResult<String> {
        let dialect = SqlDialect::DuckDb.unparser_dialect();
        let unparser = Unparser::new(dialect.as_ref());
        let statement = Parser::parse_sql(&GenericDialect, sql).unwrap().remove(0);
        restore_aggregate_ordering(plan, &unparser, SqlDialect::DuckDb, statement)
    }
    fn ordered(ascending: bool) -> Expr {
        array_agg(col("x")).order_by(vec![col("k").sort(ascending,false)]).build().unwrap()
    }
    #[test]
    fn repeated_ordered_calls_are_repaired_without_changing_literal_data() {
        let plan = plan(vec![ordered(true).alias("xs")]);
        let sql = repair(&plan, "SELECT CASE WHEN array_agg(\"x\") IS NULL THEN array_agg(\"x\") ELSE NULL END, 'array_agg(\"x\")'").unwrap();
        assert_eq!(sql.matches("array_agg(\"x\" ORDER BY \"k\" ASC NULLS LAST)").count(),2);
        assert!(sql.contains("'array_agg(\"x\")'"));
    }
    #[test]
    fn different_ordering_contracts_cannot_be_guessed_from_equal_call_syntax() {
        for second in [array_agg(col("x")), ordered(false)] {
            let plan = plan(vec![ordered(true).alias("a"),second.alias("b")]);
            assert!(repair(&plan,"SELECT array_agg(\"x\"), array_agg(\"x\")").is_err());
        }
    }
    #[test]
    fn conditionless_outer_joins_emit_an_explicit_true_predicate() {
        use datafusion::logical_expr::JoinType;
        for kind in [JoinType::Left, JoinType::Right, JoinType::Full] {
            let scan = |name: &str, column: &str| {
                let table = datafusion::datasource::empty::EmptyTable::new(Arc::new(
                    arrow::datatypes::Schema::new(vec![Field::new(column, DataType::Int64, true)])));
                LogicalPlanBuilder::scan(name, datafusion::datasource::provider_as_source(Arc::new(table)), None)
                    .unwrap().build().unwrap()
            };
            let left = scan("left_input", "left_value");
            let right = scan("right_input", "right_value");
            let plan = LogicalPlanBuilder::from(left)
                .join(right, kind, (Vec::<Column>::new(), Vec::<Column>::new()), Some(lit(true)))
                .unwrap().build().unwrap();
            // Reproduce the optimizer's conditionless join; the builder itself
            // rejects this shape when supplied directly by callers.
            let LogicalPlan::Join(mut join) = plan else { unreachable!() };
            join.filter = None;
            let sql = unparse_plan(LogicalPlan::Join(join), SqlDialect::DuckDb).unwrap();
            assert!(sql.contains("ON true"), "{sql}");
            Parser::parse_sql(&GenericDialect, &sql).unwrap();
        }
    }

}
