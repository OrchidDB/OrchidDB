//! Expansion.
//!
//! SQL audit [L04](../../../tools/diagnostics/sql-lowering-audit.md#l04-fixed-traversals-and-redundant-label-or-cast-removal): Fixed hops become endpoint joins. Label predicates and trail checks
//! have different purposes: proving a fixed label cannot remove trail uniqueness.

use super::*;
use datafusion::common::tree_node::TreeNode;

impl<'a> LoweringContext<'a> {
    /// A mapped edge fixes endpoint labels. Apply that metadata before building
    /// a union of candidate tables; SQL execution must never open unrelated
    /// sources just to discard their labels in a join.
    pub(super) fn mapped_endpoint_labels(&self, labels: &LabelExpr, types: &LabelExpr, dir: Direction) -> RelResult<LabelExpr> {
        if self.options.mapping.as_ref().is_some_and(|m| m.managed_table.is_some()) { return Ok(labels.clone()); }
        let Some(mapping) = &self.options.mapping else { return Ok(labels.clone()); };
        // Dynamic logical labels are not physical endpoint identity names.
        if mapping.labels().iter().any(|label| mapping.node(label).is_some_and(|node| node.labels_column.is_some())) {
            return Ok(labels.clone());
        }
        let types = mapping::resolve_names(types, || mapping.rel_types(), "relationship")?;
        let allowed = mapping::resolve_names(labels, || mapping.labels(), "label")?;
        let mut endpoints = BTreeSet::new();
        for kind in types {
            for edge in mapping.edge_partitions(&kind) {
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

    /// Recover a relational node binding from a lossless value (for example an
    /// element of nodes(path)). Identity equality stays inside SQL; no graph
    /// lookup or per-row execution boundary is needed to re-enter expansion.
    pub(super) fn bind_native_node(&mut self, input: LoweredNode, binding: &str) -> RelResult<LoweredNode> {
        self.bind_native_node_with_labels(input, binding, &LabelExpr::Any)
    }
    pub(super) fn bind_native_node_with_labels(&mut self, input: LoweredNode, binding: &str, labels: &LabelExpr) -> RelResult<LoweredNode> {
        if self.language != Language::Cypher || has_binding_shape(&input.plan, binding).is_some()
            || !has_exact_col(&input.plan, binding)
            || !native_values::is_value(&col_exact(binding).get_type(input.plan.schema())?) {
            return Ok(input);
        }
        // Do not introduce scans of external computed sources merely to decode
        // a value. Those sources retain their explicit execution contract.
        if self.options.mapping.as_ref().is_some_and(|mapping| mapping.labels().iter()
            .any(|label| mapping.node(label).is_some_and(|node|
                !matches!(&node.source, mapping::MappedSource::Table(_))))) {
            return Ok(input);
        }
        let candidate = format!("__orchid_value_node_{}", self.scan_counter);
        self.scan_counter += 1;
        let nodes = self.lower_node_scan(&candidate, labels)?;
        let key = native_values::key_expr(col_exact(binding), true);
        let candidate_key = native_values::key_expr(
            self.native_expr(&nodes.plan, &IrExpr::Binding(candidate.clone()))?, true);
        let mut projection = existing_columns(&input.plan, &BTreeSet::new());
        projection.extend(duplicate_binding_projection_only(&nodes.plan, &candidate, binding)?
            .into_iter().filter(|expr| expr.schema_name().to_string() != binding));
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .join_on(nodes.plan, JoinType::Inner, vec![key.eq(candidate_key)])?
            .project(projection)?.build()?;
        let mut result = input.with_plan(plan);
        result.islands.merge(nodes.islands);
        Ok(result)
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
        let input = if let Node::GraphNodeScan { binding, labels, .. } = input
            && binding == source
            && !self.options.live_graph_reads
            && let Some(mapping) = self.options.mapping.clone().filter(|m| m.managed_table.is_none()) {
            let mut endpoints = BTreeSet::new();
            for kind in mapping::resolve_names(rel_types, || mapping.rel_types(), "relationship")? {
                for edge in mapping.edge_partitions(&kind) {
                    if dir != Direction::In { endpoints.insert(edge.src_label.clone()); }
                    if dir != Direction::Out { endpoints.insert(edge.dst_label.clone()); }
                }
            }
            mapping::lower_mapped_node_scan_restricted(self, &mapping, binding, labels, Some(&endpoints))?
        } else { self.lower_node(input)? };
        let input = self.bind_native_node(input, source)?;
        let input = if target_mode == TargetMode::Existing { self.bind_native_node(input, target)? } else { input };
        let mut input = if target_mode == TargetMode::Existing {
            self.restrict_existing_target(input, target, target_labels)?
        } else { input };
        if has_binding_shape(&input.plan, source).is_none() {
            return Err(RelError::Unsupported(format!(
                "expand source `{source}` is not an element binding"
            )));
        }

        let rel = if let Some(binding) = rel_binding {
            binding.clone()
        } else {
            // A materialized path retains its anonymous edge columns. Allocate
            // a fresh binding for every hop instead of reusing the same ID
            // column when no intervening table scan advances the counter.
            loop {
                let candidate = format!("__rel_{}", self.scan_counter);
                self.scan_counter += 1;
                if !input.plan.schema().fields().iter().any(|field| is_binding_column(field.name(), &candidate)) {
                    break candidate;
                }
            }
        };
        let encounter = if self.language == Language::Gremlin {
            let name = unique_internal_alias(&input.plan, &BTreeSet::new(), "__orchid_expand_encounter");
            let numbered = with_distinct_ordinal(input.plan, &name)?;
            let guard = unique_internal_alias(&numbered, &BTreeSet::new(), "__orchid_expand_guard");
            let mut columns = existing_columns(&numbered, &BTreeSet::new());
            columns.push(lit(1_i64).alias(&guard));
            input.plan = LogicalPlanBuilder::from(numbered).project(columns)?
                .alias(format!("__w_sql_cte_expand_input_{}", self.scan_counter))?.build()?;
            self.scan_counter += 1;
            Some((name, guard))
        } else { None };
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
        if let Some((encounter, guard)) = encounter {
            // A hash join can interleave different parent traversers. Retain
            // their encounter order and the provider's label/key scan order
            // before downstream folds, slices, or floating-point reductions.
            let joined_guard = unique_internal_alias(&expanded.plan, &BTreeSet::new(), "__orchid_expand_join_guard");
            let mut joined_columns = existing_columns(&expanded.plan, &BTreeSet::new());
            joined_columns.push(lit(1_i64).alias(&joined_guard));
            expanded.plan = LogicalPlanBuilder::from(expanded.plan).project(joined_columns)?
                .alias(format!("__w_sql_cte_expand_join_{}", self.scan_counter))?.build()?;
            self.scan_counter += 1;
            let order = vec![col_exact(&encounter).sort(true, false),
                col_exact(label_col(&rel)).sort(true, false), col_exact(id_col(&rel)).sort(true, false)];
            let columns = existing_columns(&expanded.plan, &BTreeSet::from([encounter, guard, joined_guard]));
            expanded.plan = LogicalPlanBuilder::from(expanded.plan).sort(order)?.project(columns)?.build()?;
        }
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
        let key = if matches!(data_type, DataType::Struct(_)) {
            identity_token_expr(key, data_type)
        } else if matches!(data_type, DataType::Binary | DataType::LargeBinary
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
        let has_previous = has_exact_col(&input.plan, history);
        let mut key = Self::relationship_key(&input.plan, rel);
        let mut trail_input = col_exact(history);
        if has_previous {
            let ty = trail_input.get_type(input.plan.schema())?;
            if native_values::is_value(&ty) {
                trail_input = if self.language == Language::Cypher { native_values::typed::call("items",vec![trail_input]) } else {native_values::list(trail_input)};
            }
            if matches!(trail_input.get_type(input.plan.schema())?, DataType::List(field) if native_values::is_value(field.data_type())) {
                key = native_values::typed::call("pack",vec![key]);
            }
        }
        // Fixed hops retain typed edge identities under fresh names. The
        // string trail is still available to a later variable-length hop, but
        // fixed-hop uniqueness does not need to construct/search that trail.
        let prefix = format!("{history}__e");
        let mut previous = Vec::new();
        let mut index = 0;
        while has_exact_col(&input.plan, &format!("{prefix}{index}_id")) {
            previous.push((format!("{prefix}{index}_id"), format!("{prefix}{index}_label")));
            index += 1;
        }
        let id = col_exact(id_col(rel));
        let label = col_exact(label_col(rel));
        let mut recursive = false;
        input.plan.apply(|node| {
            recursive |= matches!(node, LogicalPlan::RecursiveQuery(_));
            Ok(datafusion::common::tree_node::TreeNodeRecursion::Continue)
        })?;
        let typed = !recursive && (!has_previous || !previous.is_empty())
            && previous.iter().all(|(name, _)| col_exact(name).get_type(input.plan.schema()).ok()
                == id.get_type(input.plan.schema()).ok());
        let filtered = if has_previous {
            let predicate = if typed {
                previous.iter().map(|(old_id, old_label)|
                    id.clone().not_eq(col_exact(old_id)).or(label.clone().not_eq(col_exact(old_label))))
                    .reduce(|a, b| a.and(b)).expect("previous fixed identity")
            } else { Expr::Not(Box::new(array_has(trail_input.clone(), key.clone()))) };
            LogicalPlanBuilder::from(input.plan.clone()).filter(predicate)?.build()?
        } else { input.plan.clone() };
        let mut projection = existing_columns(&filtered, &BTreeSet::from([history.to_owned()]));
        if typed {
            projection.push(id.alias(format!("{prefix}{index}_id")));
            projection.push(label.alias(format!("{prefix}{index}_label")));
        }
        let trail = if has_previous { array_append(trail_input, key) }
            else { datafusion::functions_nested::expr_fn::make_array(vec![key]) };
        projection.push(trail.alias(history));
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
        if self.options.native_values {
            let previous=if has_exact_col(&expanded.plan,path){IrExpr::binding(path)}else{IrExpr::Lit(Lit::Null)};
            let source=IrExpr::binding(source);
            use crate::ir::runtime::ops::expand::{path_step,PathStep};
            let step=path_step(Some(path),true,target);
            let history=if !matches!(step,PathStep::EdgeAndNode) {previous} else {
                IrExpr::Call{name:"path_extend_after".into(),args:vec![previous,source.clone(),IrExpr::binding(rel)]}
            };
            let last=if matches!(step,PathStep::Edge){rel}else{target};
            let expression=IrExpr::Call{name:"path_extend_after".into(),args:vec![history,source,IrExpr::binding(last)]};
            let value=self.native_expr(&expanded.plan,&expression)?;
            let mut columns=existing_columns(&expanded.plan,&BTreeSet::from([path.to_owned()]));
            columns.push(value.alias(path));
            let plan=LogicalPlanBuilder::from(expanded.plan.clone()).project(columns)?.build()?;
            return Ok(expanded.with_plan(plan));
        }
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
        let rel = if let Some(binding) = rel_binding {
            binding.clone()
        } else {
            // A materialized path retains its anonymous edge columns. Allocate
            // a fresh binding for every hop instead of reusing the same ID
            // column when no intervening table scan advances the counter.
            loop {
                let candidate = format!("__rel_{}", self.scan_counter);
                self.scan_counter += 1;
                if !input.plan.schema().fields().iter().any(|field| is_binding_column(field.name(), &candidate)) {
                    break candidate;
                }
            }
        };
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
                Some(target) => cast_identity_expr(col_exact(name.clone()), &field_type(name).expect("endpoint column"), target),
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
        let rel = if let Some(binding) = rel_binding {
            binding.clone()
        } else {
            // A materialized path retains its anonymous edge columns. Allocate
            // a fresh binding for every hop instead of reusing the same ID
            // column when no intervening table scan advances the counter.
            loop {
                let candidate = format!("__rel_{}", self.scan_counter);
                self.scan_counter += 1;
                if !input.plan.schema().fields().iter().any(|field| is_binding_column(field.name(), &candidate)) {
                    break candidate;
                }
            }
        };
        let physical_mapping = self.options.mapping.clone().filter(|mapping|
            !self.options.live_graph_reads && mapping.managed_table.is_none());
        let source_domain = query_domains::strings(&input.plan, &label_col(source));
        let edge_scan = if let Some(mapping) = &physical_mapping {
            mapping::lower_mapped_rel_scan_restricted(self, mapping, &rel, rel_types,
                if dir == Direction::Out { source_domain.as_ref() } else { None },
                if dir == Direction::In { source_domain.as_ref() } else { None })?
        } else { self.lower_rel_scan(&rel, rel_types)? };

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
                let target_domain = query_domains::strings(&edge_scan.plan, &edge_target_label);
                let target_scan = if let Some(mapping) = &physical_mapping {
                    mapping::lower_mapped_node_scan_restricted(self, mapping, &target_scan_binding,
                        target_labels, target_domain.as_ref())?
                } else { self.lower_node_scan(&target_scan_binding, &labels)? };
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
