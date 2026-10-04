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
struct SourceCursor {
    source: Source,
    offset: usize,
    labels: Option<std::vec::IntoIter<String>>,
    label: String,
    ids: std::vec::IntoIter<ElementId>,
}
impl SourceCursor {
    fn new(source: Source) -> Self {
        Self {
            source,
            offset: 0,
            labels: None,
            label: String::new(),
            ids: vec![].into_iter(),
        }
    }
    fn next(&mut self, size: usize, state: &mut State) -> Result<Option<Vec<Row>>> {
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

fn input_rows(batch: RecordBatch, kernel: &RowKernel) -> Result<Vec<Row>> {
    if let Some(fields) = &kernel.relational_input {
        let returned = ReturnedBatches {
            fields: fields.clone(),
            result_form: crate::ir::policy::ResultForm::RowSet,
            batch,
        };
        let (bindings, values) = crate::ir::exec::batch_to_bindings(&returned)
            .ok_or_else(|| failure("Unsupported Arrow boundary value"))?;
        Ok(values
            .into_iter()
            .map(|values| Row {
                bindings: bindings.iter().cloned().zip(values).collect(),
                bulk: 1,
            })
            .collect())
    } else {
        decode_rows(&batch)
    }
}
fn run(kernel: &RowKernel, mut inputs: Vec<Vec<Row>>, state: &mut State) -> Result<Vec<Row>> {
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

fn take_range(rows: Vec<Row>, slice: &mut crate::ir::plan::Slice) -> Vec<Row> {
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
                let rows = self.output.by_ref().take(self.size).collect();
                // Encoding can include language containers; keep CPU work off
                // the async scheduler, just like scalar kernel evaluation.
                let batch = tokio::task::spawn_blocking(move || encode_rows(rows))
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
                    self.input = Some(self.inputs[0].execute(0, self.task.clone())?);
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
        size: task.session_config().batch_size().max(1),
        task,
        finished: false,
    };
    Ok(Box::pin(RecordBatchStreamAdapter::new(
        schema(),
        stream::try_unfold(cursor, Cursor::next),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::sync::atomic::AtomicUsize;
    #[derive(Debug)]
    struct Feed {
        batches: Vec<RecordBatch>,
        polls: Arc<AtomicUsize>,
        properties: Arc<PlanProperties>,
    }
    impl DisplayAs for Feed {
        fn fmt_as(&self, _: DisplayFormatType, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "Feed")
        }
    }
    impl ExecutionPlan for Feed {
        fn name(&self) -> &str {
            "Feed"
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
        fn properties(&self) -> &Arc<PlanProperties> {
            &self.properties
        }
        fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
            vec![]
        }
        fn with_new_children(
            self: Arc<Self>,
            _: Vec<Arc<dyn ExecutionPlan>>,
        ) -> Result<Arc<dyn ExecutionPlan>> {
            Ok(self)
        }
        fn execute(&self, _: usize, _: Arc<TaskContext>) -> Result<SendableRecordBatchStream> {
            let polls = self.polls.clone();
            let batches = self.batches.clone();
            Ok(Box::pin(RecordBatchStreamAdapter::new(
                schema(),
                stream::iter(batches).map(move |batch| {
                    polls.fetch_add(1, Ordering::Relaxed);
                    Ok(batch)
                }),
            )))
        }
    }
    fn properties() -> Arc<PlanProperties> {
        Arc::new(PlanProperties::new(
            EquivalenceProperties::new(schema()),
            Partitioning::UnknownPartitioning(1),
            EmissionType::Incremental,
            Boundedness::Bounded,
        ))
    }
    fn physical(plan: LogicalPlan, inputs: Vec<Arc<dyn ExecutionPlan>>) -> KernelExec {
        let LogicalPlan::Extension(e) = plan else {
            panic!()
        };
        KernelExec {
            kernel: e.node.as_any().downcast_ref::<RowKernel>().unwrap().clone(),
            input_ordering: vec![None; inputs.len()],
            inputs,
            state: Default::default(),
            properties: properties(),
        }
    }
    fn task(size: usize) -> Arc<TaskContext> {
        Arc::new(TaskContext::default().with_session_config(
            datafusion::execution::config::SessionConfig::new().with_batch_size(size),
        ))
    }
    fn rows(range: std::ops::Range<i64>) -> Vec<Row> {
        range
            .map(|i| Row {
                bindings: [("current".into(), Value::Int(i))].into(),
                bulk: (i + 1) as u64,
            })
            .collect()
    }
    #[tokio::test]
    async fn streaming_reads_only_the_requested_batch_and_preserves_bulk() {
        let polls = Arc::new(AtomicUsize::new(0));
        let feed: Arc<dyn ExecutionPlan> = Arc::new(Feed {
            batches: vec![
                encode_rows(rows(0..5)).unwrap(),
                encode_rows(rows(5..10)).unwrap(),
            ],
            polls: polls.clone(),
            properties: properties(),
        });
        let mut exec = physical(
            kernel("Read", vec![], |mut inputs, _| Ok(inputs.remove(0))),
            vec![feed],
        );
        exec.kernel.streaming = true;
        let mut stream = exec.execute(0, task(2)).unwrap();
        for (start, size) in [(0, 2), (2, 2), (4, 1)] {
            let batch = stream.try_next().await.unwrap().unwrap();
            assert_eq!(batch.num_rows(), size);
            let got = decode_rows(&batch).unwrap();
            assert_eq!(got[0].bindings["current"], Value::Int(start));
            assert_eq!(got[0].bulk, (start + 1) as u64);
            assert_eq!(polls.load(Ordering::Relaxed), 1);
        }
        drop(stream);
        assert_eq!(
            polls.load(Ordering::Relaxed),
            1,
            "unconsumed batches must not be polled"
        );
    }
    #[tokio::test]
    async fn streaming_range_splits_bulk_and_stops_upstream() {
        let polls = Arc::new(AtomicUsize::new(0));
        let feed: Arc<dyn ExecutionPlan> = Arc::new(Feed {
            batches: vec![
                encode_rows(rows(0..5)).unwrap(),
                encode_rows(rows(5..10)).unwrap(),
            ],
            polls: polls.clone(),
            properties: properties(),
        });
        let mut exec = physical(
            kernel("Slice", vec![], |mut inputs, _| Ok(inputs.remove(0))),
            vec![feed],
        );
        exec.kernel.streaming = true;
        exec.kernel.slice = Some(crate::ir::plan::Slice {
            offset: 4,
            fetch: Some(6),
            tail: None,
        });
        for expected_polls in [1, 2] {
            let batches = exec
                .execute(0, task(2))
                .unwrap()
                .try_collect::<Vec<_>>()
                .await
                .unwrap();
            let got = batches
                .iter()
                .flat_map(|b| decode_rows(b).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(got.len(), 2);
            assert_eq!(got[0].bindings["current"], Value::Int(2));
            assert_eq!(got[0].bulk, 2);
            assert_eq!(got[1].bindings["current"], Value::Int(3));
            assert_eq!(got[1].bulk, 4);
            assert_eq!(polls.load(Ordering::Relaxed), expected_polls);
        }
        exec.kernel.slice.as_mut().unwrap().fetch = Some(0);
        assert!(
            exec.execute(0, task(2))
                .unwrap()
                .try_next()
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(polls.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn global_kernel_gets_the_whole_ordered_input_once() {
        let polls = Arc::new(AtomicUsize::new(0));
        let feed: Arc<dyn ExecutionPlan> = Arc::new(Feed {
            batches: vec![
                encode_rows(rows(0..3)).unwrap(),
                encode_rows(rows(3..6)).unwrap(),
            ],
            polls: polls.clone(),
            properties: properties(),
        });
        let exec = physical(
            kernel("Barrier", vec![], |mut inputs, _| {
                let rows = inputs.remove(0);
                assert_eq!(rows.len(), 6);
                assert_eq!(rows[5].bindings["current"], Value::Int(5));
                Ok(rows)
            }),
            vec![feed],
        );
        let mut stream = exec.execute(0, task(2)).unwrap();
        assert_eq!(stream.try_next().await.unwrap().unwrap().num_rows(), 2);
        assert_eq!(polls.load(Ordering::Relaxed), 2);
        let remaining = stream.try_collect::<Vec<_>>().await.unwrap();
        assert_eq!(remaining.iter().map(|b| b.num_rows()).sum::<usize>(), 4);
    }
    #[tokio::test]
    async fn sources_emit_bounded_batches_and_state_resets_on_reexecution() {
        let plan = source_kernel(
            "Values",
            Source::Values {
                bindings: vec!["current".into()],
                rows: Arc::new((0..7).map(|i| vec![Value::Int(i)]).collect()),
                bulk: Some(vec![2; 7]),
            },
        );
        let exec = physical(plan, vec![]);
        for _ in 0..2 {
            let batches = exec
                .execute(0, task(3))
                .unwrap()
                .try_collect::<Vec<_>>()
                .await
                .unwrap();
            assert_eq!(
                batches.iter().map(|b| b.num_rows()).collect::<Vec<_>>(),
                [3, 3, 1]
            );
            let rows = batches
                .iter()
                .flat_map(|b| decode_rows(b).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(rows.len(), 7);
            assert!(rows.iter().all(|row| row.bulk == 2));
        }
    }
    #[tokio::test]
    async fn flat_fused_pipeline_does_not_recurse_and_cancellation_stops_sources() {
        let plan = source_kernel(
            "Values",
            Source::Values {
                bindings: vec!["current".into()],
                rows: Arc::new(vec![vec![Value::Int(1)]]),
                bulk: None,
            },
        );
        let mut exec = physical(plan, vec![]);
        exec.kernel.prefix = (0..10000)
            .map(|_| {
                Arc::new(|mut inputs: Vec<Vec<Row>>, _: &mut State| Ok(inputs.remove(0)))
                    as Arc<Kernel>
            })
            .collect();
        let mut stream = exec.execute(0, task(1)).unwrap();
        assert_eq!(stream.try_next().await.unwrap().unwrap().num_rows(), 1);
        exec.state
            .lock()
            .unwrap()
            .context
            .jvm
            .cancelled
            .store(true, Ordering::Release);
        assert!(
            exec.execute(0, task(1))
                .unwrap()
                .try_next()
                .await
                .unwrap_err()
                .to_string()
                .contains("cancelled")
        );
    }
}
