//! Reproducible dependent SQL boundary benchmark:
//! cargo run --features duckdb --example dependent_execution_bench -- 2048
//! Times exclude fixture construction; each trial verifies every returned value.
#[cfg(feature = "duckdb")]
mod benchmark {
    use arrow::{
        array::{Int64Array, RecordBatch},
        datatypes::{DataType, Field, Schema, SchemaRef},
    };
    use datafusion::{
        common::DFSchema,
        physical_plan::{ExecutionPlan, limit::GlobalLimitExec},
        prelude::SessionContext,
    };
    use futures::TryStreamExt;
    use orchiddb::ir::{
        QueryCost,
        rel::{
            dependent::{DependentOperation, exec::DependentOperationExec},
            sql::{SqlDialect, SqlError, SqlResult, lowering::SqlTemplate, region::RegionSession},
        },
    };
    use std::{
        sync::{Arc, Mutex},
        time::Instant,
    };

    #[derive(Debug)]
    struct Session(duckdb::Connection);
    impl RegionSession for Session {
        fn dialect(&self) -> SqlDialect {
            SqlDialect::DuckDb
        }
        fn query(&mut self, sql: &str, schema: SchemaRef) -> SqlResult<RecordBatch> {
            let mut statement = self
                .0
                .prepare(sql)
                .map_err(|e| SqlError::Execution(e.to_string()))?;
            let reader = statement
                .query_arrow([])
                .map_err(|e| SqlError::Execution(e.to_string()))?;
            Ok(arrow::compute::concat_batches(
                &schema,
                reader.collect::<Vec<_>>().iter(),
            )?)
        }
    }

    async fn trial(rows: usize, limit: bool) -> serde_json::Value {
        let ctx = SessionContext::new();
        let schema = Arc::new(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]));
        // Duplicate inputs exercise occurrence preservation, not just set equality.
        let expected = (0..rows).map(|i| (i % 97) as i64).collect::<Vec<_>>();
        let batches = expected
            .chunks(64)
            .map(|values| {
                RecordBatch::try_new(
                    schema.clone(),
                    vec![Arc::new(Int64Array::from(values.to_vec()))],
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let logical = ctx
            .read_batches(batches)
            .unwrap()
            .into_optimized_plan()
            .unwrap();
        let physical = ctx.state().create_physical_plan(&logical).await.unwrap();
        let operation = DependentOperation {
            source: Arc::new(logical),
            template: SqlTemplate {
                sql: "SELECT $1 AS value".into(),
                parameters: 1,
                dialect: "duckdb".into(),
            },
            schema: Arc::new(DFSchema::try_from(schema.as_ref().clone()).unwrap()),
        };
        let session = Arc::new(Mutex::new(Box::new(Session(
            duckdb::Connection::open_in_memory().unwrap(),
        )) as Box<dyn RegionSession>));
        let cost = Arc::new(Mutex::new(QueryCost::default()));
        let dependent = Arc::new(DependentOperationExec::new(
            &operation,
            physical,
            session,
            cost.clone(),
        ));
        let plan: Arc<dyn ExecutionPlan> = if limit {
            Arc::new(GlobalLimitExec::new(dependent.clone(), 0, Some(1)))
        } else {
            dependent.clone()
        };
        let started = Instant::now();
        let mut stream = plan.execute(0, ctx.task_ctx()).unwrap();
        let mut first_batch_micros = None;
        let mut largest_output_batch_bytes = 0;
        let mut output_batches = 0;
        let mut actual = Vec::new();
        while let Some(batch) = stream.try_next().await.unwrap() {
            first_batch_micros.get_or_insert(started.elapsed().as_micros() as u64);
            largest_output_batch_bytes =
                largest_output_batch_bytes.max(batch.get_array_memory_size());
            output_batches += 1;
            actual.extend_from_slice(
                batch
                    .column(0)
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .unwrap()
                    .values(),
            );
        }
        let elapsed_micros = started.elapsed().as_micros() as u64;
        assert_eq!(
            actual,
            if limit {
                expected[..1].to_vec()
            } else {
                expected
            }
        );
        let cost = cost.lock().unwrap().clone();
        serde_json::json!({
            "input_rows": rows, "limit": if limit { Some(1) } else { None },
            "result_rows": actual.len(), "results_verified": true,
            "sql_executions": cost.sql_executions, "sql_output_rows": cost.sql_output_rows,
            "first_batch_micros": first_batch_micros, "elapsed_micros": elapsed_micros,
            "output_batches": output_batches, "largest_output_batch_bytes": largest_output_batch_bytes,
            "operator_metrics": dependent.metrics().map(|m| m.iter().map(|metric| {
                (metric.value().name().to_string(), metric.value().as_usize())
            }).collect::<std::collections::BTreeMap<_, _>>()),
        })
    }

    pub async fn run() {
        let rows = std::env::args()
            .nth(1)
            .map(|s| s.parse().unwrap())
            .unwrap_or(2048);
        assert!(rows > 0);
        // Warm engine/library initialization separately from measured trials.
        trial(16, false).await;
        let mut full = Vec::new();
        let mut limited = Vec::new();
        for _ in 0..5 {
            full.push(trial(rows, false).await);
            limited.push(trial(rows, true).await);
        }
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({
            "benchmark": "dependent_execution", "profile": "cargo dev, unoptimized",
            "engine": "in-memory DuckDB", "input_batch_rows": 64,
            "timing_scope": "execution and result decoding; excludes fixture, physical planning, and equality assertion",
            "metric_units": "counts and bytes; binding_time and sql_time in nanoseconds",
            "full": full, "limit_one": limited,
        })).unwrap());
    }
}

#[cfg(feature = "duckdb")]
#[tokio::main]
async fn main() {
    benchmark::run().await;
}

#[cfg(not(feature = "duckdb"))]
fn main() {
    panic!("run with --features duckdb");
}
