//! Dispatch Graph IR operators to relational lowering.

use super::*;

impl LoweringContext<'_> {
    pub(super) fn lower_node(&mut self, node: &Node) -> RelResult<LoweredNode> {
        use Node::*;
        let lowered = match node {
            GraphReturn {
                fields,
                result_form,
                input,
            } => {
                let input = self.lower_node(input)?;
                if self.rdf_typed_terms_used
                    && fields.iter().any(|field| {
                        rdf::binding_identity_columns(field)
                            .iter()
                            .any(|identity| has_exact_col(&input.plan, identity))
                    })
                {
                    return Err(RelError::Unsupported(
                        "typed RDF result terms cannot be represented by ReturnedBatches; project them only after an explicit RDF-term result contract is available".into(),
                    ));
                }
                let exprs = self.return_projection(&input.plan, fields)?;
                let plan = LogicalPlanBuilder::from(input.plan)
                    .project(exprs)?
                    .build()?;
                LoweredNode {
                    plan,
                    islands: input.islands,
                    fields: Some(fields.clone()),
                    result_form: Some(*result_form),
                }
            }
            GraphNodeScan {
                binding, labels, ..
            } => self.lower_node_scan(binding, labels)?,
            GraphRelScan { binding, types, .. } => self.lower_rel_scan(binding, types)?,
            GraphSparqlTriplePattern {
                dataset,
                graph_scope,
                subject,
                predicate,
                object,
                outputs,
            } => rdf::lower_iri_quad_pattern(
                self,
                dataset,
                graph_scope,
                subject,
                predicate,
                object,
                outputs,
            )?,
            GraphValues {
                bindings,
                rows,
                bulk: _,
            } => self.lower_values(bindings, rows)?,
            // A zero-column EmptyRelation executes correctly in DataFusion,
            // but its SQL unparser emits an empty SELECT list when it is one
            // side of a cross join. A private, non-null dummy column gives
            // the SQL representation a concrete one-row relation.
            GraphOneRow => self.scan_batches(
                "one_row",
                vec![RecordBatch::try_new(
                    Arc::new(Schema::new(vec![Field::new(
                        "__w_one_row",
                        DataType::Int64,
                        false,
                    )])),
                    vec![Arc::new(Int64Array::from(vec![0_i64])) as ArrayRef],
                )?],
            )?,
            GraphEmpty => LoweredNode::new(LogicalPlanBuilder::empty(false).build()?),
            GraphCorrelate { .. } => {
                let Some(plan) = &self.correlate_plan else {
                    return Err(RelError::Unsupported(
                        "GraphCorrelate outside GraphApply".into(),
                    ));
                };
                LoweredNode::new(plan.clone())
            }
            GraphBind {
                bind,
                kind,
                expr,
                input,
            } => self.lower_bind(bind, *kind, expr.as_ref(), input)?,
            GraphExpand {
                source,
                target,
                target_mode,
                target_labels,
                rel_binding,
                rel_types,
                dir,
                length,
                path,
                history,
                match_mode,
                input,
                ..
            } => {
                if self.language == Language::Cypher
                    && history.is_some()
                    && matches!(
                        match_mode,
                        crate::ir::policy::MatchMode::DifferentRelationships
                    )
                {
                    // SQL expansion does not carry relationship-history
                    // state across pattern segments. Use the runtime rather
                    // than silently count walks that reuse a relationship.
                    return Err(RelError::Unsupported(
                        "Cypher relationship-history expansion requires runtime".into(),
                    ));
                }
                if length.is_variable_length() {
                    // Cypher represents a variable relationship through its
                    // synthetic path binding, then projects the user-visible
                    // relationship variable from that path. Fixed expands use
                    // `rel_binding` directly.
                    let path_binding = path.as_ref().or(rel_binding.as_ref());
                    self.lower_expand_varlen(
                        input,
                        source,
                        target,
                        *target_mode,
                        target_labels,
                        path_binding,
                        rel_types,
                        *dir,
                        length,
                    )?
                } else {
                    self.lower_expand(
                        input,
                        source,
                        target,
                        *target_mode,
                        target_labels,
                        rel_binding.as_ref(),
                        rel_types,
                        *dir,
                        path.as_deref(),
                    )?
                }
            }
            GraphFilter { condition, input } => {
                let input = self.lower_node(input)?;
                let condition = self.lower_expr(&input.plan, condition)?;
                // The filter reads projection outputs. SQL WHERE reads input
                // columns, so flattening an alias that shadows an input name
                // changes its meaning (e.g. edge current -> vertex current).
                let filter_input = if matches!(input.plan, LogicalPlan::Projection(_)) {
                    let alias = format!("__graph_filter_input_{}", self.scan_counter);
                    self.scan_counter += 1;
                    LogicalPlanBuilder::from(input.plan.clone()).alias(alias)?.build()?
                } else { input.plan.clone() };
                let plan = LogicalPlanBuilder::from(filter_input)
                    .filter(condition)?
                    .build()?;
                input.with_plan(plan)
            }
            GraphProject {
                mode, items, input, ..
            } => self.lower_project(*mode, items, input)?,
            GraphCurrentProject {
                expr,
                fields,
                input,
            } => self.lower_current_project(expr, fields, input)?,
            GraphAggregate {
                group, aggs, input, ..
            } => {
                if self.language == Language::Gremlin && aggs.iter().any(|agg| matches!(agg.kind, AggKind::CollectRows | AggKind::CollectTraversers | AggKind::Min | AggKind::Max | AggKind::Sum | AggKind::Avg)) {
                    return Err(RelError::Unsupported("Gremlin aggregate requires native values and empty-stream semantics".into()));
                }
                let input = self.lower_node(input)?;
                if input
                    .plan
                    .schema()
                    .fields()
                    .iter()
                    .any(|field| field.name().starts_with("__rdf:term:"))
                {
                    return Err(RelError::Unsupported(
                        "aggregation over RDF terms requires an identity-aware aggregate contract"
                            .into(),
                    ));
                }
                if aggs.len() == 1
                    && aggs[0].distinct
                    && matches!(
                        aggs[0].kind,
                        AggKind::CollectRows | AggKind::CollectTraversers
                    )
                {
                    return self.lower_first_distinct_collect(input, group, &aggs[0]);
                }
                let mut group_exprs = apply_correlation_key_columns(&input.plan)
                    .iter()
                    .map(|key| col_exact(key).alias(key))
                    .collect::<Vec<_>>();
                group_exprs.extend(
                    group
                        .iter()
                        .map(|item| {
                            self.lower_expr(&input.plan, &item.expr)
                                .map(|expr| expr.alias(item.alias.clone()))
                        })
                        .collect::<RelResult<Vec<_>>>()?,
                );
                let agg_calls = aggs;
                let needs_row_count_barrier = aggs.iter().any(|agg| {
                    (matches!(agg.kind, AggKind::CountRows | AggKind::CountBulk) && agg.arg.is_none())
                        || matches!((&agg.kind, &agg.arg), (AggKind::EngineFunction, Some(IrExpr::Call { args, .. })) if args.is_empty())
                });
                let aggs = aggs
                    .iter()
                    .map(|agg| {
                        let expr = match agg.kind {
                            AggKind::EngineFunction
                            | AggKind::StDev
                            | AggKind::StDevP
                            | AggKind::PercentileCont
                            | AggKind::PercentileDisc => {
                                self.lower_engine_aggregate(&input.plan, agg)?
                            }
                            AggKind::CountRows | AggKind::CountBulk => match &agg.arg {
                                Some(arg) => df_count(self.lower_expr(&input.plan, arg)?),
                                None => count_input_rows(&input.plan),
                            },
                            AggKind::CountDistinct => {
                                let Some(arg) = &agg.arg else {
                                    return Err(RelError::Unsupported(
                                        "count distinct without an argument".into(),
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
                            // `DISTINCT` changes the result of these, unlike
                            // MIN/MAX where it is a no-op, so it has to be
                            // carried onto the aggregate rather than dropped.
                            // SQL's SUM/AVG are NULL over an empty or all-NULL
                            // group; Kuzu's identity for these is zero. Only
                            // the `OrNull`/plain-AVG kinds want SQL's answer.
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
                                let arg = self.lower_required_agg_arg(&input.plan, &agg.arg)?;
                                if agg
                                    .arg
                                    .as_ref()
                                    .is_some_and(|expr| self.is_blob_property_expr(expr))
                                {
                                    blob_extreme(arg, false)
                                } else {
                                    df_min(arg)
                                }
                            }
                            AggKind::Max | AggKind::MaxOrNull => {
                                let arg = self.lower_required_agg_arg(&input.plan, &agg.arg)?;
                                if agg
                                    .arg
                                    .as_ref()
                                    .is_some_and(|expr| self.is_blob_property_expr(expr))
                                {
                                    blob_extreme(arg, true)
                                } else {
                                    df_max(arg)
                                }
                            }
                            AggKind::CollectRows | AggKind::CollectTraversers => {
                                let arg = self.lower_required_agg_arg(&input.plan, &agg.arg)?;
                                // Kuzu's COLLECT ignores null inputs and
                                // returns NULL when every input is null.
                                // DuckDB's array_agg retains nulls unless the
                                // aggregate carries an explicit filter.
                                let collect = df_array_agg(arg.clone()).filter(arg.is_not_null());
                                if agg.distinct {
                                    // `DISTINCT` collects in first-appearance
                                    // order, which no SQL ordering expresses,
                                    // so leave it to the engine.
                                    distinct_if(collect.build()?, true)?
                                } else {
                                    // Direct evaluation collects in scan
                                    // order; SQL aggregates have no inherent
                                    // order at all. Pin it to the element ids
                                    // so both sides agree.
                                    let keys = scan_order_keys(&input.plan);
                                    if keys.is_empty() {
                                        collect.build()?
                                    } else {
                                        collect.order_by(keys).build()?
                                    }
                                }
                            }
                        };
                        Ok(expr.alias(agg.alias.clone()))
                    })
                    .collect::<RelResult<Vec<_>>>()?;
                // The DataFusion unparser can incorrectly discard the FROM
                // side of COUNT(*) when it contains an UNWIND/cross join.
                // Give that input a real SQL CTE boundary; the SQL wrapper
                // extracts this internal alias before unparsing.
                let aggregate_input = if needs_row_count_barrier {
                    let barrier_id = self.scan_counter;
                    let name = format!("__w_sql_cte_aggregate_{barrier_id}");
                    self.scan_counter += 1;
                    let mut columns = input
                        .plan
                        .schema()
                        .fields()
                        .iter()
                        .map(|field| col_exact(field.name()))
                        .collect::<Vec<_>>();
                    // Keep this projection from being removed as an identity:
                    // a standalone join needs a SELECT list when unparsed.
                    columns.push(lit(1_i64).alias(format!("__w_cte_guard_{barrier_id}")));
                    LogicalPlanBuilder::from(input.plan.clone())
                        .project(columns)?
                        .alias(name)?
                        .build()?
                } else {
                    input.plan.clone()
                };
                let plan = LogicalPlanBuilder::from(aggregate_input)
                    .aggregate(group_exprs, aggs)?
                    .build()?;
                let plan = if group.is_empty() {
                    gremlin::with_empty_count_defaults(
                        self.correlate_plan.as_ref(),
                        plan,
                        &apply_correlation_key_columns(&input.plan),
                        agg_calls,
                    )?
                } else {
                    plan
                };
                input.with_plan(plan)
            }
            GraphDistinct { keys, input, .. } => {
                let input = self.lower_node(input)?;
                let barrier_id = self.scan_counter;
                self.scan_counter += 1;
                let mut identity_keys = Vec::new();
                for key in keys {
                    let aliases = rdf::binding_identity_columns(key);
                    let present = aliases
                        .iter()
                        .filter(|name| has_exact_col(&input.plan, name))
                        .count();
                    if present != 0 && present != aliases.len() {
                        return Err(RelError::Unsupported(format!(
                            "RDF DISTINCT key `{key}` has incomplete term identity metadata"
                        )));
                    }
                    if present == aliases.len() {
                        identity_keys.extend(aliases);
                    }
                }
                let mut distinct_keys = keys.clone();
                distinct_keys.extend(identity_keys);
                let plan = keyed_distinct(input.plan.clone(), &distinct_keys, barrier_id)?;
                input.with_plan(plan)
            }
            GraphSort { keys, input } => {
                let input = self.lower_node(input)?;
                // Gremlin inserts an internal source-order marker to make
                // interpreter scans deterministic. The relational scan is
                // already emitted in that catalog order. Materializing this
                // marker as a SQL SORT can cause DataFusion to discard a
                // later user-facing order().by(...) as redundant.
                if keys.len() == 1
                    && matches!(
                        &keys[0].expr,
                        IrExpr::Call { name, .. } if name == "gremlin_scan_order"
                    )
                {
                    return Ok(input);
                }
                let mut sorts = Vec::new();
                for key in keys {
                    sorts.extend(self.sort_exprs(&input.plan, key)?);
                }
                let plan = LogicalPlanBuilder::from(input.plan.clone())
                    .sort(sorts)?
                    .build()?;
                input.with_plan(plan)
            }
            GraphSlice { slice, input } => {
                let input = self.lower_node(input)?;
                let Slice {
                    offset,
                    fetch,
                    tail,
                } = slice;
                if tail.is_some() {
                    return Err(RelError::Unsupported("tail slice".into()));
                }
                let correlation_keys = apply_correlation_key_columns(&input.plan);
                let plan = if correlation_keys.is_empty() {
                    LogicalPlanBuilder::from(input.plan.clone())
                        .limit(*offset as usize, fetch.map(|n| n as usize))?
                        .build()?
                } else {
                    partitioned_limit(input.plan.clone(), &correlation_keys, *offset, *fetch)?
                };
                input.with_plan(plan)
            }
            GraphJoin {
                kind,
                left,
                right,
                condition,
            } => self.lower_join(*kind, left, right, condition.as_ref())?,
            GraphApply {
                kind,
                correlation,
                outputs,
                left,
                right,
                ..
            } => self.lower_apply(*kind, correlation, outputs, left, right)?,
            GraphUnion {
                all,
                align,
                left,
                right,
            } => self.lower_union(*all, *align, left, right)?,
            GraphChoose {
                selector,
                arms,
                default,
                unmatched,
                input,
                ..
            } => self.lower_choose(selector, arms, default.as_deref(), *unmatched, input)?,
            GraphCoalesce {
                success,
                output,
                correlation,
                input,
                arms,
                ..
            } => self.lower_coalesce(*success, output, correlation, input, arms)?,
            GraphUnwind {
                input_expr,
                bind,
                outer,
                input,
            } => self.lower_unwind(input_expr, bind, *outer, input)?,
            GraphGroupMap {
                key,
                value,
                output,
                input,
            } => self.lower_group_map(key, value, output, input)?,
            GraphCollect {
                value,
                distinct,
                order,
                alias,
                input,
            } => self.lower_collect(value, *distinct, order, alias, input)?,
            GraphQuantifier {
                kind,
                item_binding,
                input_expr,
                predicate,
                output,
                input,
            } => {
                self.lower_quantifier(*kind, item_binding, input_expr, predicate, output, input)?
            }
            GraphRepeat {
                loop_name,
                times,
                emit,
                until,
                until_traversal,
                path,
                prefix_predicate,
                prefix_traversal,
                seed,
                body,
                ..
            } => self.lower_repeat(repeat::RepeatSpec {
                loop_name: loop_name.as_deref(),
                times: *times,
                emit,
                until: until.as_ref(),
                until_traversal: until_traversal.as_deref(),
                path: path.as_deref(),
                prefix_predicate: prefix_predicate.as_ref(),
                prefix_traversal: prefix_traversal.as_deref(),
                seed,
                body,
            })?,
            other => {
                return Err(RelError::Unsupported(format!(
                    "{}",
                    unsupported_node_name(other)
                )));
            }
        };
        Ok(lowered)
    }
}
