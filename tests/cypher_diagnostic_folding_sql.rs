#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, diagnostics::without_constant_folding,
    rel::{RelBackend, sql::{SqlDialect, unparse, DuckDbExecutor, execute_lowered_sql}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

#[tokio::test]
async fn diagnostic_mode_preserves_scalar_and_quantifier_computation() {
    without_constant_folding(async {
        for (query,expected,operator) in [
            ("RETURN 1 + 2 AS result","3","+"),
            ("RETURN 1 = 2 AS result","false","="),
            ("RETURN single(x IN [1,2,3] WHERE x > 1) AS result","false", "unnest"),
        ] {
            let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
            let lowered = RelBackend::new().with_language_functions(true).lower(&plan,&PropertyGraph::new()).unwrap();
            let sql = unparse(&lowered,SqlDialect::DuckDb).unwrap();
            assert!(sql.to_lowercase().contains(operator),"{query}: {sql}");
            if query == "RETURN 1 = 2 AS result" {
                assert_eq!(sql, "SELECT (1 = 2) AS \"result\"");
            }
            assert_ne!(sql,format!("SELECT {expected} AS \"result\""));
            let result = execute_lowered_sql(&mut DuckDbExecutor::default(),&lowered).await.unwrap_or_else(|error| panic!("{query}: {error}\n{sql}"));
            assert_eq!(result.batch.num_rows(),1);
            assert_eq!(arrow::util::display::array_value_to_string(result.batch.column(0),0).unwrap(),expected);
        }
        let query = "RETURN single(x IN [['abc'],['abc','def']] WHERE single(y IN x WHERE y = 'def')) AS result";
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered = RelBackend::new().lower(&plan, &PropertyGraph::new()).unwrap();
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        assert!(sql.to_lowercase().contains("unnest"), "{sql}");
        assert_ne!(sql, "SELECT true AS \"result\"");
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap();
        assert_eq!(result.batch.num_rows(), 1);
        assert_eq!(arrow::util::display::array_value_to_string(result.batch.column(0), 0).unwrap(), "true");
    }).await;
    let plan = CypherPlanner::new().plan(&parse_query("RETURN 1 + 2 AS result").unwrap()).unwrap();
    let lowered = RelBackend::new().lower(&plan,&PropertyGraph::new()).unwrap();
    assert_eq!(unparse(&lowered,SqlDialect::DuckDb).unwrap(),"SELECT 3 AS \"result\"");
}
