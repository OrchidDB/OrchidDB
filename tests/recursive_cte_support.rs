//! Capability guard for lowering variable-length patterns to `WITH RECURSIVE`.
//!
//! DataFusion's unparser refuses to emit a `LogicalPlan::RecursiveQuery` as a
//! whole (`not_impl_err!` in `datafusion-sql`), so the plan is going to be
//! assembled into SQL text by this crate: each term is unparsed on its own and
//! wrapped in the CTE. That plan rests on two behaviours neither of which we
//! control, so they are pinned here rather than assumed:
//!
//! 1. The unparser emits a recursive term *including its self-reference* —
//!    `FROM "generate_numbers"` — even though it will not emit the enclosing
//!    `RecursiveQuery`.
//! 2. The assembled statement is accepted and evaluated by the target engine.
//!
//! If either regresses, the lowering has to change, and it is much cheaper to
//! learn that here than from a corpus-wide mismatch.

#![cfg(feature = "duckdb")]

use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, RecordBatch};
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::datasource::cte_worktable::CteWorkTable;
use datafusion::datasource::{MemTable, provider_as_source};
use datafusion::logical_expr::{LogicalPlanBuilder, col, lit};
use datafusion::sql::unparser::Unparser;
use datafusion::sql::unparser::dialect::DuckDBDialect;

use new_graph::ir::rel::sql::{DuckDbExecutor, SqlExecutor, SqlValue};

/// Counts 1..5, the smallest query with all three parts of a recursive CTE:
/// a base case, a recursive step, and a termination condition.
#[test]
fn unparsed_terms_assemble_into_a_working_recursive_cte() {
    let schema = Arc::new(Schema::new(vec![Field::new("n", DataType::Int64, false)]));
    let seed = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![Arc::new(Int64Array::from(vec![1i64])) as ArrayRef],
    )
    .expect("seed batch");
    let provider = Arc::new(MemTable::try_new(Arc::clone(&schema), vec![vec![seed]]).expect("seed"));

    // Base case. For a graph walk this is the match input, not a constant,
    // which is why it reads a table here.
    let static_term = LogicalPlanBuilder::scan("seed", provider_as_source(provider), None)
        .expect("scan seed")
        .build()
        .expect("static term");

    // Recursive step: reads the working table by name and advances.
    let work = Arc::new(CteWorkTable::new("generate_numbers", Arc::clone(&schema)));
    let recursive_term =
        LogicalPlanBuilder::scan("generate_numbers", provider_as_source(work), None)
            .expect("scan worktable")
            .filter(col("n").lt(lit(5i64)))
            .expect("termination condition")
            .project(vec![(col("n") + lit(1i64)).alias("n")])
            .expect("advance")
            .build()
            .expect("recursive term");

    let dialect = DuckDBDialect::new();
    let unparser = Unparser::new(&dialect);
    let static_sql = unparser
        .plan_to_sql(&static_term)
        .expect("unparse static term")
        .to_string();
    let recursive_sql = unparser
        .plan_to_sql(&recursive_term)
        .expect("unparse recursive term")
        .to_string();

    // The self-reference is the part worth pinning: the unparser will not emit
    // the `RecursiveQuery` wrapper, but it does emit the term's reference to
    // the CTE, which is what makes assembling the wrapper by hand viable.
    assert!(
        recursive_sql.contains("\"generate_numbers\""),
        "recursive term lost its self-reference: {recursive_sql}"
    );

    let statement = format!(
        "WITH RECURSIVE \"generate_numbers\" AS (\n  {static_sql}\n  UNION ALL\n  {recursive_sql}\n)\nSELECT \"n\" FROM \"generate_numbers\""
    );

    let mut executor = DuckDbExecutor::new();
    let setup = vec![
        "CREATE OR REPLACE TABLE \"seed\" (\"n\" BIGINT)".to_string(),
        "INSERT INTO \"seed\" VALUES (1)".to_string(),
    ];
    let rows = executor.run(&setup, &statement).expect("run assembled cte");

    let values: Vec<i64> = rows
        .iter()
        .map(|row| match row.as_slice() {
            [SqlValue::Int(value)] => *value,
            other => panic!("unexpected row shape: {other:?}"),
        })
        .collect();
    assert_eq!(values, vec![1, 2, 3, 4, 5], "statement was:\n{statement}");
}
