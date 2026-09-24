//! Join/apply lowering and correlation maintenance.

use super::*;

impl<'a> LoweringContext<'a> {
    pub(super) fn lower_join(
        &mut self,
        kind: JoinKind,
        left: &Node,
        right: &Node,
        condition: Option<&IrExpr>,
    ) -> RelResult<LoweredNode> {
        let left = self.lower_node(left)?;
        let right = self.lower_node(right)?;
        let join_type = match kind {
            JoinKind::Inner => JoinType::Inner,
            JoinKind::LeftOuter => JoinType::Left,
            JoinKind::RightOuter => JoinType::Right,
            JoinKind::FullOuter => JoinType::Full,
            JoinKind::Cross => JoinType::Inner,
        };
        let plan = if matches!(kind, JoinKind::Cross) && condition.is_none() {
            LogicalPlanBuilder::from(left.plan.clone())
                .cross_join(right.plan.clone())?
                .build()?
        } else {
            let expr = match condition {
                Some(condition) => {
                    vec![self.lower_expr_for_join(&left.plan, &right.plan, condition)?]
                }
                None => Vec::new(),
            };
            LogicalPlanBuilder::from(left.plan.clone())
                .join_on(right.plan.clone(), join_type, expr)?
                .build()?
        };
        let mut islands = left.islands;
        islands.merge(right.islands);
        // A join widens the row: both sides' fields survive. Keeping only
        // `left.fields` silently drops everything the right side binds, so
        // any later reference to it fails to resolve ("Referenced column
        // … was not found"). This stayed latent while nothing in the Cypher
        // path emitted `GraphJoin`; uncorrelated `MATCH (a), (b)` now does.
        // Left order wins on collision, matching the interpreter's
        // `join_op`, which inserts right bindings with `or_insert_with`.
        let fields = match (left.fields, right.fields) {
            (Some(mut left_fields), Some(right_fields)) => {
                for field in right_fields {
                    if !left_fields.contains(&field) {
                        left_fields.push(field);
                    }
                }
                Some(left_fields)
            }
            (left_fields, right_fields) => left_fields.or(right_fields),
        };
        Ok(LoweredNode {
            plan,
            islands,
            fields,
            result_form: left.result_form,
        })
    }

    pub(super) fn lower_apply(
        &mut self,
        kind: ApplyKind,
        correlation: &[String],
        outputs: &[String],
        left: &Node,
        right: &Node,
    ) -> RelResult<LoweredNode> {
        if self.language == Language::Cypher && outputs.is_empty()
            && let Some(pushed) = apply::push_simple_existence(kind, left, right)
        {
            return self.lower_node(&pushed);
        }
        let left = self.lower_node(left)?;
        if let Some(joined) = self.try_simple_expand_apply(kind, &left, right, outputs)? {
            return Ok(joined);
        }
        let (left_plan, mut key_cols, mut cleanup) =
            if self.language == Language::Cypher && kind == ApplyKind::Inner
                && !gremlin::has_per_input_barrier(right)
            {
                // Ordinary MATCH absorbs its input directly. Redundant
                // correlation aliases introduce projection boundaries that
                // prevent the SQL engine from reordering joins across MATCH.
                (left.plan.clone(), Vec::new(), BTreeSet::new())
            } else {
                with_apply_correlation_keys(left.plan.clone(), correlation)?
            };
        // Per-row apply: every correlated right side that can aggregate,
        // slice, or multiply rows needs one identity per input occurrence.
        // Semi/anti joins may keep binding-value keys (existence is equal for
        // equal values), unless there are no keys at all — then a constant
        // join would test existence globally rather than per input row.
        let per_row_identity = match kind {
            ApplyKind::Scalar | ApplyKind::Optional => true,
            ApplyKind::Inner => gremlin::has_per_input_barrier(right),
            ApplyKind::Semi | ApplyKind::Anti => key_cols.is_empty(),
        };
        let left_plan = if per_row_identity && first_correlate_bindings(right).is_some() {
            let barrier_id = self.scan_counter;
            self.scan_counter += 1;
            apply::with_row_identity(left_plan, &mut key_cols, &mut cleanup, barrier_id)?
        } else {
            left_plan
        };
        let left = left.with_plan(left_plan);
        let previous = self.correlate_plan.replace(left.plan.clone());
        let right = self.lower_node(right);
        self.correlate_plan = previous;
        let right = right?;
        match kind {
            ApplyKind::Inner => {
                let mut right = right;
                right.islands.merge(left.islands);
                // The right side normally absorbs the left through
                // `correlate_plan`, which is why returning it alone is
                // usually right. When it did not — an uncorrelated pattern
                // such as `UNWIND ... MATCH ...` or a comma-separated match —
                // the left is a genuine cross-product factor, and dropping it
                // loses both its multiplicity and its bindings.
                let keyed = if absorbed_correlation(&left.plan, &right.plan) {
                    None
                } else {
                    gremlin::keyed_apply_join(
                        left.plan.clone(),
                        right.plan.clone(),
                        &key_cols,
                        outputs,
                        JoinType::Inner,
                        &mut cleanup,
                    )?
                };
                if let Some(plan) = keyed {
                    right.plan = plan;
                } else if !absorbed_correlation(&left.plan, &right.plan) {
                    // Keep a joined MATCH subtree on the right side of the
                    // Cartesian product. Without this boundary SQL join
                    // precedence can bind its first INNER JOIN to the UNWIND
                    // input on the left, changing both names and row counts.
                    let barrier_id = self.scan_counter;
                    let name = format!("__w_sql_cte_apply_{barrier_id}");
                    self.scan_counter += 1;
                    let mut columns = right
                        .plan
                        .schema()
                        .fields()
                        .iter()
                        .map(|field| col_exact(field.name()))
                        .collect::<Vec<_>>();
                    columns.push(lit(1_i64).alias(format!("__w_cte_guard_{barrier_id}")));
                    let right_input = LogicalPlanBuilder::from(right.plan.clone())
                        .project(columns)?
                        .alias(name)?
                        .build()?;
                    // GraphApply's interpreter lets an inner traversal's
                    // synthetic Gremlin path replace the incoming path. Keep
                    // that same ownership here: a cross join with both
                    // columns named `__path` leaves DataFusion unable to
                    // resolve the right CTE's qualified field alongside the
                    // outer unqualified field.
                    let left_plan = if has_exact_col(&left.plan, "__path")
                        && has_exact_col(&right.plan, "__path")
                    {
                        let projections = existing_columns_by_name(
                            &left.plan,
                            &BTreeSet::from(["__path".to_string()]),
                        );
                        LogicalPlanBuilder::from(left.plan.clone())
                            .project(projections)?
                            .build()?
                    } else {
                        left.plan.clone()
                    };
                    right.plan = LogicalPlanBuilder::from(left_plan)
                        .cross_join(right_input)?
                        .build()?;
                }
                if !cleanup.is_empty() {
                    let projections = existing_columns_by_name(&right.plan, &cleanup);
                    right.plan = LogicalPlanBuilder::from(right.plan)
                        .project(projections)?
                        .build()?;
                }
                Ok(right)
            }
            ApplyKind::Semi | ApplyKind::Anti => {
                self.lower_existence_apply(kind, &key_cols, cleanup, left, right)
            }
            ApplyKind::Optional => self.lower_left_apply(&key_cols, outputs, cleanup, left, right),
            ApplyKind::Scalar => {
                let right = apply::guard_scalar_cardinality(right, &key_cols)?;
                self.lower_left_apply(&key_cols, outputs, cleanup, left, right)
            }
        }
    }

    pub(super) fn lower_existence_apply(
        &mut self,
        kind: ApplyKind,
        key_cols: &[String],
        mut cleanup: BTreeSet<String>,
        left: LoweredNode,
        right: LoweredNode,
    ) -> RelResult<LoweredNode> {
        let (left_plan, right_plan, join_exprs, right_cleanup) =
            prepare_apply_join_inputs(left.plan.clone(), right.plan.clone(), key_cols, &[])?;
        cleanup.extend(right_cleanup);
        let join_type = match kind {
            ApplyKind::Semi => JoinType::LeftSemi,
            ApplyKind::Anti => JoinType::LeftAnti,
            _ => unreachable!("existence apply only handles semi/anti"),
        };
        let mut plan = LogicalPlanBuilder::from(left_plan)
            .join_on(right_plan, join_type, join_exprs)?
            .build()?;
        if !cleanup.is_empty() {
            let projections = existing_columns_by_name(&plan, &cleanup);
            plan = LogicalPlanBuilder::from(plan)
                .project(projections)?
                .build()?;
        }
        let mut islands = left.islands;
        islands.merge(right.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: left.fields,
            result_form: left.result_form,
        })
    }

    pub(super) fn lower_left_apply(
        &mut self,
        key_cols: &[String],
        outputs: &[String],
        mut cleanup: BTreeSet<String>,
        left: LoweredNode,
        right: LoweredNode,
    ) -> RelResult<LoweredNode> {
        let outputs = right_apply_output_columns(&right.plan, outputs)?;
        let (left_plan, right_plan, join_exprs, right_cleanup) =
            prepare_apply_join_inputs(left.plan.clone(), right.plan.clone(), key_cols, &outputs)?;
        cleanup.extend(right_cleanup);
        let mut plan = LogicalPlanBuilder::from(left_plan)
            .join_on(right_plan, JoinType::Left, join_exprs)?
            .build()?;
        if !cleanup.is_empty() {
            let projections = existing_columns_by_name(&plan, &cleanup);
            plan = LogicalPlanBuilder::from(plan)
                .project(projections)?
                .build()?;
        }
        let mut islands = left.islands;
        islands.merge(right.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: left.fields,
            result_form: left.result_form,
        })
    }

}

// Carry an explicit ordinal through projections so ORDER BY expressions that
// are absent from the final result can still determine the surviving row.
pub(super) fn with_distinct_ordinal(plan: LogicalPlan, ordinal: &str) -> RelResult<LogicalPlan> {
    if let LogicalPlan::Projection(projection) = &plan {
        let input = with_distinct_ordinal(projection.input.as_ref().clone(), ordinal)?;
        let mut expr = projection.expr.clone();
        expr.push(col_exact(ordinal));
        return Ok(LogicalPlanBuilder::from(input).project(expr)?.build()?);
    }
    if let LogicalPlan::Filter(filter) = &plan {
        let input = with_distinct_ordinal(filter.input.as_ref().clone(), ordinal)?;
        return Ok(LogicalPlanBuilder::from(input)
            .filter(filter.predicate.clone())?
            .build()?);
    }
    let order = match &plan {
        LogicalPlan::Sort(sort) => sort.expr.clone(),
        _ => Vec::new(),
    };
    let window = df_window::row_number()
        .order_by(order)
        .build()?
        .alias(ordinal);
    Ok(LogicalPlanBuilder::from(plan)
        .window(vec![window])?
        .build()?)
}

pub(super) fn keyed_distinct(plan: LogicalPlan, keys: &[String], barrier_id: usize) -> RelResult<LogicalPlan> {
    let mut partition = Vec::new();
    if keys.is_empty() {
        partition.extend(existing_columns_by_name(&plan, &BTreeSet::new()));
    } else {
        for key in keys {
            if has_binding_shape(&plan, key).is_some() {
                partition.push(col_exact(id_col(key)));
                partition.push(col_exact(label_col(key)));
            } else if has_exact_col(&plan, key) {
                partition.push(col_exact(key));
            } else {
                // Missing bindings share the same unbound key.
                partition.push(lit(ScalarValue::Null));
            }
        }
    }
    partition.extend(apply_correlation_key_columns(&plan).iter().map(col_exact));
    let ordinal = unique_internal_alias(
        &plan,
        &BTreeSet::new(),
        format!("__distinct_ordinal_{barrier_id}"),
    );
    let mut cleanup = BTreeSet::from([ordinal.clone()]);
    let rank = unique_internal_alias(&plan, &cleanup, format!("__distinct_rank_{barrier_id}"));
    cleanup.insert(rank.clone());
    // Keep the two windows in separate SQL scopes: DuckDB cannot reference
    // one window result from another window in the same SELECT.
    let numbered = with_distinct_ordinal(plan, &ordinal)?;
    let input_guard = unique_internal_alias(
        &numbered,
        &cleanup,
        format!("__distinct_input_guard_{barrier_id}"),
    );
    cleanup.insert(input_guard.clone());
    let mut columns = existing_columns_by_name(&numbered, &BTreeSet::new());
    columns.push(lit(1_i64).alias(input_guard));
    let numbered = LogicalPlanBuilder::from(numbered)
        .project(columns)?
        .alias(format!("__w_sql_cte_distinct_{barrier_id}"))?
        .build()?;
    let window = df_window::row_number()
        .partition_by(partition)
        .order_by(vec![col_exact(&ordinal).sort(true, false)])
        .build()?
        .alias(&rank);
    let ranked = LogicalPlanBuilder::from(numbered)
        .window(vec![window])?
        .build()?;
    let guard = unique_internal_alias(&ranked, &cleanup, format!("__distinct_guard_{barrier_id}"));
    cleanup.insert(guard.clone());
    let mut columns = existing_columns_by_name(&ranked, &BTreeSet::new());
    columns.push(lit(1_i64).alias(guard));
    let ranked = LogicalPlanBuilder::from(ranked)
        .project(columns)?
        .alias(format!("__w_sql_cte_distinct_rank_{barrier_id}"))?
        .build()?;
    let selected = LogicalPlanBuilder::from(ranked)
        .filter(binary(col_exact(&rank), BinaryOp::Eq, lit(1_u64)))?
        .sort(vec![col_exact(&ordinal).sort(true, false)])?
        .build()?;
    let projection = existing_columns_by_name(&selected, &cleanup);
    Ok(LogicalPlanBuilder::from(selected)
        .project(projection)?
        .build()?)
}

pub(super) fn partitioned_limit(
    plan: LogicalPlan,
    partition_cols: &[String],
    offset: u64,
    fetch: Option<u64>,
) -> RelResult<LogicalPlan> {
    if partition_cols.is_empty() {
        return LogicalPlanBuilder::from(plan)
            .limit(offset as usize, fetch.map(|n| n as usize))?
            .build()
            .map_err(RelError::from);
    }
    let row_number = unique_internal_alias(&plan, &BTreeSet::new(), "__apply_row_number");
    let partition_by = partition_cols.iter().map(col_exact).collect::<Vec<_>>();
    let mut predicate = binary(col_exact(&row_number), BinaryOp::Gt, lit(offset));
    if let Some(fetch) = fetch {
        predicate = Expr::and(
            predicate,
            binary(col_exact(&row_number), BinaryOp::Lte, lit(offset + fetch)),
        );
    }
    let window = df_window::row_number()
        .partition_by(partition_by)
        .build()?
        .alias(row_number.clone());
    let mut cleanup = BTreeSet::new();
    cleanup.insert(row_number);
    let windowed = LogicalPlanBuilder::from(plan)
        .window(vec![window])?
        .filter(predicate)?
        .build()?;
    let projections = existing_columns_by_name(&windowed, &cleanup);
    LogicalPlanBuilder::from(windowed)
        .project(projections)?
        .build()
        .map_err(RelError::from)
}

pub(super) fn correlation_key_columns(plan: &LogicalPlan, bindings: &[String]) -> RelResult<Vec<String>> {
    let mut out = Vec::new();
    for binding in bindings {
        if has_exact_col(plan, binding) {
            out.push(binding.clone());
        } else if binding.starts_with("__") {
            continue;
        } else if has_binding_shape(plan, binding).is_some() {
            out.push(id_col(binding));
            out.push(label_col(binding));
        } else {
            return Err(RelError::Unsupported(format!(
                "apply correlation `{binding}` is not available relationally"
            )));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

pub(super) fn unique_internal_alias(
    plan: &LogicalPlan,
    reserved: &BTreeSet<String>,
    base: impl AsRef<str>,
) -> String {
    let base = base.as_ref();
    if !has_exact_col(plan, base) && !reserved.contains(base) {
        return base.to_string();
    }
    for suffix in 1.. {
        let candidate = format!("{base}_{suffix}");
        if !has_exact_col(plan, &candidate) && !reserved.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!("unbounded alias search")
}

pub(super) fn with_apply_correlation_keys(
    plan: LogicalPlan,
    correlation: &[String],
) -> RelResult<(LogicalPlan, Vec<String>, BTreeSet<String>)> {
    let key_cols = correlation_key_columns(&plan, correlation)?;
    if key_cols.is_empty() {
        return Ok((plan, Vec::new(), BTreeSet::new()));
    }
    let mut cleanup = BTreeSet::new();
    let mut projections = existing_columns(&plan, &BTreeSet::new());
    let mut aliases = Vec::with_capacity(key_cols.len());
    for (idx, key) in key_cols.iter().enumerate() {
        let alias = unique_internal_alias(&plan, &cleanup, format!("__apply_corr_key_{idx}"));
        cleanup.insert(alias.clone());
        projections.push(col_exact(key).alias(alias.clone()));
        aliases.push(alias);
    }
    let plan = LogicalPlanBuilder::from(plan)
        .project(projections)?
        .build()?;
    Ok((plan, aliases, cleanup))
}

pub(super) fn right_apply_output_columns(right: &LogicalPlan, outputs: &[String]) -> RelResult<Vec<String>> {
    let mut out = Vec::new();
    for output in outputs {
        if output.starts_with("__") && !output.starts_with("__rdf:term:") {
            continue;
        }
        if has_exact_col(right, output) {
            out.push(output.clone());
        } else if has_binding_shape(right, output).is_some() {
            out.extend(binding_column_names(right, output)?);
        } else {
            return Err(RelError::Unsupported(format!(
                "apply output `{output}` is not available relationally"
            )));
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

pub(super) fn binding_column_names(plan: &LogicalPlan, binding: &str) -> RelResult<Vec<String>> {
    let Some(_) = has_binding_shape(plan, binding) else {
        return Err(RelError::Unsupported(format!(
            "binding `{binding}` is not an element binding"
        )));
    };
    Ok(plan
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .filter(|name| is_binding_column(name, binding))
        .collect())
}

pub(super) fn prepare_apply_join_inputs(
    left: LogicalPlan,
    right: LogicalPlan,
    key_cols: &[String],
    output_cols: &[String],
) -> RelResult<(LogicalPlan, LogicalPlan, Vec<Expr>, BTreeSet<String>)> {
    let mut cleanup = BTreeSet::new();
    let (left, key_pairs) = if key_cols.is_empty() {
        let left_key = unique_internal_alias(&left, &cleanup, "__apply_left_key_0");
        cleanup.insert(left_key.clone());
        let right_key = unique_internal_alias(&right, &cleanup, "__apply_right_key_0");
        cleanup.insert(right_key.clone());
        let mut projections = existing_columns(&left, &BTreeSet::new());
        projections.push(lit(1_i64).alias(left_key.clone()));
        let left = LogicalPlanBuilder::from(left)
            .project(projections)?
            .build()?;
        (left, vec![(left_key, right_key)])
    } else {
        (
            left,
            key_cols
                .iter()
                .enumerate()
                .map(|(idx, key)| {
                    let alias =
                        unique_internal_alias(&right, &cleanup, format!("__apply_right_key_{idx}"));
                    cleanup.insert(alias.clone());
                    (key.clone(), alias)
                })
                .collect::<Vec<_>>(),
        )
    };

    let mut right_projections = Vec::new();
    if key_cols.is_empty() {
        let right_key = &key_pairs[0].1;
        right_projections.push(lit(1_i64).alias(right_key.clone()));
    } else {
        for (key, alias) in key_cols
            .iter()
            .zip(key_pairs.iter().map(|(_, alias)| alias))
        {
            if !has_exact_col(&right, key) {
                return Err(RelError::Unsupported(format!(
                    "apply right side dropped correlation key `{key}`"
                )));
            }
            right_projections.push(col_exact(key).alias(alias));
        }
    }
    for col in output_cols {
        if has_exact_col(&right, col) {
            right_projections.push(col_exact(col));
        }
    }
    let right = LogicalPlanBuilder::from(right)
        .project(right_projections)?
        .build()?;
    let join_exprs = key_pairs
        .into_iter()
        .map(|(left_key, right_key)| {
            // A non-null key needs only equality, exposing a hash-join key
            // even when other correlations require null-safe matching.
            // In particular the per-occurrence row number is never null.
            let non_null = left.schema().field_with_unqualified_name(&left_key)
                .is_ok_and(|field| !field.is_nullable());
            let left = col_exact(left_key);
            let right = col_exact(right_key);
            if non_null {
                left.eq(right)
            } else {
                left.clone().eq(right.clone()).or(left.is_null().and(right.is_null()))
            }
        })
        .collect::<Vec<_>>();
    Ok((left, right, join_exprs, cleanup))
}

impl LoweringContext<'_> {
    pub(super) fn lower_expr_for_join(
        &self,
        left: &LogicalPlan,
        right: &LogicalPlan,
        expr: &IrExpr,
    ) -> RelResult<Expr> {
        let joined = LogicalPlanBuilder::from(left.clone())
            .cross_join(right.clone())?
            .build()?;
        self.lower_expr(&joined, expr)
    }
}

/// Did lowering the right side of an `Apply` pull the left side in?
///
/// Correlated right sides reach the left through `GraphCorrelate`, so every
/// left column reappears in the right plan's schema. An uncorrelated right
/// side carries none of them, and the two need joining instead.
pub(super) fn absorbed_correlation(left: &LogicalPlan, right: &LogicalPlan) -> bool {
    let right_names: BTreeSet<&str> = right
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().as_str())
        .collect();
    left.schema()
        .fields()
        .iter()
        .filter(|field| field.name() != "__w_one_row")
        .all(|field| right_names.contains(field.name().as_str()))
}
