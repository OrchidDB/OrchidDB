//! Aggregates.

use super::*;

impl<'a> LoweringContext<'a> {
    pub(super) fn lower_required_agg_arg(&self, plan: &LogicalPlan, arg: &Option<IrExpr>) -> RelResult<Expr> {
        let Some(arg) = arg else {
            return Err(RelError::Unsupported(
                "aggregate requires an argument".to_string(),
            ));
        };
        self.lower_expr(plan, arg)
    }

    pub(super) fn lower_count_if(
        &self,
        plan: &LogicalPlan,
        original: &IrExpr,
        value: Expr,
        distinct: bool,
    ) -> RelResult<Expr> {
        let data_type = value
            .get_type(plan.schema())
            .map_err(|err| RelError::Unsupported(format!("count_if argument type: {err}")))?;
        let truthy = match data_type {
            DataType::Boolean => value.clone(),
            DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Decimal128(_, _)
            | DataType::Decimal256(_, _) => binary(value.clone(), BinaryOp::Neq, lit(0_i64)),
            DataType::Float16 | DataType::Float32 | DataType::Float64 => Expr::and(
                binary(value.clone(), BinaryOp::Neq, lit(0.0_f64)),
                Expr::Not(Box::new(df_math::isnan(value.clone()))),
            ),
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
                if expression_has_wide_numeric_cast(original) =>
            {
                let numeric = Expr::TryCast(TryCast::new(
                    Box::new(value.clone()),
                    DataType::Decimal128(38, 0),
                ));
                binary(numeric, BinaryOp::Neq, lit(0_i64))
            }
            _ => lit(false),
        };
        let count = df_count(value).filter(truthy);
        if distinct {
            Ok(count.distinct().build()?)
        } else {
            Ok(count.build()?)
        }
    }

    pub(super) fn is_blob_property_expr(&self, expr: &IrExpr) -> bool {
        let IrExpr::Property { name, .. } = expr else {
            return false;
        };
        let is_blob = |schema: &Schema| {
            schema
                .fields()
                .iter()
                .find(|field| field.name().eq_ignore_ascii_case(name))
                .and_then(|field| field.metadata().get("new_graph.value_type"))
                .is_some_and(|kind| kind == "blob")
        };
        self.graph.labels().into_iter().any(|label| {
            self.graph
                .node_table(&label)
                .is_ok_and(|table| is_blob(table.batch.schema().as_ref()))
        }) || self.graph.rel_types().into_iter().any(|rel_type| {
            self.graph.edge_tables(&rel_type).is_ok_and(|tables| {
                tables
                    .iter()
                    .any(|table| is_blob(table.batch.schema().as_ref()))
            })
        })
    }

    /// Lower `collect(DISTINCT x)` as two aggregates. The first keeps the
    /// earliest scan position for each `(group, x)` pair; the second orders
    /// those unique values by that position. A plain SQL
    /// `array_agg(DISTINCT x)` is allowed to return hash order and therefore
    /// does not implement Cypher's first-appearance rule.
    pub(super) fn lower_first_distinct_collect(
        &self,
        input: LoweredNode,
        group: &[ProjectionItem],
        agg: &AggCall,
    ) -> RelResult<LoweredNode> {
        let value_name = "__w_collect_value";
        let value = self.lower_required_agg_arg(&input.plan, &agg.arg)?;

        let mut group_projection = apply_correlation_key_columns(&input.plan)
            .iter()
            .map(|key| (key.clone(), col_exact(key)))
            .collect::<Vec<_>>();
        group_projection.extend(
            group
                .iter()
                .map(|item| {
                    Ok((
                        item.alias.clone(),
                        self.lower_expr(&input.plan, &item.expr)?,
                    ))
                })
                .collect::<RelResult<Vec<_>>>()?,
        );

        let order = scan_order_keys(&input.plan);
        if order.is_empty() {
            return Err(RelError::Unsupported(
                "collect(DISTINCT) without a stable scan-order key".into(),
            ));
        }
        let mut projection = group_projection
            .iter()
            .map(|(name, expr)| expr.clone().alias(name))
            .collect::<Vec<_>>();
        projection.push(value.alias(value_name));
        for (index, sort) in order.iter().enumerate() {
            projection.push(
                sort.expr
                    .clone()
                    .alias(format!("__w_collect_order_{index}")),
            );
        }
        let projected = LogicalPlanBuilder::from(input.plan.clone())
            .project(projection)?
            .filter(col_exact(value_name).is_not_null())?
            .build()?;

        let mut unique_groups = group_projection
            .iter()
            .map(|(name, _)| col_exact(name))
            .collect::<Vec<_>>();
        unique_groups.push(col_exact(value_name));
        let earliest = (0..order.len())
            .map(|index| {
                df_min(col_exact(format!("__w_collect_order_{index}")))
                    .alias(format!("__w_collect_first_{index}"))
            })
            .collect::<Vec<_>>();
        let unique = LogicalPlanBuilder::from(projected)
            .aggregate(unique_groups, earliest)?
            .alias("__w_collect_unique")?
            .build()?;

        let final_groups = group_projection
            .iter()
            .map(|(name, _)| col_exact(name))
            .collect::<Vec<_>>();
        let order = (0..order.len())
            .map(|index| col_exact(format!("__w_collect_first_{index}")).sort(true, false))
            .collect::<Vec<_>>();
        let collected = df_array_agg(col_exact(value_name))
            .order_by(order)
            .build()?
            .alias(agg.alias.clone());
        let plan = LogicalPlanBuilder::from(unique)
            .aggregate(final_groups, vec![collected])?
            .build()?;
        Ok(input.with_plan(plan))
    }

}

/// Element-id columns of `plan`, in schema order, as ascending sort keys.
///
/// These reconstruct the row order direct evaluation would have seen, which
/// is what an unordered SQL aggregate otherwise loses.
pub(super) fn scan_order_keys(plan: &LogicalPlan) -> Vec<datafusion::logical_expr::SortExpr> {
    plan.schema()
        .fields()
        .iter()
        .filter(|field| field.name().ends_with(ID_SUFFIX) && !field.name().contains(PROP_MARKER))
        .map(|field| col_exact(field.name()).sort(true, false))
        .collect()
}

pub(super) fn count_input_rows(plan: &LogicalPlan) -> Expr {
    let Some(field) = plan.schema().fields().first() else {
        return count_all();
    };
    // Referencing an input column is intentional. DataFusion's SQL unparser
    // otherwise emits `SELECT count(1)` without a FROM clause for some
    // cross-join plans. Coalescing preserves COUNT(*) semantics for nulls.
    // Keep native values where possible: converting millions of joined IDs
    // to strings just to count rows dominates otherwise cheap hash joins.
    if !field.is_nullable() {
        return df_count(col_exact(field.name()));
    }
    let zero = match field.data_type() {
        DataType::Int64 => Some(lit(0_i64)),
        DataType::UInt64 => Some(lit(0_u64)),
        DataType::Boolean => Some(lit(false)),
        _ => None,
    };
    if let Some(zero) = zero {
        return df_count(df_core::coalesce(vec![col_exact(field.name()), zero]));
    }
    df_count(df_core::coalesce(vec![
        cast_utf8(col_exact(field.name())),
        lit(""),
    ]))
}

/// Preserve the original blob display text while ordering its `\\xNN`
/// escapes as non-ASCII bytes rather than as a leading backslash.
pub(super) fn blob_extreme(value: Expr, maximum: bool) -> Expr {
    let key = df_string::replace(value.clone(), lit("\\x"), lit("\u{00ff}"));
    let packed = concat_exprs(vec![key, lit("\u{1}"), value]);
    let extreme = if maximum {
        df_max(packed)
    } else {
        df_min(packed)
    };
    df_string::split_part(extreme, lit("\u{1}"), lit(2_i64))
}

/// Apply `DISTINCT` to an aggregate call when the query asked for it.
pub(super) fn distinct_if(expr: Expr, distinct: bool) -> RelResult<Expr> {
    if distinct {
        Ok(expr.distinct().build()?)
    } else {
        Ok(expr)
    }
}
