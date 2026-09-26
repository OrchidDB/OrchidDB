//! Relational contracts for per-row apply and scalar subqueries.

use super::*;

fn simple_expand(mut node: &Node) -> Option<&Node> {
    while let Node::GraphApply {
        kind: ApplyKind::Inner,
        left,
        right,
        ..
    } = node
    {
        if !matches!(left.as_ref(), Node::GraphCorrelate { .. }) {
            return None;
        }
        node = right;
    }
    match node {
        Node::GraphExpand {
            input,
            length,
            path: None,
            ..
        } if matches!(input.as_ref(), Node::GraphCorrelate { .. })
            && length.min == 1
            && length.max == Some(1) =>
        {
            Some(node)
        }
        _ => None,
    }
}

/// An existence predicate that only uses already-bound endpoints can run
/// before an unrelated expansion multiplies the row count. Unlike OPTIONAL,
/// semi/anti apply never adds bindings or changes surviving multiplicities.
pub(super) fn push_simple_existence(kind: ApplyKind, left: &Node, right: &Node) -> Option<Node> {
    if !matches!(kind, ApplyKind::Semi | ApplyKind::Anti) {
        return None;
    }
    let Node::GraphExpand {
        source,
        target,
        target_mode,
        history,
        ..
    } = simple_expand(right)?
    else {
        return None;
    };
    let mut dependencies = vec![source.clone()];
    if *target_mode == TargetMode::Existing {
        dependencies.push(target.clone());
    }
    let probe_history = history;
    let left = match left {
        Node::GraphApply {
            kind: ApplyKind::Inner,
            left,
            right,
            ..
        } if matches!(left.as_ref(), Node::GraphOneRow) => right.as_ref(),
        _ => left,
    };
    let Node::GraphExpand {
        target,
        rel_binding,
        history,
        path,
        target_mode: TargetMode::BindNew,
        ..
    } = left
    else {
        return None;
    };
    if dependencies.contains(target)
        || probe_history
            .as_ref()
            .is_some_and(|name| history.as_ref() == Some(name))
        || rel_binding
            .iter()
            .chain(history.iter())
            .chain(path.iter())
            .any(|name| dependencies.contains(name))
    {
        return None;
    }
    let mut expanded = left.clone();
    let Node::GraphExpand { input, .. } = &mut expanded else {
        unreachable!()
    };
    *input = Box::new(Node::GraphApply {
        kind,
        correlation: dependencies,
        outputs: Vec::new(),
        optional_missing: crate::ir::policy::OptionalMissing::Null,
        left: input.clone(),
        right: Box::new(right.clone()),
    });
    Some(expanded)
}

impl LoweringContext<'_> {
    /// A single correlated hop has no per-input barrier. Evaluate its graph
    /// relation once and join on just its endpoints, instead of copying the
    /// complete outer relation into the right side and joining it back again.
    pub(super) fn try_simple_expand_apply(
        &mut self,
        kind: ApplyKind,
        left: &LoweredNode,
        right: &Node,
        outputs: &[String],
    ) -> RelResult<Option<LoweredNode>> {
        if self.language != Language::Cypher
            || self.options.mapping.is_some()
            || !matches!(
                kind,
                ApplyKind::Optional | ApplyKind::Semi | ApplyKind::Anti
            )
        {
            return Ok(None);
        }
        let Some(right) = simple_expand(right) else {
            return Ok(None);
        };
        let Node::GraphExpand {
            source,
            target,
            target_mode,
            target_labels,
            rel_binding,
            rel_types,
            dir,
            length,
            history,
            path,
            input,
            ..
        } = right
        else {
            return Ok(None);
        };
        if !matches!(input.as_ref(), Node::GraphCorrelate { .. })
            || length.min != 1
            || length.max != Some(1)
            || path.is_some()
            || source == target
            || has_binding_shape(&left.plan, source) != Some(BindingShape::Node)
            || history
                .as_ref()
                .is_some_and(|name| has_exact_col(&left.plan, name))
            || rel_binding
                .as_ref()
                .is_some_and(|name| has_binding_shape(&left.plan, name).is_some())
        {
            return Ok(None);
        }
        let existing = *target_mode == TargetMode::Existing;
        if (!existing
            && (*target_mode != TargetMode::BindNew
                || has_binding_shape(&left.plan, target).is_some()))
            || (existing && has_binding_shape(&left.plan, target) != Some(BindingShape::Node))
        {
            return Ok(None);
        }
        let seed = Node::GraphNodeScan {
            graph: "default".into(),
            binding: source.clone(),
            labels: LabelExpr::Any,
        };
        let right = self.lower_expand(
            &seed,
            source,
            target,
            TargetMode::BindNew,
            if existing {
                &LabelExpr::Any
            } else {
                target_labels
            },
            rel_binding.as_ref(),
            rel_types,
            *dir,
            None,
            history.as_deref().filter(|_| self.options.mapping.is_some()),
        )?;
        let mut bindings = vec![source];
        if existing {
            bindings.push(target);
        }
        let mut conditions = Vec::new();
        let mut projection = Vec::new();
        let mut cleanup = BTreeSet::new();
        for binding in bindings {
            for column in [id_col(binding), label_col(binding)] {
                let alias = unique_internal_alias(
                    &left.plan,
                    &cleanup,
                    format!("__simple_apply_{}_{}", self.scan_counter, cleanup.len()),
                );
                conditions.push(col_exact(&column).eq(col_exact(&alias)));
                projection.push(col_exact(column).alias(&alias));
                cleanup.insert(alias);
            }
        }
        if kind == ApplyKind::Optional {
            for column in right_apply_output_columns(&right.plan, outputs)? {
                if !has_exact_col(&left.plan, &column) {
                    projection.push(col_exact(column));
                }
            }
        }
        let right_plan = LogicalPlanBuilder::from(right.plan)
            .project(projection)?
            .build()?;
        let join_type = match kind {
            ApplyKind::Optional => JoinType::Left,
            ApplyKind::Semi => JoinType::LeftSemi,
            ApplyKind::Anti => JoinType::LeftAnti,
            _ => unreachable!(),
        };
        // Preserve left-side filters at the outer-join boundary. Without a
        // subquery the SQL unparser moves those filters into ON, retaining
        // rows that should have been filtered out before the optional match.
        let left_plan = if kind == ApplyKind::Optional {
            let guard = unique_internal_alias(&left.plan, &cleanup, "__simple_apply_guard");
            let mut projection = existing_columns_by_name(&left.plan, &BTreeSet::new());
            projection.push(lit(1_i64).alias(&guard));
            cleanup.insert(guard);
            LogicalPlanBuilder::from(left.plan.clone())
                .project(projection)?
                .alias(format!("__w_sql_cte_simple_apply_{}", self.scan_counter))?
                .build()?
        } else {
            left.plan.clone()
        };
        let joined = LogicalPlanBuilder::from(left_plan)
            .join_on(right_plan, join_type, conditions)?
            .build()?;
        let projection = existing_columns_by_name(&joined, &cleanup);
        let plan = LogicalPlanBuilder::from(joined)
            .project(projection)?
            .build()?;
        let mut islands = left.islands.clone();
        islands.merge(right.islands);
        Ok(Some(LoweredNode {
            plan,
            islands,
            fields: left.fields.clone(),
            result_form: left.result_form,
        }))
    }
}

/// Give each input occurrence its own correlation identity. Binding values alone
/// cannot distinguish duplicate outer rows: joining on them squares their
/// multiplicity when the right subtree starts at `GraphCorrelate`.
pub(super) fn with_row_identity(
    plan: LogicalPlan,
    key_cols: &mut Vec<String>,
    cleanup: &mut BTreeSet<String>,
    barrier_id: usize,
) -> RelResult<LogicalPlan> {
    let key = unique_internal_alias(&plan, cleanup, "__apply_corr_key_row");
    let row_number = df_window::row_number()
        .window_frame(datafusion::logical_expr::WindowFrame::new(None))
        .build()?.alias(&key);
    let windowed = LogicalPlanBuilder::from(plan)
        .window(vec![row_number])?
        .build()?;
    let guard = unique_internal_alias(&windowed, cleanup, "__w_apply_row_guard");
    let mut columns = existing_columns_by_name(&windowed, &BTreeSet::new());
    columns.push(lit(1_i64).alias(&guard));
    let plan = LogicalPlanBuilder::from(windowed)
        .project(columns)?
        .alias(format!("__w_sql_cte_apply_row_{barrier_id}"))?
        .build()?;
    key_cols.push(key.clone());
    cleanup.insert(key);
    cleanup.insert(guard);
    Ok(plan)
}

/// A scalar right side may emit zero or one row per input. Keep the first row
/// only long enough to identify a second one; evaluating the guarded cast on
/// that row raises an execution error in DuckDB. The guard is
/// part of the right relation, so an outer projection cannot silently discard
/// it while selecting the scalar output.
pub(super) fn guard_scalar_cardinality(
    mut right: LoweredNode,
    key_cols: &[String],
) -> RelResult<LoweredNode> {
    let keys = key_cols.iter().map(col_exact).collect::<Vec<_>>();
    if rules::unique_on(&right.plan, &keys) {
        return Ok(right);
    }
    let rank = unique_internal_alias(&right.plan, &BTreeSet::new(), "__apply_scalar_rank");
    let row_number = df_window::row_number()
        .window_frame(datafusion::logical_expr::WindowFrame::new(None))
        .partition_by(key_cols.iter().map(col_exact).collect())
        .build()?
        .alias(&rank);
    let ranked = LogicalPlanBuilder::from(right.plan)
        .window(vec![row_number])?
        .build()?;
    let bad_cast = Expr::Cast(Cast::new(
        // Keep the invalid cast dependent on this row. Otherwise constant
        // folding raises it even when the CASE branch is never selected.
        Box::new(datafusion::functions::string::expr_fn::concat(vec![
            lit("scalar subquery returned more than one row: "),
            Expr::Cast(Cast::new(Box::new(col_exact(&rank)), DataType::Utf8)),
        ])),
        DataType::Boolean,
    ));
    let guard = Expr::Case(Case::new(
        None,
        vec![(
            Box::new(binary(col_exact(&rank), BinaryOp::Gt, lit(1_u64))),
            Box::new(bad_cast),
        )],
        Some(Box::new(lit(true))),
    ));
    let guarded = LogicalPlanBuilder::from(ranked).filter(guard)?.build()?;
    let projections = existing_columns_by_name(&guarded, &BTreeSet::from([rank]));
    right.plan = LogicalPlanBuilder::from(guarded)
        .project(projections)?
        .build()?;
    Ok(right)
}

/// Evaluate coalesce arms against only the input occurrences that earlier
/// arms did not produce from. A left anti join advances the unproductive set,
/// while a union retains every row of the first productive arm for that input.
pub(super) fn lower_coalesce(
    ctx: &mut LoweringContext<'_>,
    success: CoalesceSuccess,
    _output: &str,
    correlation: &[String],
    input: &Node,
    arms: &[Node],
) -> RelResult<LoweredNode> {
    if success != CoalesceSuccess::FirstNonEmpty {
        return Err(RelError::Unsupported("GraphCoalesce success policy".into()));
    }
    let input = ctx.lower_node(input)?;
    let (left_plan, mut key_cols, mut cleanup) =
        with_apply_correlation_keys(input.plan.clone(), correlation)?;
    let barrier_id = ctx.scan_counter;
    ctx.scan_counter += 1;
    let mut remaining = with_row_identity(left_plan, &mut key_cols, &mut cleanup, barrier_id)?;
    let mut selected: Option<LogicalPlan> = None;
    let mut islands = input.islands;

    for (arm_index, arm) in arms.iter().enumerate() {
        let branch = ctx.lower_with_correlate(remaining.clone(), arm)?;
        if !absorbed_correlation(&remaining, &branch.plan) {
            return Err(RelError::Unsupported(
                "GraphCoalesce arm did not consume its correlated input".into(),
            ));
        }
        islands.merge(branch.islands);
        let matched = branch.plan;
        let (left_plan, right_plan, join_exprs, _) =
            prepare_apply_join_inputs(remaining, matched.clone(), &key_cols, &[])?;
        remaining = LogicalPlanBuilder::from(left_plan)
            .join_on(right_plan, JoinType::LeftAnti, join_exprs)?
            .build()?;
        if arm_index + 1 < arms.len() {
            // Later arms read this same unproductive relation. A CTE keeps
            // its SQL from being inlined through every later branch.
            let guard = unique_internal_alias(
                &remaining,
                &cleanup,
                format!("__w_coalesce_remaining_guard_{arm_index}"),
            );
            let mut columns = existing_columns_by_name(&remaining, &BTreeSet::new());
            columns.push(lit(1_i64).alias(&guard));
            cleanup.insert(guard);
            remaining = LogicalPlanBuilder::from(remaining)
                .project(columns)?
                .alias(format!(
                    "__w_sql_cte_coalesce_remaining_{barrier_id}_{arm_index}"
                ))?
                .build()?;
        }
        selected = Some(match selected {
            Some(previous) => LogicalPlanBuilder::from(previous)
                .union_by_name(matched)?
                .build()?,
            None => matched,
        });
    }

    let selected = match selected {
        Some(plan) => plan,
        None => LogicalPlanBuilder::from(remaining)
            .filter(lit(false))?
            .build()?,
    };
    let projections = existing_columns_by_name(&selected, &cleanup);
    let plan = LogicalPlanBuilder::from(selected)
        .project(projections)?
        .build()?;
    Ok(LoweredNode {
        plan,
        islands,
        fields: input.fields,
        result_form: input.result_form,
    })
}
