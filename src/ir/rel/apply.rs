//! Relational contracts for per-row apply and scalar subqueries.

use super::*;

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
    let row_number = df_window::row_number().alias(&key);
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
    let rank = unique_internal_alias(&right.plan, &BTreeSet::new(), "__apply_scalar_rank");
    let row_number = df_window::row_number()
        .partition_by(key_cols.iter().map(col_exact).collect())
        .build()?
        .alias(&rank);
    let ranked = LogicalPlanBuilder::from(right.plan)
        .window(vec![row_number])?
        .build()?;
    let bad_cast = Expr::Cast(Cast::new(
        Box::new(lit("scalar subquery returned more than one row")),
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
