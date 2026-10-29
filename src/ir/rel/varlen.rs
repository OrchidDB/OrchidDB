//! Variable-length expansion lowering.
//!
//! The primary lowering uses a recursive CTE and therefore has no implicit
//! depth cap. Bounded unrolling remains as a compatibility fallback for plan
//! shapes that cannot safely be expressed by the recursive lowering.
//!
//! SQL audit [L05](../../../tools/diagnostics/sql-lowering-audit.md#l05-variable-paths-and-payload-demand): Recursive construction and bounded unrolling are compile-time choices.
//! Backend recursion limits never authorize silently truncating an unbounded path.

use datafusion::datasource::cte_worktable::CteWorkTable;
use datafusion::datasource::provider_as_source;
use datafusion::logical_expr::expr::Case;

use super::*;
use super::apply::with_row_identity;

const VARLEN_UNROLL_CAP: u32 = 6;
const WORK_CUR_ID: &str = "__w_cur_id";
const WORK_CUR_LABEL: &str = "__w_cur_label";
const WORK_DEPTH: &str = "__w_depth";
const WORK_TRAIL: &str = "__w_trail";
const WORK_NODES: &str = "__w_nodes";
const WORK_RELS: &str = "__w_rels";

impl LoweringContext<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_expand_varlen(
        &mut self,
        input: &Node,
        source: &str,
        target: &str,
        target_mode: TargetMode,
        target_labels: &LabelExpr,
        rel_binding: Option<&String>,
        rel_types: &LabelExpr,
        dir: Direction,
        length: &crate::ir::plan::Length,
        history: Option<&str>,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let mut input = self.bind_native_node(input, source)?;
        if target_mode == TargetMode::Existing {
            input = self.bind_native_node(input, target)?;
            input = self.restrict_existing_target(input, target, target_labels)?;
        }
        if let Some(history) = history {
            if !has_exact_col(&input.plan, history) {
                let mut projection = existing_columns(&input.plan, &BTreeSet::new());
                projection.push(Self::empty_relationship_history().alias(history));
                let plan = LogicalPlanBuilder::from(input.plan.clone()).project(projection)?.build()?;
                input = input.with_plan(plan);
            } else if native_values::is_value(&col_exact(history).get_type(input.plan.schema())?) {
                // SQL reentry transports a dynamic trail as one opaque cell.
                // Normalize it before both seed and recursive branches, so a
                // zero-hop branch cannot UNION an opaque cell with a list.
                let mut projection = existing_columns(&input.plan, &BTreeSet::from([history.to_owned()]));
                projection.push(native_values::typed::call("list", vec![col_exact(history)]).alias(history));
                let plan = LogicalPlanBuilder::from(input.plan.clone()).project(projection)?.build()?;
                input = input.with_plan(plan);
            }
        }
        if has_binding_shape(&input.plan, source).is_none() {
            return Err(RelError::Unsupported(format!(
                "expand source `{source}` is not an element binding"
            )));
        }

        let recursive = self.lower_expand_varlen_recursive(
            input.clone(),
            source,
            target,
            target_mode,
            target_labels,
            rel_binding,
            rel_types,
            dir,
            length,
            history,
            false,
        );
        match recursive {
            Ok(lowered) => Ok(lowered),
            Err(recursive_error) if length.max.is_some_and(|upper| upper <= VARLEN_UNROLL_CAP) => {
                self.lower_expand_varlen_unrolled(
                    input,
                    source,
                    target,
                    target_mode,
                    target_labels,
                    rel_binding,
                    rel_types,
                    dir,
                    length,
                    history,
                )
                .map_err(|fallback_error| {
                    RelError::Unsupported(format!(
                        "recursive varlen lowering failed ({recursive_error}); \
                         bounded-unroll fallback also failed ({fallback_error})"
                    ))
                })
            }
            // An unbounded query must never silently become a capped query.
            Err(error) => Err(error),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn lower_expand_varlen_recursive(
        &mut self,
        input: LoweredNode,
        source: &str,
        target: &str,
        target_mode: TargetMode,
        target_labels: &LabelExpr,
        rel_binding: Option<&String>,
        rel_types: &LabelExpr,
        dir: Direction,
        length: &crate::ir::plan::Length,
        history: Option<&str>,
        stop_at_target: bool,
    ) -> RelResult<LoweredNode> {
        // Gremlin path values have a different rendering contract. Traversals
        // that do not expose the relationship binding still use recursion.
        if self.language == Language::Gremlin && rel_binding.is_some() {
            return Err(RelError::Unsupported(
                "recursive Gremlin path materialization".into(),
            ));
        }

        // Reentry may compose independently lowered recursive regions. Reserve
        // their work-table names before introducing another positional scope.
        let mut names = BTreeSet::new();
        input.plan.apply_with_subqueries(|node| {
            match node {
                LogicalPlan::RecursiveQuery(query) => { names.insert(query.name.clone()); },
                LogicalPlan::TableScan(scan) => { names.insert(scan.table_name.to_string()); },
                _ => {},
            }
            Ok(datafusion::common::tree_node::TreeNodeRecursion::Continue)
        })?;
        loop {
            self.scan_counter += 1;
            if !names.contains(&format!("__graph_varlen_{}", self.scan_counter)) { break; }
        }
        let uniq = self.scan_counter;
        let cte_name = format!("__graph_varlen_{uniq}");
        let edge_binding = format!("__w_edge_{uniq}");
        let input_columns = output_fields(&input.plan);
        let effective_upper = length.max;
        // The accumulator holds intermediate nodes only; the endpoints are
        // already bound and are added when the full path is rendered.
        let native_path = self.options.native_values && self.language == Language::Cypher;
        let prior_path = rel_binding.filter(|name| has_exact_col(&input.plan, name));
        let seed_nodes = if native_path {
            match prior_path { Some(name) => col_exact(name), None => native_values::literal(&Value::Path(vec![]))? }
        } else { lit("") };

        let edge_scan = self.lower_rel_scan(&edge_binding, rel_types)?;
        // The cursor id crosses the recursive union: the seed (source node)
        // and every step (edge endpoint) must share one identity type.
        let type_of = |plan: &LogicalPlan, name: String| {
            plan.schema()
                .field_with_unqualified_name(&name)
                .map(|field| field.data_type().clone())
                .ok()
        };
        let mut work_id_type = type_of(&input.plan, id_col(source));
        for endpoint in [src_id_col(&edge_binding), dst_id_col(&edge_binding)] {
            if let (Some(current), Some(next)) = (&work_id_type, type_of(&edge_scan.plan, endpoint)) {
                if let Some(common) = common_identity_type(current, &next) {
                    work_id_type = Some(common);
                }
            }
        }
        let work_id = |plan: &LogicalPlan, name: String| -> Expr {
            match (&work_id_type, type_of(plan, name.clone())) {
                (Some(target), Some(actual)) if &actual != target => {
                    cast_identity_expr(col_exact(name), &actual, target)
                }
                _ => col_exact(name),
            }
        };

        let mut seed_projection = input_columns
            .iter()
            .map(|name| col_exact(name).alias(name))
            .collect::<Vec<_>>();
        seed_projection.extend([
            work_id(&input.plan, id_col(source)).alias(WORK_CUR_ID),
            col_exact(label_col(source)).alias(WORK_CUR_LABEL),
            lit(0_i64).alias(WORK_DEPTH),
            history.map(col_exact).unwrap_or_else(|| lit(",")).alias(WORK_TRAIL),
        ]);
        // An unobserved path needs only the traversal cursor and trail. Do not
        // allocate or carry display payload through the recursive worktable.
        if rel_binding.is_some() {
            seed_projection.extend([seed_nodes.alias(WORK_NODES), lit("").alias(WORK_RELS)]);
        }
        let static_term = LogicalPlanBuilder::from(input.plan.clone())
            .project(seed_projection)?
            .build()?;

        let work_schema = Arc::new(static_term.schema().as_arrow().clone());
        let work_table = Arc::new(CteWorkTable::new(&cte_name, work_schema));
        let mut recursive_input =
            LogicalPlanBuilder::scan(&cte_name, provider_as_source(work_table), None)?.build()?;

        if stop_at_target && target_mode == TargetMode::Existing {
            let reached = identity_compare(&[&recursive_input], WORK_CUR_ID, BinaryOp::Eq, &id_col(target))
                .and(col_exact(WORK_CUR_LABEL).eq(col_exact(label_col(target))));
            recursive_input = LogicalPlanBuilder::from(recursive_input).filter(Expr::Not(Box::new(reached)))?.build()?;
        }

        // A bounded query can stop producing work as soon as its upper bound
        // is reached. Unbounded queries terminate solely through trail
        // semantics: every iteration consumes a previously unused edge.
        if let Some(upper) = effective_upper {
            recursive_input = LogicalPlanBuilder::from(recursive_input)
                .filter(binary(
                    col_exact(WORK_DEPTH),
                    BinaryOp::Lt,
                    lit(i64::from(upper)),
                ))?
                .build()?;
        }

        let source_is_src = Expr::and(
            binary(
                col_exact(WORK_CUR_ID),
                BinaryOp::Eq,
                work_id(&edge_scan.plan, src_id_col(&edge_binding)),
            ),
            binary(
                col_exact(WORK_CUR_LABEL),
                BinaryOp::Eq,
                col_exact(src_label_col(&edge_binding)),
            ),
        );
        let source_is_dst = Expr::and(
            binary(
                col_exact(WORK_CUR_ID),
                BinaryOp::Eq,
                work_id(&edge_scan.plan, dst_id_col(&edge_binding)),
            ),
            binary(
                col_exact(WORK_CUR_LABEL),
                BinaryOp::Eq,
                col_exact(dst_label_col(&edge_binding)),
            ),
        );
        let join_predicate = match dir {
            Direction::Out => source_is_src.clone(),
            Direction::In => source_is_dst.clone(),
            Direction::Both => Expr::or(source_is_src.clone(), source_is_dst.clone()),
        };
        let mut recursive_joined = LogicalPlanBuilder::from(recursive_input)
            .join_on(
                edge_scan.plan.clone(),
                JoinType::Inner,
                vec![join_predicate],
            )?
            .build()?;

        let edge_key = if history.is_some() { Self::relationship_key(&edge_scan.plan, &edge_binding) } else { concat_exprs(vec![
            lit(","),
            Self::relationship_key(&edge_scan.plan, &edge_binding),
            lit(","),
        ]) };
        let edge_key = if matches!(col_exact(WORK_TRAIL).get_type(recursive_joined.schema())?, DataType::List(field) if native_values::is_value(field.data_type())) {
            native_values::typed::call("pack", vec![edge_key])
        } else { edge_key };
        let unused = if history.is_some() {
            Expr::Not(Box::new(datafusion::functions_nested::expr_fn::array_has(col_exact(WORK_TRAIL), edge_key.clone())))
        } else {
            binary(df_unicode::strpos(col_exact(WORK_TRAIL), edge_key.clone()), BinaryOp::Eq, lit(0_i64))
        };
        recursive_joined = LogicalPlanBuilder::from(recursive_joined)
            .filter(unused)?
            .build()?;

        // Path values contain intermediate nodes only. At recursive depth d,
        // the current cursor is an intermediate node iff d > 0; joining it
        // here supplies its properties for faithful display.
        let mut islands = input.islands.clone();
        islands.merge(edge_scan.islands);
        let current_node_display = if rel_binding.is_some() {
            let node_binding = format!("__w_node_{uniq}");
            // A recursive cursor must be an endpoint of one of the selected
            // relationship mappings. Do not union unrelated node sources just
            // to reconstruct intermediate path values.
            let labels = self.mapped_endpoint_labels(&LabelExpr::Any, rel_types, Direction::Both)?;
            let node_scan = self.lower_node_scan(&node_binding, &labels)?;
            let node_join = vec![
                identity_compare(
                    &[&recursive_joined, &node_scan.plan],
                    &id_col(&node_binding),
                    BinaryOp::Eq,
                    WORK_CUR_ID,
                ),
                binary(
                    col_exact(label_col(&node_binding)),
                    BinaryOp::Eq,
                    col_exact(WORK_CUR_LABEL),
                ),
            ];
            recursive_joined = LogicalPlanBuilder::from(recursive_joined)
                .join_on(node_scan.plan, JoinType::Inner, node_join)?
                .build()?;
            islands.merge(node_scan.islands);
            Some(if native_path {
                let first = IrExpr::Call { name: "path_extend_after".into(), args: vec![
                    IrExpr::binding(WORK_NODES), IrExpr::binding(&node_binding), IrExpr::binding(&edge_binding)] };
                let next = IrExpr::Call { name: "path_append".into(), args: vec![
                    IrExpr::Call { name: "path_append".into(), args: vec![IrExpr::binding(WORK_NODES), IrExpr::binding(&node_binding)] },
                    IrExpr::binding(&edge_binding)] };
                if prior_path.is_some() { case_when(col_exact(WORK_DEPTH).eq(lit(0_i64)), self.native_expr(&recursive_joined, &first)?, self.native_expr(&recursive_joined, &next)?) }
                else { self.native_expr(&recursive_joined, &next)? }
            } else { self.cypher_element_display_expr(&recursive_joined, &node_binding, BindingShape::Node)? })
        } else {
            None
        };

        let (next_id, next_label) = match dir {
            Direction::Out => (
                work_id(&edge_scan.plan, dst_id_col(&edge_binding)),
                col_exact(dst_label_col(&edge_binding)),
            ),
            Direction::In => (
                work_id(&edge_scan.plan, src_id_col(&edge_binding)),
                col_exact(src_label_col(&edge_binding)),
            ),
            Direction::Both => (
                case_when(
                    source_is_src.clone(),
                    work_id(&edge_scan.plan, dst_id_col(&edge_binding)),
                    work_id(&edge_scan.plan, src_id_col(&edge_binding)),
                ),
                case_when(
                    source_is_src,
                    col_exact(dst_label_col(&edge_binding)),
                    col_exact(src_label_col(&edge_binding)),
                ),
            ),
        };
        let next_rels = if native_path { col_exact(WORK_RELS) } else if rel_binding.is_some() {
            let edge_display = if self.language == Language::Gremlin {
                gremlin_element_display_expr(&recursive_joined, &edge_binding)?
            } else {
                self.cypher_element_display_expr(
                    &recursive_joined,
                    &edge_binding,
                    BindingShape::Edge,
                )?
            };
            append_csv(col_exact(WORK_RELS), edge_display)
        } else {
            col_exact(WORK_RELS)
        };
        let next_nodes = match current_node_display {
            Some(display) if native_path => display,
            Some(display) => case_when(
                binary(col_exact(WORK_DEPTH), BinaryOp::Eq, lit(0_i64)),
                col_exact(WORK_NODES),
                append_csv(col_exact(WORK_NODES), display),
            ),
            None => col_exact(WORK_NODES),
        };

        let mut recursive_projection = input_columns
            .iter()
            .map(|name| col_exact(name).alias(name))
            .collect::<Vec<_>>();
        recursive_projection.extend([
            next_id.alias(WORK_CUR_ID),
            next_label.alias(WORK_CUR_LABEL),
            binary(col_exact(WORK_DEPTH), BinaryOp::Add, lit(1_i64)).alias(WORK_DEPTH),
            (if history.is_some() {
                datafusion::functions_nested::expr_fn::array_append(col_exact(WORK_TRAIL), edge_key)
            } else { string_concat(col_exact(WORK_TRAIL), edge_key) }).alias(WORK_TRAIL),
        ]);
        if rel_binding.is_some() {
            recursive_projection.extend([next_nodes.alias(WORK_NODES), next_rels.alias(WORK_RELS)]);
        }
        let recursive_term = LogicalPlanBuilder::from(recursive_joined)
            .project(recursive_projection)?
            .build()?;

        let recursive_query = LogicalPlanBuilder::from(static_term)
            .to_recursive_query(cte_name, recursive_term, false)?
            .build()?;
        let mut consumed = LogicalPlanBuilder::from(recursive_query)
            .filter(binary(
                col_exact(WORK_DEPTH),
                BinaryOp::Gte,
                lit(i64::from(length.min)),
            ))?
            .build()?;
        if let Some(upper) = effective_upper {
            consumed = LogicalPlanBuilder::from(consumed)
                .filter(binary(
                    col_exact(WORK_DEPTH),
                    BinaryOp::Lte,
                    lit(i64::from(upper)),
                ))?
                .build()?;
        }

        match target_mode {
            TargetMode::Existing => {
                let same_target =
                    identity_compare(&[&consumed], &id_col(target), BinaryOp::Eq, WORK_CUR_ID);
                consumed = LogicalPlanBuilder::from(consumed)
                    .filter(Expr::and(
                        same_target,
                        binary(
                            col_exact(label_col(target)),
                            BinaryOp::Eq,
                            col_exact(WORK_CUR_LABEL),
                        ),
                    ))?
                    .build()?;
            }
            TargetMode::BindNew
            | TargetMode::ReplaceCurrent
            | TargetMode::ReplaceCurrentAndBindLabel
            | TargetMode::BindNewOrReplaceCurrent => {
                let target_scan_binding = if has_binding_shape(&consumed, target).is_some() {
                    format!("__w_target_{uniq}")
                } else {
                    target.to_string()
                };
                let target_scan = self.lower_node_scan(&target_scan_binding, target_labels)?;
                let target_id = identity_compare(
                    &[&consumed, &target_scan.plan],
                    &id_col(&target_scan_binding),
                    BinaryOp::Eq,
                    WORK_CUR_ID,
                );
                consumed = LogicalPlanBuilder::from(consumed)
                    .join_on(
                        target_scan.plan,
                        JoinType::Inner,
                        vec![
                            target_id,
                            binary(
                                col_exact(label_col(&target_scan_binding)),
                                BinaryOp::Eq,
                                col_exact(WORK_CUR_LABEL),
                            ),
                        ],
                    )?
                    .build()?;
                islands.merge(target_scan.islands);
                if target_scan_binding != target {
                    let mut projections = existing_columns_excluding_bindings(
                        &consumed,
                        &[target, target_scan_binding.as_str()],
                    );
                    projections.extend(duplicate_binding_projection_only(
                        &consumed,
                        &target_scan_binding,
                        target,
                    )?);
                    consumed = LogicalPlanBuilder::from(consumed)
                        .project(projections)?
                        .build()?;
                }
            }
        }

        let mut projection_names = input_columns;
        for field in consumed.schema().fields() {
            let name = field.name();
            if is_binding_column(name, target) && !projection_names.contains(name) {
                projection_names.push(name.clone());
            }
        }
        let mut final_projection = projection_names
            .into_iter()
            .filter(|name| !rel_binding.is_some_and(|rel| name == rel || name == &path_inner_col(rel) || name == &path_len_col(rel)))
            .filter(|name| has_exact_col(&consumed, name) && history != Some(name.as_str()))
            .map(|name| col_exact(&name).alias(name))
            .collect::<Vec<_>>();
        if let Some(history) = history {
            final_projection.push(col_exact(WORK_TRAIL).alias(history));
        }
        if let Some(rel) = rel_binding {
            if native_path {
                let full = IrExpr::Call { name: "path_append".into(), args: vec![IrExpr::binding(WORK_NODES), IrExpr::binding(target)] };
                let rendered = self.native_expr(&consumed, &full)?;
                let rendered = if prior_path.is_some() { case_when(col_exact(WORK_DEPTH).eq(lit(0_i64)), col_exact(WORK_NODES), rendered) } else { rendered };
                final_projection.push(rendered.alias(rel));
                // The user relationship variable omits the endpoint nodes.
                let inner = IrExpr::Call { name: "recursive_relationship_path".into(), args: vec![
                    full, IrExpr::Lit(Lit::String(source.to_owned())), IrExpr::binding(source),
                ] };
                final_projection.push(self.native_expr(&consumed, &inner)?.alias(path_inner_col(rel)));
                final_projection.push(col_exact(WORK_DEPTH).alias(path_len_col(rel)));
            } else {
            let source_display =
                self.cypher_element_display_expr(&consumed, source, BindingShape::Node)?;
            let target_display =
                self.cypher_element_display_expr(&consumed, target, BindingShape::Node)?;
            let rels = concat_exprs(vec![lit("], _RELS: ["), col_exact(WORK_RELS), lit("]}")]);
            final_projection.push(
                concat_exprs(vec![
                    lit("{_NODES: ["),
                    append_csv(
                        join_csv(source_display, col_exact(WORK_NODES)),
                        target_display,
                    ),
                    rels.clone(),
                ])
                .alias(rel),
            );
            final_projection.push(
                concat_exprs(vec![lit("{_NODES: ["), col_exact(WORK_NODES), rels])
                    .alias(path_inner_col(rel)),
            );
            final_projection.push(col_exact(WORK_DEPTH).alias(path_len_col(rel)));
            }
        }
        let plan = LogicalPlanBuilder::from(consumed)
            .project(final_projection)?
            .build()?;
        Ok(LoweredNode {
            plan,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

    /// Shortest paths share the finite, edge-unique recursive relation with
    /// variable expansion. Selection is per input *occurrence* and destination;
    /// duplicate input rows and equal-length ties must not collapse together.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn lower_shortest_path(
        &mut self, input: &Node, source: &str, target: Option<&str>,
        direction: Direction, rel_types: &LabelExpr, max_distance: Option<f64>,
        include_edges: bool, output: &str, all_paths: bool,
    ) -> RelResult<LoweredNode> {
        if !self.options.native_values {
            return Err(RelError::Unsupported("shortest paths require typed path values".into()));
        }
        let input = self.lower_node(input)?;
        let original = output_fields(&input.plan);
        let labels = self.mapped_endpoint_labels(&LabelExpr::Any, rel_types, Direction::Both)?;
        let mut input = self.bind_native_node_with_labels(input, source, &labels)?;
        if let Some(target) = target { input = self.bind_native_node_with_labels(input, target, &labels)?; }
        if max_distance.is_some_and(|bound| bound < 0.0) {
            input.plan = LogicalPlanBuilder::from(input.plan).filter(lit(false))?.build()?;
        }
        self.scan_counter += 1;
        let serial = self.scan_counter;
        let mut keys = Vec::new();
        input.plan = with_row_identity(input.plan, &mut keys, &mut BTreeSet::new(), serial)?;
        let destination = target.map(str::to_owned).unwrap_or_else(|| format!("__shortest_target_{serial}"));
        let path = format!("__shortest_path_{serial}");
        let max = max_distance.filter(|n| n.is_finite() && *n <= f64::from(u32::MAX))
            .map(|n| n.max(0.0).floor() as u32);
        let mut expanded = self.lower_expand_varlen_recursive(input, source, &destination,
            if target.is_some() { TargetMode::Existing } else { TargetMode::BindNew },
            &labels, Some(&path), rel_types, direction,
            &crate::ir::plan::Length { min: 0, max }, None, true)?;
        keys.extend([id_col(&destination), label_col(&destination)]);
        let ordering = col_exact(path_len_col(&path)).sort(true, false);
        let rank = if all_paths { df_window::rank() } else { df_window::row_number() }
            .partition_by(keys.iter().map(col_exact).collect())
            .order_by(vec![ordering]).build()?.alias("__shortest_rank");
        expanded.plan = LogicalPlanBuilder::from(expanded.plan).window(vec![rank])?
            .filter(col_exact("__shortest_rank").eq(lit(1_u64)))?.build()?;
        let mut projection = original.iter().filter(|name| name.as_str() != output)
            .map(col_exact).collect::<Vec<_>>();
        let value = if include_edges { col_exact(&path) }
            else { native_values::typed::call("node_path", vec![col_exact(&path)]) };
        projection.push(value.alias(output));
        expanded.plan = LogicalPlanBuilder::from(expanded.plan).project(projection)?.build()?;
        Ok(expanded)
    }

    /// Compatibility implementation for bounded plans the recursive lowering
    /// declines. Unlike the old primary lowering, this is never used to
    /// approximate an unbounded range.
    #[allow(clippy::too_many_arguments)]
    fn lower_expand_varlen_unrolled(
        &mut self,
        input: LoweredNode,
        source: &str,
        target: &str,
        target_mode: TargetMode,
        target_labels: &LabelExpr,
        rel_binding: Option<&String>,
        rel_types: &LabelExpr,
        dir: Direction,
        length: &crate::ir::plan::Length,
        history: Option<&str>,
    ) -> RelResult<LoweredNode> {
        let lo = length.min;
        let hi = length.max.ok_or_else(|| {
            RelError::Unsupported("cannot safely unroll an unbounded variable path".into())
        })?;
        if hi > VARLEN_UNROLL_CAP {
            return Err(RelError::Unsupported(format!(
                "variable-length expand upper bound {hi} exceeds unroll cap {VARLEN_UNROLL_CAP}"
            )));
        }

        self.scan_counter += 1;
        let uniq = self.scan_counter;
        let input_columns = output_fields(&input.plan);
        let mut branches: Vec<LogicalPlan> = Vec::new();
        let mut islands = input.islands.clone();

        if lo == 0 {
            let plan = match target_mode {
                TargetMode::Existing => {
                    let same = Expr::and(
                        identity_compare(&[&input.plan], &id_col(target), BinaryOp::Eq, &id_col(source)),
                        binary(
                            col_exact(label_col(target)),
                            BinaryOp::Eq,
                            col_exact(label_col(source)),
                        ),
                    );
                    LogicalPlanBuilder::from(input.plan.clone())
                        .filter(same)?
                        .build()?
                }
                _ => {
                    let mut projections =
                        existing_columns(&input.plan, &BTreeSet::from([target.to_string()]));
                    projections.extend(duplicate_binding_projection_only(
                        &input.plan,
                        source,
                        target,
                    )?);
                    LogicalPlanBuilder::from(input.plan.clone())
                        .project(projections)?
                        .build()?
                }
            };
            let plan = match rel_binding {
                Some(rel) => self.project_varlen_path(plan, rel, (source, target), &[], &[])?,
                None => plan,
            };
            branches.push(self.varlen_branch_projection(
                plan,
                &input_columns,
                target,
                rel_binding,
            )?);
        }

        for k in lo.max(1)..=hi {
            let mut chain = LoweredNode {
                plan: input.plan.clone(),
                islands: IslandReport::default(),
                fields: None,
                result_form: None,
            };
            let mut hop_rels = Vec::new();
            let mut hop_source = source.to_string();
            for hop in 1..=k {
                let hop_rel = format!("__vlr_{uniq}_{k}_{hop}");
                let (hop_target, hop_mode, hop_labels) = if hop == k {
                    (target.to_string(), target_mode, target_labels.clone())
                } else {
                    (
                        format!("__vln_{uniq}_{k}_{hop}"),
                        TargetMode::BindNew,
                        LabelExpr::Any,
                    )
                };
                chain = match dir {
                    Direction::Out | Direction::In => self.lower_expand_direction(
                        chain,
                        &hop_source,
                        &hop_target,
                        hop_mode,
                        &hop_labels,
                        Some(&hop_rel),
                        rel_types,
                        dir,
                    )?,
                    Direction::Both => self.lower_expand_both(
                        chain,
                        &hop_source,
                        &hop_target,
                        hop_mode,
                        &hop_labels,
                        Some(&hop_rel),
                        rel_types,
                    )?,
                };
                if let Some(history) = history {
                    chain = self.track_relationship(chain, history, &hop_rel)?;
                }
                hop_rels.push(hop_rel);
                hop_source = hop_target;
            }
            let mut distinct_filters = Vec::new();
            for i in 0..hop_rels.len() {
                for j in (i + 1)..hop_rels.len() {
                    let same = Expr::and(
                        identity_compare(
                            &[&chain.plan],
                            &id_col(&hop_rels[i]),
                            BinaryOp::Eq,
                            &id_col(&hop_rels[j]),
                        ),
                        binary(
                            col_exact(label_col(&hop_rels[i])),
                            BinaryOp::Eq,
                            col_exact(label_col(&hop_rels[j])),
                        ),
                    );
                    distinct_filters.push(Expr::Not(Box::new(same)));
                }
            }
            let mut plan = chain.plan;
            if let Some(filter) = distinct_filters.into_iter().reduce(Expr::and) {
                plan = LogicalPlanBuilder::from(plan).filter(filter)?.build()?;
            }
            islands.merge(chain.islands);
            if let Some(rel) = rel_binding {
                let hop_nodes: Vec<String> = (1..k)
                    .map(|hop| format!("__vln_{uniq}_{k}_{hop}"))
                    .collect();
                plan =
                    self.project_varlen_path(plan, rel, (source, target), &hop_nodes, &hop_rels)?;
            }
            branches.push(self.varlen_branch_projection(
                plan,
                &input_columns,
                target,
                rel_binding,
            )?);
        }

        let mut union_plan: Option<LogicalPlan> = None;
        for branch in branches {
            union_plan = Some(match union_plan {
                None => branch,
                Some(current) => LogicalPlanBuilder::from(current)
                    .union_by_name(branch)?
                    .build()?,
            });
        }
        let plan = union_plan.ok_or_else(|| {
            RelError::Unsupported("variable-length expand had no branches".into())
        })?;
        Ok(LoweredNode {
            plan,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

    fn project_varlen_path(
        &self,
        plan: LogicalPlan,
        rel_binding: &str,
        endpoints: (&str, &str),
        hop_nodes: &[String],
        hop_rels: &[String],
    ) -> RelResult<LogicalPlan> {
        let render = |binding: &str, shape: BindingShape| -> RelResult<Expr> {
            if self.language == Language::Gremlin {
                gremlin_element_display_expr(&plan, binding)
            } else {
                self.cypher_element_display_expr(&plan, binding, shape)
            }
        };
        let render_list = |bindings: &[&str], shape: BindingShape| -> RelResult<Vec<Expr>> {
            let mut parts = Vec::new();
            for (index, binding) in bindings.iter().enumerate() {
                if index > 0 {
                    parts.push(lit(","));
                }
                parts.push(render(binding, shape)?);
            }
            Ok(parts)
        };
        let render_path = |nodes: &[&str]| -> RelResult<Expr> {
            let rels = hop_rels.iter().map(String::as_str).collect::<Vec<_>>();
            let mut parts = vec![lit("{_NODES: [")];
            parts.extend(render_list(nodes, BindingShape::Node)?);
            parts.push(lit("], _RELS: ["));
            parts.extend(render_list(&rels, BindingShape::Edge)?);
            parts.push(lit("]}"));
            Ok(concat_exprs(parts))
        };

        let inner_nodes = hop_nodes.iter().map(String::as_str).collect::<Vec<_>>();
        // A Cypher path includes its endpoints; the relationship variable
        // (`path_inner_col`) does not. Gremlin keeps its existing contract.
        let full = if self.language == Language::Gremlin {
            render_path(&inner_nodes)?
        } else {
            let (source, target) = endpoints;
            let mut nodes = vec![source];
            nodes.extend(inner_nodes.iter().copied());
            nodes.push(target);
            render_path(&nodes)?
        };
        let inner = render_path(&inner_nodes)?;

        let mut projections = existing_columns(
            &plan,
            &BTreeSet::from([
                rel_binding.to_string(),
                path_len_col(rel_binding),
                path_inner_col(rel_binding),
            ]),
        );
        projections.push(full.alias(rel_binding));
        projections.push(inner.alias(path_inner_col(rel_binding)));
        projections.push(lit(hop_rels.len() as i64).alias(path_len_col(rel_binding)));
        Ok(LogicalPlanBuilder::from(plan)
            .project(projections)?
            .build()?)
    }

    fn varlen_branch_projection(
        &self,
        plan: LogicalPlan,
        input_columns: &[String],
        target: &str,
        rel_binding: Option<&String>,
    ) -> RelResult<LogicalPlan> {
        let mut names = Vec::new();
        for name in input_columns {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        for field in plan.schema().fields() {
            let name = field.name();
            let keep = is_binding_column(name, target)
                || rel_binding.is_some_and(|rel| is_binding_column(name, rel));
            if keep && !names.iter().any(|existing| existing == name) {
                names.push(name.clone());
            }
        }
        let projections = names
            .into_iter()
            .filter(|name| has_exact_col(&plan, name))
            .map(|name| col_exact(&name).alias(name))
            .collect::<Vec<_>>();
        Ok(LogicalPlanBuilder::from(plan)
            .project(projections)?
            .build()?)
    }
}

pub(super) fn case_when(condition: Expr, then_expr: Expr, else_expr: Expr) -> Expr {
    Expr::Case(Case::new(
        None,
        vec![(Box::new(condition), Box::new(then_expr))],
        Some(Box::new(else_expr)),
    ))
}

fn append_csv(existing: Expr, value: Expr) -> Expr {
    case_when(
        binary(existing.clone(), BinaryOp::Eq, lit("")),
        value.clone(),
        concat_exprs(vec![existing, lit(","), value]),
    )
}

/// Comma-joins two list fragments, either of which may be empty.
fn join_csv(left: Expr, right: Expr) -> Expr {
    case_when(
        binary(right.clone(), BinaryOp::Eq, lit("")),
        left.clone(),
        append_csv(left, right),
    )
}
