//! General relational execution and table functions are independent of search.
use arrow::{
    array::{Int32Array, Int64Array, RecordBatch},
    datatypes::{DataType, Field, Schema, SchemaRef},
};
use datafusion::{common::DFSchema, prelude::SessionContext};
use orchiddb::ir::{
    QueryCost,
    rel::{
        dependent::{DependentOperation, exec::DependentOperationExec},
        sql::{SqlDialect, SqlError, SqlResult, lowering::SqlTemplate, region::RegionSession},
    },
};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
struct RecordingSession {
    dialect: SqlDialect,
    calls: Arc<Mutex<Vec<String>>>,
    fail: bool,
}
impl RegionSession for RecordingSession {
    fn dialect(&self) -> SqlDialect {
        self.dialect
    }
    fn query(&mut self, sql: &str, _schema: SchemaRef) -> SqlResult<RecordBatch> {
        self.calls.lock().unwrap().push(sql.into());
        if self.fail {
            return Err(SqlError::Execution("backend unavailable".into()));
        }
        let value = if sql.contains("CAST(2 AS BIGINT)") {
            20
        } else if sql.contains("CAST(3 AS BIGINT)") {
            30
        } else {
            return Err(SqlError::Execution(format!(
                "unexpected bound query: {sql}"
            )));
        };
        // A driver can return a narrower physical integer; execution must use
        // the logical output schema rather than leaking the driver's type.
        Ok(RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "result",
                DataType::Int32,
                false,
            )])),
            vec![Arc::new(Int32Array::from(vec![value]))],
        )?)
    }
}
async fn run(
    values: Vec<i64>,
    dialect: SqlDialect,
    fail: bool,
) -> (
    datafusion::common::Result<Vec<RecordBatch>>,
    Vec<String>,
    QueryCost,
) {
    let ctx = SessionContext::new();
    let input = RecordBatch::try_new(
        Arc::new(Schema::new(vec![Field::new(
            "input",
            DataType::Int64,
            false,
        )])),
        vec![Arc::new(Int64Array::from(values))],
    )
    .unwrap();
    let logical = ctx
        .read_batch(input)
        .unwrap()
        .into_optimized_plan()
        .unwrap();
    let physical = ctx.state().create_physical_plan(&logical).await.unwrap();
    let schema = Arc::new(
        DFSchema::try_from(Schema::new(vec![Field::new(
            "result",
            DataType::Int64,
            false,
        )]))
        .unwrap(),
    );
    let operation = DependentOperation {
        source: Arc::new(logical),
        template: SqlTemplate {
            sql: "SELECT $1 AS result".into(),
            parameters: 1,
            dialect: "duckdb".into(),
        },
        schema,
    };
    let calls = Arc::new(Mutex::new(Vec::new()));
    let cost = Arc::new(Mutex::new(QueryCost::default()));
    let session = Arc::new(Mutex::new(Box::new(RecordingSession {
        dialect,
        calls: calls.clone(),
        fail,
    }) as Box<dyn RegionSession>));
    let execution = Arc::new(DependentOperationExec::new(
        &operation,
        physical,
        session,
        cost.clone(),
    ));
    let result = datafusion::physical_plan::collect(execution, ctx.task_ctx()).await;
    let recorded = calls.lock().unwrap().clone();
    let measured = cost.lock().unwrap().clone();
    (result, recorded, measured)
}
#[tokio::test]
async fn dependent_execution_binds_each_source_row_and_preserves_output_types() {
    let (result, calls, cost) = run(vec![2, 3], SqlDialect::DuckDb, false).await;
    let batches = result.unwrap();
    let values = batches
        .iter()
        .flat_map(|batch| {
            batch
                .column(0)
                .as_any()
                .downcast_ref::<Int64Array>()
                .unwrap()
                .values()
                .to_vec()
        })
        .collect::<Vec<_>>();
    assert_eq!(values, vec![20, 30]);
    assert_eq!(
        calls,
        vec![
            "SELECT CAST(2 AS BIGINT) AS result",
            "SELECT CAST(3 AS BIGINT) AS result"
        ]
    );
    assert_eq!(cost.sql_executions, 2);
    assert_eq!(cost.sql_output_rows, 2);
}
#[tokio::test]
async fn empty_dependent_source_does_not_query_the_engine() {
    let (result, calls, cost) = run(vec![], SqlDialect::DuckDb, false).await;
    assert_eq!(
        result
            .unwrap()
            .iter()
            .map(RecordBatch::num_rows)
            .sum::<usize>(),
        0
    );
    assert!(calls.is_empty());
    assert_eq!(cost.sql_executions, 0);
}
#[tokio::test]
async fn dependent_execution_rejects_wrong_owner_before_querying() {
    let (result, calls, _) = run(vec![2], SqlDialect::Postgres, false).await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("session dialect mismatch")
    );
    assert!(calls.is_empty());
}
#[tokio::test]
async fn dependent_backend_error_is_not_retried() {
    let (result, calls, _) = run(vec![2, 3], SqlDialect::DuckDb, true).await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("backend unavailable")
    );
    assert_eq!(calls.len(), 1);
}

#[cfg(feature = "duckdb")]
#[test]
fn correlated_and_prepare_time_table_functions_return_the_same_rows() {
    use datafusion::{
        common::ScalarValue,
        logical_expr::{LogicalPlanBuilder, col, lit},
    };
    use orchiddb::ir::rel::{
        dependent::{ArgumentBinding, TableFunction},
        sql::{
            DuckDbExecutor, SqlExecutor, SqlValue,
            lowering::{LoweringContext, LoweringMode, RelationLowering, lower_relation},
        },
    };
    let input_schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
    let source = LogicalPlanBuilder::scan(
        "inputs",
        datafusion::datasource::provider_as_source(Arc::new(
            datafusion::datasource::empty::EmptyTable::new(input_schema),
        )),
        None,
    )
    .unwrap()
    .project(vec![col("n").alias("count")])
    .unwrap()
    .alias("seed")
    .unwrap()
    .build()
    .unwrap();
    let schema = Arc::new(
        DFSchema::try_from(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]))
        .unwrap(),
    );
    let function = TableFunction::new(
        vec!["range".into()],
        vec![col("seed.count")],
        Some(Arc::new(source)),
        ArgumentBinding::Correlated,
        schema,
    )
    .unwrap();
    let context = LoweringContext {
        dialect: SqlDialect::DuckDb,
        mode: LoweringMode::InIsland,
    };
    let RelationLowering::Sql(query) = lower_relation(&function.clone().into_plan(), &context)
        .unwrap()
        .unwrap()
    else {
        panic!("correlation must stay in one SQL island");
    };
    let mut engine = DuckDbExecutor::default();
    let mut correlated = engine
        .run(
            &[
                "CREATE TABLE inputs(n BIGINT NOT NULL)".into(),
                "INSERT INTO inputs VALUES (0), (2), (3)".into(),
            ],
            &query.to_string(),
        )
        .unwrap();
    let mut prepare_time = function.clone();
    prepare_time.binding = ArgumentBinding::PrepareTime;
    let RelationLowering::Dependent { template, .. } =
        lower_relation(&prepare_time.into_plan(), &context)
            .unwrap()
            .unwrap()
    else {
        panic!("prepare-time arguments require a bound operation");
    };
    let mut bound = Vec::new();
    for value in [0, 2, 3] {
        bound.extend(
            engine
                .run(
                    &[],
                    &template.bind(&[ScalarValue::Int64(Some(value))]).unwrap(),
                )
                .unwrap(),
        );
    }
    let sort = |rows: &mut Vec<Vec<SqlValue>>| rows.sort_by_key(|r| format!("{r:?}"));
    sort(&mut correlated);
    sort(&mut bound);
    assert_eq!(correlated, bound);
    assert_eq!(
        correlated,
        vec![
            vec![SqlValue::Int(2), SqlValue::Int(0)],
            vec![SqlValue::Int(2), SqlValue::Int(1)],
            vec![SqlValue::Int(3), SqlValue::Int(0)],
            vec![SqlValue::Int(3), SqlValue::Int(1)],
            vec![SqlValue::Int(3), SqlValue::Int(2)]
        ]
    );
    // A filter on generated rows remains after the row-producing operation.
    let parent = LogicalPlanBuilder::from(function.into_plan())
        .filter(col("value").eq(lit(1_i64)))
        .unwrap()
        .project(vec![col("value")])
        .unwrap()
        .build()
        .unwrap();
    let lowered = orchiddb::ir::rel::LoweredPlan {
        plan: parent,
        fields: vec!["value".into()],
        result_form: orchiddb::ir::policy::ResultForm::RowSet,
        islands: Default::default(),
    };
    let query = orchiddb::ir::rel::sql::unparse(&lowered, SqlDialect::DuckDb).unwrap();
    assert_eq!(
        engine.run(&[], &query).unwrap(),
        vec![vec![SqlValue::Int(1)], vec![SqlValue::Int(1)]]
    );
}

#[test]
fn dependent_templates_reject_non_query_statements() {
    let template = SqlTemplate {
        sql: "DELETE FROM documents WHERE id = $1".into(),
        parameters: 1,
        dialect: "duckdb".into(),
    };
    let error = template
        .bind(&[datafusion::common::ScalarValue::Int64(Some(7))])
        .unwrap_err();
    assert!(error.to_string().contains("must be a query statement"));
}

#[tokio::test]
async fn optimizer_keeps_generated_row_filters_above_table_functions() {
    use datafusion::logical_expr::{LogicalPlan, LogicalPlanBuilder, col, lit};
    use orchiddb::ir::rel::dependent::{ArgumentBinding, TableFunction};
    let source = LogicalPlanBuilder::values(vec![vec![lit(2_i64)], vec![lit(3_i64)]])
        .unwrap()
        .project(vec![col("column1").alias("count")])
        .unwrap()
        .build()
        .unwrap();
    let schema = Arc::new(
        DFSchema::try_from(Schema::new(vec![Field::new(
            "value",
            DataType::Int64,
            false,
        )]))
        .unwrap(),
    );
    let function = TableFunction::new(
        vec!["range".into()],
        vec![col("count")],
        Some(Arc::new(source)),
        ArgumentBinding::Correlated,
        schema,
    )
    .unwrap();
    let plan = LogicalPlanBuilder::from(function.into_plan())
        .filter(col("value").gt(lit(0_i64)))
        .unwrap()
        .build()
        .unwrap();
    let optimized = SessionContext::new().state().optimize(&plan).unwrap();
    let LogicalPlan::Filter(filter) = optimized else {
        panic!("row filter must remain above function: {optimized}");
    };
    let LogicalPlan::Extension(extension) = filter.input.as_ref() else {
        panic!("row filter moved past table function: {}", filter.input);
    };
    assert!(extension.node.as_any().is::<TableFunction>());
}

#[cfg(feature = "duckdb")]
#[test]
fn two_table_functions_compose_in_one_sql_scope() {
    use datafusion::logical_expr::{LogicalPlanBuilder, col, lit};
    use orchiddb::ir::rel::{
        dependent::{ArgumentBinding, TableFunction},
        sql::{DuckDbExecutor, SqlExecutor, SqlValue},
    };
    let function = |name: &str, n: i64| {
        TableFunction::new(
            vec!["range".into()],
            vec![lit(n)],
            None,
            ArgumentBinding::Correlated,
            Arc::new(
                DFSchema::try_from(Schema::new(vec![Field::new(name, DataType::Int64, false)]))
                    .unwrap(),
            ),
        )
        .unwrap()
        .into_plan()
    };
    let plan = LogicalPlanBuilder::from(function("left_value", 2))
        .cross_join(function("right_value", 3))
        .unwrap()
        .project(vec![col("left_value"), col("right_value")])
        .unwrap()
        .sort(vec![
            col("left_value").sort(true, false),
            col("right_value").sort(true, false),
        ])
        .unwrap()
        .build()
        .unwrap();
    let lowered = orchiddb::ir::rel::LoweredPlan {
        plan,
        fields: vec!["left_value".into(), "right_value".into()],
        result_form: orchiddb::ir::policy::ResultForm::RowSet,
        islands: Default::default(),
    };
    let query = orchiddb::ir::rel::sql::unparse(&lowered, SqlDialect::DuckDb).unwrap();
    let rows = DuckDbExecutor::default()
        .run(&[], &query)
        .unwrap_or_else(|error| panic!("{error}\n{query}"));
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[0], vec![SqlValue::Int(0), SqlValue::Int(0)]);
    assert_eq!(rows[5], vec![SqlValue::Int(1), SqlValue::Int(2)]);
    assert!(
        query.contains("__lowered_relation_0") && query.contains("__lowered_relation_1"),
        "{query}"
    );
}

#[test]
fn table_function_rejects_ambiguous_flat_column_names_before_binding() {
    use datafusion::logical_expr::{LogicalPlanBuilder, col, lit};
    use orchiddb::ir::rel::dependent::{ArgumentBinding, TableFunction};
    let source = |alias: &str| {
        LogicalPlanBuilder::empty(true)
            .project(vec![lit(1_i64).alias("id")])
            .unwrap()
            .alias(alias)
            .unwrap()
            .build()
            .unwrap()
    };
    let output = |name: &str| {
        Arc::new(
            DFSchema::try_from(Schema::new(vec![Field::new(name, DataType::Int64, false)]))
                .unwrap(),
        )
    };
    let joined = LogicalPlanBuilder::from(source("left_source"))
        .cross_join(source("right_source"))
        .unwrap()
        .build()
        .unwrap();
    let error = TableFunction::new(
        vec!["range".into()],
        vec![col("left_source.id")],
        Some(Arc::new(joined)),
        ArgumentBinding::PrepareTime,
        output("value"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("project unique aliases"));
    let error = TableFunction::new(
        vec!["range".into()],
        vec![col("seed.id")],
        Some(Arc::new(source("seed"))),
        ArgumentBinding::Correlated,
        output("id"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("project unique aliases"));
}
