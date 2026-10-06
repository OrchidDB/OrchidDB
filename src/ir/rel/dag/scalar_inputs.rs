//! Native input evaluation must precede SQL placement on every execution path.
use super::*;
use arrow::{array::Float64Array, datatypes::DataType};
use datafusion::{
    common::ScalarValue,
    logical_expr::{ColumnarValue, LogicalPlanBuilder, Volatility, col, create_udf, lit},
};
use std::sync::atomic::{AtomicUsize, Ordering};

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
        resources.region_session = Some(Arc::new(Mutex::new(Box::new(EmbeddedRegionSession(
            executor,
        )))));
    }
    resources
}

fn lowered(plan: LogicalPlan) -> LoweredPlan {
    LoweredPlan {
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
                    format!("{error:?}").contains("local input failed"),
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
            let (physical, task, stats) = prepare_with_extensions(
                &plan,
                vec![],
                &resources,
                Arc::new(Mutex::new(Default::default())),
            )
            .await
            .unwrap();
            assert_eq!(calls.load(Ordering::SeqCst), 0, "{stats:?}");
            let batches = datafusion::physical_plan::collect(physical, task)
                .await
                .unwrap();
            let values = batches
                .iter()
                .flat_map(|batch| {
                    batch
                        .column(0)
                        .as_any()
                        .downcast_ref::<arrow::array::Int64Array>()
                        .unwrap()
                        .values()
                        .to_vec()
                })
                .collect::<Vec<_>>();
            assert_eq!(
                values,
                if volatility == Volatility::Volatile {
                    vec![7, 7]
                } else {
                    vec![1, 2]
                }
            );
            assert!(calls.load(Ordering::SeqCst) > 0);
            assert!(stats.datafusion_operators > 0, "{stats:?}");
            assert!(
                stats
                    .sql_queries
                    .iter()
                    .all(|sql| !sql.contains("local_identity"))
            );
        }
    }
}
