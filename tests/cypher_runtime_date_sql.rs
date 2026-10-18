#![cfg(feature = "duckdb")]
use arrow::{datatypes::DataType, record_batch::RecordBatch};
use datafusion::common::ScalarValue;
use orchiddb::ir::{catalog::PropertyGraph, diagnostics::without_constant_folding,
    functions::domain, rel::{RelBackend, execute_lowered,
        sql::{DuckDbExecutor, SqlDialect, execute_lowered_sql, unparse}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

fn dates(batch: &RecordBatch) -> Vec<String> {
    assert_eq!(domain::descriptor(batch.schema().field(0).data_type()),
        Some(("cypher_temporal", &DataType::Date32)));
    (0..batch.num_rows()).map(|row| {
        let value = ScalarValue::try_from_array(batch.column(0),row).unwrap();
        if value.is_null() { return "NULL".into(); }
        let storage = domain::storage(&value).unwrap();
        arrow::util::display::array_value_to_string(storage.to_array().unwrap().as_ref(),0).unwrap()
    }).collect()
}

#[tokio::test]
async fn canonical_date_is_a_runtime_cast_with_native_date_results() {
    without_constant_folding(async {
        for (query, expected) in [
            ("RETURN date('2024-02-29') AS d", vec!["2024-02-29"]),
            ("UNWIND ['2000-02-29', '0001-01-01', '9999-12-31', '2000-02-29', null] AS s RETURN date(s) AS d",
                vec!["2000-02-29", "0001-01-01", "9999-12-31", "2000-02-29", "NULL"]),
            ("UNWIND [] AS s RETURN date('2024-02-29') AS d", vec![]),
            ("RETURN date(null) AS d", vec!["NULL"]),
        ] {
            let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
            let backend = RelBackend::new().with_language_functions(true);
            let lowered = backend.lower(&plan,&PropertyGraph::new()).unwrap_or_else(|e|panic!("{query}: {e}"));
            let sql = unparse(&lowered,SqlDialect::DuckDb).unwrap();
            assert!(sql.contains("AS DATE)"),"{query}: {sql}");
            let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap_or_else(|e|panic!("{query}: {e}\n{sql}"));
            assert_eq!(dates(&result.batch),expected,"{query}: {sql}");
            let native = execute_lowered(lowered).await.unwrap();
            assert_eq!(dates(&native.batch),expected,"native {query}");
        }
    }).await;
}

#[tokio::test]
async fn unsupported_date_grammars_invalid_calendars_and_types_keep_fallback() {
    without_constant_folding(async {
        for arg in ["'20240229'", "'2024-W01-1'", "'2024-001'", "'0000-01-01'", "'+10000-01-01'", "'2023-02-29'", "'1900-02-29'", "42", "true", "{year:2024,month:2,day:29}"] {
            let query = format!("RETURN date({arg}) AS d");
            // Literal validation may reject an invalid constructor before
            // relational lowering; that is also the preserved fallback path.
            let Ok(plan) = CypherPlanner::new().plan(&parse_query(&query).unwrap()) else { continue; };
            assert!(RelBackend::new().with_language_functions(true).lower(&plan,&PropertyGraph::new()).is_err(),"{query}");
        }
    }).await;
}
