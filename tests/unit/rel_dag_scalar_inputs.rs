//! Native input evaluation must precede SQL placement on every execution path.
use super::*;
use arrow::{array::Float64Array, datatypes::DataType};
use datafusion::{
    common::ScalarValue,
    logical_expr::{ColumnarValue, LogicalPlanBuilder, Volatility, col, create_udf, lit},
};
use std::sync::atomic::{AtomicUsize, Ordering};

#[test]
fn sql_array_coercion_reuses_matching_storage_and_rejects_nonnull_null_outputs() {
    use arrow::array::{ArrayRef,Int32Array};
    let array=Arc::new(Int32Array::from(vec![Some(7),None])) as ArrayRef;
    let unchanged=coerce_sql_array(&array,&DataType::Int32).unwrap();
    assert!(Arc::ptr_eq(&array,&unchanged));
    assert!(coerce_sql_array(&array,&DataType::Null).is_err(),"non-null SQL values must not disappear during Null restoration");
    let nulls=Arc::new(Int32Array::from(vec![None,None])) as ArrayRef;
    let restored=coerce_sql_array(&nulls,&DataType::Null).unwrap();
    assert_eq!(restored.len(),2);assert_eq!(restored.logical_null_count(),2);
    for value in [128_i64,-129] {
        let array=Arc::new(arrow::array::Int64Array::from(vec![value])) as ArrayRef;
        assert!(coerce_sql_array(&array,&DataType::Int8).is_err());
    }
}

#[test]
fn sql_array_coercion_retains_sliced_list_offsets_parent_validity_and_child_nulls() {
    use arrow::array::{Array,ArrayRef,ListArray,LargeListArray,FixedSizeListArray};
    use arrow::datatypes::{Field,Int32Type};
    let values=vec![Some(vec![Some(1),Some(2)]),None,Some(vec![Some(7),None])];
    let field=Arc::new(Field::new("renamed_item",DataType::Int64,true));
    for (array,target) in [
        (Arc::new(ListArray::from_iter_primitive::<Int32Type,_,_>(values.clone())) as ArrayRef,DataType::List(field.clone())),
        (Arc::new(LargeListArray::from_iter_primitive::<Int32Type,_,_>(values.clone())) as ArrayRef,DataType::LargeList(field.clone())),
        (Arc::new(FixedSizeListArray::from_iter_primitive::<Int32Type,_,_>(values,2)) as ArrayRef,DataType::FixedSizeList(field,2)),
    ] {
        let slice=array.slice(1,2);let restored=coerce_sql_array(&slice,&target).unwrap();
        assert_eq!(restored.data_type(),&target);assert_eq!(restored.len(),2);assert!(restored.is_null(0));
        let value=ScalarValue::try_from_array(&restored,1).unwrap();
        let children=match value {ScalarValue::List(a)=>a.value(0),ScalarValue::LargeList(a)=>a.value(0),ScalarValue::FixedSizeList(a)=>a.value(0),_=>panic!()};
        assert_eq!(children.len(),2);assert_eq!(ScalarValue::try_from_array(&children,0).unwrap(),ScalarValue::Int64(Some(7)));assert!(children.is_null(1));
    }
}

#[test]
fn sql_array_coercion_restores_nested_null_fields_and_positional_struct_names() {
    use arrow::array::{ArrayRef,StructArray,Int32Array,StringArray};
    use arrow::datatypes::Field;
    let array=Arc::new(StructArray::from(vec![
        (Arc::new(Field::new("physical_null",DataType::Int32,true)),Arc::new(Int32Array::from(vec![None,None])) as ArrayRef),
        (Arc::new(Field::new("physical_text",DataType::Utf8,true)),Arc::new(StringArray::from(vec![Some("雪"),None])) as ArrayRef),
    ])) as ArrayRef;
    let target=DataType::Struct(vec![Field::new("logical_null",DataType::Null,true),Field::new("logical_text",DataType::LargeUtf8,true)].into());
    let restored=coerce_sql_array(&array,&target).unwrap();assert_eq!(restored.data_type(),&target);
    let restored=restored.as_any().downcast_ref::<StructArray>().unwrap();
    assert_eq!(restored.column(0).logical_null_count(),2);assert_eq!(ScalarValue::try_from_array(restored.column(1),0).unwrap(),ScalarValue::LargeUtf8(Some("雪".into())));
    assert!(restored.column(1).is_null(1));
}

#[test]
fn sql_array_coercion_restores_json_domain_validity_and_rejects_invalid_payloads() {
    use arrow::array::{ArrayRef,StringArray};
    use crate::ir::functions::domain;
    let array=Arc::new(StringArray::from(vec![Some("null"),None,Some("{\"a\":7}")])) as ArrayRef;
    let restored=coerce_sql_array(&array,&domain::json_type()).unwrap();
    assert!(!restored.is_null(0));assert!(restored.is_null(1));
    assert_eq!(domain::json_text(&ScalarValue::try_from_array(&restored,2).unwrap()).unwrap(),Some("{\"a\":7}".into()));
    let invalid=Arc::new(StringArray::from(vec!["not-json"])) as ArrayRef;
    assert!(coerce_sql_array(&invalid,&domain::json_type()).is_err());
}

fn vector(values: &[f64]) -> ScalarValue {
    ScalarValue::List(ScalarValue::new_list(
        &values
            .iter()
            .map(|v| ScalarValue::Float64(Some(*v)))
            .collect::<Vec<_>>(),
        &DataType::Float64,
        true,
    ))
}

fn scan(values: Vec<ScalarValue>) -> LogicalPlanBuilder {
    let array = ScalarValue::iter_to_array(values).unwrap();
    let batch = RecordBatch::try_from_iter(vec![("column1", array)]).unwrap();
    let table =
        datafusion::datasource::MemTable::try_new(batch.schema(), vec![vec![batch]]).unwrap();
    LogicalPlanBuilder::scan(
        "documents",
        datafusion::datasource::provider_as_source(Arc::new(table)),
        None,
    )
    .unwrap()
}

fn resources(mode: usize) -> DagSession {
    if mode == 0 {
        return DagSession::new(None);
    }
    let executor = Arc::new(Mutex::new(sql::DuckDbExecutor::default()));
    let mut resources = DagSession::with_shared(executor.clone(), Default::default());
    if mode == 2 {
        resources.region_session = Some(Arc::new(Mutex::new(Box::new(sql::DuckDbSession {
            executor,
        }))));
    }
    resources
}

fn lowered(plan: LogicalPlan) -> LoweredPlan {
    LoweredPlan {
        rule_context: Default::default(),
        fields: plan
            .schema()
            .fields()
            .iter()
            .map(|f| f.name().clone())
            .collect(),
        plan,
        result_form: crate::ir::policy::ResultForm::RowSet,
        islands: Default::default(),
    }
}

#[tokio::test]
async fn local_embedding_input_enters_sql_as_a_typed_vector() {
    for (mode, volatility) in (0..3).flat_map(|mode| {
        [Volatility::Immutable, Volatility::Stable].map(|volatility| (mode, volatility))
    }) {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        // Deliberately has no SQL implementation or SQL function registration.
        let embed = create_udf(
            "local_embedding",
            vec![DataType::Utf8],
            vector(&[0.0, 0.0]).data_type(),
            volatility,
            Arc::new(move |args| {
                count.fetch_add(1, Ordering::SeqCst);
                let [ColumnarValue::Scalar(ScalarValue::Utf8(Some(prompt)))] = args else {
                    return Err(DataFusionError::Execution("expected scalar prompt".into()));
                };
                Ok(ColumnarValue::Scalar(vector(&[prompt.len() as f64, 1.0])))
            }),
        );
        let resources = resources(mode);
        for (query, prompt) in ["rag", "longer query"].into_iter().enumerate() {
            let score = crate::ir::functions::search::function("vector.dot").unwrap();
            let plan = scan(vec![vector(&[1.0, 0.0]), vector(&[0.0, 1.0])])
                .project(vec![
                    score
                        .call(vec![
                            // Exercise a nested local input transformation as well.
                            embed.call(vec![datafusion::functions::string::upper().call(vec![
                                Expr::Placeholder(
                                    datafusion::logical_expr::expr::Placeholder::new_with_field(
                                        "$1".into(),
                                        Some(Arc::new(arrow::datatypes::Field::new(
                                            "prompt",
                                            DataType::Utf8,
                                            true,
                                        ))),
                                    ),
                                ),
                            ])]),
                            col("column1"),
                        ])
                        .alias("score"),
                ])
                .unwrap()
                .build()
                .unwrap()
                .with_param_values(vec![ScalarValue::Utf8(Some(prompt.into()))])
                .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), query);
            let (result, stats) =
                execute_with_extensions(lowered(plan), vec![], None, Some(&resources))
                    .await
                    .unwrap();
            let scores = result
                .batch
                .column(0)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap();
            assert_eq!(scores.values().as_ref(), &[prompt.len() as f64, 1.0]);
            assert_eq!(calls.load(Ordering::SeqCst), query + 1, "mode {mode}");
            assert_eq!(stats.duckdb_regions, 1, "{stats:?}");
            assert_eq!(stats.datafusion_operators, 0, "{stats:?}");
            assert!(!stats.sql_queries.is_empty());
            assert!(
                stats
                    .sql_queries
                    .iter()
                    .all(|sql| !sql.contains("local_embedding")),
                "{stats:?}"
            );
        }
    }
}

#[tokio::test]
async fn local_input_nulls_and_errors_preserve_native_semantics() {
    use arrow::array::Array;
    for mode in 0..3 {
        for fail in [false, true] {
            let function = create_udf(
                "local_nullable_input",
                vec![DataType::Utf8],
                DataType::Float64,
                Volatility::Immutable,
                Arc::new(move |_| {
                    if fail {
                        Err(DataFusionError::Execution("local input failed".into()))
                    } else {
                        Ok(ColumnarValue::Scalar(ScalarValue::Float64(None)))
                    }
                }),
            );
            let plan = LogicalPlanBuilder::empty(true)
                .project(vec![function.call(vec![lit("prompt")]).alias("input")])
                .unwrap()
                .build()
                .unwrap();
            let resources = resources(mode);
            let result =
                execute_with_extensions(lowered(plan), vec![], None, Some(&resources)).await;
            if fail {
                let error = result.unwrap_err();
                assert!(
                    (format!("{error:?}").contains("SQL-required lowering rejected") || format!("{error:?}").contains("local input failed")),
                    "mode {mode}: {error:?}"
                );
            } else {
                let (result, stats) = result.unwrap();
                assert_eq!(result.batch.column(0).data_type(), &DataType::Float64);
                assert_eq!(result.batch.num_rows(), 1);
                assert!(result.batch.column(0).is_null(0));
                assert_eq!(stats.duckdb_regions, 1, "{stats:?}");
                assert_eq!(stats.datafusion_operators, 0, "{stats:?}");
            }
        }
    }
}

#[tokio::test]
async fn row_dependent_and_volatile_native_calls_are_not_pre_evaluated() {
    for mode in 0..3 {
        for volatility in [Volatility::Immutable, Volatility::Volatile] {
            let calls = Arc::new(AtomicUsize::new(0));
            let count = calls.clone();
            let function = create_udf(
                "local_identity",
                vec![DataType::Int64],
                DataType::Int64,
                volatility,
                Arc::new(move |args| {
                    count.fetch_add(1, Ordering::SeqCst);
                    Ok(args[0].clone())
                }),
            );
            let argument = if volatility == Volatility::Volatile {
                lit(7_i64)
            } else {
                col("column1")
            };
            let plan = scan(vec![
                ScalarValue::Int64(Some(1)),
                ScalarValue::Int64(Some(2)),
            ])
            .project(vec![function.call(vec![argument]).alias("value")])
            .unwrap()
            .build()
            .unwrap();
            let resources = resources(mode);
            let error = prepare_with_extensions(
                &plan,
                vec![],
                &resources,
                Arc::new(Mutex::new(Default::default())),
                &Default::default(),
            )
            .await
            .unwrap_err();
            assert!(
                error.to_string().contains("SQL-required lowering rejected"),
                "{error}"
            );
            assert_eq!(
                calls.load(Ordering::SeqCst),
                0,
                "unlowered functions must never execute"
            );
        }
    }
}

#[tokio::test]
async fn unlowered_scalar_calls_cannot_become_native_sql_inputs() {
    for mode in [1, 2] {
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let native = create_udf(
            "native_identity_no_sql",
            vec![DataType::Int64],
            DataType::Int64,
            Volatility::Volatile,
            Arc::new(move |args| {
                let rows = match &args[0] {
                    ColumnarValue::Array(a) => a.len(),
                    ColumnarValue::Scalar(_) => 1,
                };
                count.fetch_add(rows, Ordering::SeqCst);
                Ok(args[0].clone())
            }),
        );
        let plan = scan(vec![
            ScalarValue::Int64(Some(1)),
            ScalarValue::Int64(Some(2)),
            ScalarValue::Int64(Some(2)),
            ScalarValue::Int64(None),
        ])
        .project(vec![native.call(vec![col("column1")]).alias("x")])
        .unwrap()
        .filter(col("x").gt(lit(1_i64)))
        .unwrap()
        .aggregate(
            Vec::<Expr>::new(),
            vec![datafusion::functions_aggregate::expr_fn::count(col("x")).alias("n")],
        )
        .unwrap()
        .build()
        .unwrap();
        let resources = resources(mode);
        let error = execute_with_extensions(lowered(plan), vec![], None, Some(&resources))
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("SQL-required lowering rejected"),
            "{error}"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "an unlowered scalar is not a native boundary"
        );
    }
}

#[test]
fn sql_region_dependency_rewrites_preserve_prepared_queries_for_every_engine() {
    use arrow::datatypes::{Field,Schema};
    let schema=Arc::new(Schema::new(vec![Field::new("answer",DataType::Int64,false)]));
    let input=LogicalPlanBuilder::empty(true).build().unwrap();
    let replacement=LogicalPlanBuilder::empty(false).build().unwrap();
    for (dialect,name) in [(sql::SqlDialect::DuckDb,"DuckDbRegion"),(sql::SqlDialect::Postgres,"PostgresRegion"),(sql::SqlDialect::resolve("trino").unwrap(),"SqlRegion")] {
        let mut prepared=sql::PreparedSql::query_only("SELECT 7 AS answer".into(),schema.clone());prepared.dialect=dialect;
        let region=DuckDbRegion {id:7,schema:Arc::new(datafusion::common::DFSchema::try_from(schema.as_ref().clone()).unwrap()),prepared:Arc::new(prepared),inputs:vec![input.clone()],input_tables:vec![],source_tables:vec![]};
        assert_eq!(region.name(),name);assert!(region.expressions().is_empty());
        assert_eq!(region.inputs(),vec![&input]);
        let rewritten=region.with_exprs_and_inputs(vec![],vec![replacement.clone()]).unwrap();
        assert_eq!(rewritten.inputs(),vec![&replacement]);
        assert_eq!(rewritten.schema(),region.schema());assert!(Arc::ptr_eq(&rewritten.prepared,&region.prepared));
        assert!(region.with_exprs_and_inputs(vec![lit(1_i64)],vec![input.clone()]).is_err());
        assert!(region.with_exprs_and_inputs(vec![],vec![]).is_err());
        assert_eq!(region,region.clone());assert_ne!(region,rewritten);
        let mut later=region.clone();later.id=8;
        assert!(region<later);assert_eq!(region.partial_cmp(&later),Some(std::cmp::Ordering::Less));
        let hash=|r:&DuckDbRegion|{let mut h=std::collections::hash_map::DefaultHasher::new();r.hash(&mut h);h.finish()};
        assert_eq!(hash(&region),hash(&region.clone()));
        assert!(format!("{region:?}").contains("7"));
        let plan=LogicalPlan::Extension(Extension {node:Arc::new(region)});
        assert!(plan.display_indent().to_string().contains(&format!("{}Region(id=7)",dialect.name())));
    }
}

#[test]
fn sql_physical_region_rewrites_preserve_typed_inputs_and_execution_resources() {
    use arrow::datatypes::{Field,Schema};
    let schema=Arc::new(Schema::new(vec![Field::new("x",DataType::Int64,true)]));
    for dialect in [sql::SqlDialect::DuckDb,sql::SqlDialect::resolve("trino").unwrap()] {
        let mut prepared=sql::PreparedSql::query_only("SELECT x FROM input".into(),schema.clone());prepared.dialect=dialect;
        let input=Arc::new(engine_exec::InputBatches::new(schema.clone(),vec![])) as Arc<dyn ExecutionPlan>;
        let exec=Arc::new(DuckDbExec {inputs:vec![input.clone()],input_tables:vec![],source_tables:vec![],region_session:None,
            cost:Default::default(),prepared:Arc::new(prepared),executor:Arc::new(Mutex::new(sql::DuckDbExecutor::default())),properties:Arc::new(PlanProperties::new(schema.clone()))});
        assert_eq!(exec.schema(),schema);assert!(exec.description().contains("live Arrow streams"));
        assert_eq!(exec.children().len(),1);
        let rewritten=exec.clone().with_new_children(vec![input]).unwrap();
        let rewritten=rewritten.as_any().downcast_ref::<DuckDbExec>().unwrap();
        assert!(Arc::ptr_eq(&rewritten.prepared,&exec.prepared));assert!(Arc::ptr_eq(&rewritten.executor,&exec.executor));assert!(Arc::ptr_eq(&rewritten.cost,&exec.cost));
        assert!(exec.clone().with_new_children(vec![]).unwrap_err().to_string().contains("dependency mismatch"));
        validate_sql_required_physical(&(exec as Arc<dyn ExecutionPlan>)).unwrap();
    }
}

#[test]
fn sql_nested_execution_statistics_accumulate_work_without_recounting_result_size() {
    let mut parent=DagStats {duckdb_regions:1,physical_plan:"outer".into(),logical_plan:"outer logical".into(),..Default::default()};
    parent.cost.sql_executions=2;parent.cost.result_rows=7;parent.cost.elapsed_micros=42;
    let mut nested=DagStats {duckdb_regions:2,postgres_regions:3,other_sql_regions:4,boundary_operators:5,output_decode_operators:6,
        physical_plan:"nested".into(),logical_plan:"nested logical".into(),sql_queries:vec!["SELECT 1".into()],
        sql_boundaries:vec![SqlBoundary {operator:"Return".into(),engine:"duckdb".into(),category:"transport".into(),reason:"result".into()}],..Default::default()};
    nested.cost.sql_executions=3;nested.cost.result_rows=100;nested.cost.elapsed_micros=999;
    parent.merge_execution(&nested);parent.merge_execution(&nested);
    assert_eq!((parent.duckdb_regions,parent.postgres_regions,parent.other_sql_regions),(5,6,8));
    assert_eq!((parent.boundary_operators,parent.output_decode_operators),(10,12));
    assert_eq!(parent.cost.sql_executions,8);assert_eq!(parent.cost.result_rows,7);assert_eq!(parent.cost.elapsed_micros,42);
    assert_eq!(parent.physical_plan.matches("Executed subplan:").count(),1);
    assert_eq!(parent.logical_plan.matches("Executed subplan:").count(),2);
    assert_eq!(parent.sql_queries,vec!["SELECT 1","SELECT 1"]);assert_eq!(parent.sql_boundaries.len(),2);
    let physical=parent.physical_plan.clone();let logical=parent.logical_plan.clone();parent.merge_execution(&DagStats::default());
    assert_eq!(parent.physical_plan,physical);assert_eq!(parent.logical_plan,logical);
}

#[tokio::test]
async fn transport_literal_input_rewrites_keep_typed_batches_and_repeatable_streams() {
    use arrow::{array::{Int64Array, RecordBatch}, datatypes::{Field, Schema}};
    let schema = Arc::new(Schema::new(vec![Field::new("value", DataType::Int64, true)]));
    let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(vec![Some(7), None]))]).unwrap();
    let empty = RecordBatch::new_empty(schema.clone());
    let input: Arc<dyn ExecutionPlan> = Arc::new(engine_exec::InputBatches::new(schema.clone(), vec![batch.clone(), empty.clone()]));
    assert!(input.children().is_empty());
    let rewritten = input.clone().with_new_children(vec![]).unwrap();
    assert!(Arc::ptr_eq(&input, &rewritten));
    assert!(input.clone().with_new_children(vec![input.clone()]).unwrap_err().to_string().contains("has no dependencies"));
    assert_eq!(engine_exec::explain(&input), "LiteralInput\n");
    for _ in 0..2 {
        let batches = engine_exec::collect(rewritten.clone(), Arc::new(TaskContext::default())).await.unwrap();
        assert_eq!(batches, vec![batch.clone(), empty.clone()]);
    }
    let empty_input: Arc<dyn ExecutionPlan> = Arc::new(engine_exec::InputBatches::new(schema.clone(), vec![]));
    let stream = engine_exec::execute_stream(empty_input.clone(), Arc::new(TaskContext::default())).unwrap();
    assert_eq!(stream.schema(), schema);
    assert!(engine_exec::collect(empty_input, Arc::new(TaskContext::default())).await.unwrap().is_empty());
}

#[tokio::test]
async fn transport_stream_adapter_preserves_schema_and_delivers_errors_in_order() {
    use arrow::{array::{Int64Array, RecordBatch}, datatypes::{Field, Schema}};
    use futures::StreamExt;
    use engine_exec::RecordBatchStream;
    let schema = Arc::new(Schema::new(vec![Field::new("value", DataType::Int64, false)]));
    let batch = RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(vec![42]))]).unwrap();
    let source = futures::stream::iter(vec![Ok(batch.clone()), Err(DataFusionError::Execution("source disconnected".into()))]);
    let mut stream = engine_exec::RecordBatchStreamAdapter::new(schema.clone(), source);
    assert_eq!(stream.schema(), schema);
    assert_eq!(stream.next().await.unwrap().unwrap(), batch);
    let error = stream.next().await.unwrap().unwrap_err();
    assert!(matches!(error, DataFusionError::Execution(ref message) if message == "source disconnected"));
    assert!(stream.next().await.is_none());
}

#[test]
fn transport_memory_reservations_keep_failed_growth_atomic_and_release_on_drop() {
    use engine_exec::{GreedyMemoryPool, MemoryPool, MemoryConsumer};
    let pool: Arc<dyn MemoryPool> = Arc::new(GreedyMemoryPool::new(8));
    let context = TaskContext::default().with_batch_size(0).with_memory_pool(pool.clone());
    assert_eq!(context.batch_size(), 1);
    assert!(Arc::ptr_eq(context.memory_pool(), &pool));
    {
        let first = MemoryConsumer::new("first").register(&pool);
        let second = MemoryConsumer::new("second").register(&pool);
        first.try_grow(5).unwrap(); second.try_grow(2).unwrap();
        assert!(matches!(first.try_grow(2), Err(DataFusionError::ResourcesExhausted(_))));
        assert_eq!((first.size(), second.size(), pool.reserved()), (5, 2, 7));
        first.try_resize(4).unwrap(); second.try_resize(0).unwrap();
        assert_eq!(pool.reserved(), 4);
        first.free(); first.free();
        assert_eq!(pool.reserved(), 0);
        first.try_resize(8).unwrap();
        assert_eq!(pool.reserved(), 8);
    }
    assert_eq!(pool.reserved(), 0);
    let pool: Arc<dyn MemoryPool> = Arc::new(GreedyMemoryPool::new(usize::MAX));
    let reservation = MemoryConsumer::new("accounting overflow").register(&pool);
    reservation.try_grow(usize::MAX).unwrap();
    assert!(reservation.try_grow(1).is_err());
    assert_eq!(reservation.size(), usize::MAX);
    drop(reservation);
    assert_eq!(pool.reserved(), 0);
}
