//! Dispatch Graph IR operators to relational lowering.

use super::*;

impl LoweringContext<'_> {
    pub(super) fn lower_node(&mut self, node: &Node) -> RelResult<LoweredNode> {
        self.memoized_lower_node(node)
    }
    pub(super) fn lower_node_inner(&mut self, node: &Node) -> RelResult<LoweredNode> {
        if let Some(replacement) = pipeline::bind_procedure_call(node, self.graph, &self.options.rule_context)? {
            return self.lower_node(&replacement);
        }
        use Node::*;
        if self.options.language_functions && self.language == Language::Cypher {
            // These need a lossless heterogeneous aggregate representation
            // before the equality fence can be lifted.
            if matches!(node,
                GraphAggregate { aggs, .. } if aggs.iter().any(|a| a.distinct && !matches!(a.kind, AggKind::CountDistinct | AggKind::CollectNonNull))) {
                return Err(RelError::Unsupported("Cypher distinct aggregate requires native values".into()));
            }
        }
        let lowered = match node {
            GraphReturn {
                fields,
                result_form,
                input,
            } => {
                let saved_distincts = self.unordered_distincts.clone();
                if self.language == Language::Cypher && self.correlate_plan.is_none() {
                    let rule = pipeline::descriptor("rel.distinct.order_demand", crate::ir::rules::Stage::RelationalOptimize,
                        crate::ir::rules::RuleKind::EquivalentRewrite, false);
                    let mut session = crate::ir::rules::RuleSession::new(self.options.rule_context.clone());
                    if session.visit(rule).map_err(RelError::RulePipeline)? {
                        if let Some(distinct) = terminal_distinct(input) {
                            self.unordered_distincts.insert(distinct);
                            session.changed(rule, || "terminal bag consumer does not observe first-seen ordering; sorted slices retain their sort".into());
                        }
                    }
                }
                let input = if let GraphCap { labels, input } = input.as_ref() {
                    self.lower_terminal_cap(labels, input)?
                } else if let GraphGroupMap { key, value, output, input } = input.as_ref() {
                    self.lower_group_map(key, value, output, input)?
                } else { self.lower_node(input)? };
                self.unordered_distincts = saved_distincts;
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
                let mut exprs = self.return_projection(&input.plan, fields)?;
                // A RETURN inside a subquery closes its visible scope, but
                // the enclosing apply still needs each outer row's identity.
                // Keep those private keys through DISTINCT/UNION and scalar
                // aggregation; the top-level RETURN never exposes them.
                if self.correlate_plan.is_some() {
                    exprs.extend(apply_correlation_key_columns(&input.plan).iter().map(col_exact));
                }
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
            GraphSparqlGraphNames { .. } => return Err(RelError::Unsupported(
                "RDF graph enumeration requires typed SPARQL lowering".into())),
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
            GraphProcedureCall {name,args,yields,mode:crate::ir::plan::ProcedureMode::Read,input} if self.options.native_values =>
                self.native_procedure(name,args,yields,input.as_deref())?,
            GraphValues {
                bindings,
                rows,
                bulk: _,
            } => self.lower_values(bindings, rows)?,
            // A zero-column EmptyRelation executes correctly in DataFusion,
            // but its SQL unparser emits an empty SELECT list when it is one
            // side of a cross join. A private, non-null dummy column gives
            // the SQL representation a concrete one-row relation.
            // External SQL bindings cannot install an internal one-row table.
            // Project a typed constant so joined SQL has a concrete SELECT list.
            GraphOneRow if self.options.mapping.is_some() => LoweredNode::new(
                LogicalPlanBuilder::empty(true)
                    .project(vec![lit(0_i64).alias("__w_one_row")])?
                    .build()?,
            ),
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
                    && path.is_some() && (!self.options.native_values || length.is_variable_length())
                {
                    return Err(RelError::Unsupported(
                        "Cypher paths require typed SQL IR residual values".into(),
                    ));
                }
                let sql_history = if self.language == Language::Cypher
                    && matches!(match_mode, crate::ir::policy::MatchMode::DifferentRelationships)
                { history.as_deref() } else { None };
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
                        sql_history,
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
                        sql_history,
                    )?
                }
            }
            GraphPathFilter {condition,input,..} if self.options.native_values => self.lower_node(&GraphFilter{condition:condition.clone(),input:input.clone()})?,
            GraphFilter { condition, input } => {
                if let Some(pushed) = self.push_expand_equality(condition, input)? {
                    return self.lower_node(&pushed);
                }
                let input = self.lower_node(input)?;
                let condition = self.lower_case_condition(&input.plan, condition)?;
                if let Some(plan) = self.filter_through_scan_projection(&input.plan, &condition)? {
                    return Ok(input.with_plan(plan));
                }
                // The filter reads projection outputs. SQL WHERE reads input
                // columns, so flattening an alias that shadows an input name
                // changes its meaning (e.g. edge current -> vertex current).
                let filter_input = if matches!(input.plan, LogicalPlan::Projection(_)) {
                    let alias = format!("__w_sql_cte_graph_filter_input_{}", self.scan_counter);
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
                if let Some(lowered) = self.lower_weighted_repeat_count(group, aggs, input)? {
                    return Ok(lowered);
                }
                if !self.options.native_values && self.language == Language::Gremlin && aggs.iter().any(|agg| matches!(agg.kind, AggKind::CollectRows | AggKind::CollectTraversers | AggKind::Min | AggKind::Max | AggKind::Avg)) {
                    return Err(RelError::Unsupported("Gremlin aggregate requires native values and empty-stream semantics".into()));
                }
                if self.language == Language::Gremlin && !self.options.tolerate_internal_path_state
                    && aggs.iter().any(|agg| agg.kind == AggKind::Sum) {
                    return Err(RelError::Unsupported("Gremlin sum traverser state requires a native relational kernel".into()));
                }
                // Count-only consumers cannot observe which representative
                // dedup kept or its order. Preserve correlation partitions.
                let mut input = if group.is_empty() && !aggs.is_empty()
                    && aggs.iter().all(|agg| matches!(agg.kind, AggKind::CountRows | AggKind::CountBulk) && agg.arg.is_none())
                {
                    self.lower_cardinality_input(input)?
                } else { self.lower_node(input)? };
                if aggs.iter().any(|agg| !matches!(agg.kind, AggKind::CountRows | AggKind::CountBulk)
                    && (agg.arg.as_ref().is_some_and(has_temporal_constructor) || input.plan.schema().fields().iter().any(|field| crate::ir::temporal::transport::contains_type(field.data_type())))) {
                    return Err(RelError::Unsupported("Temporal aggregate requires typed runtime evaluation".into()));
                }
                if !group.is_empty() && (group.iter().any(|item| has_temporal_constructor(&item.expr)) || input.plan.schema().fields().iter().any(|field| crate::ir::temporal::transport::contains_type(field.data_type()))) {
                    return Err(RelError::Unsupported("Temporal grouping requires typed runtime evaluation".into()));
                }
                let mut compact_collections = BTreeMap::new();
                for agg in aggs {
                    if let Some(expr) = self.compact_distinct_collect(&input.plan, agg)? {
                        compact_collections.insert(agg.alias.clone(), expr);
                    }
                }
                // COLLECT observes incoming ORDER BY even when its sort keys
                // are projected away. Record that ordering once, before the
                // aggregate, and share it across all collected expressions.
                let collect_ordinal = if self.language == Language::Cypher
                    && !self.options.native_values
                    && aggs.iter().any(|agg| !self.unordered_collects.contains(&(agg as *const AggCall as usize)) && !compact_collections.contains_key(&agg.alias) && crate::ir::rules::aggregate::CollectionContract::for_aggregate(agg)
                        .is_some_and(|contract| contract.order == crate::ir::rules::aggregate::CollectionOrder::PreserveInput))
                {
                    let ordinal = unique_internal_alias(&input.plan, &BTreeSet::new(), "__cypher_collect_ordinal");
                    let numbered = with_distinct_ordinal(input.plan.clone(), &ordinal)?;
                    let guard = unique_internal_alias(&numbered, &BTreeSet::new(), "__cypher_collect_guard");
                    let mut columns = existing_columns_by_name(&numbered, &BTreeSet::new());
                    columns.push(lit(1_i64).alias(guard));
                    input.plan = LogicalPlanBuilder::from(numbered).project(columns)?
                        .alias(format!("__w_sql_cte_collect_input_{}", self.scan_counter))?.build()?;
                    self.scan_counter += 1;
                    Some(ordinal)
                } else { None };
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
                let cypher_keys = (self.options.language_functions && self.language == Language::Cypher)
                    || (self.options.native_values && matches!(self.language, Language::Cypher | Language::Gremlin));
                let mut representatives = Vec::new();
                let mut hidden_keys = BTreeSet::new();
                for (i, item) in group.iter().enumerate() {
                    if let Some(keys) = self.graph_group_keys(&input.plan, &item.expr)? {
                        for (j, key) in keys.into_iter().enumerate() {
                            let alias = unique_internal_alias(&input.plan, &hidden_keys,
                                format!("__cypher_graph_group_{}_{}_{}", self.scan_counter, i, j));
                            hidden_keys.insert(alias.clone());
                            group_exprs.push(key.alias(alias));
                        }
                        let IrExpr::Binding(binding) = &item.expr else { unreachable!() };
                        // The read-only snapshot fixes all payload columns for
                        // an identity. Group by identity only and retain that
                        // identical payload for subsequent traversal/projection.
                        for column in binding_column_names(&input.plan, binding)? {
                            let suffix = column.strip_prefix(binding).expect("binding prefix");
                            representatives.push(datafusion::functions_aggregate::first_last::first_value(
                                col_exact(&column), vec![],
                            ).alias(format!("{}{suffix}", item.alias)));
                        }
                        continue;
                    }
                    let original = self.lower_expr(&input.plan, &item.expr)?;
                    if cypher_keys && !(self.language == Language::Cypher
                        && language_functions::native_grouping_type(&original.get_type(input.plan.schema())?)) {
                        let alias = unique_internal_alias(&input.plan, &hidden_keys, format!("__cypher_group_{}_{}", self.scan_counter, i));
                        hidden_keys.insert(alias.clone());
                        group_exprs.push(if self.options.native_values {
                            native_values::key_expr(original.clone(), self.language == Language::Cypher)
                        } else { language_functions::key(original.clone(), &input.plan)? }.alias(alias));
                        representatives.push(datafusion::functions_aggregate::first_last::first_value(original, vec![]).alias(&item.alias));
                        if self.language == Language::Cypher && self.options.mapping.is_some()
                            && let IrExpr::Binding(binding) = &item.expr
                            && has_binding_shape(&input.plan, binding).is_some()
                        {
                            for column in binding_column_names(&input.plan, binding)? {
                                let suffix = column.strip_prefix(binding).expect("binding prefix");
                                representatives.push(datafusion::functions_aggregate::first_last::first_value(
                                    col_exact(&column), vec![],
                                ).alias(format!("{}{suffix}", item.alias)));
                            }
                        }
                    } else {
                        group_exprs.push(original.alias(item.alias.clone()));
                    }
                }
                self.scan_counter += 1;
                // Mark the first occurrence for each distinct collection independently.
                // Filtering the relation here would change sibling aggregates and
                // lose groups whose collection contains only nulls.
                let mut collect_ranks = std::collections::BTreeMap::new();
                if let Some(ordinal) = &collect_ordinal {
                    let mut windows = Vec::new();
                    for agg in aggs.iter().filter(|agg| agg.kind == AggKind::CollectNonNull && agg.distinct && !compact_collections.contains_key(&agg.alias)) {
                        // These expressions can be repeated safely in the partition
                        // key and aggregate. More complex expressions need explicit
                        // materialization before extending this path.
                        let stable = |expr: &IrExpr| matches!(expr, IrExpr::Binding(_) | IrExpr::Property { .. } | IrExpr::Lit(_))
                            || matches!(expr, IrExpr::Call { name, args } if name == "cypher_subscript"
                                && matches!(args.as_slice(), [IrExpr::Binding(_), IrExpr::Lit(_)]));
                        if !group.iter().all(|item| stable(&item.expr))
                            || !agg.arg.as_ref().is_some_and(stable) {
                            return Err(RelError::Unsupported(format!("Distinct collection expression requires materialization: group={group:?}, arg={:?}", agg.arg)));
                        }
                        let mut partition = apply_correlation_key_columns(&input.plan).iter().map(col_exact).collect::<Vec<_>>();
                        for item in group {
                            if let Some(keys) = self.graph_group_keys(&input.plan, &item.expr)? {
                                partition.extend(keys);
                            } else {
                                partition.push(language_functions::key(self.lower_expr(&input.plan, &item.expr)?, &input.plan)?);
                            }
                        }
                        if let Some(arg)=&agg.arg && let Some(keys)=self.graph_group_keys(&input.plan,arg)? {
                            partition.extend(keys);
                        } else {
                            let value=self.lower_required_agg_arg(&input.plan,&agg.arg)?;
                            partition.push(language_functions::key(value,&input.plan)?);
                        }
                        let rank = unique_internal_alias(&input.plan, &hidden_keys, format!("__cypher_collect_rank_{}", collect_ranks.len()));
                        hidden_keys.insert(rank.clone());
                        windows.push(df_window::row_number()
                            .window_frame(datafusion::logical_expr::WindowFrame::new(None))
                            .partition_by(partition)
                            .order_by(vec![col_exact(ordinal).sort(true, false)])
                            .build()?.alias(&rank));
                        collect_ranks.insert(agg.alias.clone(), rank);
                    }
                    if !windows.is_empty() {
                        let ranked = LogicalPlanBuilder::from(input.plan.clone()).window(windows)?.build()?;
                        let guard = unique_internal_alias(&ranked, &hidden_keys, "__cypher_collect_rank_guard");
                        let mut columns = existing_columns_by_name(&ranked, &BTreeSet::new());
                        columns.push(lit(1_i64).alias(guard));
                        input.plan = LogicalPlanBuilder::from(ranked).project(columns)?
                            .alias(format!("__w_sql_cte_collect_rank_{}", self.scan_counter))?.build()?;
                        self.scan_counter += 1;
                    }
                }
                let agg_calls = aggs;
                let native_drop_empty=self.options.native_values && self.language==Language::Gremlin && group.is_empty()
                    && !aggs.is_empty() && aggs.iter().all(|a|matches!(a.kind,AggKind::Sum|AggKind::Min|AggKind::Max|AggKind::Avg));
                let empty_guard=format!("__orchid_aggregate_count_{}",self.scan_counter);
                let needs_row_count_barrier = native_drop_empty || aggs.iter().any(|agg| {
                    (matches!(agg.kind, AggKind::CountRows | AggKind::CountBulk) && agg.arg.is_none()
                        && !has_simple_count_input(&input.plan))
                        || matches!((&agg.kind, &agg.arg), (AggKind::EngineFunction, Some(IrExpr::Call { args, .. })) if args.is_empty())
                });
                let mut aggs = aggs
                    .iter()
                    .map(|agg| {
                        if let Some(expr) = compact_collections.get(&agg.alias) { return Ok(expr.clone().alias(&agg.alias)); }
                        if self.options.native_values && matches!(self.language,Language::Cypher|Language::Gremlin)
                            && !matches!(agg.kind,AggKind::CountRows|AggKind::CountBulk|AggKind::CountDistinct|AggKind::CountIf|AggKind::EngineFunction|AggKind::StDev|AggKind::StDevP|AggKind::PercentileCont|AggKind::PercentileDisc)
                            && let Some(arg)=&agg.arg {
                            let lowered=self.lower_expr(&input.plan,arg)?;
                            if self.language==Language::Gremlin || agg.kind==AggKind::CollectNonNull || matches!(&lowered,Expr::Literal(v,_) if v.is_null()) || native_values::is_value(&lowered.get_type(input.plan.schema())?) {
                                return Ok(native_values::aggregate_expr(agg,lowered,self.language==Language::Cypher)?.alias(&agg.alias));
                            }
                        }
                        let expr = match agg.kind {
                            AggKind::EngineFunction
                            | AggKind::StDev
                            | AggKind::StDevP
                            | AggKind::PercentileCont
                            | AggKind::PercentileDisc => {
                                self.lower_engine_aggregate(&input.plan, agg)?
                            }
                            AggKind::CountRows | AggKind::CountBulk => match &agg.arg {
                                Some(arg) => df_count(self.lower_count_operand(&input.plan, arg)?),
                                None => count_input_rows(&input.plan),
                            },
                            AggKind::CountDistinct => {
                                let Some(arg) = &agg.arg else {
                                    return Err(RelError::Unsupported(
                                        "count distinct without an argument".into(),
                                    ));
                                };
                                let operand = self.lower_distinct_count_operand(&input.plan, arg)?;
                                datafusion::functions_aggregate::count::count_distinct(
                                    if cypher_keys { language_functions::key(operand, &input.plan)? } else { operand },
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
                                    df_avg(self.lower_avg_arg(&input.plan, &agg.arg)?),
                                    agg.distinct,
                                )?,
                                lit(0.0_f64),
                            ]),
                            AggKind::Avg | AggKind::AvgOrNull => distinct_if(
                                df_avg(self.lower_avg_arg(&input.plan, &agg.arg)?),
                                agg.distinct,
                            )?,
                            AggKind::Min | AggKind::MinOrNull => {
                                let arg = self.lower_extreme_arg(&input.plan, &agg.arg)?;
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
                                let arg = self.lower_extreme_arg(&input.plan, &agg.arg)?;
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
                            AggKind::CollectNonNull => {
                                let ordinal = if self.unordered_collects.contains(&(agg as *const AggCall as usize)) { None }
                                    else { Some(collect_ordinal.as_deref().ok_or_else(|| RelError::Unsupported(
                                        "Distinct collection requires semantic representative selection".into()))?) };
                                self.lower_collect_non_null(&input.plan, agg, ordinal, collect_ranks.get(&agg.alias).map(String::as_str))?
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
                aggs.extend(representatives);
                if native_drop_empty {aggs.push(count_all().alias(&empty_guard));hidden_keys.insert(empty_guard.clone());}
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
                let plan=aggregates::normalize_outputs(plan)?;
                let plan=if native_drop_empty {LogicalPlanBuilder::from(plan).filter(col_exact(&empty_guard).gt(lit(0_i64)))?.build()?} else {plan};
                let plan = if group.is_empty() {
                    gremlin::with_empty_count_defaults(
                        self.correlate_plan.as_ref(),
                        plan,
                        &apply_correlation_key_columns(&input.plan),
                        agg_calls,
                        self.language,
                    )?
                } else {
                    plan
                };
                let plan = if hidden_keys.is_empty() { plan } else {
                    let columns = existing_columns_by_name(&plan, &hidden_keys);
                    LogicalPlanBuilder::from(plan).project(columns)?.build()?
                };
                input.with_plan(plan)
            }
            GraphDistinct { keys, input, .. } => {
                let input = self.lower_node(input)?;
                if input.plan.schema().fields().iter().any(|field| crate::ir::temporal::transport::contains_type(field.data_type())) {
                    return Err(RelError::Unsupported("Temporal distinct requires typed runtime evaluation".into()));
                }
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
                let plan = if self.options.native_values && matches!(self.language, Language::Cypher | Language::Gremlin) {
                    let partition = distinct_partition(&input.plan, &distinct_keys).into_iter()
                        .map(|e| native_values::key_expr(e, self.language == Language::Cypher)).collect();
                    keyed_distinct_partition(input.plan.clone(), partition, barrier_id)?
                } else if self.options.language_functions && self.language == Language::Cypher {
                    let partition = distinct_partition(&input.plan, &distinct_keys).into_iter()
                        .map(|e| language_functions::key(e, &input.plan)).collect::<RelResult<Vec<_>>>()?;
                    let order_rule = pipeline::descriptor("rel.distinct.order_demand", crate::ir::rules::Stage::RelationalOptimize,
                        crate::ir::rules::RuleKind::EquivalentRewrite, false);
                    let mut order_session = crate::ir::rules::RuleSession::new(self.options.rule_context.clone());
                    if self.unordered_distincts.contains(&(node as *const Node as usize))
                        && order_session.visit(order_rule).map_err(RelError::RulePipeline)? {
                        order_session.changed(order_rule, || "query-root order demand retained across SQL island placement".into());
                        unordered_distinct_partition(input.plan.clone(), partition, barrier_id)?
                    } else { keyed_distinct_partition(input.plan.clone(), partition, barrier_id)? }
                } else { keyed_distinct(input.plan.clone(), &distinct_keys, barrier_id)? };
                input.with_plan(plan)
            }
            GraphSort { keys, input } => {
                let input = self.lower_node(input)?;
                // Gremlin inserts an internal source-order marker to make
                // native graph scans deterministic. The relational scan is
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
                if self.options.native_values && matches!(self.language, Language::Cypher | Language::Gremlin)
                    && input.plan.schema().fields().iter().any(|field| native_values::is_value(field.data_type())) {
                    return self.native_sort(input, keys);
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
            GraphSliceExpr { offset, fetch, input } => {
                if offset.iter().chain(fetch.iter()).any(|expr| !expr_is_constant(expr, &[])) {
                    let input = self.lower_node(input)?;
                    if !apply_correlation_key_columns(&input.plan).is_empty() {
                        return Err(RelError::Unsupported("Correlated dynamic slice requires per-invocation bounds".into()));
                    }
                    let lower = |name: &str, expr: &IrExpr| -> RelResult<Expr> {
                        if let Some(bound) = dynamic_slice::bound(expr) { return Ok(bound); }
                        if expr_is_constant(expr, &[]) {
                            let n=crate::ir::runtime::ops::slice::evaluate_slice_bound(name,expr,self.graph)
                                .map_err(|_|RelError::Unsupported("Invalid slice bound requires runtime validation".into()))?;
                            if let Ok(n)=i64::try_from(n) { return Ok(lit(n)); }
                        }
                        Err(RelError::Unsupported("Dynamic slice bound requires runtime validation".into()))
                    };
                    // SQL OFFSET/LIMIT evaluates these independent scalar expressions
                    // once; placing random() in a per-row filter would be incorrect.
                    let skip=offset.as_ref().map(|e|lower("SKIP",e)).transpose()?;
                    let take=fetch.as_ref().map(|e|lower("LIMIT",e)).transpose()?;
                    let plan=LogicalPlanBuilder::from(input.plan.clone()).limit_by_expr(skip,take)?.build()?;
                    return Ok(input.with_plan(plan));
                }
                // A deterministic bound can use the ordinary LIMIT lowering.
                // Keep volatile bounds and evaluation failures on the runtime
                // path, where they retain once-only evaluation/error timing.
                let bound = |name: &str, expr: &IrExpr| -> RelResult<u64> {
                    if !expr_is_constant(expr, &[]) {
                        return Err(RelError::Unsupported("Dynamic slice bound requires runtime evaluation".into()));
                    }
                    crate::ir::runtime::ops::slice::evaluate_slice_bound(name, expr, self.graph)
                        .map_err(|_| RelError::Unsupported("Invalid slice bound requires runtime validation".into()))
                };
                let offset = offset.as_ref().map(|expr| bound("SKIP", expr)).transpose()?.unwrap_or(0);
                let fetch = fetch.as_ref().map(|expr| bound("LIMIT", expr)).transpose()?;
                self.lower_node(&GraphSlice { slice: Slice { offset, fetch, tail: None }, input: input.clone() })?
            }
            GraphSlice { slice, input } => {
                let pure = !crate::ir::analysis::contains_source_mutation(input);
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
                let plan = if pure && *fetch == Some(0) {
                    // Do not construct a zero-sized physical TopK (DF53
                    // panics); a read-only LIMIT 0 has no output rows.
                    LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
                        produce_one_row: false, schema: input.plan.schema().clone(),
                    })
                } else if correlation_keys.is_empty() {
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
            GraphSideEffect { reducer, input, .. } if reducer == "register" && self.options.tolerate_internal_path_state => self.lower_node(input)?,
            GraphGroupMap {key,value,output,input} if self.options.native_values => self.lower_group_map(key,value,output,input)?,
            GraphGroupMap { .. } => return Err(RelError::Unsupported("Intermediate group maps require typed relational values".into())),
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
                until_first,
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
                until_first: *until_first,
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

// A terminal bag has no first-seen ordering contract. Stop at any consumer
// that can observe occurrence order (notably unsorted slices and collections).
pub(super) fn terminal_distinct(node: &Node) -> Option<usize> {
    match node {
        Node::GraphReturn { input, .. } => terminal_distinct(input),
        Node::GraphDistinct { .. } => Some(node as *const Node as usize),
        Node::GraphProject { items, input, .. } if items.iter().all(|item| matches!(item.expr, IrExpr::Binding(_))) => terminal_distinct(input),
        Node::GraphSort { input, .. } | Node::GraphFilter { input, .. } => terminal_distinct(input),
        Node::GraphSlice { input, .. } if matches!(input.as_ref(), Node::GraphSort { .. }) => terminal_distinct(input),
        _ => None,
    }
}
