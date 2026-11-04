
    use super::*;
    use futures::StreamExt;
    use std::sync::atomic::AtomicUsize;
    #[derive(Debug)]
    struct Feed {
        batches: Vec<RecordBatch>,
        polls: Arc<AtomicUsize>,
        properties: Arc<PlanProperties>,
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
    fn properties() -> Arc<PlanProperties> { Arc::new(PlanProperties::new(schema())) }
    fn physical(plan: LogicalPlan, inputs: Vec<Arc<dyn ExecutionPlan>>) -> KernelExec {
        let LogicalPlan::Extension(e) = plan else {
            panic!()
        };
        let mut kernel=e.node.as_any().downcast_ref::<RowKernel>().unwrap().clone();
        kernel.boundary=Some(policy::BoundaryKind::ResultTransport);
        KernelExec {
            kernel,
            inputs,
            state: Default::default(),
            properties: properties(),
        }
    }
    fn task(size: usize) -> Arc<TaskContext> { Arc::new(TaskContext::default().with_batch_size(size)) }
    fn rows(range: std::ops::Range<i64>) -> Vec<Row> {
        range
            .map(|i| Row {
                bindings: [("current".into(), Value::Int(i))].into(),
                bulk: (i + 1) as u64,
            })
            .collect()
    }
    #[tokio::test]
    async fn expanding_bulk_keeps_arrow_batches_bounded() {
        let output = super::super::reentry::SqlOutput {
            fields: vec!["current".into()], nodes: vec![], edges: vec![],
            policy: crate::ir::policy::GraphPlanPolicy::gremlin(),
            preserve_bulk: false, transport_state: false,
            identity_type: arrow::datatypes::DataType::Int64,
            implicit_fields: vec![],
        };
        let mut exec = physical(kernel("BulkInput", vec![], |_, _| Ok(vec![Row {
            bindings: [("current".into(), Value::Int(7))].into(), bulk: 33,
        }])), vec![]);
        exec.kernel.schema = Arc::new(output.schema().as_ref().clone().try_into().unwrap());
        exec.properties = Arc::new(PlanProperties::new(output.schema()));
        exec.kernel.relational_output = Some(output);
        let batches = exec.execute(0, task(4)).unwrap().try_collect::<Vec<_>>().await.unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).collect::<Vec<_>>(), vec![4,4,4,4,4,4,4,4,1]);
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
