#![cfg(feature = "duckdb")]

use new_graph::ir::rel::sql::program::{DuckDbProgram, WorkColumn, WorkType};
use new_graph::ir::rel::sql::{DuckDbExecutor, SqlExecutor};
use std::time::Duration;

#[test]
fn expired_program_deadline_is_reported_without_leaving_a_transaction_open() {
    let mut executor = DuckDbExecutor::new();
    let mut program = DuckDbProgram::new("SELECT 1");
    program
        .add_work_table(vec![WorkColumn::new("n", WorkType::BigInt, false)])
        .unwrap();
    program.set_timeout(Duration::ZERO);

    let error = program.run(&mut executor).unwrap_err();

    assert!(error.to_string().contains("wall-clock deadline"), "{error}");
    assert!(!executor.in_transaction());
    assert_eq!(executor.run(&[], "SELECT 42").unwrap().len(), 1);
}
