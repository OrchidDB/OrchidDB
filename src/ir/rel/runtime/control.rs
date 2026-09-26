//! Correlated relational subplans. Branches and loop bodies are lowered before
//! execution and run through DataFusion with their own frontier relation.
use super::*;
use crate::ir::runtime::ops;
use crate::ir::{
    expr::IrExpr,
    plan::{ApplyKind, ChooseSelector, CoalesceSuccess},
    policy::OptionalMissing,
};

#[derive(Debug, Clone)]
struct Subplan {
    plan: LogicalPlan,
    prepared: Arc<Mutex<Option<PreparedSubplan>>>,
    observable: bool,
    batchable: bool,
    batch_names: std::collections::BTreeSet<String>,
    barrier: bool,
    group_barrier: bool,
    group_split: Option<(Box<Subplan>, Box<Subplan>)>,
    writer_split: Option<(Box<Subplan>, Box<Subplan>, crate::ir::expr::AggKind)>,
}
#[derive(Debug)]
struct PreparedSubplan {
    physical: Arc<dyn ExecutionPlan>,
    state: Arc<Mutex<State>>,
    task: Arc<TaskContext>,
}
impl Subplan {
    fn run(
        &self,
        frontier: Vec<Row>,
        graph: &PropertyGraph,
        ctx: &mut ExecutionContext,
    ) -> IrResult<Vec<Row>> {
        self.run_internal(frontier, graph, ctx, None)
    }
    fn run_internal(
        &self, frontier: Vec<Row>, graph: &PropertyGraph,
        ctx: &mut ExecutionContext, batch_key: Option<String>,
    ) -> IrResult<Vec<Row>> {
        let runtime = tokio::runtime::Handle::current();
        let error = |e: DataFusionError| RuntimeError::Runtime(e.to_string());
        // A query-local slot caches the physical operators, never their output.
        // Taking the slot permits reentrant calls to prepare another instance.
        let cached = self
            .prepared
            .lock()
            .map_err(|_| RuntimeError::Runtime("Subplan cache poisoned".into()))?
            .take();
        let live = State {
            graph: graph.clone(),
            context: std::mem::take(ctx),
            frontier,
            batch_key,
        };
        let prepared = if let Some(prepared) = cached {
            *prepared
                .state
                .lock()
                .map_err(|_| RuntimeError::Runtime("Subplan state poisoned".into()))? = live;
            prepared
        } else {
            let state = Arc::new(Mutex::new(live));
            let session = datafusion::prelude::SessionContext::new_with_config(
                datafusion::prelude::SessionConfig::new().with_target_partitions(1),
            );
            // Correlated lowering uses live native scans rather than frozen SQL
            // snapshots. Each native operator is still a DataFusion physical node.
            let planner =
                datafusion::physical_planner::DefaultPhysicalPlanner::with_extension_planners(
                    vec![Arc::new(KernelPlanner {
                        state: state.clone(),
                    })],
                );
            let physical = match runtime
                .block_on(planner.create_physical_plan(&self.plan, &session.state()))
            {
                Ok(plan) => plan,
                Err(failure) => {
                    *ctx = std::mem::take(&mut state.lock().unwrap().context);
                    return Err(error(failure));
                }
            };
            PreparedSubplan {
                physical,
                state,
                task: session.task_ctx(),
            }
        };
        let result = runtime.block_on(datafusion::physical_plan::collect(
            prepared.physical.clone(),
            prepared.task.clone(),
        ));
        let mut finished = std::mem::take(
            &mut *prepared
                .state
                .lock()
                .map_err(|_| RuntimeError::Runtime("Subplan state poisoned".into()))?,
        );
        *ctx = std::mem::take(&mut finished.context);
        // Empty query state prevents cache cycles through named group reducers,
        // and prevents one frontier's writes or bindings leaking into the next.
        *self
            .prepared
            .lock()
            .map_err(|_| RuntimeError::Runtime("Subplan cache poisoned".into()))? =
            Some(prepared);
        let mut rows = Vec::new();
        for batch in result.map_err(error)? {
            rows.extend(decode_rows(&batch).map_err(error)?);
        }
        graph.restore_execution_overlay(&finished.graph);
        Ok(rows)
    }
}

fn run_with_outer(
    plan: &Subplan,
    row: &Row,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    plan.run(vec![row.clone()], graph, ctx)
}
fn run_body_with_frontier(
    plan: &Subplan,
    rows: Vec<Row>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    plan.run(rows, graph, ctx)
}
fn contains_barrier(plan: &Subplan) -> bool {
    plan.barrier
}
fn has_observable_state(plan: &Subplan) -> bool {
    plan.observable
}
#[derive(Debug, Clone)]
struct ChooseArm {
    key: Option<Value>,
    body: Subplan,
}
#[derive(Debug, Clone)]
enum EmitMode {
    AfterLoop,
    AfterEachIteration,
    AfterEachIfPredicate(IrExpr),
    AfterEachIfTraversal(Box<Subplan>),
}
impl Compiler<'_> {
    fn subplan(&self, node: &Node) -> Result<Subplan> {
        fn observable(node: &Node) -> bool {
            crate::ir::analysis::node_effect(node) != crate::ir::analysis::Effect::Pure
                || crate::ir::analysis::children(node)
                    .into_iter()
                    .any(observable)
        }
        let compiler = Compiler {
            graph: self.graph,
            policy: self.policy.clone(),
            sql: false,
            islands: self.islands.clone(),
        };
        Ok(Subplan {
            plan: optimize::optimize(compiler.lower(node)?, None)?,
            prepared: Default::default(),
            observable: observable(node),
            batchable: optimize::batchable(node),
            batch_names: if optimize::batchable(node) {optimize::batch_names(node)} else {Default::default()},
            barrier: ops::choose::contains_barrier(node),
            group_barrier: ops::aggregate::split_group_prefix(&mut node.clone()).is_some(),
            group_split: None,
            writer_split: None,
        })
    }
    pub(super) fn lower_control(&self, node: &Node) -> Result<LogicalPlan> {
        match node {
            Node::GraphCorrelate { bindings } => {
                let bindings = bindings.clone();
                Ok(kernel("CorrelatedInput", vec![], move |_, state| {
                    let frontier = &state.frontier;
                    if bindings == ["__gremlin_group_members"] {
                        return Ok(frontier.clone());
                    }
                    Ok(frontier
                        .iter()
                        .map(|outer| {
                            let mut row = Row::new();
                            row.bulk = outer.bulk;
                            for (key, value) in &outer.bindings {
                                if state.batch_key.as_ref() == Some(key) || bindings.contains(key)
                                    || [
                                        "__path",
                                        "__path_labels",
                                        "__edge_other",
                                        "__loops",
                                        "__sack",
                                        "__sack_merge",
                                        "__bulk_enabled",
                                        "__gremlin_bulk_safe",
                                        "__repeat_loop_stack",
                                    ]
                                    .contains(&key.as_str())
                                    || key.starts_with("__loops:")
                                    || key.starts_with("__gremlin_select_history_")
                                {
                                    row.bindings.insert(key.clone(), value.clone());
                                }
                            }
                            row
                        })
                        .collect())
                }))
            }
            Node::GraphApply {
                kind,
                correlation,
                outputs,
                optional_missing,
                left,
                right,
            } => {
                let kind = *kind;
                let correlation = correlation.clone();
                let outputs = outputs.clone();
                let optional_missing = *optional_missing;
                let right = self.subplan(right)?;
                let name=if right.batchable {"BatchableLateralApply"} else {"LateralApply"};
                Ok(kernel(
                    name,
                    vec![self.lower(left)?],
                    move |mut inputs, state| {
                        apply_op(
                            kind,
                            &correlation,
                            &outputs,
                            optional_missing,
                            inputs.remove(0),
                            &right,
                            &state.graph,
                            &mut state.context,
                        )
                    },
                ))
            }
            Node::GraphCoalesce {
                success,
                output,
                correlation,
                input,
                arms,
                ..
            } => {
                let success = *success;
                let output = output.clone();
                let correlation = correlation.clone();
                let arms = arms
                    .iter()
                    .map(|a| self.subplan(a))
                    .collect::<Result<Vec<_>>>()?;
                Ok(kernel(
                    "FirstProductive",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        coalesce_op(
                            success,
                            &output,
                            &correlation,
                            inputs.remove(0),
                            &arms,
                            &state.graph,
                            &mut state.context,
                        )
                    },
                ))
            }
            Node::GraphChoose {
                selector,
                correlation,
                arms,
                default,
                input,
                ..
            } => {
                let selector = selector.clone();
                let correlation = correlation.clone();
                let arms = arms
                    .iter()
                    .map(|a| {
                        Ok(ChooseArm {
                            key: a.key.clone(),
                            body: self.subplan(&a.body)?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let default = default.as_ref().map(|d| self.subplan(d)).transpose()?;
                Ok(kernel(
                    "Conditional",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        choose_op(
                            &selector,
                            &correlation,
                            inputs.remove(0),
                            &arms,
                            default.as_ref(),
                            &state.graph,
                            &mut state.context,
                        )
                    },
                ))
            }
            Node::GraphRepeat {
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
            } => {
                let loop_name = loop_name.clone();
                let times = *times;
                let until_first = *until_first;
                let until = until.clone();
                let path = path.clone();
                let prefix_predicate = prefix_predicate.clone();
                let until_traversal = until_traversal
                    .as_ref()
                    .map(|n| self.subplan(n))
                    .transpose()?;
                let prefix_traversal = prefix_traversal
                    .as_ref()
                    .map(|n| self.subplan(n))
                    .transpose()?;
                let body = self.subplan(body)?;
                let emit = match emit {
                    crate::ir::plan::EmitMode::AfterLoop => EmitMode::AfterLoop,
                    crate::ir::plan::EmitMode::AfterEachIteration => EmitMode::AfterEachIteration,
                    crate::ir::plan::EmitMode::AfterEachIfPredicate(p) => {
                        EmitMode::AfterEachIfPredicate(p.clone())
                    }
                    crate::ir::plan::EmitMode::AfterEachIfTraversal(t) => {
                        EmitMode::AfterEachIfTraversal(Box::new(self.subplan(t)?))
                    }
                };
                Ok(kernel(
                    "RecursiveFrontier",
                    vec![self.lower(seed)?],
                    move |mut inputs, state| {
                        repeat_op(
                            loop_name.as_deref(),
                            times,
                            !matches!(emit, EmitMode::AfterLoop),
                            &emit,
                            prefix_predicate.as_ref(),
                            prefix_traversal.as_ref(),
                            until_first,
                            until.as_ref(),
                            until_traversal.as_ref(),
                            path.as_deref(),
                            inputs.remove(0),
                            &body,
                            &state.graph,
                            &mut state.context,
                        )
                    },
                ))
            }
            Node::GraphProcedureCall {
                name,
                args,
                yields,
                input,
                ..
            } => {
                let name = name.clone();
                let args = args.clone();
                let yields = yields.clone();
                let has_input = input.is_some();
                let inputs = input
                    .iter()
                    .map(|n| self.lower(n))
                    .collect::<Result<Vec<_>>>()?;
                Ok(kernel("Procedure", inputs, move |mut inputs, state| {
                    crate::ir::runtime::context::procedure_call_op(
                        &name,
                        &args,
                        &yields,
                        if has_input {
                            inputs.remove(0)
                        } else {
                            vec![Row::new()]
                        },
                        &state.graph,
                    )
                }))
            }
            Node::GraphMerge {
                outputs,
                input,
                match_arm,
                create_arm,
                ..
            } => {
                let outputs = outputs.clone();
                let match_arm = self.subplan(match_arm)?;
                let create_arm = self.subplan(create_arm)?;
                Ok(kernel(
                    "Merge",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        merge_op(
                            &outputs,
                            inputs.remove(0),
                            &match_arm,
                            &create_arm,
                            &state.graph,
                            &mut state.context,
                        )
                    },
                ))
            }
            Node::GraphReadSideEffect { label, input } => {
                let label = label.clone();
                Ok(kernel(
                    "ReadState",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        state
                            .context
                            .read_side_effect(&label, inputs.remove(0), &state.graph)
                    },
                ))
            }
            Node::GraphCap { labels, input } => {
                let labels = labels.clone();
                Ok(kernel(
                    "FinalizeState",
                    vec![self.lower(input)?],
                    move |_, state| state.context.cap_side_effects(&labels, &state.graph),
                ))
            }
            Node::GraphGroupMap {
                key,
                value,
                output,
                input,
            } => {
                let key = key.clone();
                let value = self.group_value(value)?;
                let output = output.clone();
                Ok(kernel(
                    "GroupReduce",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        groups::group_map_op(
                            &key,
                            &value,
                            &output,
                            inputs.remove(0),
                            &state.graph,
                            &mut state.context,
                        )
                    },
                ))
            }
            Node::GraphGroupSideEffect {
                label,
                key,
                value,
                key_input,
                input,
            } => {
                let label = label.clone();
                let key = key.clone();
                let value = self.group_value(value)?;
                let key_input = self.subplan(key_input)?;
                Ok(kernel(
                    "GroupState",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        let rows = inputs.remove(0);
                        groups::group_side_effect_write(
                            &mut state.context,
                            &label,
                            &key,
                            &value,
                            &key_input,
                            &rows,
                            &state.graph,
                        )?;
                        Ok(rows)
                    },
                ))
            }
            Node::GraphSideEffect {
                label,
                value_input,
                value,
                seed,
                reducer,
                eager,
                input,
            } => {
                let label = label.clone();
                let value_input = self.subplan(value_input)?;
                let value = value.clone();
                let seed = seed.clone();
                let reducer = reducer.clone();
                let eager = *eager;
                Ok(kernel(
                    "WriteState",
                    vec![self.lower(input)?],
                    move |mut inputs, state| {
                        write_side_effect(
                            &mut state.context,
                            &label,
                            &value_input,
                            &value,
                            &seed,
                            &reducer,
                            eager,
                            inputs.remove(0),
                            &state.graph,
                        )
                    },
                ))
            }
            other => Err(DataFusionError::Plan(format!(
                "Missing relational kernel for {:?}",
                std::mem::discriminant(other)
            ))),
        }
    }
}

fn apply_op(
    kind: ApplyKind,
    correlation: &[String],
    outputs: &[String],
    optional_missing: OptionalMissing,
    outer: Vec<Row>,
    right: &Subplan,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let _ = correlation;
    let mut out = Vec::new();
    // Occurrence identities, rather than value-based grouping, retain duplicate
    // outer rows. Batching never crosses a reducer, write, JVM or scope fence.
    let mut grouped = if right.batchable && outer.len() > 1 {
        let key = loop {
            let key=format!("\0orchiddb_apply_{}", NEXT_ID.fetch_add(1, Ordering::Relaxed));
            if !right.batch_names.contains(&key) && outer.iter().all(|r| !r.bindings.contains_key(&key)) {break key;}
        };
        let frontier=outer.iter().enumerate().map(|(index,row)| {
            let mut probe=row.clone(); probe.bulk=1;
            probe.bindings.insert(key.clone(), Value::UInt64(index as u64)); probe
        }).collect();
        let rows=right.run_internal(frontier,graph,ctx,Some(key.clone()))?;
        let mut groups=vec![Vec::new();outer.len()];
        for mut row in rows {
            let Some(Value::UInt64(index))=row.bindings.remove(&key) else {
                return Err(RuntimeError::Runtime("batched subplan lost occurrence identity".into()));
            };
            let group=groups.get_mut(index as usize).ok_or_else(|| RuntimeError::Runtime("invalid occurrence identity".into()))?;
            group.push(row);
        }
        Some(groups.into_iter())
    } else {None};
    for outer_row in outer {
        ctx.charge(1)?;
        // A correlated child runs on a split representing one traverser.
        // Parent bulk weights the returned results, not child reducers or
        // side effects (TraversalUtil.prepare resets the split bulk to one).
        let mut probe = outer_row.clone();
        probe.bulk = 1;
        let inner_rows = if let Some(groups)=&mut grouped {
            groups.next().expect("one group per input occurrence")
        } else {run_with_outer(right, &probe, graph, ctx)?};

        ctx.charge(inner_rows.len() as u64)?;
        match kind {
            ApplyKind::Inner => {
                for inner in inner_rows {
                    ctx.charge(1)?;
                    let mut row = outer_row.clone();
                    let mut compatible = true;
                    for (binding, value) in &inner.bindings {
                        if binding == "current" && !outputs.iter().any(|output| output == binding) {
                            continue;
                        }
                        if (binding == "__path"
                            || binding == "__path_labels"
                            || binding == "__sack")
                            || binding.starts_with("__gremlin_select_history_")
                        {
                            if outputs.iter().any(|output| output == "current") {
                                row.bindings.insert(binding.clone(), value.clone());
                            }
                            continue;
                        }
                        // A path processor clears both the label and its
                        // history. This removes child scope, rather than
                        // imposing a null equality join on the parent.
                        // Productive null labels retain a list history.
                        let retracted = matches!(value, Value::Null)
                            && inner
                                .bindings
                                .get(&format!("__gremlin_select_history_{binding}"))
                                == Some(&Value::Null);
                        if retracted {
                            if outputs.iter().any(|output| output == "current") {
                                row.bindings.insert(binding.clone(), Value::Null);
                            }
                            continue;
                        }
                        if binding != "current" {
                            if row.bindings.contains_key(binding) {
                                if is_cypher_history_binding(binding) {
                                    row.bindings.insert(binding.clone(), value.clone());
                                    continue;
                                }
                                if is_internal_apply_binding(binding) {
                                    continue;
                                }
                                let existing = row
                                    .bindings
                                    .get(binding)
                                    .expect("binding existed when compatibility was checked");
                                if existing.three_valued_eq(value) != Some(true) {
                                    compatible = false;
                                    break;
                                }
                            }
                        }
                        row.bindings.insert(binding.clone(), value.clone());
                    }
                    if compatible {
                        row.bulk = outer_row.bulk.checked_mul(inner.bulk).ok_or_else(|| {
                            RuntimeError::Runtime("correlated traverser bulk overflow".into())
                        })?;
                        ctx.charge(1)?;
                        out.push(row);
                    }
                }
            }
            ApplyKind::Optional => {
                if inner_rows.is_empty() {
                    ctx.charge(1)?;
                    let mut row = outer_row.clone();
                    let placeholder = match optional_missing {
                        OptionalMissing::Null => Value::Null,
                        OptionalMissing::Unbound => Value::Null,
                    };
                    for binding in outputs {
                        row.bindings.insert(binding.clone(), placeholder.clone());
                    }
                    out.push(row);
                } else {
                    for inner in inner_rows {
                        ctx.charge(1)?;
                        let mut row = outer_row.clone();
                        row.bulk = outer_row.bulk.checked_mul(inner.bulk).ok_or_else(|| {
                            RuntimeError::Runtime("correlated traverser bulk overflow".into())
                        })?;
                        for binding in outputs {
                            row.bindings.insert(
                                binding.clone(),
                                inner.bindings.get(binding).cloned().unwrap_or(Value::Null),
                            );
                        }
                        for (binding, value) in &inner.bindings {
                            if outputs.iter().any(|output| output == "current")
                                && ((binding == "__path"
                                    || binding == "__path_labels"
                                    || binding == "__sack")
                                    || binding.starts_with("__gremlin_select_history_")
                                    || inner.bindings.contains_key(&format!(
                                        "__gremlin_select_history_{binding}"
                                    )))
                            {
                                row.bindings.insert(binding.clone(), value.clone());
                            }
                        }
                        out.push(row);
                    }
                }
            }
            ApplyKind::Semi => {
                if !inner_rows.is_empty() {
                    ctx.charge(1)?;
                    out.push(outer_row);
                }
            }
            ApplyKind::Anti => {
                if inner_rows.is_empty() {
                    ctx.charge(1)?;
                    out.push(outer_row);
                }
            }
            ApplyKind::Scalar => {
                if inner_rows.len() != 1 {
                    if outputs.is_empty() {
                        return Err(RuntimeError::Type(
                            "scalar apply produced no output bindings".into(),
                        ));
                    }
                    ctx.charge(1)?;
                    let mut row = outer_row.clone();
                    let value = inner_rows
                        .first()
                        .and_then(|r| r.bindings.get(&outputs[0]))
                        .cloned()
                        .unwrap_or(Value::Null);
                    for binding in outputs {
                        row.bindings.insert(binding.clone(), value.clone());
                    }
                    out.push(row);
                } else {
                    let inner = &inner_rows[0];
                    ctx.charge(1)?;
                    let mut row = outer_row.clone();
                    for binding in outputs {
                        row.bindings.insert(
                            binding.clone(),
                            inner.bindings.get(binding).cloned().unwrap_or(Value::Null),
                        );
                    }
                    out.push(row);
                }
            }
        }
    }
    Ok(out)
}

fn is_internal_apply_binding(binding: &str) -> bool {
    binding.starts_with("__")
}

fn is_cypher_history_binding(binding: &str) -> bool {
    binding.starts_with("__cypher_match_history_")
        || binding.starts_with("__cypher_exists_history_")
        || binding.starts_with("__cypher_pattern_history_")
}

fn coalesce_op(
    success: CoalesceSuccess,
    output: &str,
    correlation: &[String],
    rows: Vec<Row>,
    arms: &[Subplan],
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let _ = success;
    let _ = correlation;
    let mut out = Vec::new();
    for row in rows {
        for arm in arms {
            let arm_rows = run_with_outer(arm, &row, graph, ctx)?;
            if !arm_rows.is_empty() {
                for arm_row in arm_rows {
                    let mut new_row = row.clone();
                    if let Some(value) = arm_row.bindings.get(output) {
                        new_row.bindings.insert(output.to_string(), value.clone());
                    }
                    for (binding, value) in &arm_row.bindings {
                        if (binding == "__path"
                            || binding == "__path_labels"
                            || binding == "__sack")
                            || binding.starts_with("__gremlin_select_history_")
                            || arm_row
                                .bindings
                                .contains_key(&format!("__gremlin_select_history_{binding}"))
                        {
                            new_row.bindings.insert(binding.clone(), value.clone());
                        }
                    }
                    new_row.bulk = arm_row.bulk;
                    out.push(new_row);
                }
                break;
            }
        }
    }
    Ok(out)
}

fn choose_op(
    selector: &ChooseSelector,
    correlation: &[String],
    rows: Vec<Row>,
    arms: &[ChooseArm],
    default: Option<&Subplan>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let _ = correlation;
    if let ChooseSelector::Predicates(conditions) = selector {
        // BranchStep processes one incoming traverser at a time unless an
        // option contains a barrier. Partitioning nonbarrier options first
        // reorders outputs (and their observable side effects) by option.
        if !arms.iter().any(|arm| contains_barrier(&arm.body))
            && default.is_none_or(|body| !contains_barrier(body))
        {
            let mut out = Vec::new();
            for row in rows {
                let mut matched = false;
                for (condition, arm) in conditions.iter().zip(arms) {
                    if matches!(eval(condition, &row, graph)?, Value::Bool(true)) {
                        out.extend(run_with_outer(&arm.body, &row, graph, ctx)?);
                        matched = true;
                    }
                }
                if !matched {
                    if let Some(default) = default {
                        out.extend(run_with_outer(default, &row, graph, ctx)?);
                    }
                }
            }
            return Ok(out);
        }
        let mut streams = vec![Vec::new(); arms.len()];
        let mut unmatched = Vec::new();
        for row in rows {
            let mut matched = false;
            for (index, condition) in conditions.iter().take(arms.len()).enumerate() {
                if matches!(eval(condition, &row, graph)?, Value::Bool(true)) {
                    streams[index].push(row.clone());
                    matched = true;
                }
            }
            if !matched {
                unmatched.push(row);
            }
        }
        let mut out = Vec::new();
        for (arm, stream) in arms.iter().zip(streams) {
            if !stream.is_empty() {
                out.extend(run_body_with_frontier(&arm.body, stream, graph, ctx)?);
            }
        }
        if let Some(default) = default {
            if !unmatched.is_empty() {
                out.extend(run_body_with_frontier(default, unmatched, graph, ctx)?);
            }
        }
        return Ok(out);
    }
    let mut out = Vec::new();
    for row in rows {
        let pick: Option<&Subplan> = match selector {
            ChooseSelector::Predicates(_) => unreachable!("stream selector handled above"),
            ChooseSelector::Boolean(condition) => {
                let cond = eval(condition, &row, graph)?;
                let idx = if matches!(cond, Value::Bool(true)) {
                    0
                } else {
                    1
                };
                arms.get(idx).map(|arm| &arm.body).or(default)
            }
            ChooseSelector::Value(expr) => {
                let value = eval(expr, &row, graph)?;
                arms.iter()
                    .find(|arm| arm.key.as_ref() == Some(&value))
                    .map(|arm| &arm.body)
                    .or(default)
            }
        };
        let Some(arm) = pick else {
            // No matching arm and no default — pass-through (the default
            // `unmatched` policy emitted by lowering).
            out.push(row);
            continue;
        };
        let arm_rows = run_with_outer(arm, &row, graph, ctx)?;
        for arm_row in arm_rows {
            let mut new_row = row.clone();
            new_row.bulk = arm_row.bulk;
            for (k, v) in arm_row.bindings {
                new_row.bindings.insert(k, v);
            }
            out.push(new_row);
        }
    }
    Ok(out)
}

fn repeat_op(
    loop_name: Option<&str>,
    times: Option<u32>,
    emit_each_iteration: bool,
    emit_mode: &EmitMode,
    emit_seed_predicate: Option<&IrExpr>,
    emit_seed_traversal: Option<&Subplan>,
    until_first: bool,
    until: Option<&IrExpr>,
    until_traversal: Option<&Subplan>,
    _path: Option<&str>,
    seed_rows: Vec<Row>,
    body: &Subplan,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let nested = !ctx.step_state.is_empty();
    let seed_rows = if nested {
        seed_rows
            .into_iter()
            .map(|mut row| {
                let mut stack = match row.bindings.remove("__repeat_loop_stack") {
                    Some(Value::List(stack)) => stack,
                    _ => Vec::new(),
                };
                let mut saved = std::collections::BTreeMap::new();
                for key in std::iter::once("__loops".to_string())
                    .chain(loop_name.map(|name| format!("__loops:{name}")))
                {
                    saved.insert(
                        key.clone(),
                        row.bindings.get(&key).cloned().unwrap_or(Value::Null),
                    );
                }
                stack.push(Value::Map(saved));
                row.bindings
                    .insert("__repeat_loop_stack".into(), Value::List(stack));
                row
            })
            .collect()
    } else {
        seed_rows
    };
    ctx.push_step_state_frame();
    let mut result = repeat_op_inner(
        loop_name,
        times,
        emit_each_iteration,
        emit_mode,
        emit_seed_predicate,
        emit_seed_traversal,
        until_first,
        until,
        until_traversal,
        _path,
        seed_rows,
        body,
        graph,
        ctx,
    );
    ctx.pop_step_state_frame();
    if nested {
        if let Ok(rows) = &mut result {
            for row in rows {
                if let Some(Value::List(mut stack)) = row.bindings.remove("__repeat_loop_stack") {
                    if let Some(Value::Map(saved)) = stack.pop() {
                        for (key, value) in saved {
                            if matches!(value, Value::Null) {
                                row.bindings.remove(&key);
                            } else {
                                row.bindings.insert(key, value);
                            }
                        }
                    }
                    if !stack.is_empty() {
                        row.bindings
                            .insert("__repeat_loop_stack".into(), Value::List(stack));
                    }
                }
            }
        }
    }
    result
}

fn repeat_op_inner(
    loop_name: Option<&str>,
    times: Option<u32>,
    emit_each_iteration: bool,
    emit_mode: &EmitMode,
    emit_seed_predicate: Option<&IrExpr>,
    emit_seed_traversal: Option<&Subplan>,
    until_first: bool,
    until: Option<&IrExpr>,
    until_traversal: Option<&Subplan>,
    _path: Option<&str>,
    seed_rows: Vec<Row>,
    body: &Subplan,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    // Standard RepeatStep supplies one upstream seed only after its body
    // has drained. When an unbounded body reads/writes shared state, later
    // seeds must observe the earlier seed's completed work. The source has
    // already run once; retain each seed's bulk and the shared step frame.
    if seed_rows.len() > 1
        && times.is_none()
        && !emit_each_iteration
        && emit_seed_predicate.is_none()
        && emit_seed_traversal.is_none()
        && until.is_none()
        && until_traversal.is_none()
        && has_observable_state(body)
    {
        let mut rows = Vec::new();
        for seed in seed_rows {
            rows.extend(repeat_op_inner(
                loop_name,
                times,
                emit_each_iteration,
                emit_mode,
                emit_seed_predicate,
                emit_seed_traversal,
                until_first,
                until,
                until_traversal,
                _path,
                vec![seed],
                body,
                graph,
                ctx,
            )?);
        }
        return Ok(rows);
    }
    // Termination, in priority order:
    //   1. `times = Some(N)` — at most N iterations.
    //   2. `until = Some(p)` — stop when p matches a row (that row is
    //      emitted, others continue).
    //   3. Otherwise — stop when the frontier becomes empty.
    // A resource ceiling is an error, never a successful truncated result.
    const MAX_REPEAT_ITERATIONS: u32 = 10_000;
    let trace_metrics =
        ctx.step_state.len() == 1 && std::env::var_os("ORCHIDDB_REPEAT_METRICS").is_some();
    let mut peak_expanded = seed_rows.len();
    let mut frontier = ops::barrier::compact_repeat_frontier(seed_rows)?;
    let mut peak_compacted = frontier.len();
    let mut peak_bulk = 0_u64;
    let mut out = Vec::new();
    for row in &mut frontier {
        row.bindings.insert("__loops".into(), Value::Int(0));
        if let Some(name) = loop_name {
            row.bindings
                .insert(format!("__loops:{name}"), Value::Int(0));
        }
    }
    if until_first {
        let mut continuing = Vec::new();
        for row in frontier {
            let done = if let Some(predicate) = until {
                matches!(eval(predicate, &row, graph)?, Value::Bool(true))
            } else if let Some(probe) = until_traversal {
                !run_body_with_frontier(probe, vec![row.clone()], graph, ctx)?.is_empty()
            } else {
                false
            };
            if done {
                out.push(row);
            } else {
                continuing.push(row);
            }
        }
        frontier = continuing;
    }
    if emit_each_iteration {
        if let Some(seed_predicate) = emit_seed_predicate {
            emit_matching(&frontier, Some(seed_predicate), &mut out, graph)?;
        } else if let Some(seed_probe) = emit_seed_traversal {
            emit_matching_traversal(&frontier, seed_probe, &mut out, graph, ctx)?;
        }
    }
    let mut iteration: u32 = 0;
    loop {
        if frontier.is_empty() {
            break;
        }
        ctx.charge(1)?;
        if let Some(n) = times {
            if iteration >= n {
                break;
            }
        }
        if iteration >= MAX_REPEAT_ITERATIONS {
            return Err(RuntimeError::ExecutionLimit(format!(
                "repeat exceeded {MAX_REPEAT_ITERATIONS} iterations"
            )));
        }
        let body_frontier = frontier
            .into_iter()
            .map(|mut row| {
                row.bindings
                    .insert("__loops".to_string(), Value::Int(iteration as i64));
                if let Some(name) = loop_name {
                    row.bindings
                        .insert(format!("__loops:{name}"), Value::Int(iteration as i64));
                }
                row
            })
            .collect::<Vec<_>>();
        ctx.activate_step_state_frame();
        let stepped = run_body_with_frontier(body, body_frontier, graph, ctx);
        ctx.deactivate_step_state_frame();
        let stepped = stepped?;
        peak_expanded = peak_expanded.max(stepped.len());
        let stepped = ops::barrier::compact_repeat_frontier(stepped)?;
        peak_compacted = peak_compacted.max(stepped.len());
        if trace_metrics {
            peak_bulk = peak_bulk.max(
                stepped
                    .iter()
                    .fold(0_u64, |total, row| total.saturating_add(row.bulk)),
            );
        }
        let stepped = stepped
            .into_iter()
            .map(|mut row| {
                row.bindings
                    .insert("__loops".to_string(), Value::Int((iteration + 1) as i64));
                if let Some(name) = loop_name {
                    row.bindings.insert(
                        format!("__loops:{name}"),
                        Value::Int((iteration + 1) as i64),
                    );
                }
                row
            })
            .collect::<Vec<_>>();
        // Until/times exits take precedence over emit splitting: a terminal
        // traverser is returned once even when the emit predicate rejects it.
        let at_bound = times.is_some_and(|n| iteration + 1 >= n);
        let mut continuing = Vec::new();
        for row in stepped {
            let done = if at_bound {
                true
            } else if let Some(predicate) = until {
                matches!(eval(predicate, &row, graph)?, Value::Bool(true))
            } else if let Some(probe) = until_traversal {
                !run_body_with_frontier(probe, vec![row.clone()], graph, ctx)?.is_empty()
            } else {
                false
            };
            if done {
                out.push(row);
            } else {
                continuing.push(row);
            }
        }
        if emit_each_iteration {
            match emit_mode {
                EmitMode::AfterEachIteration => emit_matching(&continuing, None, &mut out, graph)?,
                EmitMode::AfterEachIfPredicate(p) => {
                    emit_matching(&continuing, Some(p), &mut out, graph)?
                }
                EmitMode::AfterEachIfTraversal(probe) => {
                    emit_matching_traversal(&continuing, probe, &mut out, graph, ctx)?
                }
                EmitMode::AfterLoop => {}
            }
        }
        frontier = continuing;
        if frontier.is_empty() {
            break;
        }
        iteration += 1;
    }
    if !emit_each_iteration && until.is_none() && until_traversal.is_none() {
        out.extend(frontier);
    }
    if trace_metrics {
        eprintln!(
            "repeat metrics: iterations={} peak_expanded={} peak_compacted={} peak_logical_bulk={}",
            iteration + 1,
            peak_expanded,
            peak_compacted,
            peak_bulk
        );
    }
    Ok(out)
}

fn emit_matching(
    rows: &[Row],
    predicate: Option<&IrExpr>,
    out: &mut Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<()> {
    for row in rows {
        if let Some(predicate) = predicate {
            if !matches!(eval(predicate, row, graph)?, Value::Bool(true)) {
                continue;
            }
        }
        out.push(row.clone());
    }
    Ok(())
}

/// Traversal-form emit: emit each row whose probe yields ≥1 result
/// when run with `[row]` as the upstream frontier. Implements
/// `repeat(...).emit(__.traversal)` per TinkerPop semantics.
fn emit_matching_traversal(
    rows: &[Row],
    probe: &Subplan,
    out: &mut Vec<Row>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<()> {
    for row in rows {
        let produced = run_body_with_frontier(probe, vec![row.clone()], graph, ctx)?;
        if !produced.is_empty() {
            out.push(row.clone());
        }
    }
    Ok(())
}

fn merge_op(
    outputs: &[String],
    upstream: Vec<Row>,
    match_arm: &Subplan,
    create_arm: &Subplan,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::new();
    for outer_row in upstream {
        ctx.charge(1)?;
        let mut rows = run_with_outer(match_arm, &outer_row, graph, ctx)?;
        if rows.is_empty() {
            rows = run_with_outer(create_arm, &outer_row, graph, ctx)?;
        }
        ctx.charge(rows.len() as u64)?;
        for inner in rows {
            let mut row = outer_row.clone();
            for output in outputs {
                if let Some(value) = inner.bindings.get(output) {
                    row.bindings.insert(output.clone(), value.clone());
                }
            }
            out.push(row);
        }
    }
    Ok(out)
}

fn endpoint(row: &Row, binding: &str, rel_type: &str) -> IrResult<Value> {
    row.bindings.get(binding).cloned().ok_or_else(|| {
        RuntimeError::Type(format!(
            "CREATE relationship `{rel_type}` endpoint `{binding}` is not bound"
        ))
    })
}

#[path = "groups.rs"]
pub(crate) mod groups;

fn write_side_effect(
    ctx: &mut ExecutionContext,
    label: &str,
    value_input: &Subplan,
    value: &crate::ir::expr::IrExpr,
    seed: &Value,
    reducer: &str,
    eager: bool,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    if reducer == "register" {
        ctx.side_effects
            .entry(label.to_owned())
            .or_insert_with(|| seed.clone());
        return Ok(rows);
    }
    ctx.side_effect_reducers
        .entry(label.to_owned())
        .or_insert_with(|| reducer.to_owned());
    ctx.push_step_state_frame();
    let projected = run_body_with_frontier(value_input, rows.clone(), graph, ctx);
    ctx.pop_step_state_frame();
    let projected = projected?;
    let mut values = Vec::new();
    for row in &projected {
        let value = eval(value, row, graph)?;
        values.extend(std::iter::repeat_n(value, row.bulk as usize));
    }
    let state = ctx
        .side_effects
        .entry(label.to_owned())
        .or_insert_with(|| seed.clone());
    if reducer == "assign" {
        if eager && !values.is_empty() {
            *state = Value::BulkSet(values);
        } else if let Some(last) = values.pop() {
            *state = Value::BulkSet(vec![last]);
        }
    } else if reducer == "collect" || reducer == "addAll" || reducer == "tree" {
        if let Some(existing) = crate::ir::value::as_gremlin_set(state) {
            let mut items = existing.to_vec();
            for value in values {
                if !items.contains(&value) {
                    items.push(value);
                }
            }
            *state = crate::ir::value::gremlin_set(items);
        } else {
            match state {
                Value::BulkSet(items) | Value::List(items) => items.extend(values),
                _ => {
                    for value in values {
                        *state = crate::ir::runtime::scalar::reductions::apply_sack_op(
                            state, &value, reducer,
                        );
                    }
                }
            }
        }
    } else {
        for value in values {
            *state =
                crate::ir::runtime::scalar::reductions::apply_sack_op(state, &value, reducer);
        }
    }
    Ok(rows)
}

#[path = "bounded.rs"]
mod bounded;
