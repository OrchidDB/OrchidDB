//! Collection operators.

use super::*;

impl<'a> LoweringContext<'a> {
    pub(super) fn lower_unwind(
        &mut self,
        input_expr: &IrExpr,
        bind: &str,
        outer: bool,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self
            .lower_node(input)
            .map_err(|err| RelError::Unsupported(format!("GraphUnwind input: {err}")))?;
        let Some(values) = constant_unwind_values(input_expr, outer)? else {
            return self.lower_unwind_dynamic(input, input_expr, bind, outer);
        };
        let value_rows = values
            .into_iter()
            .map(|value| vec![value])
            .collect::<Vec<_>>();
        let values_plan = self
            .lower_values(&[bind.to_string()], &value_rows)
            .map_err(|err| RelError::Unsupported(format!("GraphUnwind values: {err}")))?;
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .cross_join(values_plan.plan.clone())
            .map_err(|err| RelError::Unsupported(format!("GraphUnwind cross join: {err}")))?
            .build()
            .map_err(|err| RelError::Unsupported(format!("GraphUnwind build: {err}")))?;
        let mut islands = input.islands;
        islands.merge(values_plan.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

    /// Gremlin `groupCount()` — a single-row map rendered in the tagged
    /// `m[{"key":"d[count].l"}]` form. Entry order is irrelevant: the
    /// harness comparator sorts map entries on both sides.
    pub(super) fn lower_group_map(
        &mut self,
        key: &IrExpr,
        value: &crate::ir::plan::GroupValue,
        output: &str,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        use crate::ir::plan::GroupValue;
        let input = self.lower_node(input)?;
        let key_expr = self.lower_expr(&input.plan, key)?;
        let key_type = key_expr
            .get_type(input.plan.schema())
            .map_err(|err| RelError::Unsupported(format!("group key type: {err}")))?;
        let key_text = gremlin_tagged_text_expr(key_expr, &key_type);
        let value_alias = "__gm_value";
        let mut collected_value = false;
        let value_agg = match value {
            GroupValue::CountBulk => count_all(),
            GroupValue::Aggregate(agg) => match agg.kind {
                AggKind::CountRows | AggKind::CountBulk => match &agg.arg {
                    Some(arg) => df_count(self.lower_expr(&input.plan, arg)?),
                    None => count_all(),
                },
                AggKind::CountDistinct => {
                    let Some(arg) = &agg.arg else {
                        return Err(RelError::Unsupported(
                            "group count distinct without argument".into(),
                        ));
                    };
                    datafusion::functions_aggregate::count::count_distinct(
                        self.lower_expr(&input.plan, arg)?,
                    )
                }
                AggKind::CountIf => {
                    let original = agg.arg.as_ref().ok_or_else(|| {
                        RelError::Unsupported("count_if requires an argument".into())
                    })?;
                    let arg = self.lower_expr(&input.plan, original)?;
                    self.lower_count_if(&input.plan, original, arg, agg.distinct)?
                }
                AggKind::Sum | AggKind::SumOrZero => df_core::coalesce(vec![
                    distinct_if(
                        df_sum(self.lower_required_agg_arg(&input.plan, &agg.arg)?),
                        agg.distinct,
                    )?,
                    lit(0_i64),
                ]),
                AggKind::AvgOrZero => df_core::coalesce(vec![
                    distinct_if(
                        df_avg(self.lower_required_agg_arg(&input.plan, &agg.arg)?),
                        agg.distinct,
                    )?,
                    lit(0.0_f64),
                ]),
                AggKind::Avg | AggKind::AvgOrNull => distinct_if(
                    df_avg(self.lower_required_agg_arg(&input.plan, &agg.arg)?),
                    agg.distinct,
                )?,
                AggKind::Min | AggKind::MinOrNull => {
                    df_min(self.lower_required_agg_arg(&input.plan, &agg.arg)?)
                }
                AggKind::Max | AggKind::MaxOrNull => {
                    df_max(self.lower_required_agg_arg(&input.plan, &agg.arg)?)
                }
                AggKind::CollectRows | AggKind::CollectTraversers => {
                    let arg = self.lower_required_agg_arg(&input.plan, &agg.arg)?;
                    if agg.alias.contains("unwrap") {
                        df_min(arg)
                    } else {
                        let arg_type = arg.get_type(input.plan.schema()).map_err(|err| {
                            RelError::Unsupported(format!("group value type: {err}"))
                        })?;
                        let rendered = gremlin_tagged_text_expr(arg.clone(), &arg_type);
                        collected_value = true;
                        let collect = df_array_agg(rendered).filter(arg.is_not_null());
                        if agg.distinct {
                            distinct_if(collect.build()?, true)?
                        } else {
                            collect.build()?
                        }
                    }
                }
                other => {
                    return Err(RelError::Unsupported(format!(
                        "GraphGroupMap aggregate `{other:?}`"
                    )));
                }
            },
        };
        let grouped = LogicalPlanBuilder::from(input.plan.clone())
            .aggregate(
                vec![key_text.alias("__gm_key")],
                vec![value_agg.alias(value_alias)],
            )?
            .build()?;
        let value_text = if collected_value {
            concat_exprs(vec![
                lit("l["),
                df_core::coalesce(vec![
                    datafusion::functions_nested::expr_fn::array_to_string(
                        col_exact(value_alias),
                        lit(","),
                    ),
                    lit(""),
                ]),
                lit("]"),
            ])
        } else {
            let value_type = plan_column_type(&grouped, value_alias)
                .ok_or_else(|| RelError::Unsupported("group value type is unavailable".into()))?;
            gremlin_tagged_text_expr(col_exact(value_alias), &value_type)
        };
        let entry = concat_exprs(vec![
            lit("\""),
            df_core::coalesce(vec![col_exact("__gm_key"), lit("null")]),
            lit("\":\""),
            value_text,
            lit("\""),
        ]);
        let entries = LogicalPlanBuilder::from(grouped)
            .project(vec![entry.alias("__gm_entry")])?
            .aggregate(
                Vec::<Expr>::new(),
                vec![df_array_agg(col_exact("__gm_entry")).alias("__gm_entries")],
            )?
            .build()?;
        let rendered = concat_exprs(vec![
            lit("m[{"),
            df_core::coalesce(vec![
                datafusion::functions_nested::expr_fn::array_to_string(
                    col_exact("__gm_entries"),
                    lit(","),
                ),
                lit(""),
            ]),
            lit("}]"),
        ]);
        let plan = LogicalPlanBuilder::from(entries)
            .project(vec![rendered.alias(output)])?
            .build()?;
        Ok(input.with_plan(plan))
    }

    /// Collect the rows produced for one correlated input into a list.
    /// Unlike aggregate `COLLECT`, this projection retains null elements and
    /// returns an empty list for an input that produced no rows.
    pub(super) fn lower_collect(
        &mut self,
        value: &IrExpr,
        distinct: bool,
        order: &[SortKey],
        alias: &str,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        if distinct {
            return Err(RelError::Unsupported(
                "GraphCollect DISTINCT has no stable first-occurrence ordering".into(),
            ));
        }

        let input = self.lower_node(input)?;
        let value_expr = self.lower_expr(&input.plan, value)?;
        let value_type = value_expr
            .get_type(input.plan.schema())
            .map_err(|err| RelError::Unsupported(format!("GraphCollect value type: {err}")))?;
        let correlation_keys = apply_correlation_key_columns(&input.plan);
        let order_exprs = if order.is_empty() {
            // Keep backend scan order when available. GraphUnwind currently
            // carries no element ordinal, so ties cannot portably order list
            // elements; that limitation is reported at the feature boundary.
            scan_order_keys(&input.plan)
        } else {
            order
                .iter()
                .map(|key| self.sort_exprs(&input.plan, key))
                .collect::<RelResult<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect()
        };
        let list_expr = if order_exprs.is_empty() {
            df_array_agg(value_expr)
        } else {
            df_array_agg(value_expr).order_by(order_exprs).build()?
        };
        let empty_list = Expr::Cast(Cast::new(
            Box::new(datafusion::functions_nested::expr_fn::make_array(Vec::new())),
            DataType::List(Arc::new(Field::new("item", value_type, true))),
        ));

        if correlation_keys.is_empty() {
            let plan = LogicalPlanBuilder::from(input.plan.clone())
                .aggregate(
                    Vec::<Expr>::new(),
                    vec![df_core::coalesce(vec![list_expr, empty_list]).alias(alias)],
                )?
                .build()?;
            return Ok(input.with_plan(plan));
        }

        let right_key_aliases = correlation_keys
            .iter()
            .enumerate()
            .map(|(index, _)| format!("__w_collect_key_{index}"))
            .collect::<Vec<_>>();
        let group_exprs = correlation_keys
            .iter()
            .zip(&right_key_aliases)
            .map(|(key, alias)| col_exact(key).alias(alias))
            .collect::<Vec<_>>();
        let grouped = LogicalPlanBuilder::from(input.plan.clone())
            .aggregate(group_exprs, vec![list_expr.alias("__w_collect_values")])?
            .build()?;

        // Reintroduce correlated input rows whose collection became empty
        // after UNWIND/filter. Using distinct correlation identities prevents
        // the inner result from multiplying duplicate outer rows.
        let correlated = self.correlate_plan.as_ref().ok_or_else(|| {
            RelError::Unsupported(
                "GraphCollect with correlation keys has no correlated outer input".into(),
            )
        })?;
        if !correlation_keys
            .iter()
            .all(|key| has_exact_col(correlated, key))
        {
            return Err(RelError::Unsupported(
                "GraphCollect correlation keys are unavailable from the outer input".into(),
            ));
        }
        let base = LogicalPlanBuilder::from(correlated.clone())
            .project(correlation_keys.iter().map(col_exact).collect::<Vec<_>>())?
            .build()?;
        let barrier_id = self.scan_counter;
        self.scan_counter += 1;
        let base = keyed_distinct(base, &correlation_keys, barrier_id)?;
        let join_conditions = correlation_keys
            .iter()
            .zip(&right_key_aliases)
            .map(|(left, right)| col_exact(left).eq(col_exact(right)))
            .collect::<Vec<_>>();
        let joined = LogicalPlanBuilder::from(base)
            .join_on(grouped, JoinType::Left, join_conditions)?
            .build()?;
        let mut output = correlation_keys.iter().map(col_exact).collect::<Vec<_>>();
        output.push(
            df_core::coalesce(vec![col_exact("__w_collect_values"), empty_list]).alias(alias),
        );
        let plan = LogicalPlanBuilder::from(joined).project(output)?.build()?;
        Ok(input.with_plan(plan))
    }

    /// Lower three-valued ALL/ANY/NONE/SINGLE semantics by assigning each
    /// input row an identity, unnesting its list, and reducing predicate
    /// outcomes back to one boolean per original row. The interpreter keeps
    /// the same truth table and remains the semantic oracle for these cases.
    pub(super) fn lower_quantifier(
        &mut self,
        kind: QuantifierKind,
        item_binding: &str,
        input_expr: &IrExpr,
        predicate: &IrExpr,
        output: &str,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let original_columns = output_fields(&input.plan);
        let suffix = self.scan_counter;
        self.scan_counter += 1;
        let row_id = format!("__w_quantifier_row_{suffix}");
        let list_col = format!("__w_quantifier_list_{suffix}");
        let input_null = format!("__w_quantifier_null_{suffix}");
        let total = format!("__w_quantifier_total_{suffix}");
        let true_count = format!("__w_quantifier_true_{suffix}");
        let null_count = format!("__w_quantifier_unknown_{suffix}");
        let result_row = format!("__w_quantifier_result_row_{suffix}");

        let list = self.lower_list_operand(&input.plan, input_expr)?;
        let list_type = list
            .get_type(input.plan.schema())
            .map_err(|err| RelError::Unsupported(format!("quantifier input type: {err}")))?;
        if !matches!(
            list_type,
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
        ) {
            return Err(RelError::Unsupported(
                "GraphQuantifier over non-list expression".into(),
            ));
        }

        let row_number = df_window::row_number().alias(row_id.clone());
        let numbered = LogicalPlanBuilder::from(input.plan.clone())
            .window(vec![row_number])?
            .build()?;
        let list_length = datafusion::functions_nested::expr_fn::array_length(list.clone());
        let mut base_projection = existing_columns(&numbered, &BTreeSet::new());
        base_projection.extend([
            list.clone().alias(list_col.clone()),
            list.is_null().alias(input_null.clone()),
            Expr::Cast(Cast::new(Box::new(list_length), DataType::Int64)).alias(total.clone()),
        ]);
        let base = LogicalPlanBuilder::from(numbered)
            .project(base_projection)?
            .build()?;

        let mut expanded_projection =
            existing_columns(&base, &BTreeSet::from([item_binding.into()]));
        expanded_projection.push(
            collections::outer_list(col_exact(&list_col), &list_type)?.alias(item_binding),
        );
        let mut options = datafusion::common::UnnestOptions::default();
        options.preserve_nulls = true;
        let expanded_input = LogicalPlanBuilder::from(base.clone())
            .project(expanded_projection)?
            .build()?;
        let expanded_input = collections::unnest_scope(
            expanded_input,
            format!("__w_sql_cte_quantifier_input_{suffix}"),
        )?;
        let expanded_input = collections::unnest_input(expanded_input)?;
        let expanded = LogicalPlanBuilder::from(expanded_input)
            .unnest_column_with_options(Column::new_unqualified(item_binding), options)?
            .build()?;
        let expanded = collections::unnest_scope(
            expanded,
            format!("__w_sql_cte_quantifier_items_{suffix}"),
        )?;
        let predicate = self.lower_expr(&expanded, predicate)?;
        let has_item = binary(col_exact(&total), BinaryOp::Gt, lit(0_i64));
        let true_value = Expr::and(has_item.clone(), Expr::IsTrue(Box::new(predicate.clone())));
        let unknown_value = Expr::and(has_item, predicate.is_null());
        let count_case = |condition: Expr| {
            Expr::Case(Case::new(
                None,
                vec![(Box::new(condition), Box::new(lit(1_i64)))],
                Some(Box::new(lit(0_i64))),
            ))
        };
        let reduced = LogicalPlanBuilder::from(expanded)
            .aggregate(
                vec![
                    col_exact(&row_id),
                    col_exact(&input_null),
                    col_exact(&total),
                ],
                vec![
                    df_sum(count_case(true_value)).alias(true_count.clone()),
                    df_sum(count_case(unknown_value)).alias(null_count.clone()),
                ],
            )?
            .build()?;

        let true_is_zero = binary(col_exact(&true_count), BinaryOp::Eq, lit(0_i64));
        let true_is_one = binary(col_exact(&true_count), BinaryOp::Eq, lit(1_i64));
        let true_gt_one = binary(col_exact(&true_count), BinaryOp::Gt, lit(1_i64));
        let no_unknown = binary(col_exact(&null_count), BinaryOp::Eq, lit(0_i64));
        let false_count = binary(
            binary(col_exact(&total), BinaryOp::Sub, col_exact(&true_count)),
            BinaryOp::Sub,
            col_exact(&null_count),
        );
        let has_false = binary(false_count, BinaryOp::Gt, lit(0_i64));
        let null_bool = lit(ScalarValue::Boolean(None));
        let value = match kind {
            QuantifierKind::All => Expr::Case(Case::new(
                None,
                vec![
                    (
                        Box::new(col_exact(&input_null)),
                        Box::new(null_bool.clone()),
                    ),
                    (Box::new(has_false), Box::new(lit(false))),
                    (Box::new(no_unknown.clone()), Box::new(lit(true))),
                ],
                Some(Box::new(null_bool.clone())),
            )),
            QuantifierKind::Any => Expr::Case(Case::new(
                None,
                vec![
                    (
                        Box::new(col_exact(&input_null)),
                        Box::new(null_bool.clone()),
                    ),
                    (
                        Box::new(Expr::IsTrue(Box::new(binary(
                            col_exact(&true_count),
                            BinaryOp::Gt,
                            lit(0_i64),
                        )))),
                        Box::new(lit(true)),
                    ),
                    (Box::new(no_unknown.clone()), Box::new(lit(false))),
                ],
                Some(Box::new(null_bool.clone())),
            )),
            QuantifierKind::None => Expr::Case(Case::new(
                None,
                vec![
                    (
                        Box::new(col_exact(&input_null)),
                        Box::new(null_bool.clone()),
                    ),
                    (
                        Box::new(binary(col_exact(&true_count), BinaryOp::Gt, lit(0_i64))),
                        Box::new(lit(false)),
                    ),
                    (Box::new(no_unknown.clone()), Box::new(lit(true))),
                ],
                Some(Box::new(null_bool.clone())),
            )),
            QuantifierKind::Single => Expr::Case(Case::new(
                None,
                vec![
                    (
                        Box::new(col_exact(&input_null)),
                        Box::new(null_bool.clone()),
                    ),
                    (Box::new(true_gt_one), Box::new(lit(false))),
                    (
                        Box::new(Expr::and(true_is_one, no_unknown.clone())),
                        Box::new(lit(true)),
                    ),
                    (
                        Box::new(Expr::and(true_is_zero, no_unknown)),
                        Box::new(lit(false)),
                    ),
                ],
                Some(Box::new(null_bool)),
            )),
        };
        let result = LogicalPlanBuilder::from(reduced)
            .project(vec![
                col_exact(&row_id).alias(result_row.clone()),
                value.alias(output),
            ])?
            .build()?;
        let joined = LogicalPlanBuilder::from(base)
            .join_on(
                result,
                JoinType::Inner,
                vec![binary(
                    col_exact(&row_id),
                    BinaryOp::Eq,
                    col_exact(&result_row),
                )],
            )?
            .build()?;
        let mut final_projection = original_columns
            .into_iter()
            .map(col_exact)
            .collect::<Vec<_>>();
        final_projection.push(col_exact(output));
        let plan = LogicalPlanBuilder::from(joined)
            .project(final_projection)?
            .build()?;
        Ok(input.with_plan(plan))
    }

    /// UNWIND over a non-constant expression. When the lowered expression
    /// has a real Arrow list type (e.g. it came from `collect(...)` /
    /// `array_agg`), DataFusion's unnest expands it directly.
    pub(super) fn lower_unwind_dynamic(
        &mut self,
        input: LoweredNode,
        input_expr: &IrExpr,
        bind: &str,
        outer: bool,
    ) -> RelResult<LoweredNode> {
        let list_expr = if let IrExpr::List(items) = input_expr {
            datafusion::functions_nested::expr_fn::make_array(
                items
                    .iter()
                    .map(|item| self.lower_expr(&input.plan, item))
                    .collect::<RelResult<Vec<_>>>()?,
            )
        } else {
            self.lower_list_operand(&input.plan, input_expr)?
        };
        let data_type = list_expr
            .get_type(input.plan.schema())
            .map_err(|err| RelError::Unsupported(format!("unwind expression type: {err}")))?;
        if !matches!(
            data_type,
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
        ) {
            return Err(RelError::Unsupported(
                "GraphUnwind over non-constant list expression".into(),
            ));
        }
        let mut projections = existing_columns(&input.plan, &BTreeSet::from([bind.to_string()]));
        let list_expr = if outer {
            collections::outer_list(list_expr, &data_type)?
        } else {
            list_expr
        };
        projections.push(list_expr.alias(bind));
        let mut options = datafusion::common::UnnestOptions::default();
        options.preserve_nulls = outer;
        // Keep consumers outside the UNNEST SELECT: aggregates and nested
        // scalar calls must reference its output column in a new SQL scope.
        let barrier_id = self.scan_counter;
        self.scan_counter += 1;
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .project(projections)?
            .build()?;
        let plan = collections::unnest_scope(plan, format!("__w_sql_cte_unwind_input_{barrier_id}"))?;
        let plan = collections::unnest_input(plan)?;
        let plan = LogicalPlanBuilder::from(plan)
            .unnest_column_with_options(Column::new_unqualified(bind), options)?
            .build()?;
        let plan = collections::unnest_scope(plan, format!("__w_sql_cte_unwind_{barrier_id}"))?;
        Ok(input.with_plan(plan))
    }

}
