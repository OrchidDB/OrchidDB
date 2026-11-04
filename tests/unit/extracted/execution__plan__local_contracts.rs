
    use super::*;
    use arrow::{array::Int64Array, datatypes::DataType};

    fn encoded(batch: &RecordBatch) -> Vec<u8> {
        let mut bytes=vec![];
        let mut writer=arrow::ipc::writer::StreamWriter::try_new(&mut bytes,&batch.schema()).unwrap();
        writer.write(batch).unwrap(); writer.finish().unwrap(); drop(writer); bytes
    }
    fn batch(values: Vec<Option<i64>>) -> RecordBatch {
        RecordBatch::try_new(Arc::new(Schema::new(vec![Field::new("v",DataType::Int64,true)])),
            vec![Arc::new(Int64Array::from(values))]).unwrap()
    }

    #[test]
    fn scalar_ipc_requires_exactly_one_cell_and_preserves_nulls_and_integers() {
        for value in [None,Some(i64::MIN),Some(i64::MAX)] {
            let bytes=encoded(&batch(vec![value]));
            assert_eq!(literal(&bytes).unwrap(),ScalarValue::Int64(value));
            let expr=LocalExpr::Literal {ipc:bytes,json:None,data_type:"int64".into()};
            validate_expr(&expr,&Functions::new(),0).unwrap();
            for rows in [0,3] {
                let result=evaluate(&expr,&batch(vec![Some(0);rows]),&Functions::new()).unwrap();
                assert_eq!(result.len(),rows);
                for row in 0..rows {assert_eq!(ScalarValue::try_from_array(&result,row).unwrap(),ScalarValue::Int64(value));}
            }
        }
        for values in [vec![],vec![Some(1),Some(2)]] {
            assert_eq!(literal(&encoded(&batch(values))).unwrap_err(),"invalid literal shape");
        }
        assert_eq!(literal(&schema_ipc(&Schema::empty()).unwrap()).unwrap_err(),"missing literal");
        assert!(literal(b"invalid IPC").is_err());
    }

    struct Output(ArrayRef);
    impl LocalFunction for Output {
        fn call(&self,_: &[ArrayRef],_: usize)->Result<ArrayRef,String>{Ok(self.0.clone())}
    }
    #[test]
    fn local_calls_validate_nested_inputs_and_enforce_result_shape() {
        let input=batch(vec![Some(3),None]);
        let expr=LocalExpr::Call {function:"f".into(),arguments:vec![LocalExpr::Column{index:0}],returns:"int64".into()};
        assert!(validate_expr(&expr,&Functions::new(),1).unwrap_err().contains("missing local function"));
        for output in [Arc::new(Int64Array::from(vec![1])) as ArrayRef,
            Arc::new(arrow::array::StringArray::from(vec!["a","b"])) as ArrayRef] {
            let functions=Functions::from([("f".into(),Arc::new(Output(output)) as Arc<dyn LocalFunction>)]);
            validate_expr(&expr,&functions,1).unwrap();
            assert!(evaluate(&expr,&input,&functions).unwrap_err().contains("invalid type or row count"));
            assert!(validate_expr(&expr,&functions,0).unwrap_err().contains("out of bounds"));
        }
        let functions=Functions::from([("f".into(),Arc::new(Output(input.column(0).clone())) as Arc<dyn LocalFunction>)]);
        assert_eq!(evaluate(&expr,&input,&functions).unwrap().to_data(),input.column(0).to_data());
        let cast=LocalExpr::Cast{input:Box::new(LocalExpr::Column{index:0}),data_type:"float64".into()};
        validate_expr(&cast,&functions,1).unwrap();
        assert_eq!(evaluate(&cast,&input,&functions).unwrap().data_type(),&DataType::Float64);
        let descriptor=LocalExpr::GraphElement{input:Box::new(LocalExpr::Column{index:0}),data_type:"string".into()};
        assert_eq!(evaluate(&descriptor,&input,&functions).unwrap_err(),"graph descriptor schema mismatch");
        let invalid=LocalExpr::Cast{input:Box::new(LocalExpr::Column{index:0}),data_type:"not_a_type".into()};
        assert!(validate_expr(&invalid,&functions,1).is_err());
    }

    #[test]
    fn local_input_pruning_remaps_nested_columns_and_retains_literal_payloads() {
        let literal=LocalExpr::Literal{ipc:encoded(&batch(vec![Some(7)])),json:Some(7.into()),data_type:"int64".into()};
        let mut expr=LocalExpr::Call{function:"f".into(),returns:"int64".into(),arguments:vec![
            LocalExpr::Cast{input:Box::new(LocalExpr::Column{index:8}),data_type:"int64".into()},
            LocalExpr::GraphElement{input:Box::new(LocalExpr::Column{index:3}),data_type:"string".into()},literal]};
        let mut demand=BTreeSet::new();expr.column_demand(&mut demand);
        assert_eq!(demand,BTreeSet::from([3,8]));
        expr.remap_columns(&BTreeMap::from([(3,0),(8,1)]));
        demand.clear();expr.column_demand(&mut demand);assert_eq!(demand,BTreeSet::from([0,1]));
        let LocalExpr::Call{arguments,..}=expr else{unreachable!()};
        let LocalExpr::Literal{ipc,json,..}=&arguments[2] else{unreachable!()};
        assert_eq!(literal_value(ipc),7);assert_eq!(*json,Some(7.into()));
    }
    fn literal_value(ipc:&[u8])->i64 {let ScalarValue::Int64(Some(value))=literal(ipc).unwrap() else{panic!("wrong literal type")};value}

struct NeverExecute(Arc<std::sync::atomic::AtomicUsize>);
#[async_trait::async_trait(?Send)]
impl crate::federation::Session for NeverExecute {
    fn dialect(&self) -> &str { "postgres" }
    async fn query_stream(&mut self, _: &str, _: Vec<crate::execution::stream::InputRelation>) -> Result<BatchStream, String> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err("unexpected execution during validation".into())
    }
}

fn wire_node(operation: Operation, width: usize) -> Node {
    Node { operation, columns: (0..width).map(|i| TransferColumn {
        name: format!("c{i}"), data_type: "int64".into(), nullable: true,
    }).collect(), schema_ipc: vec![] }
}

#[tokio::test]
async fn malformed_execution_plans_fail_before_opening_any_source() {
    let first = wire_node(Operation::SourceSql { source: "db".into(), dialect: "postgres".into(), sql: "SELECT 1".into() }, 1);
    let projection = |input, expressions| wire_node(Operation::LocalProject { input, expressions }, 1);
    let call = LocalExpr::Call { function: "missing".into(), arguments: vec![], returns: "int64".into() };
    let mut bad_type = wire_node(Operation::Input { one_row: true }, 1);
    bad_type.columns[0].data_type = "invalid-arrow-type".into();
    let cases = [
        (ExecutionPlan { version: 2, nodes: vec![first.clone()], result: 0 }, "protocol/result"),
        (ExecutionPlan { version: 1, nodes: vec![first.clone()], result: 1 }, "protocol/result"),
        (ExecutionPlan { version: 1, nodes: vec![], result: 0 }, "protocol/result"),
    ];
    let mut plans = cases.into_iter().collect::<Vec<_>>();
    for (node, message) in [
        (wire_node(Operation::SourceSql { source: "unknown".into(), dialect: "postgres".into(), sql: "SELECT 1".into() }, 1), "missing source"),
        (wire_node(Operation::SourceSql { source: "db".into(), dialect: "duckdb".into(), sql: "SELECT 1".into() }, 1), "source dialect mismatch"),
        (projection(1, vec![LocalExpr::Column { index: 0 }]), "topologically ordered"),
        (projection(0, vec![]), "projection width mismatch"),
        (projection(0, vec![LocalExpr::Column { index: 1 }]), "out of bounds"),
        (projection(0, vec![call]), "missing local function"),
        (wire_node(Operation::Duckdb { sql: "SELECT 1".into(), inputs: vec![] }, 1), "DuckDB completion binding"),
        (bad_type, "invalid-arrow-type"),
    ] {
        plans.push((ExecutionPlan { version: 1, nodes: vec![first.clone(), node], result: 1 }, message));
    }
    for (plan, expected) in plans {
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut sources = BTreeMap::from([("db".into(), Box::new(NeverExecute(calls.clone())) as Box<dyn crate::federation::Session>)]);
        let error = execute(&plan, &mut sources, &Functions::new(), None).await.err().expect("invalid plan must fail");
        assert!(error.contains(expected), "{expected}: {error}");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn prepared_request_validation_checks_dependencies_adapters_and_parameter_widths() {
    let first = wire_node(Operation::SourceSql { source: "db".into(), dialect: "postgres".into(), sql: "SELECT 1".into() }, 1);
    for (adapter, parameters, input, expected) in [
        ("duckdb", 0, None, "adapter mismatch"),
        ("postgres", 1, None, "unbound parameters"),
        ("postgres", 0, Some(0), "parameter count mismatch"),
        ("postgres", 1, Some(1), "topologically ordered"),
    ] {
        let template = crate::operations::RequestTemplate {
            adapter: adapter.into(), parameters, request: serde_json::json!({}), bindings: vec![],
        };
        let node = wire_node(Operation::SourceRequest { source: "db".into(), input, template }, 1);
        let plan = ExecutionPlan { version: 1, nodes: vec![first.clone(), node], result: 1 };
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut sources = BTreeMap::from([("db".into(), Box::new(NeverExecute(calls.clone())) as Box<dyn crate::federation::Session>)]);
        let error = execute(&plan, &mut sources, &Functions::new(), None).await.err().expect("invalid request must fail");
        assert!(error.contains(expected), "{expected}: {error}");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}

struct FixedSource(RecordBatch);
#[async_trait::async_trait(?Send)]
impl crate::federation::Session for FixedSource {
    fn dialect(&self) -> &str { "postgres" }
    async fn query_stream(&mut self, _: &str, _: Vec<crate::execution::stream::InputRelation>) -> Result<BatchStream, String> {
        Ok(BatchStream::new(self.0.schema(), futures::stream::iter([Ok(self.0.clone())])))
    }
}

#[tokio::test]
async fn source_streams_allow_positional_renaming_but_reject_type_and_width_changes() {
    let plan = ExecutionPlan { version: 1, result: 0, nodes: vec![wire_node(Operation::SourceSql {
        source: "db".into(), dialect: "postgres".into(), sql: "SELECT value".into(),
    }, 1)] };
    let original = batch(vec![Some(i64::MAX), None]);
    let invalid_type = RecordBatch::try_from_iter(vec![("v", Arc::new(arrow::array::StringArray::from(vec!["a", "b"])) as ArrayRef)]).unwrap();
    let invalid_width = RecordBatch::try_from_iter(vec![("v", original.column(0).clone()), ("extra", original.column(0).clone())]).unwrap();
    for (source, valid) in [(original.clone(), true), (invalid_type, false), (invalid_width, false)] {
        let mut sources = BTreeMap::from([("db".into(), Box::new(FixedSource(source)) as Box<dyn crate::federation::Session>)]);
        let result = execute(&plan, &mut sources, &Functions::new(), None).await;
        if valid {
            let batches = result.unwrap().collect_batches().await.unwrap();
            assert_eq!(batches.len(), 1);
            assert_eq!(batches[0].schema().field(0).name(), "c0");
            assert_eq!(batches[0].column(0).to_data(), original.column(0).to_data());
        } else {
            assert!(result.err().unwrap().contains("source result schema mismatch at node 0"));
        }
    }
}

struct FailedCursor(Arc<std::sync::atomic::AtomicUsize>);
#[async_trait::async_trait(?Send)]
impl crate::federation::Session for FailedCursor {
    fn dialect(&self) -> &str { "postgres" }
    async fn query_stream(&mut self, _: &str, _: Vec<crate::execution::stream::InputRelation>) -> Result<BatchStream, String> {
        self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(BatchStream::new(batch(vec![]).schema(), futures::stream::iter([Err("source cursor failed".into())])))
    }
}

#[tokio::test]
async fn source_cursor_errors_propagate_lazily_and_abort_session_reentry() {
    for count in [1, 2] {
        let node = wire_node(Operation::SourceSql { source: "db".into(), dialect: "postgres".into(), sql: "SELECT value".into() }, 1);
        let plan = ExecutionPlan { version: 1, nodes: vec![node; count], result: count - 1 };
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let mut sources = BTreeMap::from([("db".into(), Box::new(FailedCursor(calls.clone())) as Box<dyn crate::federation::Session>)]);
        let output = execute(&plan, &mut sources, &Functions::new(), None).await;
        let error = if count == 1 {
            output.unwrap().collect_batches().await.unwrap_err()
        } else {
            output.err().expect("failed spool must prevent a second query")
        };
        assert!(error.contains("source cursor failed"), "{error}");
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}

struct EventSource {
    name: &'static str,
    key: Option<&'static str>,
    concurrent: bool,
    events: Arc<std::sync::Mutex<Vec<String>>>,
}
#[async_trait::async_trait(?Send)]
impl crate::federation::Session for EventSource {
    fn dialect(&self) -> &str { "postgres" }
    fn concurrency_key(&self) -> Option<String> { self.key.map(str::to_owned) }
    fn supports_concurrent_statements(&self) -> bool { self.concurrent }
    async fn query_stream(&mut self, _: &str, _: Vec<crate::execution::stream::InputRelation>) -> Result<BatchStream, String> {
        self.events.lock().unwrap().push(format!("query:{}", self.name));
        let name = self.name;
        let events = self.events.clone();
        let values = batch(vec![Some(1)]);
        Ok(BatchStream::new(values.schema(), futures::stream::once(async move {
            events.lock().unwrap().push(format!("read:{name}"));
            Ok(values)
        })))
    }
}

#[tokio::test]
async fn session_aliases_spool_only_when_the_connection_cannot_overlap_statements() {
    for (left_key, right_key, concurrent, spool) in [
        (None, None, false, false),
        (Some("shared"), Some("shared"), false, true),
        (Some("left"), Some("right"), false, false),
        (Some("shared"), Some("shared"), true, false),
    ] {
        let events = Arc::new(std::sync::Mutex::new(vec![]));
        let mut sources = BTreeMap::new();
        for (name, key) in [("a", left_key), ("b", right_key)] {
            sources.insert(name.into(), Box::new(EventSource { name, key, concurrent, events: events.clone() }) as Box<dyn crate::federation::Session>);
        }
        let nodes = ["a", "b"].into_iter().map(|source| wire_node(Operation::SourceSql {
            source: source.into(), dialect: "postgres".into(), sql: "SELECT value".into(),
        }, 1)).collect();
        let plan = ExecutionPlan { version: 1, nodes, result: 1 };
        let batches = execute(&plan, &mut sources, &Functions::new(), None).await.unwrap().collect_batches().await.unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        let expected = if spool { vec!["query:a", "read:a", "query:b", "read:b"] }
            else { vec!["query:a", "query:b", "read:b"] };
        assert_eq!(*events.lock().unwrap(), expected, "{left_key:?} {right_key:?} concurrent={concurrent}");
    }
}

struct RetryCompletion {
    calls: Arc<std::sync::atomic::AtomicUsize>,
}
#[async_trait::async_trait(?Send)]
impl crate::federation::Session for RetryCompletion {
    fn dialect(&self) -> &str { "duckdb" }
    async fn query_stream(&mut self, sql: &str, inputs: Vec<crate::execution::stream::InputRelation>) -> Result<BatchStream,String> {
        assert_eq!(sql,"SELECT payload");
        assert!(inputs.is_empty());
        if self.calls.fetch_add(1,std::sync::atomic::Ordering::SeqCst)==0 {
            return Err("transient completion query failure".into());
        }
        let values=batch(vec![Some(42),None]);
        Ok(BatchStream::new(values.schema(),futures::stream::iter([Ok(values)])))
    }
}

#[tokio::test]
async fn lazy_completion_retries_factory_errors_but_reuses_an_initialized_session() {
    use crate::federation::Session;
    use std::sync::atomic::{AtomicUsize,Ordering};
    let attempts=Arc::new(AtomicUsize::new(0));
    let queries=Arc::new(AtomicUsize::new(0));
    let factory_attempts=attempts.clone();let session_queries=queries.clone();
    let mut completion=LazyDuckdb::new(move || {
        match factory_attempts.fetch_add(1,Ordering::SeqCst) {
            0=>Err("factory unavailable".into()),
            1=>Ok(Box::new(FixedSource(batch(vec![]))) as Box<dyn Session>),
            _=>Ok(Box::new(RetryCompletion{calls:session_queries.clone()}) as Box<dyn Session>),
        }
    });
    assert_eq!(completion.dialect(),"duckdb");
    assert_eq!(attempts.load(Ordering::SeqCst),0);
    for expected in ["factory unavailable","completion factory must return a DuckDB adapter","transient completion query failure"] {
        let error=completion.query_stream("SELECT payload",vec![]).await.err().expect("attempt must fail");
        assert_eq!(error,expected);
    }
    assert_eq!(attempts.load(Ordering::SeqCst),3);
    assert_eq!(queries.load(Ordering::SeqCst),1);
    for _ in 0..2 {
        let output=completion.query_stream("SELECT payload",vec![]).await.unwrap().collect_batches().await.unwrap();
        assert_eq!(output.len(),1);
        assert_eq!(output[0].column(0).to_data(),batch(vec![Some(42),None]).column(0).to_data());
    }
    assert_eq!(attempts.load(Ordering::SeqCst),3);
    assert_eq!(queries.load(Ordering::SeqCst),3);
}
