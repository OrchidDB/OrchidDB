
    use super::*;

#[test]
fn local_expression_serialization_preserves_aliases_and_nested_declared_casts() {
    use datafusion::logical_expr::{create_udf, lit, col, Volatility};
    let schema=DFSchema::try_from(batch(vec![Some(4)]).schema().as_ref().clone()).unwrap();
    let cast=create_udf("__engine_declared_argument_cast",vec![DataType::Int64],DataType::Float64,
        Volatility::Immutable,Arc::new(|args|Ok(args[0].clone())));
    let function=create_udf(&format!("{LOCAL}identity"),vec![DataType::Float64],DataType::Float64,
        Volatility::Immutable,Arc::new(|args|Ok(args[0].clone())));
    let expression=function.call(vec![cast.call(vec![col("v")])]).alias("result");
    let serialized=local_expr(&expression,&schema).unwrap();
    let LocalExpr::Call{function,arguments,returns}=&serialized else {panic!("missing local call")};
    assert_eq!(function,"identity");assert_eq!(returns,"float64");
    assert!(matches!(&arguments[..],[LocalExpr::Cast{input,data_type}]
        if matches!(input.as_ref(),LocalExpr::Column{index:0}) && data_type=="float64"));
    for value in [ScalarValue::Utf8(Some("nul\0and雪".into())),ScalarValue::Boolean(None),ScalarValue::Float64(Some(-1.25))] {
        let serialized=local_expr(&lit(value.clone()).alias("literal"),&schema).unwrap();
        let LocalExpr::Literal{ipc,..}=serialized else {panic!("missing literal")};
        assert_eq!(literal(&ipc).unwrap(),value);
    }
    assert!(local_expr(&col("missing"),&schema).is_err());
    assert!(local_expr(&col("v").eq(lit(4_i64)),&schema).unwrap_err().contains("requires source or DuckDB"));
}

#[test]
fn nested_local_evaluation_propagates_function_and_payload_errors() {
    struct Failure;
    impl LocalFunction for Failure {
        fn call(&self,_:&[ArrayRef],_:usize)->Result<ArrayRef,String>{Err("deliberate function failure".into())}
    }
    let functions=Functions::from([("fail".into(),Arc::new(Failure) as Arc<dyn LocalFunction>)]);
    let failing=||LocalExpr::Call{function:"fail".into(),arguments:vec![],returns:"int64".into()};
    for expression in [failing(),
        LocalExpr::Cast{input:Box::new(failing()),data_type:"float64".into()},
        LocalExpr::GraphElement{input:Box::new(failing()),data_type:"int64".into()},
        LocalExpr::Call{function:"fail".into(),arguments:vec![failing()],returns:"int64".into()}] {
        validate_expr(&expression,&functions,1).unwrap();
        assert_eq!(evaluate(&expression,&batch(vec![Some(1)]),&functions).unwrap_err(),"deliberate function failure");
    }
    let corrupt=LocalExpr::Literal{ipc:b"broken IPC".to_vec(),json:None,data_type:"int64".into()};
    assert!(validate_expr(&corrupt,&functions,1).is_err());
    assert!(evaluate(&corrupt,&batch(vec![None]),&functions).is_err());
}
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
    fn completion_detects_local_calls_structurally_without_reserving_string_contents() {
        use datafusion::logical_expr::{create_udf,lit,Volatility};
        let request:CompileRequest=serde_json::from_value(serde_json::json!({
            "version":1,"language":"cypher","dialect":"duckdb","query":"RETURN 1","tables":[]
        })).unwrap();
        let function=create_udf(&format!("{LOCAL}identity"),vec![DataType::Int64],DataType::Int64,
            Volatility::Immutable,Arc::new(|args|Ok(args[0].clone())));
        for (expr,is_local) in [
            (lit(format!("{LOCAL}payload")).alias("value"),false),
            (lit(1_i64).alias(format!("{LOCAL}alias")),false),
            (function.call(vec![lit(1_i64)]).alias("value"),true),
        ] {
            let plan=LogicalPlanBuilder::empty(true).project(vec![expr]).unwrap().build().unwrap();
            let mut builder=Builder {request:&request,bound:BTreeMap::new(),shared_occurrences:vec![],
                plan:ExecutionPlan{version:1,nodes:vec![],result:0}};
            let result=builder.complete_in_duckdb(&plan);
            if is_local {
                assert_eq!(result.unwrap_err(),"local UDF must be an explicit projection before relational completion");
                assert!(builder.plan.nodes.is_empty());
            } else {
                let id=result.expect("literal contents and output aliases must not be treated as function calls");
                assert!(matches!(builder.plan.nodes[id].operation,Operation::Duckdb{..}));
            }
        }
    }

    #[test]
    fn source_requests_reject_legacy_completion_engine_selection() {
        let base=serde_json::json!({"version":1,"language":"cypher","dialect":"duckdb",
            "query":"RETURN 1","tables":[],"sources":{"db":{"adapter":"trino"}}});
        let request:CompileRequest=serde_json::from_value(base.clone()).unwrap();
        validate_request(&request).unwrap();
        for (field,value) in [("execution_engine",serde_json::json!("db")),
            ("engines",serde_json::json!({"db":{"dialect":"duckdb"}}))] {
            let mut invalid=base.clone();invalid[field]=value;
            let request:CompileRequest=serde_json::from_value(invalid).unwrap();
            assert!(validate_request(&request).unwrap_err().contains("source execution uses sources"));
        }
    }

    #[test]
    fn local_graph_element_decoder_retains_nulls_duplicates_and_identity() {
        use crate::ir::{PropertyGraph, Row, value::Value, policy::{GraphPlanPolicy,ResultForm}};
        use crate::ir::rel::native_values;
        let graph=PropertyGraph::new();
        let node=graph.insert_node("Person",BTreeMap::from([("name".into(),Value::String("Ada".into()))]));
        let fields=vec!["entity".to_string()];
        for empty in [false,true] {
            let rows=if empty {vec![]} else {vec![node.clone(),Value::Null,node.clone()].into_iter()
                .map(|value|Row::new().with("entity",value)).collect::<Vec<_>>()};
            let input=native_values::result_batch(&fields,ResultForm::RowSet,&rows,&graph,&GraphPlanPolicy::cypher()).unwrap();
            let expression=LocalExpr::GraphElement {input:Box::new(LocalExpr::Column{index:0}),
                data_type:crate::federation::type_name(input.column(0).data_type()).unwrap()};
            validate_expr(&expression,&Functions::new(),1).unwrap();
            let result=evaluate(&expression,&input,&Functions::new()).unwrap();
            assert_eq!(result.len(),rows.len());
            let output=RecordBatch::try_new(Arc::new(Schema::new(vec![Field::new("entity",result.data_type().clone(),true)])),vec![result]).unwrap();
            let decoded=native_values::decode_rows(&output,&fields,&graph).unwrap();
            assert_eq!(decoded.iter().map(|r|r.bindings.get("entity")).collect::<Vec<_>>(),
                rows.iter().map(|r|r.bindings.get("entity")).collect::<Vec<_>>());
        }
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

#[test]
fn local_call_hoisting_respects_conditional_and_sql_argument_boundaries() {
    use datafusion::logical_expr::{create_udf,col,lit,when,Volatility};
    let request:CompileRequest=serde_json::from_value(serde_json::json!({"version":1,"language":"cypher","dialect":"duckdb","query":"RETURN 1","tables":[]})).unwrap();
    let local=create_udf(&format!("{LOCAL}positive"),vec![DataType::Int64],DataType::Boolean,
        Volatility::Immutable,Arc::new(|_|unreachable!("planning must never evaluate a local function")));
    let identity=create_udf(&format!("{LOCAL}identity"),vec![DataType::Int64],DataType::Int64,
        Volatility::Immutable,Arc::new(|_|unreachable!("planning must never evaluate a local function")));
    let input=LogicalPlanBuilder::empty(true).project(vec![lit(7_i64).alias("v")]).unwrap().build().unwrap();
    for predicate in [
        when(col("v").gt(lit(0_i64)),local.call(vec![col("v")])).otherwise(lit(false)).unwrap(),
        local.call(vec![col("v")]).and(col("v").gt(lit(0_i64))),
        col("v").lt(lit(0_i64)).or(local.call(vec![col("v")])),
        local.call(vec![col("v")+lit(1_i64)]),
        datafusion::functions::math::expr_fn::abs(identity.call(vec![col("v")])).gt(lit(0_i64)),
    ] {
        let plan=LogicalPlanBuilder::from(input.clone()).filter(predicate).unwrap().build().unwrap();
        let mut builder=Builder {request:&request,bound:BTreeMap::new(),shared_occurrences:vec![],plan:ExecutionPlan {version:1,nodes:vec![],result:0}};
        let error=builder.lower(&plan).unwrap_err();
        assert!(error.contains("local UDF must be an explicit projection"),"{error}");
        assert!(!builder.plan.nodes.iter().any(|node|matches!(node.operation,Operation::LocalProject{..})),"unsafe expression must not be hoisted before rejection");
    }
}

#[test]
fn unconditional_local_call_hoisting_avoids_user_column_collisions() {
    use datafusion::logical_expr::{create_udf,col,lit,Volatility};
    let request:CompileRequest=serde_json::from_value(serde_json::json!({"version":1,"language":"cypher","dialect":"duckdb","query":"RETURN 1","tables":[]})).unwrap();
    let local=create_udf(&format!("{LOCAL}positive"),vec![DataType::Int64],DataType::Boolean,
        Volatility::Immutable,Arc::new(|_|unreachable!("planning must never evaluate a local function")));
    let input=LogicalPlanBuilder::empty(true).project(vec![lit(7_i64).alias("v"),lit(99_i64).alias("__orchid_local_0")]).unwrap().build().unwrap();
    let plan=LogicalPlanBuilder::from(input).filter(local.call(vec![col("v")])).unwrap().build().unwrap();
    let mut builder=Builder {request:&request,bound:BTreeMap::new(),shared_occurrences:vec![],plan:ExecutionPlan {version:1,nodes:vec![],result:0}};
    let id=builder.lower(&plan).unwrap();
    assert_eq!(builder.plan.nodes[id].columns.iter().map(|c|c.name.as_str()).collect::<Vec<_>>(),vec!["v","__orchid_local_0"]);
    let lifted=builder.plan.nodes.iter().filter(|node|matches!(&node.operation,Operation::LocalProject{expressions,..}
        if expressions.iter().any(|expr|matches!(expr,LocalExpr::Call{function,..} if function=="positive")))).collect::<Vec<_>>();
    assert_eq!(lifted.len(),1,"the local function must be evaluated in exactly one stage");
    assert!(lifted[0].columns.iter().any(|c|c.name=="__orchid_local_0_"));
}

struct RequestRecorder(Arc<std::sync::Mutex<Vec<Vec<serde_json::Value>>>>);
#[async_trait::async_trait(?Send)]
impl crate::federation::Session for RequestRecorder {
    fn dialect(&self)->&str {"postgres"}
    async fn execute_requests_stream(&mut self,mut requests:crate::federation::RequestStream,
        _:Vec<TransferColumn>)->Result<BatchStream,String> {
        let mut rows=0;
        while let Some(requests)=requests.next().await {
            let requests=requests?;
            rows+=requests.len();
            self.0.lock().unwrap().push(requests);
        }
        let output=batch(vec![Some(7);rows]);
        Ok(BatchStream::new(output.schema(),futures::stream::iter([Ok(output)])))
    }
}

fn parameter_request()->crate::operations::RequestTemplate {
    use crate::operations::{RequestTemplate,RequestBinding,RequestEncoding};
    RequestTemplate {adapter:"postgres".into(),parameters:1,
        request:serde_json::json!({"value":null,"is_null":false,"literal":"$1"}),bindings:vec![
            RequestBinding {pointer:"/value".into(),parameter:0,encoding:RequestEncoding::Value},
            RequestBinding {pointer:"/is_null".into(),parameter:0,encoding:RequestEncoding::SqlNull},
        ]}
}

#[tokio::test]
async fn dependent_requests_batch_at_64_without_losing_order_nulls_or_empty_inputs() {
    for count in [0,1,63,64,65,129] {
        let values=(0..count).map(|i|if i%3==0 {None}else{Some(i as i64)}).collect::<Vec<_>>();
        let recorded=Arc::new(std::sync::Mutex::new(vec![]));
        let mut sources=BTreeMap::from([
            ("input".into(),Box::new(FixedSource(batch(values.clone()))) as Box<dyn crate::federation::Session>),
            ("requests".into(),Box::new(RequestRecorder(recorded.clone())) as Box<dyn crate::federation::Session>),
        ]);
        let plan=ExecutionPlan {version:1,result:1,nodes:vec![
            wire_node(Operation::SourceSql {source:"input".into(),dialect:"postgres".into(),sql:"SELECT v".into()},1),
            wire_node(Operation::SourceRequest {source:"requests".into(),input:Some(0),template:parameter_request()},1),
        ]};
        let result=execute(&plan,&mut sources,&Functions::new(),None).await.unwrap().collect_batches().await.unwrap();
        assert_eq!(result.iter().map(RecordBatch::num_rows).sum::<usize>(),count);
        let observed=recorded.lock().unwrap();
        assert_eq!(observed.iter().map(Vec::len).collect::<Vec<_>>(),values.chunks(64).map(<[_]>::len).collect::<Vec<_>>());
        let flattened=observed.iter().flatten().collect::<Vec<_>>();
        for (value,request) in values.iter().zip(flattened) {
            assert_eq!(request["value"],serde_json::json!(value));
            assert_eq!(request["is_null"],serde_json::json!(value.is_none()));
            assert_eq!(request["literal"],"$1");
        }
    }
}

#[tokio::test]
async fn independent_request_binds_once_and_preserves_literal_payload() {
    let recorded=Arc::new(std::sync::Mutex::new(vec![]));
    let mut sources=BTreeMap::from([("requests".into(),Box::new(RequestRecorder(recorded.clone())) as Box<dyn crate::federation::Session>)]);
    let payload=serde_json::json!({"query":"雪 '$1'\u{0000}","limit":i64::MAX});
    let template=crate::operations::RequestTemplate {adapter:"postgres".into(),parameters:0,request:payload.clone(),bindings:vec![]};
    let plan=ExecutionPlan {version:1,result:0,nodes:vec![wire_node(Operation::SourceRequest {source:"requests".into(),input:None,template},1)]};
    let output=execute(&plan,&mut sources,&Functions::new(),None).await.unwrap().collect_batches().await.unwrap();
    assert_eq!(output.iter().map(RecordBatch::num_rows).sum::<usize>(),1);
    assert_eq!(*recorded.lock().unwrap(),vec![vec![payload]]);
}

#[tokio::test]
async fn dependent_request_propagates_upstream_errors_without_sending_requests() {
    let recorded=Arc::new(std::sync::Mutex::new(vec![]));
    let calls=Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut sources=BTreeMap::from([
        ("input".into(),Box::new(FailedCursor(calls.clone())) as Box<dyn crate::federation::Session>),
        ("requests".into(),Box::new(RequestRecorder(recorded.clone())) as Box<dyn crate::federation::Session>),
    ]);
    let plan=ExecutionPlan {version:1,result:1,nodes:vec![
        wire_node(Operation::SourceSql {source:"input".into(),dialect:"postgres".into(),sql:"SELECT v".into()},1),
        wire_node(Operation::SourceRequest {source:"requests".into(),input:Some(0),template:parameter_request()},1),
    ]};
    let error=execute(&plan,&mut sources,&Functions::new(),None).await.err().expect("upstream error must propagate");
    assert_eq!(error,"source cursor failed");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst),1);
    assert!(recorded.lock().unwrap().is_empty());
}

#[tokio::test]
async fn request_binding_errors_stop_at_chunk_boundary_without_partial_chunk_delivery() {
    for invalid_row in [0,63,64,65,128] {
        let recorded=Arc::new(std::sync::Mutex::new(vec![]));
        let values=(0..=invalid_row).map(|row|if row==invalid_row {f64::NAN}else{row as f64}).collect::<Vec<_>>();
        let input=RecordBatch::try_from_iter(vec![("v",Arc::new(arrow::array::Float64Array::from(values)) as ArrayRef)]).unwrap();
        let mut sources=BTreeMap::from([
            ("input".into(),Box::new(FixedSource(input)) as Box<dyn crate::federation::Session>),
            ("requests".into(),Box::new(RequestRecorder(recorded.clone())) as Box<dyn crate::federation::Session>),
        ]);
        let mut input_node=wire_node(Operation::SourceSql {source:"input".into(),dialect:"postgres".into(),sql:"SELECT v".into()},1);
        input_node.columns[0].data_type="float64".into();
        let plan=ExecutionPlan {version:1,result:1,nodes:vec![input_node,
            wire_node(Operation::SourceRequest {source:"requests".into(),input:Some(0),template:parameter_request()},1)]};
        let error=execute(&plan,&mut sources,&Functions::new(),None).await.err().expect("non-finite parameter must fail");
        assert!(error.contains("require finite numbers"),"{error}");
        let requests=recorded.lock().unwrap();
        assert_eq!(requests.len(),invalid_row/64);
        assert!(requests.iter().all(|chunk|chunk.len()==64));
        for (row,request) in requests.iter().flatten().enumerate() {
            assert_eq!(request["value"].as_f64(),Some(row as f64));
        }
    }
}

struct ContinuationAdapter;
impl crate::operations::RequestAdapter for ContinuationAdapter {
    fn name(&self)->&str {"test_wire_continuation"}
    fn lower(&self,plan:&LogicalPlan)->Result<Option<crate::operations::PreparedOperation>,String> {
        use crate::operations::{PreparedOperation,RequestResult,RequestTemplate};
        let LogicalPlan::TableScan(scan)=plan else {return Ok(None)};
        let name=scan.table_name.to_string();
        let Some(mode)=name.strip_prefix("continuation_") else {return Ok(None)};
        let marker=||RequestResult {schema:plan.schema().clone()}.into_plan();
        let replacement=match mode {
            "missing"=>LogicalPlanBuilder::empty(true).project(vec![datafusion::logical_expr::lit(1_i64).alias("v")]).unwrap().build().unwrap(),
            "multiple"=>LogicalPlanBuilder::from(marker()).union(marker()).unwrap().build().unwrap(),
            "mismatch"=>RequestResult {schema:Arc::new(DFSchema::empty())}.into_plan(),
            _=>marker(),
        };
        let source=if mode=="dependent" {Some(Arc::new(LogicalPlanBuilder::empty(true)
            .project(vec![datafusion::logical_expr::lit(9_i64).alias("parameter")]).unwrap().build().unwrap()))} else {None};
        Ok(Some(PreparedOperation {schema:plan.schema().clone(),replacement:Some(replacement),
            template:RequestTemplate {adapter:self.name().into(),parameters:usize::from(source.is_some()),
                request:serde_json::json!({"mode":mode}),bindings:vec![]},source}))
    }
}

#[test]
fn prepared_continuations_require_one_schema_matching_result_leaf() {
    static REGISTER:std::sync::Once=std::sync::Once::new();
    REGISTER.call_once(||crate::operations::register(Arc::new(ContinuationAdapter)).unwrap());
    for mode in ["valid","dependent","missing","multiple","mismatch"] {
        let name=format!("continuation_{mode}");
        let request:CompileRequest=serde_json::from_value(serde_json::json!({
            "version":1,"language":"cypher","dialect":"duckdb","query":"RETURN 1",
            "sources":{"requests":{"adapter":"test_wire_continuation"}},
            "tables":[{"name":name,"engine":"requests","columns":[{"name":"v","data_type":"int64"}]}]
        })).unwrap();
        validate_request(&request).unwrap();
        let table=datafusion::datasource::empty::EmptyTable::new(Arc::new(Schema::new(vec![Field::new("v",DataType::Int64,true)])));
        let plan=LogicalPlanBuilder::scan(&name,datafusion::datasource::provider_as_source(Arc::new(table)),None).unwrap().build().unwrap();
        let mut builder=Builder {request:&request,bound:BTreeMap::new(),shared_occurrences:vec![],
            plan:ExecutionPlan {version:1,nodes:vec![],result:0}};
        let result=builder.lower(&plan);
        if matches!(mode,"missing"|"multiple") {
            assert!(result.unwrap_err().contains("exactly one result leaf"));
        } else if mode=="mismatch" {
            assert!(result.unwrap_err().contains("continuation schema mismatch"));
        } else {
            let id=result.unwrap();
            assert_eq!(builder.plan.nodes[id].columns.len(),1);
            assert_eq!(builder.plan.nodes[id].columns[0].name,"v");
            let requests=builder.plan.nodes.iter().filter_map(|node|match &node.operation {
                Operation::SourceRequest {input,template,..}=>Some((input,template)),_=>None
            }).collect::<Vec<_>>();
            assert_eq!(requests.len(),1);
            assert_eq!(requests[0].0.is_some(),mode=="dependent");
            assert_eq!(requests[0].1.parameters,usize::from(mode=="dependent"));
            assert!(builder.plan.nodes.iter().all(|n|!matches!(n.operation,Operation::Duckdb{..}|Operation::SourceSql{..})));
        }
    }
}
