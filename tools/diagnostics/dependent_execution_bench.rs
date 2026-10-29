//! Reproducible dependent SQL boundary benchmark:
//! cargo run --features duckdb --example dependent_execution_bench -- 2048
//! Times exclude fixture construction; each trial verifies every returned value.
#[cfg(feature = "duckdb")]
mod benchmark {
    use arrow::{
        array::{Int64Array, RecordBatch},
        datatypes::{DataType, Field, Schema},
    };
    use datafusion::{common::DFSchema,logical_expr::LogicalPlanBuilder};
    use futures::{StreamExt, TryStreamExt};
    use orchiddb::ir::rel::engine_exec::{ExecutionPlan, InputBatches, TaskContext};
    use orchiddb::ir::{
        QueryCost,
        rel::{
            dependent::{DependentOperation, exec::DependentOperationExec},
            sql::{lowering::SqlTemplate, region::RegionSession},
        },
    };
    use std::{
        sync::{Arc, Mutex},
        time::Instant,
    };

    async fn trial(rows: usize, limit: bool) -> serde_json::Value {
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
        let logical=LogicalPlanBuilder::empty(false).build().unwrap();
        let physical = Arc::new(InputBatches::new(schema.clone(), batches));
        let operation = DependentOperation {
            source: Arc::new(logical),
            template: SqlTemplate {
                sql: "SELECT $1 AS value".into(),
                parameters: 1,
                dialect: "duckdb".into(),
            },
            schema: Arc::new(DFSchema::try_from(schema.as_ref().clone()).unwrap()),
        };
        let session = Arc::new(Mutex::new(
            Box::new(orchiddb::ir::rel::sql::DuckDbSession::default()) as Box<dyn RegionSession>,
        ));
        let cost = Arc::new(Mutex::new(QueryCost::default()));
        let dependent = Arc::new(DependentOperationExec::new(
            &operation,
            physical,
            session,
            cost.clone(),
        ));
        let started = Instant::now();
        let mut stream = dependent
            .execute(0, Arc::new(TaskContext::default()))
            .unwrap()
            .take(if limit { 1 } else { usize::MAX });
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
