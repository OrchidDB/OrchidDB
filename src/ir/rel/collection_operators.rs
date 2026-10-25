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
        if !outer && self.language == Language::Gremlin
            && matches!(input_expr, IrExpr::Binding(name) if name == bind)
            && let Node::GraphCurrentProject { expr: IrExpr::Call { name, args }, fields, input: edge_input } = input
            && name == "edge_both"
            && fields.first().is_some_and(|field| field == bind)
            && let [IrExpr::Binding(edge)] = args.as_slice()
        {
            let input = self.lower_node(edge_input)?;
            if has_binding_shape(&input.plan, edge) == Some(BindingShape::Edge) {
                // Expand a single edge row to its two endpoints without hiding
                // the vertex identity/properties in an opaque scalar list.
                // The two-row discriminator preserves self-loop multiplicity
                // and evaluates the upstream relation only once.
                let ordinal = self.scan_counter;
                self.scan_counter += 1;
                let side = format!("__gremlin_endpoint_side_{ordinal}");
                let vertex = format!("__gremlin_both_vertex_{ordinal}");
                let direction = |value| LogicalPlanBuilder::empty(true).project(vec![lit(value).alias(&side)])?.build();
                let directions = LogicalPlanBuilder::from(direction(0_i64)?).union(direction(1_i64)?)?.build()?;
                let frontier = LogicalPlanBuilder::from(input.plan.clone()).cross_join(directions)?.build()?;
                let vertices = self.lower_node_scan(&vertex, &LabelExpr::Any)?;
                let plans = [&frontier, &vertices.plan];
                let outgoing = col_exact(&side).eq(lit(0_i64)).and(binding_pair_eq(&plans, &vertex, &src_id_col(edge), &src_label_col(edge)));
                let incoming = col_exact(&side).eq(lit(1_i64)).and(binding_pair_eq(&plans, &vertex, &dst_id_col(edge), &dst_label_col(edge)));
                let mut columns = existing_columns_excluding_binding(&input.plan, bind, &BTreeSet::from([bind.to_owned()]));
                columns.extend(duplicate_binding_projection_only(&vertices.plan, &vertex, bind)?);
                let plan = LogicalPlanBuilder::from(frontier).join_on(vertices.plan.clone(), JoinType::Inner, vec![outgoing.or(incoming)])?
                    .project(columns)?.build()?;
                let mut result = input.with_plan(plan);
                result.islands.merge(vertices.islands);
                return Ok(result);
            }
        }
        let input = self
            .lower_node(input)
            .map_err(|err| match err {
                RelError::RulePipeline(_) => err,
                err => RelError::Unsupported(format!("GraphUnwind input: {err}")),
            })?;
        if self.options.native_values && self.language == Language::Cypher {
            if let IrExpr::Call { name, args } = input_expr
                && (name.eq_ignore_ascii_case("range") || name == "cypher_range")
                && args.iter().all(|arg|literal_i64(arg).is_some()) {
                return self.lower_range_unwind(input,args,bind,outer);
            }
            return self.lower_unwind_dynamic(input,input_expr,bind,outer);
        }
        if let IrExpr::Call { name, args } = input_expr {
            if name.eq_ignore_ascii_case("range") || name == "cypher_range" {
                if args.iter().all(|arg| literal_i64(arg).is_some()) {
                    return self.lower_range_unwind(input, args, bind, outer);
                }
                return self.lower_unwind_dynamic(input, input_expr, bind, outer);
            }
        }
        let Some(values) = constant_unwind_values(input_expr, outer)? else {
            return self.lower_unwind_dynamic(input, input_expr, bind, outer);
        };
        let rule = super::pipeline::descriptor(
            "rel.collection.singleton_unwind",
            crate::ir::rules::Stage::RelationalOptimize,
            crate::ir::rules::RuleKind::EquivalentRewrite,
            false,
        );
        let mut session = crate::ir::rules::RuleSession::new(self.options.rule_context.clone());
        if session.visit(rule).map_err(RelError::RulePipeline)? {
            let folded = rules::fold_singleton_unwind(&input.plan, bind, &values)
                .or_else(|error| match error {
                    RelError::RulePipeline(_) => Err(error),
                    _ if self.options.native_values => Ok(None),
                    error => Err(error),
                })?;
            if let Some(plan) = folded {
                session.changed(rule, || "one scalar value; fresh binding; parent multiplicity and evaluation retained".into());
                return Ok(input.with_plan(plan));
            }
        }
        let value_rows = values
            .into_iter()
            .map(|value| vec![value])
            .collect::<Vec<_>>();
        let values_plan = self
            .lower_values(&[bind.to_string()], &value_rows)
            .map_err(|err| match err {
                RelError::RulePipeline(_) => err,
                err => RelError::Unsupported(format!("GraphUnwind values: {err}")),
            })?;
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
        if self.language == Language::Gremlin && !self.options.native_values && !self.options.tolerate_internal_path_state {
            return Err(RelError::Unsupported("Gremlin group map requires native runtime values".into()));
        }
        use crate::ir::plan::GroupValue;
        let input = self.lower_node(input)?;
        let key_expr = self.lower_expr(&input.plan, key)?;
        let key_type = key_expr
            .get_type(input.plan.schema())
            .map_err(|err| RelError::Unsupported(format!("group key type: {err}")))?;
        let key_text = if self.options.native_values {key_expr} else {gremlin_tagged_text_expr(key_expr, &key_type)};
        let value_alias = "__gm_value";
        let mut collected_value = false;
        let native_aggregate=|agg:&AggCall| -> RelResult<Expr> {
            if agg.kind == AggKind::CountBulk { return Ok(super::aggregates::count_input_bulk(&input.plan, agg.arg.as_ref().map(|arg| self.lower_count_operand(&input.plan, arg)).transpose()?)); }
            let arg=match &agg.arg {Some(arg)=>self.lower_expr(&input.plan,arg)?,None=>lit(1_i64)};
            native_values::aggregate_expr(agg,arg,false,None)
        };
        let value_agg = match value {
            GroupValue::Aggregate(agg) if self.options.native_values => native_aggregate(agg)?,
            GroupValue::Traversal { traversal, .. } => match traversal.as_ref() {
                Node::GraphAggregate{group,aggs,input:body,..} if self.options.native_values && group.is_empty() && aggs.len()==1 && matches!(body.as_ref(),Node::GraphCorrelate{..})=>native_aggregate(&aggs[0])?,
                Node::GraphAggregate { group, aggs, input: body, .. }
                    if group.is_empty() && aggs.len() == 1
                        && matches!(body.as_ref(), Node::GraphCorrelate { .. })
                        && matches!(aggs[0].kind, AggKind::CountRows | AggKind::CountBulk)
                        && aggs[0].arg.is_none() => if aggs[0].kind == AggKind::CountBulk { super::aggregates::count_input_bulk(&input.plan, None) } else { count_all() },
                Node::GraphProject { items, input: body, .. }
                    if matches!(body.as_ref(), Node::GraphCorrelate { .. }) => {
                    let current = items.iter().find(|item| item.alias == "current")
                        .ok_or_else(|| RelError::Unsupported("group traversal has no current value".into()))?;
                    if !matches!(current.expr, IrExpr::Lit(_)) {
                        return Err(RelError::Unsupported("nonconstant group assignment requires ordered reduction".into()));
                    }
                    df_min(self.lower_expr(&input.plan, &current.expr)?)
                }
                _ => return Err(RelError::Unsupported("correlated group traversal requires native execution".into())),
            },
            GroupValue::CountBulk => super::aggregates::count_input_bulk(&input.plan, None),
            GroupValue::Aggregate(agg) => match agg.kind {
                AggKind::CountBulk => super::aggregates::count_input_bulk(&input.plan, agg.arg.as_ref().map(|arg| self.lower_count_operand(&input.plan, arg)).transpose()?),
                AggKind::CountRows => match &agg.arg {
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
                    df_min(self.lower_extreme_arg(&input.plan, &agg.arg)?)
                }
                AggKind::Max | AggKind::MaxOrNull => {
                    df_max(self.lower_extreme_arg(&input.plan, &agg.arg)?)
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
        let grouped = if self.options.native_values {
            // Group identity belongs to the language, not the carrier's
            // serialized property context. Retain a representative key value.
            LogicalPlanBuilder::from(input.plan.clone()).aggregate(
                vec![native_values::key_expr(key_text.clone(), false).alias("__gm_identity")],
                vec![value_agg.alias(value_alias), datafusion::functions_aggregate::first_last::first_value(key_text, vec![]).alias("__gm_key")],
            )?.build()?
        } else {
            LogicalPlanBuilder::from(input.plan.clone()).aggregate(
                vec![key_text.alias("__gm_key")], vec![value_agg.alias(value_alias)],
            )?.build()?
        };
        if self.options.native_values {
        let scope=format!("__w_sql_cte_group_{}",self.scan_counter);self.scan_counter+=1;
        let plan=collections::unnest_scope(grouped,scope)?;
        let plan=LogicalPlanBuilder::from(plan).aggregate(Vec::<Expr>::new(),vec![df_array_agg(col_exact("__gm_key")).alias("__gm_keys"),df_array_agg(col_exact("__gm_value")).alias("__gm_values")])?.build()?;
        let list=|binding:&str|IrExpr::Call{name:"coalesce".into(),args:vec![IrExpr::binding(binding),IrExpr::List(vec![])]};
        let map=IrExpr::Call{name:"map_literal".into(),args:vec![list("__gm_keys"),list("__gm_values")]};
        let result=self.native_expr(&plan,&map)?;
        return Ok(input.with_plan(LogicalPlanBuilder::from(plan).project(vec![result.alias(output)])?.build()?));
        }
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
        let escape = |text: Expr| {
            let text = df_string::replace(text, lit("\\"), lit("\\\\"));
            let text = df_string::replace(text, lit("\""), lit("\\\""));
            let text = df_string::replace(text, lit("\n"), lit("\\n"));
            let text = df_string::replace(text, lit("\r"), lit("\\r"));
            df_string::replace(text, lit("\t"), lit("\\t"))
        };
        let entry = concat_exprs(vec![
            lit("\""),
            escape(df_core::coalesce(vec![col_exact("__gm_key"), lit("null")])),
            lit("\":\""),
            escape(value_text),
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
        // Preserve the aggregate and its renamed keys as a SQL relation.
        // Without this boundary nested collects can join against the raw
        // UNWIND input, exposing duplicate (and missing renamed) keys.
        let grouped = LogicalPlanBuilder::from(grouped)
            .alias(format!("__w_sql_cte_collect_{barrier_id}"))?
            .build()?;
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
    /// outcomes back to one boolean per original row. The shared scalar
    /// evaluator defines the truth table used by both execution paths.
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
        if !self.options.native_values
            && matches!(predicate,IrExpr::Binding(name) if name==item_binding)
            && let IrExpr::Binding(name)=input_expr
            && let Some(plan)=super::quantifier_scalar::fuse_boolean_collect(&input.plan,&Column::new_unqualified(name),kind,output)?
        { return Ok(input.with_plan(plan)); }
        let original_columns = output_fields(&input.plan);
        let constant_result = if crate::ir::diagnostics::constant_folding_enabled() { match input_expr {
            IrExpr::Lit(Lit::Null) => Some(lit(ScalarValue::Boolean(None))),
            IrExpr::List(items) if items.is_empty() =>
                Some(lit(matches!(kind, QuantifierKind::All | QuantifierKind::None))),
            _ => cypher_lists::literal_quantifier(kind, input_expr, predicate)
                .or_else(|| cypher_lists::closed_quantifier(kind, item_binding, input_expr, predicate, self.graph)),
        } } else { None };
        if let Some(value) = constant_result {
            // An empty or null collection never evaluates its predicate.
            let mut columns = existing_columns(&input.plan, &BTreeSet::new());
            columns.push(value.alias(output));
            let plan = LogicalPlanBuilder::from(input.plan.clone()).project(columns)?.build()?;
            return Ok(input.with_plan(plan));
        }
        let suffix = self.scan_counter;
        self.scan_counter += 1;
        let row_id = format!("__w_quantifier_row_{suffix}");
        let list_col = format!("__w_quantifier_list_{suffix}");
        let input_null = format!("__w_quantifier_null_{suffix}");
        let total = format!("__w_quantifier_total_{suffix}");
        let true_count = format!("__w_quantifier_true_{suffix}");
        let null_count = format!("__w_quantifier_unknown_{suffix}");

        fn has_unscoped_aggregate(plan: &LogicalPlan) -> bool {
            match plan {
                LogicalPlan::Aggregate(_) => true,
                LogicalPlan::Projection(node) => has_unscoped_aggregate(&node.input),
                LogicalPlan::Filter(node) => has_unscoped_aggregate(&node.input),
                LogicalPlan::Sort(node) => has_unscoped_aggregate(&node.input),
                LogicalPlan::Limit(node) => has_unscoped_aggregate(&node.input),
                LogicalPlan::SubqueryAlias(node) => has_unscoped_aggregate(&node.input),
                _ => false,
            }
        }
        let input = if has_unscoped_aggregate(&input.plan) {
            // Complete collection aggregates before assigning an occurrence
            // identity and exposing the same list under a second alias.
            // Otherwise SQL unparse can duplicate ARRAY_AGG in both columns
            // and mix the aggregation and window in one SELECT.
            let mut scope = format!("__w_sql_cte_quantifier_aggregate_{suffix}");
            while has_exact_col(&input.plan, &format!("{scope}_guard")) { scope.push('_'); }
            let plan = collections::unnest_scope(input.plan.clone(), scope)?;
            input.with_plan(plan)
        } else { input };

        // Materialize a dynamic operand once before inspecting its type and
        // expanding it; a volatile expression must not be evaluated twice.
        let operand = self.lower_expr(&input.plan, input_expr)?;
        let carrier = self.options.native_values && native_values::is_value(&operand.get_type(input.plan.schema())?);
        let operand_binding = format!("__w_quantifier_operand_{suffix}");
        let input = if carrier {
            let mut columns = existing_columns(&input.plan, &BTreeSet::new());
            columns.push(operand.alias(&operand_binding));
            let plan = LogicalPlanBuilder::from(input.plan.clone()).project(columns)?.build()?;
            input.with_plan(collections::unnest_scope(plan, format!("__w_sql_cte_quantifier_operand_{suffix}"))?)
        } else { input };
        let materialized_expr = IrExpr::Binding(operand_binding);
        let input_expr = if carrier { &materialized_expr } else { input_expr };
        let list = self.lower_list_operand(&input.plan, input_expr)?;
        // UNWIND accepts scalar singletons, but quantifiers accept only lists.
        // Keep that distinction when the host sees an opaque value carrier.
        let list = if carrier {
            let is_list = self.lower_case_condition(&input.plan, &IrExpr::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(IrExpr::Call { name: "typeof".into(), args: vec![input_expr.clone()] }),
                rhs: Box::new(IrExpr::Lit(Lit::String("LIST".into()))),
            })?;
            let null_list = lit(ScalarValue::try_from(&list.get_type(input.plan.schema())?)?);
            Expr::Case(Case::new(None, vec![(Box::new(is_list), Box::new(list))], Some(Box::new(null_list))))
        } else { list };
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

        if !self.options.native_values
            && matches!(predicate, IrExpr::Lit(Lit::Bool(_) | Lit::Null))
        {
            // Literal predicates depend only on list cardinality, not its
            // items. Avoid UNNEST, per-row identities and grouped reduction.
            // Materialize the operand once, including volatile list sources.
            let mut columns = existing_columns(&input.plan, &BTreeSet::new());
            columns.push(list.alias(&list_col));
            let listed = collections::unnest_scope(
                LogicalPlanBuilder::from(input.plan.clone()).project(columns)?.build()?,
                format!("__w_sql_cte_quantifier_constant_{suffix}"),
            )?;
            let length = datafusion::functions_nested::expr_fn::array_length(col_exact(&list_col));
            let empty = length.clone().eq(lit(0_u64));
            let null_bool = lit(ScalarValue::Boolean(None));
            let value = match (kind, predicate) {
                (QuantifierKind::All, IrExpr::Lit(Lit::Bool(true)))
                | (QuantifierKind::None, IrExpr::Lit(Lit::Bool(false))) => lit(true),
                (QuantifierKind::Any | QuantifierKind::Single, IrExpr::Lit(Lit::Bool(false))) => lit(false),
                (QuantifierKind::All | QuantifierKind::None, IrExpr::Lit(Lit::Bool(_))) => empty,
                (QuantifierKind::Any, IrExpr::Lit(Lit::Bool(true))) => Expr::Not(Box::new(empty)),
                (QuantifierKind::Single, IrExpr::Lit(Lit::Bool(true))) => length.eq(lit(1_u64)),
                (_, IrExpr::Lit(Lit::Null)) => Expr::Case(Case::new(None,
                    vec![(Box::new(empty), Box::new(lit(matches!(kind, QuantifierKind::All | QuantifierKind::None))))],
                    Some(Box::new(null_bool.clone())))),
                _ => unreachable!("literal predicate checked above"),
            };
            let value = Expr::Case(Case::new(None,
                vec![(Box::new(col_exact(&list_col).is_null()), Box::new(null_bool))],
                Some(Box::new(value))));
            let mut columns = original_columns.iter().map(col_exact).collect::<Vec<_>>();
            columns.push(value.alias(output));
            return Ok(input.with_plan(LogicalPlanBuilder::from(listed).project(columns)?.build()?));
        }

        if let Some(value) = self.run_quantifier_rule(&input.plan, kind, item_binding, list.clone(), predicate)? {
            let mut columns = original_columns.iter().map(col_exact).collect::<Vec<_>>();
            columns.push(value.alias(output));
            let plan = LogicalPlanBuilder::from(input.plan.clone()).project(columns)?.build()?;
            return Ok(input.with_plan(plan));
        }

        // Only repeatable SQL sources use shared-base restoration. Native
        // execution and effectful sources retain the single-stream reduction.
        let restore_from_base = !self.options.native_values
            && super::list_comparison::immutable_source_plan(&input.plan)
            && super::list_comparison::repeatable(&input.plan, &list);
        let numbered = if apply::at_most_one(&input.plan) {
            let mut columns = existing_columns(&input.plan, &BTreeSet::new());
            columns.push(lit(1_u64).alias(&row_id));
            LogicalPlanBuilder::from(input.plan.clone()).project(columns)?.build()?
        } else {
            let row_number = df_window::row_number()
                .window_frame(datafusion::logical_expr::WindowFrame::new(None))
                .build()?.alias(row_id.clone());
            LogicalPlanBuilder::from(input.plan.clone()).window(vec![row_number])?.build()?
        };
        let mut list_projection = existing_columns(&numbered, &BTreeSet::new());
        list_projection.push(list.alias(&list_col));
        let listed = LogicalPlanBuilder::from(numbered).project(list_projection)?.build()?;
        let listed = collections::unnest_scope(listed, format!("__w_sql_cte_quantifier_list_{suffix}"))?;
        let list_length = datafusion::functions_nested::expr_fn::array_length(col_exact(&list_col));
        // Scope guards establish SQL boundaries but are not quantifier data.
        // Carry only original bindings and the state used by the next stage.
        let mut base_projection = original_columns.iter().map(col_exact).collect::<Vec<_>>();
        base_projection.extend([col_exact(&row_id), col_exact(&list_col)]);
        base_projection.extend([
            col_exact(&list_col).is_null().alias(input_null.clone()),
            Expr::Cast(Cast::new(Box::new(list_length), DataType::Int64)).alias(total.clone()),
        ]);
        let base = LogicalPlanBuilder::from(listed).project(base_projection)?.build()?;
        let shadowed = original_columns.iter().any(|column| column == item_binding);
        let outer_item = format!("__w_quantifier_outer_item_{suffix}");
        // This marker only enables SQL-emission restoration. The relational
        // plan itself still reduces one stream, including native execution.
        let base = if restore_from_base {
            let mut columns = existing_columns(&base, &BTreeSet::new());
            if shadowed { columns.push(col_exact(item_binding).alias(&outer_item)); }
            LogicalPlanBuilder::from(base).project(columns)?
                .alias(format!("__w_sql_cte_quantifier_base_{suffix}"))?.build()?
        } else { base };
        let mut expanded_projection = original_columns.iter()
            .filter(|column| column.as_str() != item_binding).map(col_exact).collect::<Vec<_>>();
        expanded_projection.extend([col_exact(&row_id), col_exact(&input_null), col_exact(&total)]);
        if shadowed { expanded_projection.push(col_exact(item_binding).alias(&outer_item)); }
        expanded_projection
            .push(collections::outer_list(col_exact(&list_col), &list_type)?.alias(item_binding));
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
        let expanded =
            collections::unnest_scope(expanded, format!("__w_sql_cte_quantifier_items_{suffix}"))?;
        let predicate = self.lower_case_condition(&expanded, predicate)?;
        let predicate_col = format!("__w_quantifier_predicate_{suffix}");
        // The item and expanded list are consumed by the predicate exactly
        // here. Do not propagate them (or UNNEST guards) into grouped reduction.
        let mut evaluated = original_columns.iter().map(|column| {
            col_exact(if shadowed && column == item_binding { &outer_item } else { column })
        }).collect::<Vec<_>>();
        evaluated.extend([col_exact(&row_id), col_exact(&input_null), col_exact(&total)]);
        // Empty/null lists have a synthetic outer-UNNEST row. Do not evaluate
        // a possibly throwing or volatile predicate for that nonexistent item.
        let predicate = Expr::Case(Case::new(None, vec![(
            Box::new(binary(col_exact(&total), BinaryOp::Gt, lit(0_i64))), Box::new(predicate),
        )], Some(Box::new(lit(ScalarValue::Boolean(None))))));
        evaluated.push(predicate.alias(&predicate_col));
        let expanded = LogicalPlanBuilder::from(expanded).project(evaluated)?.build()?;
        let expanded = collections::unnest_scope(expanded, format!("__w_sql_cte_quantifier_predicate_{suffix}"))?;
        let predicate = col_exact(&predicate_col);
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
        // The row number identifies an input occurrence before expansion.
        // Every carried binding and list metadata value is invariant within
        // that occurrence, including the synthetic empty/null-list row. Hash
        // only that identity: grouping whole lists for each expanded element
        // otherwise adds list-length work to every predicate evaluation.
        // Carry values from this same stream; joining a cloned base back would
        // reevaluate volatile inputs and can lose preceding quantifier aliases.
        let mut carried = original_columns.iter().map(|column| {
            if shadowed && column == item_binding { outer_item.clone() } else { column.clone() }
        }).collect::<Vec<_>>();
        carried.extend([input_null.clone(), total.clone()]);
        let mut aggregates = carried.into_iter().map(|column| {
            datafusion::functions_aggregate::first_last::first_value(col_exact(&column), vec![]).alias(column)
        }).collect::<Vec<_>>();
        aggregates.extend([
            df_sum(count_case(true_value)).alias(true_count.clone()),
            df_sum(count_case(unknown_value)).alias(null_count.clone()),
        ]);
        let reduced = LogicalPlanBuilder::from(expanded)
            .aggregate(vec![col_exact(&row_id)], aggregates)?.build()?;

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
        let mut final_projection = original_columns.into_iter().map(|column| {
            if shadowed && column == item_binding { col_exact(&outer_item).alias(column) } else { col_exact(column) }
        }).collect::<Vec<_>>();
        final_projection.push(value.alias(output));
        let reduced = collections::unnest_scope(reduced, format!("__w_sql_cte_quantifier_reduced_{suffix}"))?;
        let plan = LogicalPlanBuilder::from(reduced).project(final_projection)?.build()?;
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
        let list_expr = if let IrExpr::List(_) = input_expr {
            self.lower_list_operand(&input.plan, input_expr)?
        } else {
            self.lower_list_operand(&input.plan, input_expr)?
        };
        let list_expr=if self.options.native_values && self.language==Language::Cypher
            && !matches!(list_expr.get_type(input.plan.schema())?,DataType::List(_)|DataType::LargeList(_)|DataType::FixedSizeList(..)) {
            native_values::list(list_expr)
        } else {list_expr};
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
        let plan =
            collections::unnest_scope(plan, format!("__w_sql_cte_unwind_input_{barrier_id}"))?;
        let plan = collections::unnest_input(plan)?;
        let plan = LogicalPlanBuilder::from(plan)
            .unnest_column_with_options(Column::new_unqualified(bind), options)?
            .build()?;
        let plan = collections::unnest_scope(plan, format!("__w_sql_cte_unwind_{barrier_id}"))?;
        Ok(input.with_plan(plan))
    }
}


#[cfg(test)]
mod rule_contract_tests {
    use super::*;

    #[test]
    fn singleton_unwind_obeys_optional_rule_policy() {
        let graph = crate::language::cypher::preparation::prepare(
            "UNWIND [1] AS x RETURN x", &BTreeMap::new(), None,
        ).unwrap();
        for optimize in [false, true] {
            let mut options = RelBackendOptions::default();
            options.rule_context.optimize = optimize;
            options.rule_context.trace_limit = 128;
            let lowered = RelBackend::with_options(options).lower(&graph, &PropertyGraph::new()).unwrap();
            let selected = lowered.rule_context.trace().iter().any(|trace|
                trace.rule_id == "rel.collection.singleton_unwind" && trace.outcome == "changed");
            assert_eq!(selected, optimize);
        }
    }

    #[tokio::test]
    async fn nested_unwind_preserves_required_rule_failure() {
        crate::ir::diagnostics::without_constant_folding(async {
            // The required scalar rule runs below two UNWIND operators. Its
            // exhausted budget must remain fatal to SQL-island selection.
            let graph = crate::language::cypher::preparation::prepare(
                "WITH toFloat($raw) + toFloat($raw) AS n UNWIND [1] AS x UNWIND [2] AS y RETURN n, x, y",
                &BTreeMap::from([("raw".into(), Value::String("12".into()))]),
                None,
            ).unwrap();
            let mut options = RelBackendOptions::default();
            options.rule_context.optimize = false;
            // Allow catalog binding and required path normalization. The
            // independent SemanticResolve budget then fails on the third
            // temporal candidate inside WITH, below both UNWIND operators.
            options.rule_context.budget.stage_visits = 6;
            let result = RelBackend::with_options(options).lower(&graph, &PropertyGraph::new());
            match result {
                Err(RelError::RulePipeline(error)) => {
                    assert!(error.to_string().contains("exhausted its rewrite budget"));
                    assert!(error.to_string().contains("scalar.temporal"));
                }
                Err(other) => panic!("required failure became a fallback miss: {other}"),
                Ok(_) => panic!("exhausted required stage unexpectedly succeeded"),
            }
        }).await;
    }
}
