//! Projection.

use super::*;

impl<'a> LoweringContext<'a> {
    pub(super) fn return_projection(
        &self,
        plan: &LogicalPlan,
        fields: &[String],
    ) -> RelResult<Vec<Expr>> {
        let mut projections = Vec::new();
        for field in fields {
            if self.language == Language::Gremlin
                && let Some(mixed) = gremlin::mixed_current_display_expr(plan, field)?
            {
                projections.push(mixed.alias(field));
            } else if has_exact_col(plan, field) {
                projections.push(col_exact(field));
            } else if let Some(star_cols) = star_expansion_columns(plan, field) {
                projections.extend(star_cols);
            } else if let Some(shape) = has_binding_shape(plan, field) {
                if self.language == Language::Cypher && self.options.mapping.is_none() {
                    return Err(RelError::Unsupported("Cypher returned graph values require native identity".into()));
                }
                if self.language == Language::Gremlin {
                    projections.push(gremlin_element_display_expr(plan, field)?.alias(field));
                    continue;
                }
                projections.push(
                    self.cypher_element_display_expr(plan, field, shape)?
                        .alias(field),
                );
            } else {
                return Err(RelError::Unsupported(format!(
                    "return field `{field}` is not available relationally"
                )));
            }
        }
        Ok(projections)
    }

    pub(super) fn project_item_exprs(
        &self,
        plan: &LogicalPlan,
        alias: &str,
        expr: &IrExpr,
    ) -> RelResult<Vec<Expr>> {
        if self.options.mapping.is_some() || self.options.tolerate_internal_path_state {
            // A label bound once is fully represented by its relational element
            // columns. Repeated labels still require real traverser history.
            if let Some(label) = alias.strip_prefix("__gremlin_select_history_")
                && !label.is_empty()
                && matches!(expr, IrExpr::Call { name, .. } if name == "select_history_append")
            {
                return Ok(Vec::new());
            }
            if alias == "__path_labels"
                && matches!(expr, IrExpr::Call { name, .. } if name == "path_attach_label")
            {
                return Ok(Vec::new());
            }
        }
        if self.options.tolerate_internal_path_state
            && alias.starts_with("__gremlin_select_history_")
            && matches!(expr, IrExpr::Call { name, .. } if name == "select_history_append")
        {
            return Err(RelError::Unsupported(
                "Gremlin label history requires traverser state".into(),
            ));
        }
        if self.language == Language::Gremlin
            && alias.starts_with("select_source_")
            && matches!(expr, IrExpr::Binding(binding) if binding == "current")
        {
            return Ok(Vec::new());
        }
        if self.language == Language::Gremlin && matches!(expr, IrExpr::Case { .. }) {
            fn has_element_result(expr: &IrExpr, plan: &LogicalPlan) -> bool {
                match expr {
                    IrExpr::Binding(binding) => has_binding_shape(plan, binding).is_some(),
                    IrExpr::Case { arms, otherwise } => arms.iter().any(|(_, value)| has_element_result(value, plan))
                        || otherwise.as_deref().is_some_and(|value| has_element_result(value, plan)),
                    _ => false,
                }
            }
            if has_element_result(expr, plan) {
                return Err(RelError::Unsupported(
                    "conditional Gremlin element projection requires native values".into(),
                ));
            }
        }
        if let IrExpr::Binding(binding) = expr {
            if has_binding_shape(plan, binding).is_some() {
                return duplicate_binding_projection_only(plan, binding, alias);
            }
        }
        if self.language == Language::Gremlin
            && let Some(projection) = self.select_element_projection(plan, alias, expr)?
        {
            return Ok(projection);
        }
        // Edge endpoint helpers are element-valued expressions. Preserve
        // their node shape as separate id/label columns so the following
        // vertex subgraph join can attach the user's mapped properties.
        if let IrExpr::Call { name, args } = expr
            && matches!(name.as_str(), "edge_src" | "edge_dst")
            && let [IrExpr::Binding(binding)] = args.as_slice()
            && has_binding_shape(plan, binding) == Some(BindingShape::Edge)
        {
            let (id, label) = if name == "edge_src" {
                (src_id_col(binding), src_label_col(binding))
            } else {
                (dst_id_col(binding), dst_label_col(binding))
            };
            return Ok(vec![
                col_exact(id).alias(id_col(alias)),
                col_exact(label).alias(label_col(alias)),
            ]);
        }
        if let IrExpr::Call { name, args } = expr
            && matches!(name.as_str(), "make_map" | "map")
            && args.len() % 2 == 0
            && args
                .chunks(2)
                .all(|pair| matches!(pair[0], IrExpr::Lit(Lit::String(_))))
        {
            if matches!(self.language, Language::Cypher | Language::Gremlin) {
                return Err(RelError::Unsupported(
                    "Map projection requires native runtime values".into(),
                ));
            }
            let rendered = if name == "make_map" {
                self.lower_make_map(plan, args)?
            } else {
                self.lower_cypher_map(plan, args)?
            };
            let mut projections = vec![rendered.alias(alias)];
            for pair in args.chunks(2) {
                let IrExpr::Lit(Lit::String(key)) = &pair[0] else {
                    return Err(RelError::Unsupported("dynamic make_map key".into()));
                };
                let value = if let IrExpr::List(items) = &pair[1] {
                    datafusion::functions_nested::expr_fn::make_array(
                        items
                            .iter()
                            .map(|item| self.lower_expr(plan, item))
                            .collect::<RelResult<Vec<_>>>()?,
                    )
                } else {
                    self.lower_expr(plan, &pair[1])?
                };
                projections.push(value.alias(prop_col(alias, key)));
            }
            return Ok(projections);
        }
        // `RETURN a.*` expands into one column per known property, in the
        // catalog's schema order, so the row formatter renders each value
        // with its native type (floats keep six decimals, booleans print
        // True/False, nulls print empty).
        if let IrExpr::Call { name, args } = expr
            && name == "cypher_property_star"
            && args.len() == 1
            && let IrExpr::Binding(binding) = &args[0]
            && let Some(shape) = has_binding_shape(plan, binding)
        {
            let keys = self.element_property_keys(plan, binding, shape);
            if keys.is_empty() {
                // Element with no properties: `RETURN x.*` prints one empty
                // cell per row.
                return Ok(vec![
                    lit(ScalarValue::Utf8(None)).alias(format!("{alias}{STAR_SEP}")),
                ]);
            }
            return Ok(keys
                .iter()
                .map(|key| {
                    col_exact(prop_col(binding, key)).alias(format!("{alias}{STAR_SEP}{key}"))
                })
                .collect());
        }
        if let IrExpr::Call { name, args } = expr
            && name == "cypher_property_star"
            && let [
                IrExpr::Property {
                    binding,
                    name: property,
                    ..
                },
            ] = args.as_slice()
        {
            let prefix = format!("{}__w_struct__", prop_col(binding, property));
            let fields = output_fields(plan)
                .into_iter()
                .filter_map(|column| {
                    column
                        .strip_prefix(&prefix)
                        .map(|field| (column.clone(), field.to_string()))
                })
                .collect::<Vec<_>>();
            if !fields.is_empty() {
                return Ok(fields
                    .into_iter()
                    .map(|(column, field)| {
                        col_exact(column).alias(format!("{alias}{STAR_SEP}{field}"))
                    })
                    .collect());
            }
        }
        if self.options.tolerate_internal_path_state
            && alias == "__path"
            && matches!(expr, IrExpr::Call { name, .. } if name.starts_with("path_"))
        {
            return Ok(vec![lit(ScalarValue::Utf8(None)).alias(alias)]);
        }
        if let IrExpr::Call { name, args } = expr
            && name == "recursive_relationship_path"
        {
            // The variable-length expand materializes the path under the
            // binding's own name, one branch per hop count.
            if let Some(IrExpr::Binding(binding)) = args.first()
                && has_exact_col(plan, binding)
            {
                // The relationship variable excludes the path's endpoints.
                let inner = path_inner_col(binding);
                let rendered = if has_exact_col(plan, &inner) {
                    col_exact(&inner)
                } else {
                    col_exact(binding)
                };
                let mut projections = vec![rendered.alias(alias)];
                let hops = path_len_col(binding);
                if has_exact_col(plan, &hops) {
                    projections.push(col_exact(hops).alias(path_len_col(alias)));
                }
                return Ok(projections);
            }
            // Otherwise the path was not materialized. A null placeholder
            // keeps count/exists-style queries over paths working, while
            // queries that actually print the path surface as mismatches.
            if self.options.tolerate_internal_path_state {
                return Ok(vec![lit(ScalarValue::Utf8(None)).alias(alias)]);
            }
        }
        if let Some(tags) = self.projected_union_tags(plan, expr)? {
            return Ok(vec![
                self.lower_expr(plan, expr)?.alias(alias),
                tags.alias(casts::value_union_tag_col(alias)),
            ]);
        }
        Ok(vec![self.lower_expr(plan, expr)?.alias(alias)])
    }
}

impl LoweringContext<'_> {
    pub(super) fn lower_project(
        &mut self,
        mode: ProjectMode,
        items: &[ProjectionItem],
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let projected_aliases = items
            .iter()
            .map(|item| item.alias.as_str())
            .collect::<BTreeSet<_>>();
        if mode == ProjectMode::ReplaceScope
            && input.plan.schema().fields().iter().any(|field| {
                field.name().starts_with("__rdf:term:")
                    && !projected_aliases.contains(field.name().as_str())
            })
        {
            return Err(RelError::Unsupported(
                "SPARQL projection would discard RDF term identity needed for typed results or joins".into(),
            ));
        }
        let mut aliases = projection_aliases(items);
        // A re-projected alias replaces its union-tag companion too, or the
        // carried and the re-emitted companion would share a name.
        aliases.extend(
            items
                .iter()
                .map(|item| casts::value_union_tag_col(&item.alias)),
        );
        if mode == ProjectMode::PreserveVisible
            && aliases
                .iter()
                .all(|alias| !has_exact_col(&input.plan, alias))
            && items.iter().all(|item| {
                self.project_item_exprs(&input.plan, &item.alias, &item.expr)
                    .is_ok_and(|exprs| exprs.is_empty())
            })
        {
            return Ok(input);
        }
        let mut projections = match mode {
            ProjectMode::PreserveVisible => existing_columns(&input.plan, &aliases),
            ProjectMode::ReplaceScope => apply_correlation_key_columns(&input.plan)
                .iter()
                .map(col_exact)
                .collect(),
            ProjectMode::ReplaceCurrent => {
                let mut excluded = aliases;
                excluded.insert("current".to_string());
                existing_columns_excluding_binding(&input.plan, "current", &excluded)
            }
        };
        for item in items {
            projections.extend(self.project_item_exprs(&input.plan, &item.alias, &item.expr)?);
        }
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .project(projections)?
            .build()?;
        let input = input.with_plan(plan);
        if self.language == Language::Gremlin {
            self.hydrate_endpoint_projections(input, items)
        } else {
            Ok(input)
        }
    }
}

impl LoweringContext<'_> {
    pub(super) fn lower_current_project(
        &mut self,
        expr: &IrExpr,
        fields: &[String],
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let alias = fields.first().map(String::as_str).unwrap_or("current");
        let mut projections = apply_correlation_key_columns(&input.plan)
            .iter()
            .map(col_exact)
            .collect::<Vec<_>>();
        let item_projections = self.project_item_exprs(&input.plan, alias, expr)?;
        if item_projections.is_empty() {
            return Err(RelError::Unsupported(
                "current projection produced no relational columns".into(),
            ));
        }
        projections.extend(item_projections);
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .project(projections.split_off(0))?
            .filter(col_exact(alias).is_not_null())?
            .build()?;
        Ok(input.with_plan(plan))
    }
}

impl LoweringContext<'_> {
    pub(super) fn sort_exprs(
        &self,
        plan: &LogicalPlan,
        key: &crate::ir::plan::SortKey,
    ) -> RelResult<Vec<datafusion::logical_expr::SortExpr>> {
        let asc = matches!(key.dir, SortDir::Asc);
        let nulls_first = matches!(key.nulls, NullsOrder::First);
        if let IrExpr::Call { name, args } = &key.expr
            && name == "gremlin_scan_order"
            && let Some(IrExpr::Binding(binding)) = args.first()
        {
            return Ok(vec![
                col_exact(label_col(binding)).sort(asc, nulls_first),
                col_exact(id_col(binding)).sort(asc, nulls_first),
            ]);
        }
        if let IrExpr::Call { name, args } = &key.expr
            && name == "gremlin_order_key"
            && let Some(IrExpr::Binding(binding)) = args.first()
        {
            // A relational column has one Arrow type, so its TinkerPop type
            // rank is constant within the sort. Elements order by their
            // provider scan key; scalars order by their native SQL value.
            if has_binding_shape(plan, binding).is_some() {
                return Ok(vec![
                    col_exact(label_col(binding)).sort(asc, nulls_first),
                    col_exact(id_col(binding)).sort(asc, nulls_first),
                ]);
            }
            if let Some(column) = resolve_column_name(plan, binding) {
                return Ok(vec![col_exact(column).sort(asc, nulls_first)]);
            }
        }
        self.lower_expr(plan, &key.expr)
            .map(|expr| vec![expr.sort(asc, nulls_first)])
    }
}
