//! Variable-length expansion lowering.
//!
//! The primary lowering uses a recursive CTE and therefore has no implicit
//! depth cap. Bounded unrolling remains as a compatibility fallback for plan
//! shapes that cannot safely be expressed by the recursive lowering.

use datafusion::datasource::cte_worktable::CteWorkTable;
use datafusion::datasource::provider_as_source;
use datafusion::logical_expr::expr::Case;

use super::*;

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
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
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
    ) -> RelResult<LoweredNode> {
        // Gremlin path values have a different rendering contract. Traversals
        // that do not expose the relationship binding still use recursion.
        if self.language == Language::Gremlin && rel_binding.is_some() {
            return Err(RelError::Unsupported(
                "recursive Gremlin path materialization".into(),
            ));
        }

        self.scan_counter += 1;
        let uniq = self.scan_counter;
        let cte_name = format!("__graph_varlen_{uniq}");
        let edge_binding = format!("__w_edge_{uniq}");
        let input_columns = output_fields(&input.plan);
        let effective_upper = match (length.max, self.options.varlen_recursive_ceiling) {
            (Some(query), Some(ceiling)) => Some(query.min(ceiling)),
            (Some(query), None) => Some(query),
            (None, ceiling) => ceiling,
        };
        let seed_nodes = if rel_binding.is_some() {
            self.cypher_element_display_expr(&input.plan, source, BindingShape::Node)?
        } else {
            lit("")
        };

        let mut seed_projection = input_columns
            .iter()
            .map(|name| col_exact(name).alias(name))
            .collect::<Vec<_>>();
        seed_projection.extend([
            col_exact(id_col(source)).alias(WORK_CUR_ID),
            col_exact(label_col(source)).alias(WORK_CUR_LABEL),
            lit(0_i64).alias(WORK_DEPTH),
            lit(",").alias(WORK_TRAIL),
            seed_nodes.alias(WORK_NODES),
            lit("").alias(WORK_RELS),
        ]);
        let static_term = LogicalPlanBuilder::from(input.plan.clone())
            .project(seed_projection)?
            .build()?;

        let work_schema = Arc::new(static_term.schema().as_arrow().clone());
        let work_table = Arc::new(CteWorkTable::new(&cte_name, work_schema));
        let mut recursive_input =
            LogicalPlanBuilder::scan(&cte_name, provider_as_source(work_table), None)?.build()?;

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

        let edge_scan = self.lower_rel_scan(&edge_binding, rel_types)?;
        let source_is_src = Expr::and(
            binary(
                col_exact(WORK_CUR_ID),
                BinaryOp::Eq,
                col_exact(src_id_col(&edge_binding)),
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
                col_exact(dst_id_col(&edge_binding)),
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

        let edge_key = concat_exprs(vec![
            lit(","),
            col_exact(label_col(&edge_binding)),
            lit(":"),
            cast_utf8(col_exact(id_col(&edge_binding))),
            lit(","),
        ]);
        recursive_joined = LogicalPlanBuilder::from(recursive_joined)
            .filter(binary(
                df_unicode::strpos(col_exact(WORK_TRAIL), edge_key.clone()),
                BinaryOp::Eq,
                lit(0_i64),
            ))?
            .build()?;

        // Path values contain intermediate nodes only. At recursive depth d,
        // the current cursor is an intermediate node iff d > 0; joining it
        // here supplies its properties for faithful display.
        let mut islands = input.islands.clone();
        islands.merge(edge_scan.islands);
        let current_node_display = if rel_binding.is_some() {
            let node_binding = format!("__w_node_{uniq}");
            let node_scan = self.lower_node_scan(&node_binding, &LabelExpr::Any)?;
            let node_join = vec![
                binary(
                    col_exact(id_col(&node_binding)),
                    BinaryOp::Eq,
                    col_exact(WORK_CUR_ID),
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
            Some(self.cypher_element_display_expr(
                &recursive_joined,
                &node_binding,
                BindingShape::Node,
            )?)
        } else {
            None
        };

        let (next_id, next_label) = match dir {
            Direction::Out => (
                col_exact(dst_id_col(&edge_binding)),
                col_exact(dst_label_col(&edge_binding)),
            ),
            Direction::In => (
                col_exact(src_id_col(&edge_binding)),
                col_exact(src_label_col(&edge_binding)),
            ),
            Direction::Both => (
                case_when(
                    source_is_src.clone(),
                    col_exact(dst_id_col(&edge_binding)),
                    col_exact(src_id_col(&edge_binding)),
                ),
                case_when(
                    source_is_src,
                    col_exact(dst_label_col(&edge_binding)),
                    col_exact(src_label_col(&edge_binding)),
                ),
            ),
        };
        let next_rels = if rel_binding.is_some() {
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
            string_concat(col_exact(WORK_TRAIL), edge_key).alias(WORK_TRAIL),
            next_nodes.alias(WORK_NODES),
            next_rels.alias(WORK_RELS),
        ]);
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
                consumed = LogicalPlanBuilder::from(consumed)
                    .filter(Expr::and(
                        binary(
                            col_exact(id_col(target)),
                            BinaryOp::Eq,
                            col_exact(WORK_CUR_ID),
                        ),
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
                consumed = LogicalPlanBuilder::from(consumed)
                    .join_on(
                        target_scan.plan,
                        JoinType::Inner,
                        vec![
                            binary(
                                col_exact(id_col(&target_scan_binding)),
                                BinaryOp::Eq,
                                col_exact(WORK_CUR_ID),
                            ),
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
            .filter(|name| has_exact_col(&consumed, name))
            .map(|name| col_exact(&name).alias(name))
            .collect::<Vec<_>>();
        if let Some(rel) = rel_binding {
            let target_display =
                self.cypher_element_display_expr(&consumed, target, BindingShape::Node)?;
            final_projection.push(
                concat_exprs(vec![
                    lit("{_NODES: ["),
                    append_csv(col_exact(WORK_NODES), target_display),
                    lit("], _RELS: ["),
                    col_exact(WORK_RELS),
                    lit("]}"),
                ])
                .alias(rel),
            );
            final_projection.push(col_exact(WORK_DEPTH).alias(path_len_col(rel)));
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
                        binary(
                            col_exact(id_col(target)),
                            BinaryOp::Eq,
                            col_exact(id_col(source)),
                        ),
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
                Some(rel) => self.project_varlen_path(plan, rel, &[], &[])?,
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
                hop_rels.push(hop_rel);
                hop_source = hop_target;
            }
            let mut distinct_filters = Vec::new();
            for i in 0..hop_rels.len() {
                for j in (i + 1)..hop_rels.len() {
                    let same = Expr::and(
                        binary(
                            col_exact(id_col(&hop_rels[i])),
                            BinaryOp::Eq,
                            col_exact(id_col(&hop_rels[j])),
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
                plan = self.project_varlen_path(plan, rel, &hop_nodes, &hop_rels)?;
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
        let mut parts = vec![lit("{_NODES: [")];
        for (index, node) in hop_nodes.iter().enumerate() {
            if index > 0 {
                parts.push(lit(","));
            }
            parts.push(render(node, BindingShape::Node)?);
        }
        parts.push(lit("], _RELS: ["));
        for (index, rel) in hop_rels.iter().enumerate() {
            if index > 0 {
                parts.push(lit(","));
            }
            parts.push(render(rel, BindingShape::Edge)?);
        }
        parts.push(lit("]}"));

        let mut projections = existing_columns(
            &plan,
            &BTreeSet::from([rel_binding.to_string(), path_len_col(rel_binding)]),
        );
        projections.push(concat_exprs(parts).alias(rel_binding));
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

fn case_when(condition: Expr, then_expr: Expr, else_expr: Expr) -> Expr {
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
