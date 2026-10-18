//! Cardinality-only collection evaluation. No list reconstruction or element
//! ordering window is needed when the consumer only asks for a filtered size.
use super::*;

// Deliberately exclude arbitrary calls, arithmetic, and nested lambdas. A SQL
// scope is not an evaluation barrier for volatile or throwing expressions.
fn pure(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Lit(_) | IrExpr::Binding(_) | IrExpr::Property { .. } => true,
        IrExpr::List(items) => items.iter().all(pure),
        IrExpr::Not(e) | IrExpr::IsNull(e) | IrExpr::IsNotNull(e) => pure(e),
        IrExpr::Binary { op, lhs, rhs } => {
            matches!(
                op,
                BinaryOp::Eq
                    | BinaryOp::Neq
                    | BinaryOp::Lt
                    | BinaryOp::Lte
                    | BinaryOp::Gt
                    | BinaryOp::Gte
                    | BinaryOp::And
                    | BinaryOp::Or
            ) && pure(lhs)
                && pure(rhs)
        }
        IrExpr::Call { name, args } => {
            matches!(
                name.as_str(),
                "cypher_eq"
                    | "cypher_neq"
                    | "cypher_lt"
                    | "cypher_lte"
                    | "cypher_gt"
                    | "cypher_gte"
            ) && args.iter().all(pure)
        }
        _ => false,
    }
}

fn filtered_size(expr: &IrExpr) -> Option<(&IrExpr, &str, &IrExpr)> {
    let IrExpr::Call { name, args } = expr else {
        return None;
    };
    if name != "size" {
        return None;
    }
    let [source] = args.as_slice() else {
        return None;
    };
    let mut source = source;
    // The frontend represents [x IN xs WHERE p] as an identity map over a
    // filter. Eliminate only that exact identity map, never a throwing body.
    if let IrExpr::ListTransform { list, item, map } = source {
        if !matches!(map.as_ref(), IrExpr::Binding(binding) if binding == item) {
            return None;
        }
        source = list.as_ref();
    }
    let IrExpr::ListFilter {
        list,
        item,
        predicate,
    } = source
    else {
        return None;
    };
    (pure(list) && pure(predicate)).then_some((list.as_ref(), item.as_str(), predicate.as_ref()))
}

impl LoweringContext<'_> {
    pub(super) fn prepare_collection_projection(
        &mut self,
        mut input: LoweredNode,
        items: &[ProjectionItem],
    ) -> RelResult<(LoweredNode, Vec<ProjectionItem>)> {
        let mut rewritten = items.to_vec();
        if self.language != Language::Cypher || self.options.native_values {
            return Ok((input, rewritten));
        }
        for item in &mut rewritten {
            let Some((list, binding, predicate)) = filtered_size(&item.expr) else {
                continue;
            };
            let suffix = self.scan_counter;
            self.scan_counter += 1;
            let reserved = items
                .iter()
                .map(|item| item.alias.clone())
                .chain(std::iter::once(binding.to_owned()))
                .collect();
            let result = unique_internal_alias(
                &input.plan,
                &reserved,
                format!("__w_filter_size_result_{suffix}"),
            );
            input = self.lower_filtered_size(input, list, binding, predicate, &result, suffix)?;
            item.expr = IrExpr::Binding(result);
        }
        Ok((input, rewritten))
    }

    fn lower_filtered_size(
        &self,
        input: LoweredNode,
        source: &IrExpr,
        binding: &str,
        predicate: &IrExpr,
        output: &str,
        suffix: usize,
    ) -> RelResult<LoweredNode> {
        let original = output_fields(&input.plan);
        let list = self.lower_list_operand(&input.plan, source)?;
        let ty = list.get_type(input.plan.schema())?;
        if !matches!(
            ty,
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
        ) {
            return Err(RelError::Unsupported(
                "filtered size requires a homogeneous SQL list".into(),
            ));
        }
        let mut reserved = BTreeSet::from([binding.to_owned(), output.to_owned()]);
        let mut fresh = |base: &str| {
            let alias = unique_internal_alias(&input.plan, &reserved, format!("{base}_{suffix}"));
            reserved.insert(alias.clone());
            alias
        };
        let row = fresh("__w_filter_size_row");
        let list_name = fresh("__w_filter_size_list");
        let total = fresh("__w_filter_size_total");
        let shadow = fresh("__w_filter_size_shadow");
        let numbered = with_distinct_ordinal(input.plan.clone(), &row)?;
        let mut columns = existing_columns(&numbered, &BTreeSet::new());
        columns.push(list.alias(&list_name));
        let listed = collections::unnest_scope(
            LogicalPlanBuilder::from(numbered)
                .project(columns)?
                .build()?,
            format!("__w_sql_cte_filter_size_list_{suffix}"),
        )?;
        let mut columns = existing_columns(&listed, &BTreeSet::from([binding.to_owned()]));
        let shadowed = original.iter().any(|name| name == binding);
        if shadowed {
            columns.push(col_exact(binding).alias(&shadow));
        }
        columns.push(
            datafusion::functions_nested::expr_fn::array_length(col_exact(&list_name))
                .alias(&total),
        );
        columns.push(collections::outer_list(col_exact(&list_name), &ty)?.alias(binding));
        let expanded = collections::unnest_scope(
            LogicalPlanBuilder::from(listed).project(columns)?.build()?,
            format!("__w_sql_cte_filter_size_input_{suffix}"),
        )?;
        let expanded = collections::unnest_input(expanded)?;
        let mut options = datafusion::common::UnnestOptions::default();
        options.preserve_nulls = true;
        let expanded = LogicalPlanBuilder::from(expanded)
            .unnest_column_with_options(Column::new_unqualified(binding), options)?
            .build()?;
        let expanded =
            collections::unnest_scope(expanded, format!("__w_sql_cte_filter_size_items_{suffix}"))?;
        let predicate = self.lower_case_condition(&expanded, predicate)?;
        if !matches!(
            predicate.get_type(expanded.schema())?,
            DataType::Boolean | DataType::Null
        ) {
            return Err(RelError::Unsupported(
                "filtered size predicate requires boolean evaluation".into(),
            ));
        }
        let count = Expr::Case(Case::new(
            None,
            vec![(
                Box::new(col_exact(&total).gt(lit(0_u64))),
                Box::new(Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(Expr::IsTrue(Box::new(predicate))),
                        Box::new(lit(1_i64)),
                    )],
                    Some(Box::new(lit(0_i64))),
                ))),
            )],
            Some(Box::new(lit(0_i64))),
        ));
        let mut groups = original
            .iter()
            .map(|name| {
                col_exact(if shadowed && name == binding {
                    &shadow
                } else {
                    name
                })
            })
            .collect::<Vec<_>>();
        groups.extend([col_exact(&row), col_exact(&total)]);
        let reduced = LogicalPlanBuilder::from(expanded)
            .aggregate(groups, vec![df_sum(count).alias(output)])?
            .build()?;
        // Sort the reduced rows in their original occurrence order. A scope
        // boundary keeps the aggregate expression (and its UNNEST item) in
        // the grouping SELECT rather than inlining it into the outer sort.
        let reduced = collections::unnest_scope(
            reduced, format!("__w_sql_cte_filter_size_reduced_{suffix}"),
        )?;
        let mut columns = original
            .iter()
            .map(|name| {
                if shadowed && name == binding {
                    col_exact(&shadow).alias(name)
                } else {
                    col_exact(name)
                }
            })
            .collect::<Vec<_>>();
        columns.push(
            Expr::Case(Case::new(
                None,
                vec![(
                    Box::new(col_exact(&total).is_null()),
                    Box::new(lit(ScalarValue::Int64(None))),
                )],
                Some(Box::new(col_exact(output))),
            ))
            .alias(output),
        );
        Ok(input.with_plan(
            LogicalPlanBuilder::from(reduced)
                .sort(vec![col_exact(&row).sort(true, false)])?
                .project(columns)?
                .build()?,
        ))
    }
}

impl LoweringContext<'_> {
    /// Inclusive typed range. The shared generator uses inclusive integer
    /// iteration, so i64::MAX never needs an overflowing `stop + 1` rewrite.
    /// Dynamic zero-step validation remains native until its error contract is
    /// available across adapters.
    pub(super) fn lower_dynamic_integer_range(
        &self,
        plan: &LogicalPlan,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        let ([start, stop] | [start, stop, _]) = args else {
            return Err(RelError::Unsupported("range arity".into()));
        };
        let step = match args.get(2) {
            None => lit(1_i64),
            Some(expr) if literal_i64(expr).is_some_and(|value| value != 0) => lit(literal_i64(expr).unwrap()),
            Some(IrExpr::Binding(binding)) => {
                temporal_semantics::finite_values(plan, binding)
                    .filter(|values| !values.is_empty() && values.iter().all(|value| matches!(value,
                        ScalarValue::Int64(Some(value)) if *value != 0
                    ) || matches!(value, ScalarValue::Int32(Some(value)) if *value != 0)))
                    .ok_or_else(|| RelError::Unsupported("dynamic range step requires a proven nonzero integer domain".into()))?;
                self.lower_expr(plan, &IrExpr::Binding(binding.clone()))?.cast_to(&DataType::Int64, plan.schema())?
            }
            _ => return Err(RelError::Unsupported("dynamic range requires a proven nonzero integer step".into())),
        };
        fn stable_bound(expr: &IrExpr) -> bool {
            match expr {
                IrExpr::Binding(_) | IrExpr::Lit(_) => true,
                IrExpr::Binary { lhs, rhs, .. } => stable_bound(lhs) && stable_bound(rhs),
                IrExpr::Call { name, args }
                    if matches!(
                        name.as_str(),
                        "cypher_add" | "cypher_sub" | "cypher_mul" | "cypher_div"
                    ) =>
                {
                    args.iter().all(stable_bound)
                }
                _ => false,
            }
        }
        if !stable_bound(start) || !stable_bound(stop) {
            return Err(RelError::Unsupported(
                "dynamic range bounds require stable typed expressions".into(),
            ));
        }
        let start = self.lower_expr(plan, start)?;
        let stop = self.lower_expr(plan, stop)?;
        let integer = |kind: DataType| matches!(kind, DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 | DataType::UInt8 | DataType::UInt16 | DataType::UInt32);
        if !integer(start.get_type(plan.schema())?) || !integer(stop.get_type(plan.schema())?) {
            return Err(RelError::Unsupported(
                "dynamic range bounds require integer types".into(),
            ));
        }
        Ok(datafusion::functions_nested::expr_fn::gen_series(
            start.cast_to(&DataType::Int64, plan.schema())?,
            stop.cast_to(&DataType::Int64, plan.schema())?,
            step,
        ))
    }
}

/// A deliberately narrow lambda shape: its source is validated string label
/// metadata and its body is a total unary string conversion of the item itself.
pub(super) fn lowercase_label_transform(expr: &IrExpr) -> Option<&str> {
    let IrExpr::ListTransform { list, item, map } = expr else { return None; };
    let IrExpr::Call { name, args } = list.as_ref() else { return None; };
    if name != "cypher_labels" { return None; }
    let [IrExpr::Binding(binding)] = args.as_slice() else { return None; };
    let IrExpr::Call { name, args } = map.as_ref() else { return None; };
    if !["tolower", "lower", "lcase"].iter().any(|candidate| name.eq_ignore_ascii_case(candidate)) { return None; }
    matches!(args.as_slice(), [IrExpr::Binding(value)] if value == item).then_some(binding.as_str())
}

impl LoweringContext<'_> {
    pub(super) fn lower_label_transform(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        let unsupported = || RelError::Unsupported("Label transformation requires immutable string label metadata".into());
        let binding = lowercase_label_transform(expr).ok_or_else(unsupported)?;
        if self.language != Language::Cypher || self.options.mapping.is_some()
            || resolve_column_name(plan, binding).is_some()
            || has_binding_shape(plan, binding) != Some(BindingShape::Node)
        { return Err(unsupported()); }
        let column = node_labels_col(binding);
        let operand = col_exact(&column);
        let ty = operand.get_type(plan.schema())?;
        let DataType::List(field) = &ty else { return Err(unsupported()); };
        if field.data_type() != &DataType::Utf8 { return Err(unsupported()); }
        let domain = temporal_semantics::finite_values(plan, &column).ok_or_else(unsupported)?;
        let mut arms = Vec::new();
        for source in domain {
            if source.is_null() { continue; }
            let ScalarValue::List(array) = &source else { return Err(unsupported()); };
            let items = array.value(0);
            let mut mapped = Vec::new();
            for index in 0..items.len() {
                let ScalarValue::Utf8(Some(text)) = ScalarValue::try_from_array(&items, index)? else {
                    return Err(unsupported());
                };
                // Fold through the shared evaluator, including Unicode case
                // rules, rather than delegating semantics to SQL collation.
                let conversion = IrExpr::Call { name: "tolower".into(), args: vec![IrExpr::Lit(Lit::String(text))] };
                let Some(Expr::Literal(value @ ScalarValue::Utf8(Some(_)), _)) = self.try_constant_fold(&conversion)? else {
                    return Err(unsupported());
                };
                mapped.push(value);
            }
            let mapped = ScalarValue::List(ScalarValue::new_list_nullable(&mapped, &DataType::Utf8));
            arms.push((Box::new(operand.clone().eq(lit(source))), Box::new(lit(mapped))));
        }
        let null = lit(ScalarValue::try_from(&ty)?);
        if arms.is_empty() { return Ok(null); }
        Ok(Expr::Case(Case::new(None, arms, Some(Box::new(null)))))
    }
}
