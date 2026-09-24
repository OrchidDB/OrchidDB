#![cfg(feature = "duckdb")]
use new_graph::ir::{
    catalog::PropertyGraph,
    functions::{DuckDbCatalog, FunctionKind, OperatorTable},
    rel::{
        RelBackend,
        sql::{self, DuckDbExecutor, SqlDialect},
    },
};
use new_graph::language::cypher::{parse_query, planner::CypherPlanner};

async fn query(query: &str) -> Vec<Vec<String>> {
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    let lowered = RelBackend::new()
        .lower(&plan, &PropertyGraph::new())
        .unwrap();
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb).await.unwrap();
    let result = sql::execute_prepared(&mut DuckDbExecutor::new(), &prepared)
        .unwrap_or_else(|e| panic!("{query}: {e}\n{}", prepared.query));
    let batch = result.batch;
    (0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|col| {
                    arrow::util::display::array_value_to_string(batch.column(col).as_ref(), row)
                        .unwrap()
                })
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn ordinary_names_resolve_scalar_overloads_and_nested_calls() {
    assert_eq!(query("WITH 'abc' AS s, 7 AS n RETURN jaro_similarity(s, 'abc'), bit_count(n), sha256(s), md5_number_lower(s)").await[0][..2], ["1.0", "3"]);
    assert_eq!(
        query("WITH 'abc' AS s RETURN hex(unhex(hex(s)))").await,
        vec![vec!["616263"]]
    );
    assert_eq!(
        query("RETURN typeof(bit_count(7)), typeof(bit_count(CAST(7, 'INT32')))").await,
        vec![vec!["TINYINT", "TINYINT"]]
    );
}

#[tokio::test]
async fn engine_null_handling_and_lists_are_not_interpreter_folded() {
    assert_eq!(
        query(
            "RETURN constant_or_null(42, NULL), nullif(3, 3), list_zip([1, 2], [3, 4]) IS NOT NULL"
        )
        .await,
        vec![vec!["", "", "true"]]
    );
    assert_eq!(
        query("RETURN concat_ws('-', NULL, 'x')").await,
        vec![vec!["x"]]
    );
}

#[tokio::test]
async fn standard_language_semantics_win_name_collisions() {
    assert_eq!(
        query("WITH 100 AS n RETURN log(n), log10(n), toInteger('bad')").await[0][1..],
        ["2.0", ""]
    );
    let log: f64 = query("WITH 100 AS n RETURN log(n)").await[0][0]
        .parse()
        .unwrap();
    assert!((log - 100.0_f64.ln()).abs() < 1e-10);
}

#[test]
fn catalog_contains_all_installed_overloads_and_rejects_bad_calls() {
    let catalog = DuckDbCatalog::new().unwrap();
    assert!(catalog.functions().count() > 1000);
    assert!(catalog.overloads("BIT_COUNT").len() > 1);
    assert!(
        catalog
            .overloads("range")
            .iter()
            .filter(|f| f.kind == FunctionKind::Table)
            .all(|f| f.kind == FunctionKind::Table)
    );
    let schema = datafusion::common::DFSchema::empty();
    assert!(
        catalog
            .bind(
                "function_that_does_not_exist",
                FunctionKind::Scalar,
                &[],
                &schema
            )
            .unwrap_err()
            .to_string()
            .contains("unknown function")
    );
    assert!(
        catalog
            .bind("duckdb_functions", FunctionKind::Scalar, &[], &schema)
            .unwrap_err()
            .to_string()
            .contains("table-valued")
    );
    assert!(
        catalog
            .bind(
                "bit_count",
                FunctionKind::Scalar,
                &[datafusion::prelude::lit("abc")],
                &schema
            )
            .is_err()
    );
}

#[test]
fn catalog_binding_does_not_evaluate_calls() {
    use datafusion::prelude::lit;
    let conn = duckdb::Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE SEQUENCE bind_probe; CREATE MACRO probe() AS nextval('bind_probe')")
        .unwrap();
    let observer = conn.try_clone().unwrap();
    let catalog = DuckDbCatalog::from_connection(conn).unwrap();
    let schema = datafusion::common::DFSchema::empty();
    // error() and side-effecting calls are bound, never evaluated.
    catalog
        .bind(
            "error",
            FunctionKind::Scalar,
            &[lit("must not execute")],
            &schema,
        )
        .unwrap();
    assert!(
        catalog
            .bind("probe", FunctionKind::Scalar, &[], &schema)
            .is_ok()
    );
    let next: i64 = observer
        .query_row("SELECT nextval('bind_probe')", [], |row| row.get(0))
        .unwrap();
    assert_eq!(next, 1, "binding must not advance the sequence");
}

#[test]
fn unknown_null_call_and_wrong_arity_fail_during_lowering() {
    for q in ["RETURN made_up_function(NULL)", "RETURN bit_count(1, 2)"] {
        let plan = CypherPlanner::new().plan(&parse_query(q).unwrap()).unwrap();
        assert!(
            RelBackend::new()
                .lower(&plan, &PropertyGraph::new())
                .is_err(),
            "{q}"
        );
    }
}

#[test]
fn native_binding_preserves_column_list_and_unsigned_types() {
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;
    let schema = datafusion::common::DFSchema::try_from(Schema::new(vec![
        Field::new(
            "xs",
            DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
            true,
        ),
        Field::new("n", DataType::UInt64, true),
    ]))
    .unwrap();
    let catalog = DuckDbCatalog::new().unwrap();
    let list = datafusion::prelude::col("xs");
    let result = catalog
        .bind(
            "list_zip",
            FunctionKind::Scalar,
            &[list.clone(), list],
            &schema,
        )
        .unwrap();
    assert!(matches!(result, DataType::List(_)), "{result}");
    assert_eq!(
        catalog
            .bind(
                "bit_count",
                FunctionKind::Scalar,
                &[datafusion::prelude::col("n")],
                &schema
            )
            .unwrap(),
        DataType::Int8
    );
}

#[tokio::test]
async fn nested_struct_results_can_be_passed_through_a_binding() {
    assert_eq!(
        query("WITH list_zip([1, 2], [3, 4]) AS z RETURN list_count(z)").await,
        vec![vec!["2"]]
    );
}
