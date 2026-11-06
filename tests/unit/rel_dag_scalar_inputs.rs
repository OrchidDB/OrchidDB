//! Native input evaluation must precede SQL placement on every execution path.
use super::*;
use crate::ir::{GraphPlan, GraphPlanPolicy, PropertyGraph};
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

async fn boundary_rows(exec: Arc<dyn ExecutionPlan>) -> Vec<Vec<ScalarValue>> {
    use futures::TryStreamExt;
    let batches = exec
        .execute(0, Arc::new(TaskContext::default()))
        .unwrap()
        .try_collect::<Vec<_>>()
        .await
        .unwrap();
    batches
        .iter()
        .flat_map(|batch| {
            (0..batch.num_rows()).map(move |row| {
                batch
                    .columns()
                    .iter()
                    .map(|column| ScalarValue::try_from_array(column, row).unwrap())
                    .collect()
            })
        })
        .collect()
}

fn physical_boundary(
    prepared: sql::PreparedSql,
    resources: &DagSession,
    source_tables: Vec<SourceTable>,
    inputs: Vec<Arc<dyn ExecutionPlan>>,
    input_tables: Vec<sql::TableData>,
) -> Arc<DuckDbExec> {
    Arc::new(DuckDbExec {
        properties: Arc::new(PlanProperties::new(prepared.output_schema())),
        prepared: Arc::new(prepared),
        inputs,
        input_tables,
        source_tables,
        region_session: resources.region_session.clone(),
        executor: resources.executor.clone(),
        cost: Default::default(),
    })
}

#[tokio::test]
async fn aliased_empty_relations_preserve_schema_and_unit_row_cardinality() {
    use arrow::datatypes::{Field, Schema};
    for fields in [
        vec![],
        vec![
            Field::new("n", DataType::Int64, true),
            Field::new("s", DataType::Utf8, true),
        ],
    ] {
        let schema = Arc::new(Schema::new(fields));
        for one_row in [false, true] {
            let empty = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
                produce_one_row: one_row,
                schema: Arc::new(
                    datafusion::common::DFSchema::try_from(schema.as_ref().clone()).unwrap(),
                ),
            });
            let aliased = LogicalPlanBuilder::from(empty)
                .alias("outer_scope")
                .unwrap()
                .alias("renamed_scope")
                .unwrap()
                .build()
                .unwrap();
            let exec =
                compile_execution(&aliased, &[], &resources(0), &Default::default()).unwrap();
            assert_eq!(exec.schema().as_ref(), schema.as_ref());
            validate_sql_required_physical(&exec).unwrap();
            let rows = boundary_rows(exec).await;
            assert_eq!(rows.len(), usize::from(one_row));
            if one_row {
                assert_eq!(rows[0].len(), schema.fields().len());
                assert!(rows[0].iter().all(ScalarValue::is_null));
            }
        }
    }
}

#[test]
fn execution_compilation_rejects_relational_operators_without_sql_lowering() {
    let plan = LogicalPlanBuilder::empty(true)
        .project(vec![lit(7_i64).alias("value")])
        .unwrap()
        .build()
        .unwrap();
    let error = compile_execution(&plan, &[], &resources(0), &Default::default())
        .unwrap_err()
        .to_string();
    assert!(error.contains("requires SQL lowering"), "{error}");
}

#[tokio::test]
async fn sql_boundaries_preserve_duplicate_null_rows_and_reject_extra_partitions() {
    use arrow::datatypes::{Field, Schema};
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        true,
    )]));
    for mode in [0, 2] {
        let resource = resources(mode);
        let prepared=sql::PreparedSql::query_only("SELECT value FROM (VALUES (CAST(NULL AS BIGINT)), (7), (7)) t(value) ORDER BY value NULLS LAST".into(),schema.clone());
        let exec = physical_boundary(prepared, &resource, vec![], vec![], vec![]);
        assert_eq!(
            exec.execute(1, Arc::new(TaskContext::default()))
                .err()
                .unwrap()
                .to_string(),
            "Execution error: DuckDB region has one partition"
        );
        assert_eq!(
            boundary_rows(exec).await,
            vec![
                vec![ScalarValue::Int64(Some(7))],
                vec![ScalarValue::Int64(Some(7))],
                vec![ScalarValue::Int64(None)]
            ]
        );
    }
}

#[tokio::test]
async fn source_tables_stream_across_sessions_and_replay_for_joins_and_local_inputs() {
    use arrow::{
        array::Int64Array,
        datatypes::{Field, Schema},
    };
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value x",
        DataType::Int64,
        true,
    )]));
    for mode in 0..5 {
        let source = Arc::new(Mutex::new(sql::DuckDbExecutor::default()));
        source.lock().unwrap().execute_batch("CREATE TABLE \"source a\" (\"value x\" BIGINT); INSERT INTO \"source a\" VALUES (7),(NULL),(7); CREATE TABLE \"source b\" (\"value x\" BIGINT); INSERT INTO \"source b\" VALUES (7),(8)").unwrap();
        source.lock().unwrap().execute_batch("CREATE SCHEMA \"scope space\"; CREATE TABLE \"scope space\".\"source a\" AS SELECT * FROM \"source a\"").unwrap();
        let target = Arc::new(Mutex::new(sql::DuckDbExecutor::default()));
        let mut resource = DagSession::with_shared(source, Default::default());
        resource.region_session = Some(Arc::new(Mutex::new(Box::new(sql::DuckDbSession {
            executor: target,
        }))));
        let mut sources = vec![SourceTable {
            name: "source a".into(),
            input_name: "source a".into(),
            schema: schema.clone(),
            uses: if mode == 1 { 2 } else { 1 },
        }];
        let mut inputs = vec![];
        let mut tables = vec![];
        let query = match mode {
            0 => "SELECT \"value x\" FROM \"source a\" ORDER BY \"value x\" NULLS LAST",
            1 => {
                "SELECT a.\"value x\" FROM \"source a\" a JOIN \"source a\" b ON a.\"value x\" IS NOT DISTINCT FROM b.\"value x\" ORDER BY a.\"value x\" NULLS LAST"
            }
            2 => {
                sources.push(SourceTable {
                    name: "source b".into(),
                    input_name: "source b".into(),
                    schema: schema.clone(),
                    uses: 1,
                });
                "SELECT a.\"value x\" FROM \"source a\" a JOIN \"source b\" b ON a.\"value x\"=b.\"value x\" ORDER BY a.\"value x\""
            }
            4 => {
                sources[0].name = "\"scope space\".\"source a\"".into();
                sources[0].input_name = "qualified input".into();
                "SELECT \"value x\" FROM \"qualified input\" ORDER BY \"value x\" NULLS LAST"
            }
            _ => {
                let batch =
                    RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(vec![1]))])
                        .unwrap();
                inputs.push(
                    Arc::new(engine_exec::InputBatches::new(schema.clone(), vec![batch]))
                        as Arc<dyn ExecutionPlan>,
                );
                tables.push(sql::TableData {
                    name: "local input".into(),
                    schema: schema.clone(),
                    batches: vec![],
                });
                "SELECT a.\"value x\" FROM \"source a\" a CROSS JOIN \"local input\" ORDER BY a.\"value x\" NULLS LAST"
            }
        };
        let exec = physical_boundary(
            sql::PreparedSql::query_only(query.into(), schema.clone()),
            &resource,
            sources,
            inputs,
            tables,
        );
        let expected = match mode {
            1 => vec![Some(7), Some(7), Some(7), Some(7), None],
            2 => vec![Some(7), Some(7)],
            _ => vec![Some(7), Some(7), None],
        };
        assert_eq!(
            boundary_rows(exec).await,
            expected
                .into_iter()
                .map(|v| vec![ScalarValue::Int64(v)])
                .collect::<Vec<_>>(),
            "mode={mode}"
        );
    }
}

#[tokio::test]
async fn caller_sessions_accept_typed_batches_and_reject_setup_statements() {
    use arrow::{
        array::Int64Array,
        datatypes::{Field, Schema},
    };
    use futures::TryStreamExt;
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value",
        DataType::Int64,
        true,
    )]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![Arc::new(Int64Array::from(vec![Some(8), None, Some(8)]))],
    )
    .unwrap();
    let resource = resources(2);
    let mut prepared = sql::PreparedSql::query_only(
        "SELECT value FROM incoming ORDER BY value NULLS LAST".into(),
        schema.clone(),
    );
    prepared.tables.push(sql::TableData {
        name: "incoming".into(),
        schema: schema.clone(),
        batches: vec![batch],
    });
    assert_eq!(
        boundary_rows(physical_boundary(
            prepared,
            &resource,
            vec![],
            vec![],
            vec![]
        ))
        .await,
        vec![
            vec![ScalarValue::Int64(Some(8))],
            vec![ScalarValue::Int64(Some(8))],
            vec![ScalarValue::Int64(None)]
        ]
    );
    let mut prepared = sql::PreparedSql::query_only("SELECT 1 AS value".into(), schema);
    prepared
        .setup
        .push("CREATE TABLE unwanted(value BIGINT)".into());
    let stream = physical_boundary(prepared, &resource, vec![], vec![], vec![])
        .execute(0, Arc::new(TaskContext::default()))
        .unwrap();
    let error = stream
        .try_collect::<Vec<_>>()
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Caller SQL sessions require typed input relations"),
        "{error}"
    );
    let executor = resource.executor.clone();
    let count = executor
        .lock()
        .unwrap()
        .connection()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM information_schema.tables WHERE table_name='unwanted'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn dependent_sql_operations_use_default_and_supplied_sessions_and_preserve_empty_inputs() {
    use arrow::datatypes::{Field, Schema};
    let schema = Arc::new(
        datafusion::common::DFSchema::try_from(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            true,
        )]))
        .unwrap(),
    );
    for mode in [0, 2] {
        for one_row in [false, true] {
            let resource = resources(mode);
            let plan = super::super::dependent::DependentOperation {
                source: Arc::new(LogicalPlanBuilder::empty(one_row).build().unwrap()),
                template: sql::lowering::SqlTemplate {
                    sql: "SELECT CAST(7 AS BIGINT) AS value".into(),
                    parameters: 0,
                    dialect: "duckdb".into(),
                },
                schema: schema.clone(),
            }
            .into_plan();
            let exec = compile_execution(&plan, &[], &resource, &Default::default()).unwrap();
            validate_sql_required_physical(&exec).unwrap();
            assert!(exec.execute(1, Arc::new(TaskContext::default())).is_err());
            let expected = if one_row {
                vec![vec![ScalarValue::Int64(Some(7))]]
            } else {
                vec![]
            };
            assert_eq!(
                boundary_rows(exec).await,
                expected,
                "mode={mode}, one_row={one_row}"
            );
        }
    }
}

#[derive(Debug)]
struct UnapprovedOperator(Arc<dyn ExecutionPlan>);
impl ExecutionPlan for UnapprovedOperator {
    fn name(&self) -> &str {
        "InputBatches"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn properties(&self) -> &Arc<PlanProperties> {
        self.0.properties()
    }
    fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
        vec![&self.0]
    }
    fn with_new_children(
        self: Arc<Self>,
        children: Vec<Arc<dyn ExecutionPlan>>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        if children.len() != 1 {
            return Err(DataFusionError::Plan("one child required".into()));
        }
        Ok(Arc::new(Self(children[0].clone())))
    }
    fn execute(&self, _: usize, _: Arc<TaskContext>) -> Result<SendableRecordBatchStream> {
        panic!("physical-plan validation must not execute an unapproved operator")
    }
}

#[test]
fn physical_sql_audit_checks_nested_operator_identity_instead_of_trusting_names() {
    let schema = Arc::new(arrow::datatypes::Schema::empty());
    let input =
        Arc::new(engine_exec::InputBatches::new(schema.clone(), vec![])) as Arc<dyn ExecutionPlan>;
    validate_sql_required_physical(&input).unwrap();
    let unapproved = Arc::new(UnapprovedOperator(input)) as Arc<dyn ExecutionPlan>;
    let prepared = sql::PreparedSql::query_only("SELECT 1".into(), schema);
    let nested = physical_boundary(
        prepared,
        &resources(0),
        vec![],
        vec![unapproved.clone()],
        vec![],
    ) as Arc<dyn ExecutionPlan>;
    for plan in [unapproved, nested] {
        let error = validate_sql_required_physical(&plan)
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("rejected executable operator InputBatches"),
            "{error}"
        );
    }
}

#[tokio::test]
async fn poisoned_sql_resources_return_errors_instead_of_panicking_or_executing() {
    use futures::TryStreamExt;
    let resource = resources(0);
    let executor = resource.executor.clone();
    assert!(
        std::thread::spawn(move || {
            let _guard = executor.lock().unwrap();
            panic!("simulate a failed SQL owner");
        })
        .join()
        .is_err()
    );
    assert!(
        resource
            .rule_context()
            .unwrap_err()
            .to_string()
            .contains("SQL session poisoned")
    );
    let resource = resources(2);
    let session = resource.region_session.clone().unwrap();
    assert!(
        std::thread::spawn(move || {
            let _guard = session.lock().unwrap();
            panic!("simulate a failed caller session");
        })
        .join()
        .is_err()
    );
    let schema = Arc::new(arrow::datatypes::Schema::new(vec![
        arrow::datatypes::Field::new("value", DataType::Int64, true),
    ]));
    let prepared = sql::PreparedSql::query_only("SELECT CAST(7 AS BIGINT) AS value".into(), schema);
    let exec = physical_boundary(prepared, &resource, vec![], vec![], vec![]);
    let error = exec
        .execute(0, Arc::new(TaskContext::default()))
        .unwrap()
        .try_collect::<Vec<_>>()
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("SQL session poisoned"), "{error}");
}

#[tokio::test]
async fn qualified_external_sources_execute_through_the_planned_caller_session() {
    use arrow::datatypes::{Field, Schema};
    use datafusion::common::TableReference;
    for reference in [
        TableReference::partial("scope space", "source a"),
        TableReference::partial("other", "source a"),
        TableReference::bare("literal.dot"),
        TableReference::bare("embedded\"quote"),
    ] {
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value x",
            DataType::Int64,
            true,
        )]));
        let source = Arc::new(Mutex::new(sql::DuckDbExecutor::default()));
        source
            .lock()
            .unwrap()
            .execute_batch("CREATE SCHEMA \"scope space\"; CREATE SCHEMA other")
            .unwrap();
        let name = reference.to_quoted_string();
        source
            .lock()
            .unwrap()
            .execute_batch(&format!(
                "CREATE TABLE {name} (\"value x\" BIGINT); INSERT INTO {name} VALUES (7),(NULL),(7)"
            ))
            .unwrap();
        let mut resource = DagSession::with_shared(source, [reference.to_string()].into());
        resource.region_session = Some(Arc::new(Mutex::new(Box::new(sql::DuckDbSession {
            executor: Arc::new(Mutex::new(sql::DuckDbExecutor::default())),
        }))));
        let plan = LogicalPlanBuilder::scan(
            reference.clone(),
            datafusion::datasource::provider_as_source(Arc::new(
                datafusion::datasource::empty::EmptyTable::new(schema),
            )),
            None,
        )
        .unwrap()
        .build()
        .unwrap();
        let (result, stats) = execute_with_extensions(lowered(plan), vec![], None, Some(&resource))
            .await
            .unwrap_or_else(|error| panic!("{reference:?}: {error}"));
        assert_eq!(stats.duckdb_regions, 1);
        let mut rows = (0..result.batch.num_rows())
            .map(|row| ScalarValue::try_from_array(result.batch.column(0), row).unwrap())
            .collect::<Vec<_>>();
        rows.sort_by_key(|value| match value {
            ScalarValue::Int64(value) => *value,
            _ => panic!(),
        });
        assert_eq!(
            rows,
            vec![
                ScalarValue::Int64(None),
                ScalarValue::Int64(Some(7)),
                ScalarValue::Int64(Some(7))
            ]
        );
    }
}

#[test]
fn external_source_bindings_preserve_scan_contract_and_local_table_ownership() {
    use datafusion::common::{Column, TableReference};
    use datafusion::logical_expr::TableScan;
    let original = scan(vec![ScalarValue::Int64(Some(7))]).build().unwrap();
    let LogicalPlan::TableScan(original) = original else {
        panic!()
    };
    let name = TableReference::partial("scope", "table");
    let predicate = Expr::Column(Column::new(Some(name.clone()), "column1")).gt(lit(3_i64));
    let plan = LogicalPlan::TableScan(
        TableScan::try_new(
            name.clone(),
            original.source,
            Some(vec![0]),
            vec![predicate],
            Some(2),
        )
        .unwrap(),
    );
    let external: std::collections::BTreeSet<String> =
        [name.to_string(), "__orchid_source_input_0".into()].into();
    let mut reserved = external.clone();
    let (bound, sources) =
        exchange::bind_sources(plan.clone(), &external, &mut reserved, true).unwrap();
    assert_eq!(bound.schema(), plan.schema());
    assert_eq!(sources.len(), 1);
    assert_eq!(sources[0].name, name.to_quoted_string());
    assert_eq!(sources[0].input_name, "__orchid_source_input_1");
    assert_eq!(sources[0].uses, 1);
    let LogicalPlan::Projection(projection) = bound else {
        panic!()
    };
    let LogicalPlan::TableScan(scan) = projection.input.as_ref() else {
        panic!()
    };
    assert_eq!(scan.fetch, Some(2));
    assert_eq!(scan.projection, Some(vec![0]));
    assert_eq!(
        scan.filters,
        vec![Expr::Column(Column::new(Some(scan.table_name.clone()), "column1")).gt(lit(3_i64))]
    );
    let (unbound, _) =
        exchange::bind_sources(plan.clone(), &external, &mut reserved, false).unwrap();
    assert_eq!(unbound, plan);
    let local = super::scalar_inputs::scan(vec![ScalarValue::Int64(Some(9))])
        .build()
        .unwrap();
    let (unchanged, sources) =
        exchange::bind_sources(local.clone(), &external, &mut reserved, true).unwrap();
    assert_eq!(unchanged, local);
    assert!(sources.is_empty());
    assert!(
        !reserved.contains("documents"),
        "local data must still be materialized"
    );
}

#[tokio::test]
async fn caller_source_bindings_keep_same_named_schemas_and_repeated_scans_distinct() {
    use arrow::datatypes::{Field, Schema};
    use datafusion::common::{Column, TableReference};
    let schema = Arc::new(Schema::new(vec![Field::new(
        "value x",
        DataType::Int64,
        true,
    )]));
    for repeat in [false, true] {
        let source = Arc::new(Mutex::new(sql::DuckDbExecutor::default()));
        source.lock().unwrap().execute_batch("CREATE SCHEMA first; CREATE SCHEMA second; CREATE TABLE first.items (\"value x\" BIGINT); INSERT INTO first.items VALUES (7),(NULL),(7); CREATE TABLE second.items (\"value x\" BIGINT); INSERT INTO second.items VALUES (7),(8)").unwrap();
        let left_name = TableReference::partial("first", "items");
        let right_name = if repeat {
            left_name.clone()
        } else {
            TableReference::partial("second", "items")
        };
        let mut resource = DagSession::with_shared(
            source,
            [
                left_name.to_string(),
                right_name.to_string(),
                "__orchid_source_input_0".into(),
            ]
            .into(),
        );
        resource.region_session = Some(Arc::new(Mutex::new(Box::new(sql::DuckDbSession {
            executor: Arc::new(Mutex::new(sql::DuckDbExecutor::default())),
        }))));
        let source_plan = |name, alias| {
            LogicalPlanBuilder::scan(
                name,
                datafusion::datasource::provider_as_source(Arc::new(
                    datafusion::datasource::empty::EmptyTable::new(schema.clone()),
                )),
                None,
            )
            .unwrap()
            .alias(alias)
            .unwrap()
            .build()
            .unwrap()
        };
        let left = source_plan(left_name, "left side");
        let right = source_plan(right_name, "right side");
        let left_column = Expr::Column(Column::new(
            Some(TableReference::bare("left side")),
            "value x",
        ));
        let right_column = Expr::Column(Column::new(
            Some(TableReference::bare("right side")),
            "value x",
        ));
        let plan = LogicalPlanBuilder::from(left)
            .join_on(
                right,
                datafusion::logical_expr::JoinType::Inner,
                [left_column.clone().eq(right_column)],
            )
            .unwrap()
            .project(vec![left_column.alias("value")])
            .unwrap()
            .build()
            .unwrap();
        let (result, stats) = execute_with_extensions(lowered(plan), vec![], None, Some(&resource))
            .await
            .unwrap();
        assert_eq!(stats.duckdb_regions, 1);
        assert_eq!(result.batch.num_rows(), if repeat { 4 } else { 2 });
        for row in 0..result.batch.num_rows() {
            assert_eq!(
                ScalarValue::try_from_array(result.batch.column(0), row).unwrap(),
                ScalarValue::Int64(Some(7))
            );
        }
    }
}

fn exchange_dialects() -> [sql::SqlDialect; 3] {
    [
        sql::SqlDialect::DuckDb,
        sql::SqlDialect::Postgres,
        sql::SqlDialect::Custom(&sql::trino::TRINO),
    ]
}

fn local_exchange_identity(kind: DataType) -> Arc<datafusion::logical_expr::ScalarUDF> {
    Arc::new(create_udf(
        "local_exchange_identity",
        vec![kind.clone()],
        kind,
        Volatility::Volatile,
        Arc::new(|args| Ok(args[0].clone())),
    ))
}

#[test]
fn exchange_keeps_ordered_native_inputs_outside_unordered_transport() {
    use datafusion::logical_expr::Sort;
    let source = scan(vec![
        ScalarValue::Int64(Some(7)),
        ScalarValue::Int64(Some(2)),
    ])
    .build()
    .unwrap();
    let identity = local_exchange_identity(DataType::Int64);
    for fetch in [None, Some(1)] {
        let ordered = LogicalPlan::Sort(Sort {
            expr: vec![identity.call(vec![col("column1")]).sort(true, true)],
            input: Arc::new(source.clone()),
            fetch,
        });
        let parent = LogicalPlanBuilder::from(ordered)
            .project(vec![col("column1")])
            .unwrap()
            .build()
            .unwrap();
        for dialect in exchange_dialects() {
            let mut eligibility = SqlEligibility {
                dialect: Some(dialect),
                ..Default::default()
            };
            assert!(eligibility.visit(&parent).is_some());
            let mut next = 0;
            assert!(
                exchange::cut_inputs(&parent, &mut eligibility, &mut next, &Default::default())
                    .unwrap()
                    .is_none(),
                "{dialect:?}, fetch={fetch:?}"
            );
            assert_eq!(next, 0, "reject before allocating a transport name");
        }
    }
}

#[test]
fn exchange_cardinality_carriers_preserve_empty_schemas_and_avoid_external_names() {
    let empty = LogicalPlanBuilder::empty(false).build().unwrap();
    let parent = LogicalPlanBuilder::from(empty.clone())
        .project(vec![lit(7_i64).alias("value")])
        .unwrap()
        .build()
        .unwrap();
    let external = [
        "__orchid_runtime_input_0".into(),
        "__orchid_runtime_input_1".into(),
    ]
    .into();
    for dialect in exchange_dialects() {
        let mut eligibility = SqlEligibility {
            dialect: Some(dialect),
            ..Default::default()
        };
        let mut next = 0;
        let (rewritten, bindings) =
            exchange::cut_inputs(&parent, &mut eligibility, &mut next, &external)
                .unwrap()
                .unwrap();
        assert_eq!(rewritten.schema(), parent.schema());
        assert_eq!(next, 3);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].0, empty);
        let table = &bindings[0].1;
        assert_eq!(table.name, "__orchid_runtime_input_2");
        assert_eq!(table.schema.fields().len(), 1);
        assert_eq!(table.schema.field(0).name(), "__orchid_cardinality");
        assert_eq!(table.schema.field(0).data_type(), &DataType::Int64);
        assert!(!table.schema.field(0).is_nullable());
        assert!(table.batches.is_empty());
        assert!(rewritten.inputs()[0].schema().fields().is_empty());
    }
}

#[test]
fn exchange_rejects_duplicate_transport_field_names_without_losing_qualifiers() {
    use datafusion::common::{Column, TableReference};
    let identity = local_exchange_identity(DataType::Int64);
    let native = scan(vec![ScalarValue::Int64(Some(7))])
        .project(vec![
            identity
                .call(vec![col("column1")])
                .alias_qualified(Some(TableReference::bare("left")), "value"),
            identity
                .call(vec![col("column1")])
                .alias_qualified(Some(TableReference::bare("right")), "value"),
        ])
        .unwrap()
        .build()
        .unwrap();
    let parent = LogicalPlanBuilder::from(native)
        .project(vec![Expr::Column(Column::new(
            Some(TableReference::bare("left")),
            "value",
        ))])
        .unwrap()
        .build()
        .unwrap();
    for dialect in exchange_dialects() {
        let mut eligibility = SqlEligibility {
            dialect: Some(dialect),
            ..Default::default()
        };
        assert!(
            exchange::cut_inputs(&parent, &mut eligibility, &mut 0, &Default::default())
                .unwrap()
                .is_none(),
            "{dialect:?}"
        );
    }
}

#[test]
fn exchange_unnest_preserves_typed_list_dependencies_and_output_schema() {
    let kind = DataType::List(Arc::new(arrow::datatypes::Field::new(
        "item",
        DataType::Int64,
        true,
    )));
    let identity = local_exchange_identity(kind);
    let list = ScalarValue::List(ScalarValue::new_list(
        &[ScalarValue::Int64(Some(7)), ScalarValue::Int64(None)],
        &DataType::Int64,
        true,
    ));
    let native = scan(vec![list])
        .project(vec![identity.call(vec![col("column1")]).alias("items")])
        .unwrap()
        .build()
        .unwrap();
    let plan = LogicalPlanBuilder::from(native.clone())
        .unnest_column("items")
        .unwrap()
        .build()
        .unwrap();
    let LogicalPlan::Unnest(original) = &plan else {
        panic!()
    };
    for dialect in exchange_dialects() {
        let mut eligibility = SqlEligibility {
            dialect: Some(dialect),
            ..Default::default()
        };
        let (rewritten, bindings) =
            exchange::cut_inputs(&plan, &mut eligibility, &mut 0, &Default::default())
                .unwrap()
                .unwrap();
        let LogicalPlan::Unnest(updated) = &rewritten else {
            panic!()
        };
        assert_eq!(updated.exec_columns, original.exec_columns);
        assert_eq!(updated.schema, original.schema);
        assert_eq!(updated.input.schema(), original.input.schema());
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].0, native);
        assert_eq!(bindings[0].1.schema.as_ref(), native.schema().as_arrow());
    }
}

#[tokio::test]
async fn zero_column_empty_inputs_remain_empty_after_sql_cardinality_transport() {
    for mode in [0, 2] {
        let resource = resources(mode);
        for filter in [true, false] {
            let plan = LogicalPlanBuilder::empty(false)
                .filter(lit(filter))
                .unwrap()
                .project(vec![lit(7_i64).alias("value")])
                .unwrap()
                .build()
                .unwrap();
            let (result, _) = execute_with_extensions(lowered(plan), vec![], None, Some(&resource))
                .await
                .unwrap();
            assert_eq!(result.batch.num_rows(), 0);
            assert_eq!(result.batch.num_columns(), 1);
            assert_eq!(result.batch.column(0).data_type(), &DataType::Int64);
        }
    }
}

#[tokio::test]
async fn zero_column_batches_retain_every_row_across_sql_input_transport() {
    use arrow::datatypes::{Field, Schema};
    for mode in [0, 2] {
        for count in [0, 1, 3, 64] {
            let resource = resources(mode);
            let empty = Arc::new(Schema::empty());
            let batch = RecordBatch::try_new_with_options(
                empty.clone(),
                vec![],
                &arrow::array::RecordBatchOptions::new().with_row_count(Some(count)),
            )
            .unwrap();
            let carrier = Arc::new(Schema::new(vec![Field::new(
                "__orchid_cardinality",
                DataType::Int64,
                false,
            )]));
            let table = sql::TableData {
                name: "empty schema input".into(),
                schema: carrier.clone(),
                batches: vec![],
            };
            let exec = physical_boundary(
                sql::PreparedSql::query_only(
                    "SELECT __orchid_cardinality FROM \"empty schema input\"".into(),
                    carrier,
                ),
                &resource,
                vec![],
                vec![Arc::new(engine_exec::InputBatches::new(empty, vec![batch]))],
                vec![table],
            );
            assert_eq!(
                boundary_rows(exec).await,
                vec![vec![ScalarValue::Int64(Some(1))]; count],
                "mode={mode}, cardinality={count}"
            );
        }
    }
}

#[tokio::test]
async fn source_only_dialects_are_rejected_before_caller_execution() {
    #[derive(Debug)]
    struct PostgresSource;
    impl sql::region::RegionSession for PostgresSource {
        fn dialect(&self) -> sql::SqlDialect {
            sql::SqlDialect::Postgres
        }
        fn query(
            &mut self,
            _: &str,
            _: arrow::datatypes::SchemaRef,
        ) -> sql::SqlResult<RecordBatch> {
            panic!("source-only session must not execute completion SQL")
        }
    }
    let mut resource = resources(0);
    resource.region_session = Some(Arc::new(Mutex::new(Box::new(PostgresSource))));
    let plan = LogicalPlanBuilder::empty(true)
        .project(vec![lit(7_i64).alias("value")])
        .unwrap()
        .build()
        .unwrap();
    let error = execute_with_extensions(lowered(plan), vec![], None, Some(&resource))
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("source dialect, not an executable engine"),
        "{error}"
    );
}

#[test]
fn sql_array_coercion_ignores_values_outside_sliced_nested_arrays() {
    use arrow::array::{Array,ArrayRef,ListArray,LargeListArray,FixedSizeListArray};
    use arrow::datatypes::{Field,Int64Type};
    let rows=vec![Some(vec![Some(128),Some(129)]),Some(vec![Some(7),None]),Some(vec![Some(-129),Some(-130)])];
    let item=Arc::new(Field::new("item",DataType::Int8,true));
    let targets=[DataType::List(item.clone()),DataType::LargeList(item.clone()),DataType::FixedSizeList(item,2)];
    for array in [
        Arc::new(ListArray::from_iter_primitive::<Int64Type,_,_>(rows.clone())) as ArrayRef,
        Arc::new(LargeListArray::from_iter_primitive::<Int64Type,_,_>(rows.clone())) as ArrayRef,
        Arc::new(FixedSizeListArray::from_iter_primitive::<Int64Type,_,_>(rows,2)) as ArrayRef,
    ] {
        for target in &targets {
            for (offset,length) in [(0,0),(1,1),(3,0)] {
                let sliced=array.slice(offset,length);
                let result=coerce_sql_array(&sliced,target).expect("hidden overflow must not invalidate visible values");
                assert_eq!(result.len(),length);assert_eq!(result.data_type(),target);
                if length==0 {continue;}
                let value=ScalarValue::try_from_array(&result,0).unwrap();
                let children=match value {ScalarValue::List(a)=>a.value(0),ScalarValue::LargeList(a)=>a.value(0),ScalarValue::FixedSizeList(a)=>a.value(0),_=>panic!()};
                assert_eq!(ScalarValue::try_from_array(&children,0).unwrap(),ScalarValue::Int8(Some(7)));
                assert!(children.is_null(1));
            }
            assert!(coerce_sql_array(&array,target).is_err(),"visible overflow must still fail");
        }
    }
}

#[test]
fn sql_array_coercion_recursively_restricts_nested_child_slices() {
    use arrow::datatypes::Field;
    let nested=|values:Vec<ScalarValue>,kind:&DataType| {
        let inner=ScalarValue::List(ScalarValue::new_list_nullable(&values,kind));
        ScalarValue::List(ScalarValue::new_list_nullable(&[inner.clone()],&inner.data_type()))
    };
    let good=nested(vec![ScalarValue::Int64(Some(7)),ScalarValue::Int64(None)],&DataType::Int64);
    let bad=nested(vec![ScalarValue::Int64(Some(128))],&DataType::Int64);
    let source=ScalarValue::iter_to_array(vec![bad.clone(),good,bad]).unwrap();
    let item=Arc::new(Field::new("item",DataType::Int8,true));
    let expected=nested(vec![ScalarValue::Int8(Some(7)),ScalarValue::Int8(None)],&DataType::Int8);
    for inner in [DataType::List(item.clone()),DataType::LargeList(item.clone()),DataType::FixedSizeList(item,2)] {
        let field=Arc::new(Field::new("item",inner,true));
        for target in [DataType::List(field.clone()),DataType::LargeList(field.clone()),DataType::FixedSizeList(field,1)] {
            let result=coerce_sql_array(&source.slice(1,1),&target).unwrap();
            assert_eq!(ScalarValue::try_from_array(&result,0).unwrap(),expected.cast_to(&target).unwrap());
            assert!(coerce_sql_array(&source,&target).is_err());
        }
    }
}

#[test]
fn sql_array_coercion_retains_list_layout_limits_and_zero_width_parent_rows() {
    use arrow::array::{Array,ArrayRef,FixedSizeListArray,LargeListArray,NullArray};
    use arrow::buffer::{OffsetBuffer,ScalarBuffer};
    use arrow::datatypes::{Field,Int64Type};
    let fixed=Arc::new(FixedSizeListArray::from_iter_primitive::<Int64Type,_,_>(
        vec![Some(vec![Some(1),Some(2)])],2)) as ArrayRef;
    let target=DataType::FixedSizeList(Arc::new(Field::new("item",DataType::Int8,true)),3);
    assert!(coerce_sql_array(&fixed,&target).is_err(),"layout changes cannot invent a third child");
    // NullArray carries only a length, so this checks offset bounds without
    // allocating a multi-gigabyte value buffer.
    let length=i32::MAX as usize+1;
    let field=Arc::new(Field::new("item",DataType::Null,true));
    let large=Arc::new(LargeListArray::new(field.clone(),OffsetBuffer::new(ScalarBuffer::from(vec![0_i64,length as i64])),
        Arc::new(NullArray::new(length)),None)) as ArrayRef;
    assert!(coerce_sql_array(&large,&DataType::List(field)).is_err(),"32-bit offsets must not wrap");
    let empty=Arc::new(FixedSizeListArray::from_iter_primitive::<Int64Type,_,_>(
        vec![Some(vec![]),None,Some(vec![])],0)) as ArrayRef;
    assert_eq!(empty.len(),3);
    let item=Arc::new(Field::new("item",DataType::Int8,true));
    for target in [DataType::List(item.clone()),DataType::LargeList(item.clone()),DataType::FixedSizeList(item,0)] {
        let result=coerce_sql_array(&empty,&target).unwrap();
        assert_eq!(result.len(),3);assert_eq!(result.null_count(),1);
        assert!(!result.is_null(0));assert!(result.is_null(1));assert!(!result.is_null(2));
    }
}

#[derive(Debug)]
struct SessionBoundaryObserver {
    calls: Arc<AtomicUsize>,
    reject: bool,
}
impl crate::ir::rel::runtime::policy::BoundaryExecutable for SessionBoundaryObserver {
    fn execute(&self, boundary: &crate::ir::rel::runtime::CompiledKernel,
        inputs: Vec<Vec<crate::ir::runtime::Row>>, state: &mut crate::ir::rel::runtime::KernelState,
    ) -> Result<Vec<crate::ir::runtime::Row>, crate::ir::diagnostics::QueryExecutionError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        if self.reject { return Err("session boundary rejected".into()); }
        boundary.invoke_default(inputs,state)
    }
}

#[test]
fn session_boundary_overrides_survive_recreation_without_leaking_between_sessions() {
    use crate::ir::rel::runtime::{CompileOptions,KernelState,CompiledKernel,compile_for_host};
    use crate::ir::rel::runtime::policy::{BoundaryKind,ExecutableSet};
    let graph=PropertyGraph::new();
    let graph_plan=GraphPlan::new(GraphPlanPolicy::cypher(),crate::ir::plan::Node::GraphOneRow);
    let compiled=compile_for_host(&graph_plan,&graph,CompileOptions{sql_islands:false,..Default::default()}).unwrap();
    let kernel=CompiledKernel::from_plan(&compiled).expect("literal input boundary");
    assert_eq!(kernel.boundary_kind(),Some(BoundaryKind::LiteralInput));
    let calls=Arc::new(AtomicUsize::new(0));
    let mut original=DagSession::new(None);
    original.set_boundary_executables(ExecutableSet::default().with(BoundaryKind::LiteralInput,
        Arc::new(SessionBoundaryObserver{calls:calls.clone(),reject:false})));
    let recreated=original.recreated(None);
    let isolated=DagSession::new(None);
    for (session,expected_calls) in [(&original,1),(&recreated,2),(&isolated,2)] {
        let mut state=KernelState::default(); state.frontier=vec![crate::ir::runtime::Row::new()]; state.context.rule_context=session.rule_context().unwrap();
        assert_eq!(kernel.invoke(vec![],&mut state).unwrap().len(),1);
        assert_eq!(calls.load(Ordering::Relaxed),expected_calls);
    }
    original.set_boundary_executables(ExecutableSet::default());
    for (session,expected_calls) in [(&original,2),(&recreated,3)] {
        let mut state=KernelState::default(); state.frontier=vec![crate::ir::runtime::Row::new()]; state.context.rule_context=session.rule_context().unwrap();
        assert_eq!(kernel.invoke(vec![],&mut state).unwrap().len(),1);
        assert_eq!(calls.load(Ordering::Relaxed),expected_calls);
    }
}

#[test]
fn session_boundary_failures_propagate_without_poisoning_later_query_contexts() {
    use crate::ir::rel::runtime::{CompileOptions,KernelState,CompiledKernel,compile_for_host};
    use crate::ir::rel::runtime::policy::{BoundaryKind,ExecutableSet};
    let graph_plan=GraphPlan::new(GraphPlanPolicy::cypher(),crate::ir::plan::Node::GraphOneRow);
    let compiled=compile_for_host(&graph_plan,&PropertyGraph::new(),CompileOptions{sql_islands:false,..Default::default()}).unwrap();
    let kernel=CompiledKernel::from_plan(&compiled).unwrap();
    let calls=Arc::new(AtomicUsize::new(0)); let mut session=DagSession::new(None);
    session.set_boundary_executables(ExecutableSet::default().with(BoundaryKind::LiteralInput,
        Arc::new(SessionBoundaryObserver{calls:calls.clone(),reject:true})));
    let mut state=KernelState::default(); state.frontier=vec![crate::ir::runtime::Row::new()]; state.context.rule_context=session.rule_context().unwrap();
    assert!(kernel.invoke(vec![],&mut state).unwrap_err().to_string().contains("session boundary rejected"));
    assert_eq!(calls.load(Ordering::Relaxed),1);
    session.set_boundary_executables(ExecutableSet::default());
    let mut next=KernelState::default(); next.frontier=vec![crate::ir::runtime::Row::new()]; next.context.rule_context=session.rule_context().unwrap();
    assert_eq!(kernel.invoke(vec![],&mut next).unwrap().len(),1);
    assert_eq!(calls.load(Ordering::Relaxed),1);
}
