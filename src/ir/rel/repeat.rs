//! `GraphRepeat` lowering.
//!
//! The primary lowering is a recursive CTE whose work table carries the
//! repeat frontier: the element identity (`id`, `label`) of every binding the
//! body's `GraphCorrelate` leaf consumes, any carried scalar bindings, the
//! iteration count and — for `until` loops — whether the row has terminated.
//! Element properties are re-attached from the node/relationship scans both
//! when the body consumes the frontier and after the recursion, so the work
//! table schema stays fixed across iterations.
//!
//! Repeat lowering preserves the language execution contract:
//!
//! * `times(n)` bounds the iteration count;
//! * `until(p)` is checked after every step; matching rows stop advancing and
//!   are emitted (after the loop) when no `emit` is attached;
//! * `emit()` / `emit(p)` emit every stepped row (matching `p`), and a prefix
//!   `emit(p).repeat(...)` additionally emits matching seed rows;
//! * `loops()` is the iteration count: the body sees the 0-based iteration,
//!   `until` / `emit` see the post-step count.
//!
//! Unbounded loops terminate through the frontier becoming empty or `until`.
//! A loop that is still live after
//! [`MAX_REPEAT_ITERATIONS`] iterations fails loudly instead of returning a
//! truncated answer: the recursive term raises a runtime error.
//!
//! Bodies the recursive form cannot express (per-iteration barriers such as
//! `order`/`limit`/`dedup`, nested recursion, bodies that would reference the
//! work table more than once) fall back to bounded unrolling for small
//! `times(n)` loops and decline otherwise.

use std::any::Any;

use datafusion::common::tree_node::TreeNodeRecursion;
use datafusion::datasource::cte_worktable::CteWorkTable;
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};

use super::varlen::case_when;
use super::*;
use crate::ir::plan::EmitMode;

/// Loop ceiling shared with the relational control kernel; reaching it is an
/// error, never a truncated result.
const MAX_REPEAT_ITERATIONS: u32 = 10_000;
/// Largest `times(n)` the unrolled fallback expands.
const REPEAT_UNROLL_CAP: u32 = 8;
const LOOPS_BINDING: &str = "__loops";

/// Arguments of a `GraphRepeat` node.
pub(super) struct RepeatSpec<'n> {
    pub loop_name: Option<&'n str>,
    pub times: Option<u32>,
    pub emit: &'n EmitMode,
    pub until_first: bool,
    pub until: Option<&'n IrExpr>,
    pub until_traversal: Option<&'n Node>,
    pub path: Option<&'n str>,
    pub prefix_predicate: Option<&'n IrExpr>,
    pub prefix_traversal: Option<&'n Node>,
    pub seed: &'n Node,
    pub body: &'n Node,
}

/// One column group of the recursive work table.
#[derive(Debug, Clone)]
enum StateCol {
    /// An element binding carried by identity only. `seed_type` is `None`
    /// when the seed lacks the binding (the body introduces it); the seed row
    /// then carries a typed null identity.
    Element {
        binding: String,
        shape: BindingShape,
        absent_id_type: Option<DataType>,
    },
    /// A scalar column carried verbatim. `absent_type` is set when the seed
    /// lacks the column, which then starts as a typed null.
    Scalar {
        column: String,
        absent_type: Option<DataType>,
    },
}

impl LoweringContext<'_> {
    pub(super) fn lower_repeat(&mut self, spec: RepeatSpec<'_>) -> RelResult<LoweredNode> {
        if spec.path.is_some() && !self.options.tolerate_internal_path_state {
            return Err(RelError::Unsupported("GraphRepeat with path".into()));
        }
        match self.lower_repeat_recursive(&spec) {
            Ok(lowered) => Ok(lowered),
            Err(recursive_error) if unroll_applicable(&spec) => {
                self.lower_repeat_unrolled(&spec).map_err(|unroll_error| {
                    RelError::Unsupported(format!(
                        "recursive GraphRepeat lowering failed ({recursive_error}); \
                         bounded-unroll fallback also failed ({unroll_error})"
                    ))
                })
            }
            Err(error) => Err(error),
        }
    }

    fn lower_repeat_recursive(&mut self, spec: &RepeatSpec<'_>) -> RelResult<LoweredNode> {
        check_recursive_body(spec.body)?;
        let Some(bindings) = repeat_correlate_bindings(spec.body) else {
            return Err(RelError::Unsupported(
                "GraphRepeat body does not consume the frontier".into(),
            ));
        };
        let loop_columns = loop_columns(spec.loop_name);

        let seed = self.lower_node(spec.seed)?;
        if output_fields(&seed.plan)
            .iter()
            .any(|field| field == LOOPS_BINDING || field.starts_with("__loops:"))
        {
            // The outer loop counter would have to survive the inner loop's
            // rows; use the native control kernel or unrolled form.
            return Err(RelError::Unsupported(
                "GraphRepeat nested in a loop-counting scope".into(),
            ));
        }
        let seed_plan = strip_root_sorts(seed.plan.clone());

        let mut state = Vec::new();
        let mut absent = Vec::new();
        for binding in &bindings {
            if binding == LOOPS_BINDING || binding.starts_with("__loops:") {
                continue;
            }
            if let Some(shape) = has_binding_shape(&seed_plan, binding) {
                state.push(StateCol::Element {
                    binding: binding.clone(),
                    shape,
                    absent_id_type: None,
                });
            } else if has_exact_col(&seed_plan, binding) {
                state.push(StateCol::Scalar {
                    column: binding.clone(),
                    absent_type: None,
                });
            } else {
                absent.push(binding.clone());
            }
        }
        for key in apply_correlation_key_columns(&seed_plan) {
            state.push(StateCol::Scalar {
                column: key,
                absent_type: None,
            });
        }

        self.scan_counter += 1;
        let uniq = self.scan_counter;
        let cte_name = format!("__graph_repeat_{uniq}");
        let depth_col = format!("__rep_{uniq}_depth");
        let stop_col = format!("__rep_{uniq}_stop");
        let has_until = spec.until.is_some() || spec.until_traversal.is_some();
        let mut seed_input = LogicalPlanBuilder::from(seed_plan)
            .alias(format!("__rep_{uniq}_seed"))?
            .build()?;

        let mut seed_islands = seed.islands.clone();
        let seed_stop = if spec.until_first {
            let mut fields = output_fields(&seed_input).iter().map(col_exact).collect::<Vec<_>>();
            for column in &loop_columns { fields.push(lit(0_i64).alias(column)); }
            seed_input = LogicalPlanBuilder::from(seed_input).project(fields)?.build()?;
            if let Some(probe) = spec.until_traversal {
                let (joined, matched) = self.probe_match(seed_input, probe, &mut seed_islands)?;
                seed_input = joined;
                Some(matched)
            } else if let Some(predicate) = spec.until {
                Some(case_when(self.lower_expr(&seed_input, predicate)?, lit(true), lit(false)))
            } else { None }
        } else { None };

        // At most two passes: the first discovers the types of bindings the
        // body introduces but the seed lacks (e.g. Gremlin's `__path`), the
        // second carries them.
        let mut attempt = 0;
        let (static_term, recursive_term, body_islands) = loop {
            attempt += 1;
            let mut islands = IslandReport::default();
            let static_term = build_static_term(
                &seed_input,
                &state,
                &depth_col,
                has_until.then_some(stop_col.as_str()),
                seed_stop.as_ref(),
            )?;
            let work_schema = Arc::new(static_term.schema().as_arrow().clone());
            let work_table = Arc::new(CteWorkTable::new(&cte_name, work_schema));
            let mut work =
                LogicalPlanBuilder::scan(&cte_name, provider_as_source(work_table), None)?
                    .build()?;
            if let Some(live) = live_filter(
                spec.times,
                has_until.then_some(stop_col.as_str()),
                &depth_col,
            ) {
                work = LogicalPlanBuilder::from(work).filter(live)?.build()?;
            }
            // The body sees the 0-based iteration as `loops()`.
            let mut feed_projection = output_fields(&work)
                .iter()
                .map(col_exact)
                .collect::<Vec<_>>();
            for loop_column in &loop_columns {
                feed_projection.push(col_exact(&depth_col).alias(loop_column));
            }
            let work = LogicalPlanBuilder::from(work)
                .project(feed_projection)?
                .build()?;
            let feed = self.repeat_rehydrate(work, &state, &mut islands)?;
            let body = self.lower_with_correlate(feed, spec.body)?;
            islands.merge(body.islands);
            let stepped = body.plan;

            // Resolve bindings the seed lacks from the body's output.
            let mut resolved = false;
            let mut still_absent = Vec::new();
            for binding in std::mem::take(&mut absent) {
                if let Some(shape) = has_binding_shape(&stepped, &binding) {
                    let id_type =
                        column_type(&stepped, &id_col(&binding)).unwrap_or(DataType::Int64);
                    state.push(StateCol::Element {
                        binding,
                        shape,
                        absent_id_type: Some(id_type),
                    });
                    resolved = true;
                } else if let Some(data_type) = column_type(&stepped, &binding) {
                    let data_type = if data_type == DataType::Null {
                        DataType::Utf8
                    } else {
                        data_type
                    };
                    state.push(StateCol::Scalar {
                        column: binding,
                        absent_type: Some(data_type),
                    });
                    resolved = true;
                } else {
                    still_absent.push(binding);
                }
            }
            absent = still_absent;
            if resolved {
                if attempt >= 2 {
                    return Err(RelError::Unsupported(
                        "GraphRepeat body state did not stabilize".into(),
                    ));
                }
                continue;
            }

            if !has_exact_col(&stepped, &depth_col) {
                return Err(RelError::Unsupported(
                    "GraphRepeat body does not preserve iteration state".into(),
                ));
            }
            for col in &state {
                match col {
                    StateCol::Element { binding, shape, .. } => {
                        if has_binding_shape(&stepped, binding) != Some(*shape) {
                            return Err(RelError::Unsupported(format!(
                                "GraphRepeat body changes the shape of `{binding}`"
                            )));
                        }
                    }
                    StateCol::Scalar { column, .. } => {
                        if !has_exact_col(&stepped, column) {
                            return Err(RelError::Unsupported(format!(
                                "GraphRepeat body drops carried column `{column}`"
                            )));
                        }
                    }
                }
            }

            // `until` / `emit` observe the post-step loop count.
            let next_depth = binary(col_exact(&depth_col), BinaryOp::Add, lit(1_i64));
            let mut stepped_projection = output_fields(&stepped)
                .into_iter()
                .filter(|field| !loop_columns.contains(field))
                .map(col_exact)
                .collect::<Vec<_>>();
            for loop_column in &loop_columns {
                stepped_projection.push(next_depth.clone().alias(loop_column));
            }
            let mut stepped = LogicalPlanBuilder::from(stepped)
                .project(stepped_projection)?
                .build()?;
            // `until(<traversal>)` is decided per element: join the set of
            // elements for which the probe is productive.
            let mut until_matched = None;
            if let Some(probe) = spec.until_traversal {
                let (joined, matched) = self.probe_match(stepped, probe, &mut islands)?;
                stepped = joined;
                until_matched = Some(matched);
            }

            let mut recursive_projection = Vec::new();
            for col in &state {
                match col {
                    StateCol::Element { binding, .. } => {
                        recursive_projection
                            .push(col_exact(id_col(binding)).alias(id_col(binding)));
                        recursive_projection
                            .push(col_exact(label_col(binding)).alias(label_col(binding)));
                    }
                    StateCol::Scalar { column, .. } => {
                        recursive_projection.push(col_exact(column).alias(column));
                    }
                }
            }
            recursive_projection.push(next_depth.alias(&depth_col));
            if let Some(until) = spec.until {
                let matched = self.lower_expr(&stepped, until)?;
                recursive_projection
                    .push(case_when(matched, lit(true), lit(false)).alias(&stop_col));
            } else if let Some(matched) = until_matched {
                recursive_projection.push(matched.alias(&stop_col));
            }
            let recursive_term = LogicalPlanBuilder::from(stepped)
                .project(recursive_projection)?
                .build()?;
            check_recursive_term(&recursive_term, &cte_name)?;
            break (static_term, recursive_term, islands);
        };

        let recursive_query = LogicalPlanBuilder::from(static_term)
            .to_recursive_query(cte_name, recursive_term, false)?
            .build()?;

        let emit_each = !matches!(spec.emit, EmitMode::AfterLoop);
        let depth = || col_exact(&depth_col);
        let structural = if emit_each {
            (spec.prefix_predicate.is_none() && spec.prefix_traversal.is_none())
                .then(|| {
                    let advanced = binary(depth(), BinaryOp::Gte, lit(1_i64));
                    if spec.until_first && has_until { Expr::or(advanced, col_exact(&stop_col)) } else { advanced }
                })
        } else if has_until {
            Some(binary(col_exact(&stop_col), BinaryOp::Eq, lit(true)))
        } else if let Some(times) = spec.times {
            Some(binary(depth(), BinaryOp::Eq, lit(i64::from(times))))
        } else {
            // Without `times`/`until` the loop only ends once the frontier
            // is empty, so nothing survives it. The recursion still runs (and
            // still trips the iteration guard) because the filter is not a
            // constant.
            Some(binary(depth(), BinaryOp::Lt, lit(0_i64)))
        };
        let mut selected = recursive_query;
        if let Some(structural) = structural {
            selected = LogicalPlanBuilder::from(selected)
                .filter(structural)?
                .build()?;
        }
        let mut with_loops = output_fields(&selected)
            .iter()
            .map(col_exact)
            .collect::<Vec<_>>();
        for loop_column in &loop_columns {
            with_loops.push(
                case_when(
                    binary(depth(), BinaryOp::Eq, lit(0_i64)),
                    Expr::Cast(Cast::new(Box::new(lit(ScalarValue::Null)), DataType::Int64)),
                    depth(),
                )
                .alias(loop_column),
            );
        }
        let selected = LogicalPlanBuilder::from(selected)
            .project(with_loops)?
            .build()?;
        let mut islands = seed_islands;
        islands.merge(body_islands);
        let mut output = self.repeat_rehydrate(selected, &state, &mut islands)?;

        let mut probe_columns = Vec::new();
        if emit_each {
            let iteration_rows = binary(depth(), BinaryOp::Gte, lit(1_i64));
            let iteration_rows = match spec.emit {
                EmitMode::AfterEachIfPredicate(predicate) => {
                    Expr::and(iteration_rows, self.lower_expr(&output, predicate)?)
                }
                EmitMode::AfterEachIfTraversal(probe) => {
                    let before = output_fields(&output);
                    let (joined, matched) = self.probe_match(output, probe, &mut islands)?;
                    probe_columns.extend(
                        output_fields(&joined)
                            .into_iter()
                            .filter(|field| !before.contains(field)),
                    );
                    output = joined;
                    Expr::and(iteration_rows, matched)
                }
                _ => iteration_rows,
            };
            let seed_rows = match (spec.prefix_predicate, spec.prefix_traversal) {
                (Some(predicate), _) => Some(self.lower_expr(&output, predicate)?),
                (None, Some(probe)) => {
                    let before = output_fields(&output);
                    let (joined, matched) = self.probe_match(output, probe, &mut islands)?;
                    probe_columns.extend(
                        output_fields(&joined)
                            .into_iter()
                            .filter(|field| !before.contains(field)),
                    );
                    output = joined;
                    Some(matched)
                }
                (None, None) => None,
            };
            let condition = match seed_rows {
                Some(seed_rows) => Expr::or(
                    iteration_rows,
                    Expr::and(binary(depth(), BinaryOp::Eq, lit(0_i64)), seed_rows),
                ),
                None => iteration_rows,
            };
            let condition = if has_until { Expr::or(condition, col_exact(&stop_col)) } else { condition };
            output = LogicalPlanBuilder::from(output)
                .filter(condition)?
                .build()?;
        }

        let final_projection = output_fields(&output)
            .into_iter()
            .filter(|field| {
                *field != depth_col && *field != stop_col && !probe_columns.contains(field)
            })
            .map(col_exact)
            .collect::<Vec<_>>();
        let plan = LogicalPlanBuilder::from(output)
            .project(final_projection)?
            .build()?;
        Ok(LoweredNode {
            plan,
            islands,
            fields: seed.fields,
            result_form: seed.result_form,
        })
    }

    /// Re-attach element properties (and relationship endpoints) to a plan
    /// that carries element bindings by identity only.
    fn repeat_rehydrate(
        &mut self,
        mut plan: LogicalPlan,
        state: &[StateCol],
        islands: &mut IslandReport,
    ) -> RelResult<LogicalPlan> {
        for col in state {
            let StateCol::Element { binding, shape, .. } = col else {
                continue;
            };
            self.scan_counter += 1;
            let scan_binding = format!("__rep_el_{}", self.scan_counter);
            let scan = match shape {
                BindingShape::Node => self.lower_node_scan(&scan_binding, &LabelExpr::Any)?,
                BindingShape::Edge => self.lower_rel_scan(&scan_binding, &LabelExpr::Any)?,
            };
            islands.merge(scan.islands);
            let mut projection = output_fields(&plan)
                .iter()
                .map(col_exact)
                .collect::<Vec<_>>();
            for field in output_fields(&scan.plan) {
                if field == id_col(&scan_binding) || field == label_col(&scan_binding) {
                    continue;
                }
                let Some(suffix) = field.strip_prefix(scan_binding.as_str()) else {
                    continue;
                };
                let target = format!("{binding}{suffix}");
                if has_exact_col(&plan, &target) {
                    continue;
                }
                projection.push(col_exact(&field).alias(target));
            }
            let joined = LogicalPlanBuilder::from(plan)
                .join_on(
                    scan.plan,
                    JoinType::Left,
                    vec![
                        binary(
                            col_exact(id_col(binding)),
                            BinaryOp::Eq,
                            col_exact(id_col(&scan_binding)),
                        ),
                        binary(
                            col_exact(label_col(binding)),
                            BinaryOp::Eq,
                            col_exact(label_col(&scan_binding)),
                        ),
                    ],
                )?
                .build()?;
            plan = LogicalPlanBuilder::from(joined)
                .project(projection)?
                .build()?;
        }
        Ok(plan)
    }

    /// Join `plan` with the set of `current` elements for which the
    /// sub-traversal `probe` produces at least one result, returning the
    /// joined plan and a boolean "probe is productive" expression.
    ///
    /// The set is computed once over every element of the frontier's shape,
    /// not per frontier row, so it never re-reads a recursive work table.
    /// That is only sound when the probe observes nothing but the element
    /// itself: probes that read loop counters, labels, sacks or the path fail
    /// to lower over the element scan and are declined.
    fn probe_match(
        &mut self,
        plan: LogicalPlan,
        probe: &Node,
        islands: &mut IslandReport,
    ) -> RelResult<(LogicalPlan, Expr)> {
        let Some(shape) = has_binding_shape(&plan, CURRENT_BINDING) else {
            return Err(RelError::Unsupported(
                "GraphRepeat probe over a non-element traverser".into(),
            ));
        };
        let mut probe = probe.clone();
        strip_path_tracking(&mut probe)?;
        match first_correlate_bindings(&probe) {
            Some(bindings) if bindings.iter().all(|binding| binding == CURRENT_BINDING) => {}
            Some(bindings) => {
                return Err(RelError::Unsupported(format!(
                    "GraphRepeat probe correlates traverser state {bindings:?}"
                )));
            }
            None => {
                return Err(RelError::Unsupported(
                    "GraphRepeat probe does not consume the traverser".into(),
                ));
            }
        }
        let elements = match shape {
            BindingShape::Node => Node::GraphNodeScan {
                graph: "default".into(),
                binding: CURRENT_BINDING.into(),
                labels: LabelExpr::Any,
            }
            .bind_node(CURRENT_BINDING),
            BindingShape::Edge => Node::GraphBind {
                bind: CURRENT_BINDING.into(),
                kind: crate::ir::plan::BindKind::Edge,
                expr: None,
                input: Node::GraphRelScan {
                    graph: "default".into(),
                    binding: CURRENT_BINDING.into(),
                    types: LabelExpr::Any,
                    dir: crate::ir::plan::Direction::Out,
                }
                .boxed(),
            },
        };
        let semi = Node::GraphApply {
            kind: crate::ir::plan::ApplyKind::Semi,
            correlation: vec![CURRENT_BINDING.into()],
            outputs: Vec::new(),
            optional_missing: crate::ir::policy::OptionalMissing::Null,
            left: elements.boxed(),
            right: probe.boxed(),
        };
        let productive = self.lower_node(&semi)?;
        islands.merge(productive.islands);
        self.scan_counter += 1;
        let uniq = self.scan_counter;
        let key_id = format!("__rep_probe_{uniq}_id");
        let key_label = format!("__rep_probe_{uniq}_label");
        let matched_set = LogicalPlanBuilder::from(productive.plan)
            .project(vec![
                col_exact(id_col(CURRENT_BINDING)).alias(&key_id),
                col_exact(label_col(CURRENT_BINDING)).alias(&key_label),
            ])?
            .distinct()?
            .build()?;
        let joined = LogicalPlanBuilder::from(plan)
            .join_on(
                matched_set,
                JoinType::Left,
                vec![
                    binary(
                        col_exact(id_col(CURRENT_BINDING)),
                        BinaryOp::Eq,
                        col_exact(&key_id),
                    ),
                    binary(
                        col_exact(label_col(CURRENT_BINDING)),
                        BinaryOp::Eq,
                        col_exact(&key_label),
                    ),
                ],
            )?
            .build()?;
        Ok((joined, col_exact(&key_id).is_not_null()))
    }

    /// `repeat(body).times(n)` via bounded unrolling: apply the lowered body
    /// n times, feeding each iteration's plan into the body's
    /// `GraphCorrelate` leaf. Used for bodies the recursive form declines
    /// (per-iteration barriers), where every iteration is its own subplan.
    fn lower_repeat_unrolled(&mut self, spec: &RepeatSpec<'_>) -> RelResult<LoweredNode> {
        if spec.until.is_some() || spec.until_traversal.is_some() {
            return Err(RelError::Unsupported(
                "GraphRepeat with until termination".into(),
            ));
        }
        if spec.prefix_traversal.is_some() {
            return Err(RelError::Unsupported(
                "GraphRepeat with emit sub-traversal".into(),
            ));
        }
        let Some(times) = spec.times else {
            return Err(RelError::Unsupported(
                "GraphRepeat without times bound".into(),
            ));
        };
        if times > REPEAT_UNROLL_CAP {
            return Err(RelError::Unsupported(format!(
                "GraphRepeat times {times} exceeds unroll cap {REPEAT_UNROLL_CAP}"
            )));
        }
        if times > 1 {
            let mut pending = vec![spec.body];
            while let Some(node) = pending.pop() {
                if matches!(node, Node::GraphDistinct { .. }) {
                    // The repeat control kernel shares a seen set across rounds.
                    // Independent SQL DISTINCT windows would reset that state.
                    return Err(RelError::Unsupported(
                        "GraphRepeat with stateful deduplication across iterations".into(),
                    ));
                }
                pending.extend(node_children(node));
            }
        }
        // Whether the seed itself is emitted (`emit()` before `repeat`).
        let emit_seed = match (spec.emit, spec.prefix_predicate) {
            (EmitMode::AfterLoop, _) => false,
            (_, Some(predicate)) => match constant_value_expr(predicate) {
                Ok(Some(Value::Bool(value))) => value,
                _ => {
                    return Err(RelError::Unsupported(
                        "GraphRepeat with non-constant emit predicate".into(),
                    ));
                }
            },
            // The seed is only emitted when a
            // prefix-emit predicate/traversal was attached.
            (EmitMode::AfterEachIteration, None) => false,
            (EmitMode::AfterEachIfPredicate(_) | EmitMode::AfterEachIfTraversal(_), None) => {
                return Err(RelError::Unsupported(
                    "GraphRepeat with conditional emit".into(),
                ));
            }
        };
        let emit_each = !matches!(spec.emit, EmitMode::AfterLoop);

        let seed = self.lower_node(spec.seed)?;
        let mut islands = seed.islands.clone();
        let mut current = seed.plan.clone();
        let mut emitted: Vec<LogicalPlan> = Vec::new();
        if emit_seed {
            emitted.push(current.clone());
        }
        let correlate_bindings = repeat_correlate_bindings(spec.body);
        for _ in 0..times {
            // The body re-lowers with fixed binding names each iteration;
            // restrict the incoming plan to the bindings its correlate leaf
            // consumes so re-introduced scans do not collide with leftover
            // columns from the previous iteration.
            let feed = match &correlate_bindings {
                Some(bindings) => {
                    let mut projections = apply_correlation_key_columns(&current)
                        .iter()
                        .map(col_exact)
                        .collect::<Vec<_>>();
                    for field in output_fields(&current) {
                        if bindings
                            .iter()
                            .any(|binding| field == *binding || is_binding_column(&field, binding))
                            && !projections
                                .iter()
                                .any(|expr| matches!(expr, Expr::Column(col) if col.name == field))
                        {
                            projections.push(col_exact(&field));
                        }
                    }
                    if projections.is_empty() {
                        current.clone()
                    } else {
                        LogicalPlanBuilder::from(current.clone())
                            .project(projections)?
                            .build()?
                    }
                }
                None => current.clone(),
            };
            let iteration = self.lower_with_correlate(feed, spec.body)?;
            islands.merge(iteration.islands);
            current = iteration.plan;
            if emit_each {
                emitted.push(current.clone());
            }
        }
        if !emit_each {
            emitted.push(current);
        }
        let mut union_plan: Option<LogicalPlan> = None;
        for branch in emitted {
            union_plan = Some(match union_plan {
                None => branch,
                Some(plan) => LogicalPlanBuilder::from(plan)
                    .union_by_name(branch)?
                    .build()?,
            });
        }
        let plan = union_plan
            .ok_or_else(|| RelError::Unsupported("GraphRepeat emitted no iterations".into()))?;
        Ok(LoweredNode {
            plan,
            islands,
            fields: seed.fields,
            result_form: seed.result_form,
        })
    }
}

const CURRENT_BINDING: &str = "current";
const PATH_BINDING: &str = "__path";

/// Remove the (unobservable) path bookkeeping from an existence probe so it
/// can run over a plain element scan. Only operators whose result does not
/// depend on the removed path are accepted; anything else still referencing
/// `__path` fails to lower afterwards and is declined.
fn strip_path_tracking(node: &mut Node) -> RelResult<()> {
    match node {
        Node::GraphCorrelate { bindings } => {
            bindings.retain(|binding| binding != PATH_BINDING);
            Ok(())
        }
        Node::GraphExpand { path, input, .. } => {
            *path = None;
            strip_path_tracking(input)
        }
        Node::GraphProject { items, input, .. } => {
            items.retain(|item| item.alias != PATH_BINDING);
            strip_path_tracking(input)?;
            if items.is_empty() {
                let inner = std::mem::replace(input.as_mut(), Node::GraphEmpty);
                *node = inner;
            }
            Ok(())
        }
        Node::GraphApply {
            correlation,
            outputs,
            left,
            right,
            ..
        } => {
            correlation.retain(|binding| binding != PATH_BINDING);
            outputs.retain(|binding| binding != PATH_BINDING);
            strip_path_tracking(left)?;
            strip_path_tracking(right)
        }
        Node::GraphUnion { left, right, .. } | Node::GraphJoin { left, right, .. } => {
            strip_path_tracking(left)?;
            strip_path_tracking(right)
        }
        Node::GraphFilter { input, .. }
        | Node::GraphCurrentProject { input, .. }
        | Node::GraphAggregate { input, .. }
        | Node::GraphBind { input, .. }
        | Node::GraphSlice { input, .. }
        | Node::GraphDistinct { input, .. }
        | Node::GraphSort { input, .. }
        | Node::GraphUnwind { input, .. } => strip_path_tracking(input),
        Node::GraphNodeScan { .. }
        | Node::GraphRelScan { .. }
        | Node::GraphValues { .. }
        | Node::GraphOneRow
        | Node::GraphEmpty => Ok(()),
        other => Err(RelError::Unsupported(format!(
            "GraphRepeat probe with {}",
            node_kind(other)
        ))),
    }
}

fn unroll_applicable(spec: &RepeatSpec<'_>) -> bool {
    spec.until.is_none()
        && spec.until_traversal.is_none()
        && spec.prefix_traversal.is_none()
        && spec.times.is_some_and(|times| times <= REPEAT_UNROLL_CAP)
}

fn loop_columns(loop_name: Option<&str>) -> Vec<String> {
    let mut columns = vec![LOOPS_BINDING.to_string()];
    if let Some(name) = loop_name {
        columns.push(format!("{LOOPS_BINDING}:{name}"));
    }
    columns
}

fn column_type(plan: &LogicalPlan, name: &str) -> Option<DataType> {
    plan.schema()
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .map(|field| field.data_type().clone())
}

fn typed_null(data_type: DataType) -> Expr {
    Expr::Cast(Cast::new(Box::new(lit(ScalarValue::Null)), data_type))
}

/// A bag-valued recursion does not observe the order of its seed, and an
/// `ORDER BY` inside the CTE's first branch is not valid SQL. Only sorts
/// without a fetch are dropped; a top-k keeps its sort.
fn strip_root_sorts(plan: LogicalPlan) -> LogicalPlan {
    match plan {
        LogicalPlan::Sort(sort) if sort.fetch.is_none() => {
            strip_root_sorts(sort.input.as_ref().clone())
        }
        other => other,
    }
}

fn build_static_term(
    seed: &LogicalPlan,
    state: &[StateCol],
    depth_col: &str,
    stop_col: Option<&str>,
    seed_stop: Option<&Expr>,
) -> RelResult<LogicalPlan> {
    let mut projection = Vec::new();
    for col in state {
        match col {
            StateCol::Element {
                binding,
                absent_id_type: None,
                ..
            } => {
                projection.push(col_exact(id_col(binding)).alias(id_col(binding)));
                projection.push(col_exact(label_col(binding)).alias(label_col(binding)));
            }
            StateCol::Element {
                binding,
                absent_id_type: Some(id_type),
                ..
            } => {
                projection.push(typed_null(id_type.clone()).alias(id_col(binding)));
                projection.push(typed_null(DataType::Utf8).alias(label_col(binding)));
            }
            StateCol::Scalar {
                column,
                absent_type: None,
            } => projection.push(col_exact(column).alias(column)),
            StateCol::Scalar {
                column,
                absent_type: Some(data_type),
            } => projection.push(typed_null(data_type.clone()).alias(column)),
        }
    }
    projection.push(lit(0_i64).alias(depth_col));
    if let Some(stop_col) = stop_col {
        projection.push(seed_stop.cloned().unwrap_or_else(|| lit(false)).alias(stop_col));
    }
    Ok(LogicalPlanBuilder::from(seed.clone())
        .project(projection)?
        .build()?)
}

/// Rows of the work table that advance into another iteration. When the
/// loop is not bounded by a `times(n)` within the iteration ceiling, a live
/// row at the ceiling raises an error instead of being silently dropped.
fn live_filter(times: Option<u32>, stop_col: Option<&str>, depth_col: &str) -> Option<Expr> {
    let mut live: Option<Expr> = None;
    if let Some(stop_col) = stop_col {
        live = Some(Expr::Not(Box::new(col_exact(stop_col))));
    }
    if let Some(times) = times {
        let bound = binary(col_exact(depth_col), BinaryOp::Lt, lit(i64::from(times)));
        live = Some(match live {
            Some(live) => Expr::and(live, bound),
            None => bound,
        });
    }
    let guarded = times.is_none_or(|times| times > MAX_REPEAT_ITERATIONS);
    if !guarded {
        return live;
    }
    let at_ceiling = binary(
        col_exact(depth_col),
        BinaryOp::Gte,
        lit(i64::from(MAX_REPEAT_ITERATIONS)),
    );
    let live_expr = live.unwrap_or_else(|| lit(true));
    Some(case_when(
        Expr::and(live_expr.clone(), at_ceiling),
        repeat_limit_error(),
        live_expr,
    ))
}

fn repeat_limit_error() -> Expr {
    let udf = Arc::new(ScalarUDF::new_from_impl(RepeatLimitError::new()));
    udf.call(vec![lit(format!(
        "repeat exceeded {MAX_REPEAT_ITERATIONS} iterations"
    ))])
}

/// Only row-local operators may appear in a recursive body: barriers observe
/// the whole per-iteration frontier (and `dedup` shares state across
/// iterations), which a recursive term cannot express.
fn check_recursive_body(body: &Node) -> RelResult<()> {
    let mut pending = vec![body];
    while let Some(node) = pending.pop() {
        let ok = match node {
            Node::GraphExpand {
                length, history, ..
            } => length.min == 1 && length.max == Some(1) && history.is_none(),
            Node::GraphCorrelate { .. }
            | Node::GraphFilter { .. }
            | Node::GraphProject { .. }
            | Node::GraphCurrentProject { .. }
            | Node::GraphBind { .. }
            | Node::GraphUnwind { .. }
            | Node::GraphQuantifier { .. }
            | Node::GraphListComprehension { .. }
            | Node::GraphSelect { .. }
            | Node::GraphReturn { .. }
            | Node::GraphValues { .. }
            | Node::GraphOneRow
            | Node::GraphEmpty
            | Node::GraphNodeScan { .. }
            | Node::GraphRelScan { .. }
            | Node::GraphApply { .. }
            | Node::GraphUnion { .. }
            | Node::GraphJoin { .. }
            | Node::GraphChoose { .. }
            | Node::GraphCoalesce { .. } => true,
            _ => false,
        };
        if !ok {
            return Err(RelError::Unsupported(format!(
                "GraphRepeat body with {} is not recursion-safe",
                node_kind(node)
            )));
        }
        pending.extend(node_children(node));
    }
    Ok(())
}

fn node_kind(node: &Node) -> String {
    let debug = format!("{node:?}");
    debug
        .split(|c: char| !c.is_ascii_alphanumeric())
        .next()
        .unwrap_or("operator")
        .to_string()
}

/// The recursive term must reference the work table exactly once (Postgres
/// requires it; DataFusion's work table is single-read) and must not contain
/// constructs that the SQL emitter hoists into separate CTEs.
fn check_recursive_term(plan: &LogicalPlan, cte_name: &str) -> RelResult<()> {
    let mut references = 0usize;
    let mut problem: Option<&'static str> = None;
    plan.apply_with_subqueries(|node| {
        // Subplans that never read the frontier (e.g. a precomputed probe
        // set) are evaluated independently of the iteration; barriers there
        // are ordinary relational work.
        if !reads_work_table(node, cte_name) {
            return Ok(TreeNodeRecursion::Jump);
        }
        match node {
            LogicalPlan::TableScan(scan) if scan.table_name.table() == cte_name => {
                references += 1;
            }
            LogicalPlan::RecursiveQuery(_) => problem = Some("nested recursion"),
            LogicalPlan::SubqueryAlias(alias)
                if alias.alias.table().starts_with("__w_collect_unique")
                    || alias.alias.table().starts_with("__w_sql_cte_") =>
            {
                problem = Some("a hoisted CTE barrier");
            }
            LogicalPlan::Aggregate(_)
            | LogicalPlan::Window(_)
            | LogicalPlan::Sort(_)
            | LogicalPlan::Limit(_)
            | LogicalPlan::Distinct(_) => problem = Some("a per-iteration barrier"),
            _ => {}
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    if let Some(problem) = problem {
        return Err(RelError::Unsupported(format!(
            "GraphRepeat recursive term contains {problem}"
        )));
    }
    if references != 1 {
        return Err(RelError::Unsupported(format!(
            "GraphRepeat recursive term references the frontier {references} times"
        )));
    }
    Ok(())
}

fn reads_work_table(plan: &LogicalPlan, cte_name: &str) -> bool {
    let mut found = false;
    let _ = plan.apply_with_subqueries(|node| {
        if matches!(node, LogicalPlan::TableScan(scan) if scan.table_name.table() == cte_name) {
            found = true;
            return Ok(TreeNodeRecursion::Stop);
        }
        Ok(TreeNodeRecursion::Continue)
    });
    found
}

/// `error(message)`: raises at execution time for any row that reaches it.
/// DuckDB provides a built-in of the same name, so the unparsed SQL keeps the
/// behavior; it is volatile so no optimizer folds it away.
#[derive(Debug, PartialEq, Eq, Hash)]
struct RepeatLimitError {
    signature: Signature,
}

impl RepeatLimitError {
    fn new() -> Self {
        Self {
            signature: Signature::exact(vec![DataType::Utf8], Volatility::Volatile),
        }
    }
}

impl ScalarUDFImpl for RepeatLimitError {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn name(&self) -> &str {
        "error"
    }

    fn signature(&self) -> &Signature {
        &self.signature
    }

    fn return_type(&self, _arg_types: &[DataType]) -> datafusion::common::Result<DataType> {
        Ok(DataType::Boolean)
    }

    fn invoke_with_args(
        &self,
        args: ScalarFunctionArgs,
    ) -> datafusion::common::Result<ColumnarValue> {
        if args.number_rows == 0 {
            return Ok(ColumnarValue::Scalar(ScalarValue::Boolean(None)));
        }
        let message = match args.args.first() {
            Some(ColumnarValue::Scalar(ScalarValue::Utf8(Some(message)))) => message.clone(),
            _ => "repeat iteration limit exceeded".to_string(),
        };
        Err(DataFusionError::Execution(message))
    }
}

/// Follow the input side of applies first: their right-hand probes have a
/// narrower local correlation scope, not the repeat frontier's state.
fn repeat_correlate_bindings(node: &Node) -> Option<Vec<String>> {
    if let Node::GraphCorrelate { bindings } = node {
        return Some(bindings.clone());
    }
    node_children(node)
        .into_iter()
        .find_map(repeat_correlate_bindings)
}
