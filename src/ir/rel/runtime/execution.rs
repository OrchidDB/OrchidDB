//! Demand-driven native execution. Batch-local reads stream; global operators
//! collect their inputs in order. No shared query mutex is held across await.
use super::*;
use crate::ir::{ElementId, plan::LabelExpr, runtime::ops::source};
use futures::TryStreamExt;

#[derive(Clone)]
pub(super) enum Source {
    Values {
        bindings: Vec<String>,
        rows: Arc<Vec<Vec<Value>>>,
        bulk: Option<Vec<u64>>,
    },
    Nodes {
        binding: String,
        labels: LabelExpr,
    },
    Edges {
        binding: String,
        types: LabelExpr,
    },
}
pub(super) struct SourceCursor {
    source: Source,
    offset: usize,
    labels: Option<std::vec::IntoIter<String>>,
    label: String,
    ids: std::vec::IntoIter<ElementId>,
}
impl SourceCursor {
    pub(super) fn new(source: Source) -> Self {
        Self {
            source,
            offset: 0,
            labels: None,
            label: String::new(),
            ids: vec![].into_iter(),
        }
    }
    pub(super) fn next(&mut self, size: usize, state: &mut State) -> Result<Option<Vec<Row>>> {
        state.context.jvm.check().map_err(failure)?;
        if let Source::Values {
            bindings,
            rows,
            bulk,
        } = &self.source
        {
            if self.offset == rows.len() {
                return Ok(None);
            }
            let end = self.offset.saturating_add(size).min(rows.len());
            let bulks = bulk
                .as_ref()
                .map(|b| &b[self.offset.min(b.len())..end.min(b.len())]);
            let out =
                source::values_op(bindings, &rows[self.offset..end], bulks).map_err(failure)?;
            self.offset = end;
            return Ok(Some(out));
        }
        let graph = &state.graph;
        let (binding, expr, nodes) = match &self.source {
            Source::Nodes { binding, labels } => (binding, labels, true),
            Source::Edges { binding, types } => (binding, types, false),
            _ => unreachable!(),
        };
        if self.labels.is_none() {
            self.labels = Some(
                if nodes {
                    source::matching_labels(expr, graph)
                } else {
                    source::matching_rel_types(expr, graph)
                }
                .into_iter(),
            );
        }
        let mut rows = Vec::with_capacity(size);
        while rows.len() < size {
            state.context.charge(1).map_err(failure)?;
            let Some(id) = self.ids.next() else {
                let Some(label) = self.labels.as_mut().unwrap().next() else {
                    break;
                };
                self.ids = if nodes {
                    graph.node_ids(&label).map_err(failure)?
                } else {
                    graph.edge_ids(&label)
                }
                .into_iter();
                self.label = label;
                continue;
            };
            let value = if nodes {
                if !graph.node_matches_labels(&self.label, id.clone(), expr) {
                    continue;
                }
                Value::Node {
                    label: self.label.clone(),
                    id,
                }
            } else {
                let (src_label, src_id, dst_label, dst_id) = graph
                    .edge_endpoints(&self.label, id.clone())
                    .ok_or_else(|| failure("Missing edge endpoints"))?;
                Value::Edge {
                    rel_type: self.label.clone(),
                    id,
                    src_label,
                    src_id,
                    dst_label,
                    dst_id,
                    projected_properties: None,
                }
            };
            rows.push(Row::new().with(binding, value));
        }
        graph.check_source().map_err(failure)?;
        Ok((!rows.is_empty()).then_some(rows))
    }
}

pub(super) fn source_kernel(name: &str, source: Source) -> LogicalPlan {
    let LogicalPlan::Extension(e) = kernel(name, vec![], |mut rows, _| Ok(rows.remove(0))) else {
        unreachable!()
    };
    let mut k = e.node.as_any().downcast_ref::<RowKernel>().unwrap().clone();
    k.source = Some(source);
    k.streaming = true;
    LogicalPlan::Extension(Extension { node: Arc::new(k) })
}

pub(super) fn input_rows(batch: RecordBatch, kernel: &RowKernel) -> Result<Vec<Row>> {
    if let Some(fields) = &kernel.relational_input {
        let mut rows = super::super::native_values::decode_boundary_rows(&batch, fields).map_err(|e| failure(e.to_string()))?;
        if kernel.relational_input_state {
            super::super::native_values::restore_traverser_state(&mut rows).map_err(|e| failure(e.to_string()))?;
        }
        Ok(rows)
    } else {
        decode_rows(&batch)
    }
}
pub(super) fn run(kernel: &RowKernel, inputs: Vec<Vec<Row>>, state: &mut State) -> Result<Vec<Row>> {
    super::policy::validate_kernel(kernel, state.context.rule_context.lowering_policy).map_err(|e| failure(e.to_string()))?;
    if let Some(executable) = kernel.boundary.and_then(|kind| state.context.rule_context.executables.get(kind)) {
        state.context.jvm.check().map_err(failure)?;
        return executable.execute(kernel, inputs, state).map_err(|e| failure(e.to_string()));
    }
    run_default(kernel, inputs, state)
}
pub(super) fn run_default(kernel: &RowKernel, inputs: Vec<Vec<Row>>, state: &mut State) -> Result<Vec<Row>> {
    policy::validate_kernel(kernel, state.context.rule_context.lowering_policy).map_err(|e| failure(e.to_string()))?;
    let preparation = state.context.scalar_preparation.clone();
    crate::ir::runtime::scalar::preparation::with_preparation(&preparation, || run_prepared(kernel, inputs, state))
}

fn run_prepared(kernel: &RowKernel, mut inputs: Vec<Vec<Row>>, state: &mut State) -> Result<Vec<Row>> {
    state.context.charge(1).map_err(failure)?;
    for rows in &mut inputs {
        state.graph.normalize_source_rows(rows).map_err(failure)?;
        if kernel.relational_input.is_none() {
            state.graph.prefetch_source(rows);
        }
    }
    state.graph.check_source().map_err(failure)?;
    if kernel.relational_input.is_none() {
        let cost = &mut state.context.query_cost;
        cost.native_kernel_calls = cost.native_kernel_calls.saturating_add(1);
        if kernel.boundary.is_none() {
            cost.native_relational_kernel_calls = cost.native_relational_kernel_calls.saturating_add(1);
        }
        if kernel.source.is_none() {
            cost.native_input_rows = cost
                .native_input_rows
                .saturating_add(inputs.iter().map(|rows| rows.len() as u64).sum::<u64>());
        }
    }
    for stage in &kernel.prefix {
        let rows = stage(inputs, state).map_err(|e| DataFusionError::External(Box::new(e)))?;
        state.context.charge(1).map_err(failure)?;
        inputs = vec![rows];
    }
    let rows =
        (kernel.kernel)(inputs, state).map_err(|e| DataFusionError::External(Box::new(e)))?;
    state.graph.check_source().map_err(failure)?;
    if kernel.relational_input.is_none() {
        let cost = &mut state.context.query_cost;
        cost.native_output_rows = cost.native_output_rows.saturating_add(rows.len() as u64);
    }
    Ok(rows)
}

pub(super) fn take_range(rows: Vec<Row>, slice: &mut crate::ir::plan::Slice) -> Vec<Row> {
    let mut out = Vec::new();
    for mut row in rows {
        if slice.fetch == Some(0) {
            break;
        }
        if slice.offset >= row.bulk {
            slice.offset -= row.bulk;
            continue;
        }
        row.bulk -= slice.offset;
        slice.offset = 0;
        if let Some(remaining) = &mut slice.fetch {
            row.bulk = row.bulk.min(*remaining);
            *remaining -= row.bulk;
        }
        if row.bulk > 0 {
            out.push(row);
        }
    }
    out
}

// Arrow batches are bounded by emitted rows, including multiplicity expanded
// at a SQL boundary. One compact traverser can represent millions of rows.
pub(super) fn take_output_rows(output: &mut std::vec::IntoIter<Row>, size: usize, expands_bulk: bool) -> Vec<Row> {
    if !expands_bulk { return output.by_ref().take(size).collect(); }
    let mut remaining = size.max(1) as u64;
    let mut rows = Vec::new();
    while remaining > 0 {
        let Some(next) = output.as_mut_slice().first_mut() else { break; };
        if next.bulk > remaining {
            let mut part = next.clone();
            part.bulk = remaining;
            next.bulk -= remaining;
            rows.push(part);
            break;
        }
        remaining -= next.bulk;
        rows.push(output.next().unwrap());
    }
    rows
}

struct Cursor {
    kernel: RowKernel,
    state: Arc<Mutex<State>>,
    task: Arc<TaskContext>,
    inputs: Vec<Arc<dyn ExecutionPlan>>,
    input: Option<SendableRecordBatchStream>,
    source: Option<SourceCursor>,
    slice: Option<crate::ir::plan::Slice>,
    batch: Option<(RecordBatch, usize)>,
    output: std::vec::IntoIter<Row>,
    size: usize,
    finished: bool,
}
impl Cursor {
    async fn next(mut self) -> Result<Option<(RecordBatch, Self)>> {
        loop {
            if self.output.len() > 0 {
                self.state
                    .lock()
                    .map_err(|_| failure("Query state poisoned"))?
                    .context
                    .jvm
                    .check()
                    .map_err(failure)?;
                let expands_bulk = self.kernel.relational_output.as_ref().is_some_and(|output|
                    !output.preserve_bulk && output.policy.language == crate::ir::policy::Language::Gremlin);
                let rows = take_output_rows(&mut self.output, self.size, expands_bulk);
                // Encoding can include language containers; keep CPU work off
                // the async scheduler, just like scalar kernel evaluation.
                let kernel = self.kernel.clone();
                let state = self.state.clone();
                let batch = tokio::task::spawn_blocking(move || {
                    let state = state.lock().map_err(|_| failure("Query state poisoned"))?;
                    kernel.encode_output(rows, &state).map_err(|e| failure(e.to_string()))
                })
                    .await
                    .map_err(failure)??;
                return Ok(Some((batch, self)));
            }
            if self.finished
                || self
                    .slice
                    .as_ref()
                    .is_some_and(|slice| slice.fetch == Some(0))
            {
                return Ok(None);
            }
            let state = self.state.clone();
            let kernel = self.kernel.clone();
            if let Some(mut source) = self.source.take() {
                let size = self.size;
                let (source, rows) = tokio::task::spawn_blocking(move || {
                    let mut state = state.lock().map_err(|_| failure("Query state poisoned"))?;
                    let rows = source
                        .next(size, &mut state)?
                        .map(|rows| run(&kernel, vec![rows], &mut state))
                        .transpose()?;
                    Ok::<_, DataFusionError>((source, rows))
                })
                .await
                .map_err(failure)??;
                self.source = Some(source);
                match rows {
                    Some(rows) => self.output = rows.into_iter(),
                    None => self.finished = true,
                }
            } else if self.kernel.streaming && self.inputs.len() == 1 {
                if self.input.is_none() {
                    let input = self.inputs[0].execute(0, self.task.clone())?;
                    self.input = Some(if needs_source_spool(&self.kernel, &self.state, &self.inputs[0])? {
                        // Hydration/callbacks can query the same caller connection
                        // that the upstream SQL producer owns. Drain to bounded
                        // IPC storage before invoking them, releasing that lease.
                        let schema = input.schema();
                        let stream = crate::execution::stream::BatchStream::new(schema.clone(), input.map_err(|e|e.to_string()));
                        let replay = crate::execution::stream::BatchSource::spool(stream, &std::env::temp_dir()).await.map_err(failure)?;
                        let stream = replay.open().map_err(failure)?;
                        Box::pin(RecordBatchStreamAdapter::new(schema, stream.map_err(failure))) as SendableRecordBatchStream
                    } else { input });
                }
                if self.batch.is_none() {
                    match self.input.as_mut().unwrap().try_next().await? {
                        Some(batch) => self.batch = Some((batch, 0)),
                        None => {
                            self.finished = true;
                            continue;
                        }
                    }
                }
                let (batch, offset) = self.batch.as_mut().unwrap();
                let count = self.size.min(batch.num_rows() - *offset);
                let chunk = batch.slice(*offset, count);
                *offset += count;
                if *offset == batch.num_rows() {
                    self.batch = None;
                }
                let mut slice = self.slice.take();
                let (rows, slice) = tokio::task::spawn_blocking(move || {
                    let rows = input_rows(chunk, &kernel)?;
                    let rows = match &mut slice {
                        Some(slice) => take_range(rows, slice),
                        None => rows,
                    };
                    let mut state = state.lock().map_err(|_| failure("Query state poisoned"))?;
                    Ok::<_, DataFusionError>((run(&kernel, vec![rows], &mut state)?, slice))
                })
                .await
                .map_err(failure)??;
                self.output = rows.into_iter();
                self.slice = slice;
            } else {
                // Sorts, reducers, mutation and opaque callbacks retain their
                // whole-input semantics. Decode each incoming batch promptly
                // instead of retaining both all Arrow batches and all Rows.
                let mut inputs = Vec::new();
                for input in &self.inputs {
                    let mut stream = input.execute(0, self.task.clone())?;
                    let mut rows = Vec::new();
                    while let Some(batch) = stream.try_next().await? {
                        self.state
                            .lock()
                            .map_err(|_| failure("Query state poisoned"))?
                            .context
                            .jvm
                            .check()
                            .map_err(failure)?;
                        rows.extend(input_rows(batch, &kernel)?);
                    }
                    inputs.push(rows);
                }
                self.output = tokio::task::spawn_blocking(move || {
                    let mut state = state.lock().map_err(|_| failure("Query state poisoned"))?;
                    run(&kernel, inputs, &mut state)
                })
                .await
                .map_err(failure)??
                .into_iter();
                self.finished = true;
            }
        }
    }
}

pub(super) fn execute(
    exec: &KernelExec,
    task: Arc<TaskContext>,
) -> Result<SendableRecordBatchStream> {
    let cursor = Cursor {
        source: exec.kernel.source.clone().map(SourceCursor::new),
        slice: exec.kernel.slice.clone(),
        kernel: exec.kernel.clone(),
        state: exec.state.clone(),
        inputs: exec.inputs.clone(),
        input: None,
        batch: None,
        output: vec![].into_iter(),
        size: task.batch_size(),
        task,
        finished: false,
    };
    Ok(Box::pin(RecordBatchStreamAdapter::new(
        Arc::new(exec.kernel.schema.as_arrow().clone()),
        stream::try_unfold(cursor, Cursor::next),
    )))
}

#[cfg(test)]
#[path = "../../../../tests/unit/extracted/ir__rel__runtime__execution__tests.rs"]
mod tests;

fn has_sql_source(plan: &Arc<dyn ExecutionPlan>) -> bool {
    matches!(plan.name(), "DuckDbExec" | "TrinoExec") || plan.children().iter().any(|child| has_sql_source(child))
}
pub(super) fn needs_source_spool(kernel: &RowKernel, state: &Arc<Mutex<State>>, input: &Arc<dyn ExecutionPlan>) -> Result<bool> {
    Ok(kernel.relational_input.is_none()
        && state.lock().map_err(|_|failure("Query state poisoned"))?.graph.source.is_some()
        && has_sql_source(input))
}
