//! Expansion.

use super::*;

impl<'a> LoweringContext<'a> {
    /// A mapped edge fixes endpoint labels. Apply that metadata before building
    /// a union of candidate tables; SQL execution must never open unrelated
    /// sources just to discard their labels in a join.
    pub(super) fn mapped_endpoint_labels(&self, labels: &LabelExpr, types: &LabelExpr, dir: Direction) -> RelResult<LabelExpr> {
        let Some(mapping) = &self.options.mapping else { return Ok(labels.clone()); };
        let types = mapping::resolve_names(types, || mapping.rel_types(), "relationship")?;
        let allowed = mapping::resolve_names(labels, || mapping.labels(), "label")?;
        let mut endpoints = BTreeSet::new();
        for kind in types {
            if let Some(edge) = mapping.edge(&kind) {
                if dir != Direction::Out { endpoints.insert(edge.src_label.clone()); }
                if dir != Direction::In { endpoints.insert(edge.dst_label.clone()); }
            }
        }
        Ok(LabelExpr::AnyOf(allowed.into_iter().filter(|label| endpoints.contains(label)).collect()))
    }

    /// Bound endpoints still have to satisfy labels written in the pattern.
    /// Reuse the node scan contract so multi-label managed nodes and mapped
    /// schemas have the same semantics as newly bound endpoints.
    pub(super) fn restrict_existing_target(
        &mut self,
        mut input: LoweredNode,
        target: &str,
        labels: &LabelExpr,
    ) -> RelResult<LoweredNode> {
        if matches!(labels, LabelExpr::Any) { return Ok(input); }
        let binding = format!("__w_target_labels_{}", self.scan_counter);
        self.scan_counter += 1;
        let allowed = self.lower_node_scan(&binding, labels)?;
        let conditions = vec![
            identity_compare(&[&input.plan, &allowed.plan], &id_col(target), BinaryOp::Eq, &id_col(&binding)),
            binary(col_exact(label_col(target)), BinaryOp::Eq, col_exact(label_col(&binding))),
        ];
        input.plan = LogicalPlanBuilder::from(input.plan)
            .join_on(allowed.plan, JoinType::LeftSemi, conditions)?.build()?;
        let columns = input.plan.schema().fields().iter()
            .map(|field| col_exact(field.name()).alias(field.name())).collect::<Vec<_>>();
        input.plan = LogicalPlanBuilder::from(input.plan).project(columns)?.build()?;
        input.islands.merge(allowed.islands);
        Ok(input)
    }

    pub(super) fn lower_bind(
        &mut self,
        bind: &str,
        kind: BindKind,
        expr: Option<&IrExpr>,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let Some(expr) = expr else {
            if has_binding_shape(&input.plan, bind).is_some() || has_exact_col(&input.plan, bind) {
                return Ok(input);
            }
            if has_binding_shape(&input.plan, "current").is_some() {
                let projections = duplicate_binding_projection(&input.plan, "current", bind)?;
                let plan = LogicalPlanBuilder::from(input.plan.clone())
                    .project(projections)?
                    .build()?;
                return Ok(input.with_plan(plan));
            }
            return match kind {
                BindKind::Node | BindKind::Edge => Err(RelError::Unsupported(format!(
                    "metadata bind `{bind}` has no source element"
                ))),
                BindKind::Scalar => Ok(input),
            };
        };

        let mut projections = existing_columns_excluding_binding(
            &input.plan,
            bind,
            &BTreeSet::from([casts::value_union_tag_col(bind)]),
        );
        projections.extend(self.project_item_exprs(&input.plan, bind, expr)?);
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .project(projections)?
            .build()?;
        Ok(input.with_plan(plan))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_expand(
        &mut self,
        input: &Node,
        source: &str,
        target: &str,
        target_mode: TargetMode,
        target_labels: &LabelExpr,
        rel_binding: Option<&String>,
        rel_types: &LabelExpr,
        dir: Direction,
        path: Option<&str>,
        history: Option<&str>,
    ) -> RelResult<LoweredNode> {
        if self.language == Language::Gremlin
            && !self.options.tolerate_internal_path_state
            && path.is_some()
        {
            return Err(RelError::Unsupported("Observed Gremlin paths require native traverser values".into()));
        }
        let input = self.lower_node(input)?;
        let input = if target_mode == TargetMode::Existing {
            self.restrict_existing_target(input, target, target_labels)?
        } else { input };
        if has_binding_shape(&input.plan, source).is_none() {
            return Err(RelError::Unsupported(format!(
                "expand source `{source}` is not an element binding"
            )));
        }

        let rel = rel_binding
            .cloned()
            .unwrap_or_else(|| format!("__rel_{}", self.scan_counter));
        let mut expanded = match dir {
            Direction::Out | Direction::In => self.lower_expand_direction(
                input,
                source,
                target,
                target_mode,
                target_labels,
                Some(&rel),
                rel_types,
                dir,
            ),
            Direction::Both => self.lower_expand_both(
                input,
                source,
                target,
                target_mode,
                target_labels,
                Some(&rel),
                rel_types,
            ),
        }?;
        if let Some(history) = history {
            expanded = self.track_relationship(expanded, history, &rel)?;
        }
        if let Some(path) = path {
            expanded = self.materialize_fixed_path(expanded, path, source, target, &rel)?;
        } else if rel_binding.is_none() {
            let excluded = binding_column_names(&expanded.plan, &rel)?
                .into_iter()
                .collect::<BTreeSet<_>>();
            let projections = existing_columns(&expanded.plan, &excluded);
            let plan = LogicalPlanBuilder::from(expanded.plan.clone())
                .project(projections)?
                .build()?;
            expanded = expanded.with_plan(plan);
        }
        Ok(expanded)
    }

    /// Keep exact edge identities as an SQL list. Labels are part of identity;
    /// membership is element-wise, never a substring match on serialized paths.
    pub(super) fn relationship_key(plan: &LogicalPlan, rel: &str) -> Expr {
        let column = id_col(rel);
        let key = col_exact(column.clone());
        let field = plan.schema().field_with_unqualified_name(&column).expect("relationship id");
        let mut data_type = field.data_type();
        while let DataType::Dictionary(_, value_type) = data_type {
            data_type = value_type;
        }
        let key = if matches!(data_type, DataType::Binary | DataType::LargeBinary
            | DataType::BinaryView | DataType::FixedSizeBinary(_)) {
            // Binary identities may contain arbitrary bytes, not UTF-8.
            datafusion::functions::encoding::expr_fn::encode(
                key, lit("hex"))
        } else {
            cast_utf8(key)
        };
        let label = col_exact(label_col(rel));
        // Length-prefix the label so ("A:B", "c") cannot collide with
        // ("A", "B:c") in relationship-history membership.
        concat_exprs(vec![cast_utf8(df_unicode::character_length(label.clone())), lit(":"), label, lit(":"), key])
    }

    pub(super) fn empty_relationship_history() -> Expr {
        // A typed sentinel avoids an untyped empty array in SQL. Real keys
        // always contain ':', so cannot be empty.
        datafusion::functions_nested::expr_fn::make_array(vec![lit("")])
    }

    pub(super) fn track_relationship(&self, input: LoweredNode, history: &str, rel: &str) -> RelResult<LoweredNode> {
        use datafusion::functions_nested::expr_fn::{array_has, array_append};
        let previous = if has_exact_col(&input.plan, history) { col_exact(history) } else { Self::empty_relationship_history() };
        let key = Self::relationship_key(&input.plan, rel);
        let filtered = LogicalPlanBuilder::from(input.plan.clone())
            .filter(Expr::Not(Box::new(array_has(previous.clone(), key.clone()))))?.build()?;
        let mut projection = existing_columns(&filtered, &BTreeSet::from([history.to_owned()]));
        projection.push(array_append(previous, key).alias(history));
        let plan = LogicalPlanBuilder::from(filtered).project(projection)?.build()?;
        Ok(input.with_plan(plan))
    }

    pub(super) fn materialize_fixed_path(
        &self,
        expanded: LoweredNode,
        path: &str,
        source: &str,
        target: &str,
        rel: &str,
    ) -> RelResult<LoweredNode> {
        let source_display =
            self.cypher_element_display_expr(&expanded.plan, source, BindingShape::Node)?;
        let target_display =
            self.cypher_element_display_expr(&expanded.plan, target, BindingShape::Node)?;
        let rel_display =
            self.cypher_element_display_expr(&expanded.plan, rel, BindingShape::Edge)?;
        let (expanded, rendered) = if has_exact_col(&expanded.plan, path) {
            let with_target = df_string::replace(
                col_exact(path),
                lit("], _RELS: ["),
                concat_exprs(vec![lit(","), target_display, lit("], _RELS: [")]),
            );
            let path_stage = "__w_path_with_target";
            let rel_stage = "__w_path_next_rel";
            let excluded = BTreeSet::from([path_stage.to_string(), rel_stage.to_string()]);
            let mut stage_projection = existing_columns(&expanded.plan, &excluded);
            stage_projection.push(with_target.alias(path_stage));
            stage_projection.push(rel_display.alias(rel_stage));
            let stage_plan = LogicalPlanBuilder::from(expanded.plan.clone())
                .project(stage_projection)?
                .build()?;
            let expanded = expanded.with_plan(stage_plan);
            let prefix = df_unicode::substring(
                col_exact(path_stage),
                lit(1_i64),
                binary(
                    df_unicode::length(col_exact(path_stage)),
                    BinaryOp::Sub,
                    lit(2_i64),
                ),
            );
            (
                expanded,
                concat_exprs(vec![prefix, lit(","), col_exact(rel_stage), lit("]}")]),
            )
        } else {
            (
                expanded,
                concat_exprs(vec![
                    lit("{_NODES: ["),
                    source_display,
                    lit(","),
                    target_display,
                    lit("], _RELS: ["),
                    rel_display,
                    lit("]}"),
                ]),
            )
        };
        let previous_len = path_len_col(path);
        let path_len = if has_exact_col(&expanded.plan, &previous_len) {
            binary(col_exact(&previous_len), BinaryOp::Add, lit(1_i64))
        } else {
            lit(1_i64)
        };
        let excluded = BTreeSet::from([
            path.to_string(),
            previous_len.clone(),
            "__w_path_with_target".to_string(),
            "__w_path_next_rel".to_string(),
        ]);
        let mut projections = existing_columns(&expanded.plan, &excluded);
        projections.push(rendered.alias(path));
        projections.push(path_len.alias(previous_len));
        let plan = LogicalPlanBuilder::from(expanded.plan.clone())
            .project(projections)?
            .build()?;
        Ok(expanded.with_plan(plan))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_expand_both(
        &mut self,
        input: LoweredNode,
        source: &str,
        target: &str,
        target_mode: TargetMode,
        target_labels: &LabelExpr,
        rel_binding: Option<&String>,
        rel_types: &LabelExpr,
    ) -> RelResult<LoweredNode> {
        let rel = rel_binding
            .cloned()
            .unwrap_or_else(|| format!("__rel_{}", self.scan_counter));
        let edge_scan = self.lower_rel_scan(&rel, rel_types)?;

        // Orient the edge relation, not the upstream rows. OR predicates on
        // endpoints force nested-loop joins on large graphs; these traversal
        // columns give both joins ordinary equality keys. Keep the physical
        // edge endpoints untouched for relationship values and path rendering.
        let from_id = format!("{rel}__traverse_from_id");
        let from_label = format!("{rel}__traverse_from_label");
        let to_id = format!("{rel}__traverse_to_id");
        let to_label = format!("{rel}__traverse_to_label");
        let field_type = |name: String| {
            edge_scan
                .plan
                .schema()
                .field_with_unqualified_name(&name)
                .map(|field| field.data_type().clone())
                .ok()
        };
        let endpoint_type = match (field_type(src_id_col(&rel)), field_type(dst_id_col(&rel))) {
            (Some(src), Some(dst)) => common_identity_type(&src, &dst),
            _ => None,
        };
        let orient = |reverse: bool| -> RelResult<LogicalPlan> {
            let mut builder = LogicalPlanBuilder::from(edge_scan.plan.clone());
            if reverse && self.language != Language::Gremlin {
                // Cypher undirected matching emits a physical self-loop once.
                // Gremlin both()/bothE() concatenate outgoing and incoming
                // traversers, so the same self-loop participates twice.
                builder = builder.filter(
                    identity_compare(&[&edge_scan.plan], &src_id_col(&rel), BinaryOp::Neq, &dst_id_col(&rel))
                        .or(col_exact(src_label_col(&rel)).not_eq(col_exact(dst_label_col(&rel)))),
                )?;
            }
            let mut projection = existing_columns(&edge_scan.plan, &BTreeSet::new());
            let (src_id, src_label, dst_id, dst_label) = if reverse {
                (
                    dst_id_col(&rel),
                    dst_label_col(&rel),
                    src_id_col(&rel),
                    src_label_col(&rel),
                )
            } else {
                (
                    src_id_col(&rel),
                    src_label_col(&rel),
                    dst_id_col(&rel),
                    dst_label_col(&rel),
                )
            };
            // Both orientations feed one union, so endpoints of different
            // identity types (e.g. text people, integer companies) share one.
            let endpoint = |name: String| match &endpoint_type {
                Some(target) => Expr::Cast(Cast::new(Box::new(col_exact(name)), target.clone())),
                None => col_exact(name),
            };
            projection.extend([
                endpoint(src_id).alias(&from_id),
                col_exact(src_label).alias(&from_label),
                endpoint(dst_id).alias(&to_id),
                col_exact(dst_label).alias(&to_label),
            ]);
            Ok(builder.project(projection)?.build()?)
        };
        let oriented = LogicalPlanBuilder::from(orient(false)?)
            .union(orient(true)?)?
            .build()?;
        let source_join = vec![binding_pair_eq(&[&input.plan, &oriented], source, &from_id, &from_label)];
        let mut joined = LogicalPlanBuilder::from(input.plan.clone())
            .join_on(oriented, JoinType::Inner, source_join)?
            .build()?;

        match target_mode {
            TargetMode::Existing => {
                let opposite = binding_pair_eq(&[&joined], target, &to_id, &to_label);
                joined = LogicalPlanBuilder::from(joined).filter(opposite)?.build()?;
            }
            TargetMode::BindNew
            | TargetMode::ReplaceCurrent
            | TargetMode::ReplaceCurrentAndBindLabel
            | TargetMode::BindNewOrReplaceCurrent => {
                let target_scan_binding = if has_binding_shape(&joined, target).is_some() {
                    format!("__target_{}", self.scan_counter)
                } else {
                    target.to_string()
                };
                let labels = self.mapped_endpoint_labels(target_labels, rel_types, Direction::Both)?;
                let target_scan = self.lower_node_scan(&target_scan_binding, &labels)?;
                let target_join = vec![binding_pair_eq(
                    &[&joined, &target_scan.plan],
                    &target_scan_binding,
                    &to_id,
                    &to_label,
                )];
                joined = LogicalPlanBuilder::from(joined)
                    .join_on(target_scan.plan, JoinType::Inner, target_join)?
                    .build()?;
                if target_scan_binding != target {
                    let mut projections = existing_columns_excluding_bindings(
                        &joined,
                        &[target, target_scan_binding.as_str()],
                    );
                    projections.extend(duplicate_binding_projection_only(
                        &joined,
                        &target_scan_binding,
                        target,
                    )?);
                    joined = LogicalPlanBuilder::from(joined)
                        .project(projections)?
                        .build()?;
                }
            }
        }

        let projections = existing_columns(
            &joined,
            &BTreeSet::from([from_id, from_label, to_id, to_label]),
        );
        let joined = LogicalPlanBuilder::from(joined)
            .project(projections)?
            .build()?;
        let mut islands = input.islands;
        islands.merge(edge_scan.islands);
        Ok(LoweredNode {
            plan: joined,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_expand_direction(
        &mut self,
        input: LoweredNode,
        source: &str,
        target: &str,
        target_mode: TargetMode,
        target_labels: &LabelExpr,
        rel_binding: Option<&String>,
        rel_types: &LabelExpr,
        dir: Direction,
    ) -> RelResult<LoweredNode> {
        let rel = rel_binding
            .cloned()
            .unwrap_or_else(|| format!("__rel_{}", self.scan_counter));
        let edge_scan = self.lower_rel_scan(&rel, rel_types)?;

        let (edge_source_id, edge_source_label, edge_target_id, edge_target_label) = match dir {
            Direction::Out => (
                src_id_col(&rel),
                src_label_col(&rel),
                dst_id_col(&rel),
                dst_label_col(&rel),
            ),
            Direction::In => (
                dst_id_col(&rel),
                dst_label_col(&rel),
                src_id_col(&rel),
                src_label_col(&rel),
            ),
            Direction::Both => unreachable!("both expands are split before lowering"),
        };

        let source_join = vec![
            identity_compare(
                &[&input.plan, &edge_scan.plan],
                &id_col(source),
                BinaryOp::Eq,
                &edge_source_id,
            ),
            binary(
                col_exact(label_col(source)),
                BinaryOp::Eq,
                col_exact(edge_source_label),
            ),
        ];
        let mut joined = LogicalPlanBuilder::from(input.plan.clone())
            .join_on(edge_scan.plan.clone(), JoinType::Inner, source_join)?
            .build()?;

        match target_mode {
            TargetMode::Existing => {
                let filters = vec![
                    identity_compare(&[&joined], &id_col(target), BinaryOp::Eq, &edge_target_id),
                    binary(
                        col_exact(label_col(target)),
                        BinaryOp::Eq,
                        col_exact(edge_target_label),
                    ),
                ];
                let filter = filters.into_iter().reduce(Expr::and).expect("filters");
                joined = LogicalPlanBuilder::from(joined).filter(filter)?.build()?;
            }
            TargetMode::BindNew
            | TargetMode::ReplaceCurrent
            | TargetMode::ReplaceCurrentAndBindLabel
            | TargetMode::BindNewOrReplaceCurrent => {
                let target_scan_binding = if has_binding_shape(&joined, target).is_some() {
                    format!("__target_{}", self.scan_counter)
                } else {
                    target.to_string()
                };
                let labels = self.mapped_endpoint_labels(target_labels, rel_types, dir)?;
                let target_scan = self.lower_node_scan(&target_scan_binding, &labels)?;
                let target_join = vec![
                    identity_compare(
                        &[&joined, &target_scan.plan],
                        &id_col(&target_scan_binding),
                        BinaryOp::Eq,
                        &edge_target_id,
                    ),
                    binary(
                        col_exact(label_col(&target_scan_binding)),
                        BinaryOp::Eq,
                        col_exact(edge_target_label),
                    ),
                ];
                joined = LogicalPlanBuilder::from(joined)
                    .join_on(target_scan.plan, JoinType::Inner, target_join)?
                    .build()?;
                if target_scan_binding != target {
                    let mut projections = existing_columns_excluding_bindings(
                        &joined,
                        &[target, target_scan_binding.as_str()],
                    );
                    projections.extend(duplicate_binding_projection_only(
                        &joined,
                        &target_scan_binding,
                        target,
                    )?);
                    joined = LogicalPlanBuilder::from(joined)
                        .project(projections)?
                        .build()?;
                }
            }
        }

        let mut islands = input.islands;
        islands.merge(edge_scan.islands);
        Ok(LoweredNode {
            plan: joined,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

}
