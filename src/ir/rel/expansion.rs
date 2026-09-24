//! Expansion.

use super::*;

impl<'a> LoweringContext<'a> {
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
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
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
        let orient = |reverse: bool| -> RelResult<LogicalPlan> {
            let mut builder = LogicalPlanBuilder::from(edge_scan.plan.clone());
            if reverse && self.language != Language::Gremlin {
                // Cypher undirected matching emits a physical self-loop once.
                // Gremlin both()/bothE() concatenate outgoing and incoming
                // traversers, so the same self-loop participates twice.
                builder = builder.filter(
                    col_exact(src_id_col(&rel))
                        .not_eq(col_exact(dst_id_col(&rel)))
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
            projection.extend([
                col_exact(src_id).alias(&from_id),
                col_exact(src_label).alias(&from_label),
                col_exact(dst_id).alias(&to_id),
                col_exact(dst_label).alias(&to_label),
            ]);
            Ok(builder.project(projection)?.build()?)
        };
        let oriented = LogicalPlanBuilder::from(orient(false)?)
            .union(orient(true)?)?
            .build()?;
        let source_join = vec![binding_pair_eq(source, &from_id, &from_label)];
        let mut joined = LogicalPlanBuilder::from(input.plan.clone())
            .join_on(oriented, JoinType::Inner, source_join)?
            .build()?;

        match target_mode {
            TargetMode::Existing => {
                let opposite = binding_pair_eq(target, &to_id, &to_label);
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
                let target_scan = self.lower_node_scan(&target_scan_binding, target_labels)?;
                let target_join = vec![binding_pair_eq(&target_scan_binding, &to_id, &to_label)];
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
            binary(
                col_exact(id_col(source)),
                BinaryOp::Eq,
                col_exact(edge_source_id),
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
                    binary(
                        col_exact(id_col(target)),
                        BinaryOp::Eq,
                        col_exact(edge_target_id),
                    ),
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
                let target_scan = self.lower_node_scan(&target_scan_binding, target_labels)?;
                let target_join = vec![
                    binary(
                        col_exact(id_col(&target_scan_binding)),
                        BinaryOp::Eq,
                        col_exact(edge_target_id),
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
