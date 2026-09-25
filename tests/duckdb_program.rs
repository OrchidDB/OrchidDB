#![cfg(feature = "duckdb")]

use orchiddb::ir::rel::sql::program::{
    DuckDbProgram, ProgramStage, WorkColumn, WorkStage, WorkType,
};
use orchiddb::ir::rel::sql::{DuckDbExecutor, SqlExecutor, SqlValue};
use std::time::Duration;

fn numbers_program() -> (DuckDbProgram, String) {
    let mut program = DuckDbProgram::new("");
    let table = program
        .add_work_table(vec![WorkColumn::new("n", WorkType::BigInt, false)])
        .unwrap();
    program.push(ProgramStage::Write(WorkStage::insert(
        &table,
        "SELECT n FROM source WHERE n <= 2",
    )));
    program.set_result_sql(format!("SELECT n FROM {} ORDER BY n", table.sql_name()));
    (program, table.sql_name().to_string())
}

#[test]
fn program_reads_source_and_cleans_up_on_success() {
    let mut executor = DuckDbExecutor::new();
    executor
        .execute_batch("CREATE TABLE source(n BIGINT); INSERT INTO source VALUES (1), (2), (3)")
        .unwrap();
    let (program, work_name) = numbers_program();
    assert_eq!(
        program.run(&mut executor).unwrap(),
        vec![vec![SqlValue::Int(1)], vec![SqlValue::Int(2)]]
    );
    assert!(!executor.in_transaction());
    assert!(
        executor
            .run(&[], &format!("SELECT * FROM {work_name}"))
            .is_err()
    );
    assert_eq!(
        executor.run(&[], "SELECT count(*) FROM source").unwrap(),
        vec![vec![SqlValue::Int(3)]]
    );
}

#[test]
fn failed_stage_rolls_back_work_tables_and_keeps_session_usable() {
    let mut executor = DuckDbExecutor::new();
    executor
        .execute_batch("CREATE TABLE source(n BIGINT); INSERT INTO source VALUES (7)")
        .unwrap();
    let (mut program, work_name) = numbers_program();
    let table = program
        .add_work_table(vec![WorkColumn::new("value", WorkType::BigInt, false)])
        .unwrap();
    program.push(ProgramStage::Write(WorkStage::insert(
        &table,
        "SELECT missing_column FROM source",
    )));
    assert!(program.run(&mut executor).is_err());
    assert!(!executor.in_transaction());
    assert!(
        executor
            .run(&[], &format!("SELECT * FROM {work_name}"))
            .is_err()
    );
    assert!(
        executor
            .run(&[], &format!("SELECT * FROM {}", table.sql_name()))
            .is_err()
    );
    assert_eq!(
        executor.run(&[], "SELECT n FROM source").unwrap(),
        vec![vec![SqlValue::Int(7)]]
    );
    assert_eq!(
        numbers_program().0.run(&mut executor).unwrap(),
        Vec::<Vec<SqlValue>>::new()
    );
}

#[test]
fn repeat_terminates_from_duckdb_rows_and_budget_failure_rolls_back() {
    let mut executor = DuckDbExecutor::new();
    let mut program = DuckDbProgram::new("");
    let frontier = program
        .add_work_table(vec![WorkColumn::new("n", WorkType::BigInt, false)])
        .unwrap();
    program.push(ProgramStage::Write(WorkStage::insert(
        &frontier, "SELECT 0",
    )));
    program.push(ProgramStage::RepeatUntilEmpty {
        steps: vec![WorkStage::insert(
            &frontier,
            format!(
                "SELECT max(n) + 1 FROM {} HAVING max(n) < 3",
                frontier.sql_name()
            ),
        )],
        until_empty: format!(
            "SELECT 1 FROM {} WHERE n = (SELECT max(n) FROM {}) AND n < 3 LIMIT 1",
            frontier.sql_name(),
            frontier.sql_name()
        ),
        max_iterations: 4,
    });
    program.set_result_sql(format!("SELECT max(n) FROM {}", frontier.sql_name()));
    assert_eq!(
        program.run(&mut executor).unwrap(),
        vec![vec![SqlValue::Int(3)]]
    );
    assert!(
        executor
            .run(&[], &format!("SELECT * FROM {}", frontier.sql_name()))
            .is_err()
    );

    let mut limited = DuckDbProgram::new("");
    let table = limited
        .add_work_table(vec![WorkColumn::new("n", WorkType::BigInt, false)])
        .unwrap();
    limited.push(ProgramStage::RepeatUntilEmpty {
        steps: vec![WorkStage::insert(&table, "SELECT 1")],
        until_empty: format!("SELECT 1 FROM {} LIMIT 1", table.sql_name()),
        max_iterations: 2,
    });
    limited.set_result_sql(format!("SELECT * FROM {}", table.sql_name()));
    let error = limited.run(&mut executor).unwrap_err();
    assert!(
        error.to_string().contains("exceeded 2 iterations"),
        "{error}"
    );
    assert!(!executor.in_transaction());
    assert!(
        executor
            .run(&[], &format!("SELECT * FROM {}", table.sql_name()))
            .is_err()
    );
    assert_eq!(
        executor.run(&[], "SELECT 42").unwrap(),
        vec![vec![SqlValue::Int(42)]]
    );
}

#[test]
fn caller_transaction_is_preserved() {
    let mut executor = DuckDbExecutor::new();
    executor.begin().unwrap();
    let (program, _) = numbers_program();
    assert!(program.run(&mut executor).is_err());
    assert!(executor.in_transaction());
    executor.rollback().unwrap();
}

#[test]
fn interrupted_stage_rolls_back_and_session_recovers() {
    let mut executor = DuckDbExecutor::new();
    executor
        .execute_batch("CREATE TABLE source(n BIGINT); INSERT INTO source VALUES (17)")
        .unwrap();
    executor.set_timeouts(Duration::from_millis(1), Duration::from_secs(1));
    let mut program = DuckDbProgram::new("");
    let work = program
        .add_work_table(vec![WorkColumn::new("n", WorkType::BigInt, false)])
        .unwrap();
    program.push(ProgramStage::Write(WorkStage::insert(
        &work,
        "SELECT i FROM range(1000000000) t(i)",
    )));
    program.set_result_sql(format!("SELECT count(*) FROM {}", work.sql_name()));
    let error = program.run(&mut executor).unwrap_err();
    assert!(error.to_string().contains("timed out"), "{error}");
    assert!(!executor.in_transaction());
    executor.set_timeouts(Duration::from_secs(2), Duration::from_secs(2));
    assert!(
        executor
            .run(&[], &format!("SELECT * FROM {}", work.sql_name()))
            .is_err()
    );
    assert_eq!(
        executor.run(&[], "SELECT n FROM source").unwrap(),
        vec![vec![SqlValue::Int(17)]]
    );
}
