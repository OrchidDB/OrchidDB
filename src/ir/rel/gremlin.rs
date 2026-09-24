//! Gremlin traverser-semantics helpers for relational lowering.
//!
//! Gremlin child traversals (`by()` modulators, `local()`, `map()`, branch
//! arms, ...) run once per input traverser. Relationally that means every
//! correlated right side carries an explicit per-occurrence identity
//! (`__apply_corr_key_row`, see `apply::with_row_identity`), and any barrier
//! inside the child (limit, aggregate, dedup, order) is partitioned by it.
//! The helpers here join such a right side back to its input occurrences.

use super::*;

/// Join a correlated right side back to its left input occurrences when the
/// right side replaced (projected away) the left bindings instead of
/// carrying them through. The row-identity keys make this an equijoin per
/// input traverser; a cross join would multiply every input by every
/// produced row.
///
/// Declared outputs are right-owned, replacing any matching input binding.
/// Other colliding columns are left-owned; noncolliding right columns survive
/// as additional apply outputs.
pub(super) fn keyed_apply_join(
    left: LogicalPlan,
    right: LogicalPlan,
    key_cols: &[String],
    outputs: &[String],
    join_type: JoinType,
    cleanup: &mut BTreeSet<String>,
) -> RelResult<Option<LogicalPlan>> {
    if key_cols.is_empty() || !key_cols.iter().all(|key| has_exact_col(&right, key)) {
        return Ok(None);
    }
    let is_output = |name: &str| outputs.iter().any(|output| is_binding_column(name, output));
    // Declared apply outputs are right-owned: the child's value replaces the
    // input's binding of the same name (e.g. `local(count())` replaces
    // `current`). Drop every left column of those bindings first.
    let left_keep = left
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .filter(|name| key_cols.contains(name) || !is_output(name))
        .collect::<Vec<_>>();
    let left = LogicalPlanBuilder::from(left)
        .project(left_keep.iter().map(col_exact).collect::<Vec<_>>())?
        .build()?;
    let left_names: BTreeSet<String> = left_keep.into_iter().collect();
    let right_outputs = right
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .filter(|name| !key_cols.contains(name))
        .filter(|name| {
            is_output(name)
                || (!left_names.contains(name) && !conflicts_with_left_binding(&left, name))
        })
        .collect::<Vec<_>>();
    let (left_plan, right_plan, join_exprs, right_cleanup) =
        prepare_apply_join_inputs(left, right, key_cols, &right_outputs)?;
    cleanup.extend(right_cleanup);
    Ok(Some(
        LogicalPlanBuilder::from(left_plan)
            .join_on(right_plan, join_type, join_exprs)?
            .build()?,
    ))
}

/// True when `name` belongs to a binding the left side already defines with
/// a different representation, e.g. a scalar `current` on the right while the
/// left carries the element columns `current__id`/`current__label`.
fn conflicts_with_left_binding(left: &LogicalPlan, name: &str) -> bool {
    if has_binding_shape(left, name).is_some() {
        return true;
    }
    left.schema().fields().iter().any(|field| {
        let binding = field.name();
        !binding.starts_with("__") && name != binding && is_binding_column(name, binding)
    })
}

/// Whether a correlated child contains an operator whose result depends on
/// the whole per-input group (slices, aggregates, dedup, ordering, folds).
/// Such children need a per-occurrence identity; pure streaming children are
/// already exact when correlated by binding values.
pub(super) fn has_per_input_barrier(node: &Node) -> bool {
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if matches!(
            node,
            Node::GraphSlice { .. }
                | Node::GraphSliceExpr { .. }
                | Node::GraphAggregate { .. }
                | Node::GraphGroupMap { .. }
                | Node::GraphDistinct { .. }
                | Node::GraphSort { .. }
                | Node::GraphBarrier { .. }
                | Node::GraphCollect { .. }
        ) {
            return true;
        }
        stack.extend(node_children(node));
    }
    false
}

/// An ungrouped aggregate inside a correlated child runs once per input
/// occurrence and must emit its empty-input result for inputs whose child
/// stream is empty (`local(outE().count())` is `0`, not a dropped traverser).
/// Grouping by the correlation keys alone loses those inputs, so restore them
/// from the correlated input's key set with a left join. Only count-shaped
/// aggregates have a defined empty value here; any other aggregate keeps the
/// grouped result unchanged.
pub(super) fn with_empty_count_defaults(
    correlate_plan: Option<&LogicalPlan>,
    aggregated: LogicalPlan,
    key_cols: &[String],
    aggs: &[AggCall],
) -> RelResult<LogicalPlan> {
    let Some(correlate_plan) = correlate_plan else {
        return Ok(aggregated);
    };
    if key_cols.is_empty()
        || aggs.is_empty()
        || !aggs.iter().all(|agg| {
            matches!(
                agg.kind,
                AggKind::CountRows | AggKind::CountBulk | AggKind::CountDistinct | AggKind::CountIf
            )
        })
        || !key_cols
            .iter()
            .all(|key| has_exact_col(correlate_plan, key))
    {
        return Ok(aggregated);
    }
    let keys = LogicalPlanBuilder::from(correlate_plan.clone())
        .project(key_cols.iter().map(col_exact).collect::<Vec<_>>())?
        .distinct()?
        .build()?;
    let mut renamed = Vec::new();
    let mut projections = Vec::new();
    let mut join_exprs = Vec::new();
    for (idx, key) in key_cols.iter().enumerate() {
        let alias =
            unique_internal_alias(&aggregated, &BTreeSet::new(), format!("__agg_key_{idx}"));
        projections.push(col_exact(key).alias(&alias));
        let left = col_exact(key);
        let right = col_exact(&alias);
        join_exprs.push(
            left.clone()
                .eq(right.clone())
                .or(left.is_null().and(right.is_null())),
        );
        renamed.push(alias);
    }
    for agg in aggs {
        projections.push(col_exact(&agg.alias));
    }
    let right = LogicalPlanBuilder::from(aggregated)
        .project(projections)?
        .build()?;
    let joined = LogicalPlanBuilder::from(keys)
        .join_on(right, JoinType::Left, join_exprs)?
        .build()?;
    let mut out = key_cols.iter().map(col_exact).collect::<Vec<_>>();
    for agg in aggs {
        out.push(df_core::coalesce(vec![col_exact(&agg.alias), lit(0_i64)]).alias(&agg.alias));
    }
    Ok(LogicalPlanBuilder::from(joined).project(out)?.build()?)
}

/// Gremlin streams are heterogeneous: after a union/choose one traverser's
/// `current` may be an element while another's is a scalar. Relationally the
/// two representations live in different columns (`current__id`/... versus
/// `current`), and exactly one of them is populated on each row. Render the
/// result the way the interpreter renders mixed values: element display text
/// for element rows, tagged scalar text otherwise.
pub(super) fn mixed_current_display_expr(
    plan: &LogicalPlan,
    field: &str,
) -> RelResult<Option<Expr>> {
    if !has_exact_col(plan, field) || has_binding_shape(plan, field).is_none() {
        return Ok(None);
    }
    let Some(data_type) = plan_column_type(plan, field) else {
        return Ok(None);
    };
    if data_type == DataType::Null {
        return Ok(Some(gremlin_element_display_expr(plan, field)?));
    }
    // Only primitive scalars have an interpreter-compatible tagged text
    // form here; nested values keep their typed column.
    if !matches!(
        data_type,
        DataType::Boolean
            | DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::Float32
            | DataType::Float64
            | DataType::Utf8
            | DataType::LargeUtf8
            | DataType::Utf8View
    ) {
        return Ok(None);
    }
    let element = gremlin_element_display_expr(plan, field)?;
    let scalar = gremlin_tagged_text_expr(col_exact(field), &data_type);
    Ok(Some(Expr::Case(Case::new(
        None,
        vec![(
            Box::new(col_exact(id_col(field)).is_not_null()),
            Box::new(element),
        )],
        Some(Box::new(scalar)),
    ))))
}

/// How many times each Gremlin step label can be bound on one traverser's
/// history. A label bound inside a repeat body, or by more than one step,
/// may have several history entries; `usize::MAX` marks "unbounded".
pub(super) fn label_bind_counts(root: &Node) -> BTreeMap<String, usize> {
    fn walk(node: &Node, in_loop: bool, counts: &mut BTreeMap<String, usize>) {
        if let Node::GraphBind { bind, .. } = node
            && bind != "current"
        {
            let entry = counts.entry(bind.clone()).or_insert(0);
            *entry = if in_loop {
                usize::MAX
            } else {
                entry.saturating_add(1)
            };
        }
        if let Node::GraphRepeat {
            seed,
            body,
            until_traversal,
            prefix_traversal,
            ..
        } = node
        {
            walk(seed, in_loop, counts);
            walk(body, true, counts);
            for child in until_traversal.iter().chain(prefix_traversal.iter()) {
                walk(child, true, counts);
            }
            return;
        }
        for child in node_children(node) {
            walk(child, in_loop, counts);
        }
    }
    let mut counts = BTreeMap::new();
    walk(root, false, &mut counts);
    counts
}

impl LoweringContext<'_> {
    /// Endpoint expressions carry identity, but their properties belong to
    /// the vertex table rather than to the edge being projected. Attach the
    /// vertex columns so subsequent values(), select(), and navigation see
    /// the same element as a vertex scan would produce.
    pub(super) fn hydrate_endpoint_projections(
        &mut self,
        mut input: LoweredNode,
        items: &[ProjectionItem],
    ) -> RelResult<LoweredNode> {
        for item in items {
            if !matches!(&item.expr, IrExpr::Call { name, args }
                if matches!(name.as_str(), "edge_src" | "edge_dst")
                    && matches!(args.as_slice(), [IrExpr::Binding(_)]))
                || has_binding_shape(&input.plan, &item.alias) != Some(BindingShape::Node)
            {
                continue;
            }
            let mut ordinal = 0;
            let binding = loop {
                let candidate = format!("__gremlin_endpoint_{ordinal}");
                if !input
                    .plan
                    .schema()
                    .fields()
                    .iter()
                    .any(|field| is_binding_column(field.name(), &candidate))
                {
                    break candidate;
                }
                ordinal += 1;
            };
            let vertices = self.lower_node_scan(&binding, &LabelExpr::Any)?;
            let mut projections = existing_columns(&input.plan, &BTreeSet::new());
            projections.extend(
                duplicate_binding_projection_only(&vertices.plan, &binding, &item.alias)?
                    .into_iter()
                    .skip(2),
            );
            let joined = LogicalPlanBuilder::from(input.plan.clone())
                .join_on(
                    vertices.plan,
                    JoinType::Inner,
                    vec![binding_pair_eq(
                        &item.alias,
                        &id_col(&binding),
                        &label_col(&binding),
                    )],
                )?
                .project(projections)?
                .build()?;
            input.islands.merge(vertices.islands);
            input = input.with_plan(joined);
        }
        Ok(input)
    }

    /// `select(pop, label)` reads the label's history. Relationally only the
    /// latest binding of a label is carried, which answers `last` always and
    /// every pop when the label has at most one history entry. Anything else
    /// needs the history and must decline rather than return the last entry.
    pub(super) fn check_select_pop(&self, label: &str, pop: &IrExpr) -> RelResult<()> {
        let IrExpr::Lit(Lit::String(pop)) = pop else {
            return Err(RelError::Unsupported("dynamic Gremlin select pop".into()));
        };
        if pop == "last" {
            return Ok(());
        }
        match self.gremlin_label_binds.get(label).copied().unwrap_or(0) {
            0 | 1 => Ok(()),
            _ => Err(RelError::Unsupported(format!(
                "Gremlin select({pop}, `{label}`) over a repeatedly bound label needs label history"
            ))),
        }
    }

    /// `select(label)` whose label is an element binding keeps the element
    /// shape (id/label/property columns) under the projection alias, so
    /// later steps (`by('name')`, `out()`, ...) can use it as an element.
    pub(super) fn select_element_projection(
        &self,
        plan: &LogicalPlan,
        alias: &str,
        expr: &IrExpr,
    ) -> RelResult<Option<Vec<Expr>>> {
        let IrExpr::Call { name, args } = expr else {
            return Ok(None);
        };
        if name != "select_key_or_binding_pop" || args.len() != 5 {
            return Ok(None);
        }
        let IrExpr::Binding(label) = &args[1] else {
            return Ok(None);
        };
        if has_binding_shape(plan, label).is_none() || has_exact_col(plan, label) {
            return Ok(None);
        }
        self.check_select_pop(label, &args[4])?;
        let label_always_bound = plan
            .schema()
            .field_with_unqualified_name(&id_col(label))
            .is_ok_and(|field| !field.is_nullable());
        if let IrExpr::Binding(source) = &args[0]
            && !label_always_bound
            && has_binding_shape(plan, source).is_none()
            && has_exact_col(plan, source)
            && !matches!(
                plan_column_type(plan, source),
                Some(
                    DataType::Boolean
                        | DataType::Int8
                        | DataType::Int16
                        | DataType::Int32
                        | DataType::Int64
                        | DataType::Float32
                        | DataType::Float64
                        | DataType::Null
                )
            )
        {
            // The source could be a map holding the key. A bound label wins
            // only on rows where it is bound; for a nullable label binding
            // the unbound rows would need the map lookup instead.
            return Err(RelError::Unsupported(format!(
                "Gremlin select `{label}` over a possibly map-valued traverser"
            )));
        }
        Ok(Some(duplicate_binding_projection_only(plan, label, alias)?))
    }
}
