#![cfg(feature = "duckdb")]
use orchiddb::engine::GraphEngine;
use orchiddb::ir::{
    catalog::PropertyGraph,
    rel::{
        RelBackend,
        sql::{DuckDbExecutor, execute_lowered_sql},
    },
};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

async fn sql_scalar(query: &str) -> datafusion::common::ScalarValue {
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    let lowered = RelBackend::new()
        .with_language_functions(true)
        .lower(&plan, &PropertyGraph::new())
        .unwrap_or_else(|error| panic!("lowering failed for {query}: {error}"));
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await
        .unwrap_or_else(|error| panic!("SQL execution failed for {query}: {error}"));
    assert_eq!(result.batch.num_rows(), 1, "{query}");
    datafusion::common::ScalarValue::try_from_array(result.batch.column(0), 0).unwrap()
}

#[tokio::test]
async fn qualified_temporal_constructors_with_null_arguments_lower_without_clocks() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for kind in ["date", "localtime", "time", "localdatetime", "datetime"] {
        for clock in ["transaction", "statement", "realtime"] {
            let query = format!("RETURN {kind}.{clock}(NULL) AS value");
            assert!(sql_scalar(&query).await.is_null(), "{query}");
            let result = engine.cypher(&query).await.unwrap();
            assert_eq!(
                result.stats.cost.native_kernel_calls, 0,
                "{query}: {}",
                result.stats.physical_plan
            );
            assert!(!result.stats.sql_queries.is_empty(), "{query}");
        }
    }
}

#[tokio::test]
async fn explicit_qualified_constructor_arguments_are_deterministic() {
    let scalar = sql_scalar("RETURN date.realtime('1984-10-11') AS value").await;
    assert_eq!(
        orchiddb::ir::functions::domain::storage(&scalar)
            .map(|v| arrow::util::display::array_value_to_string(v.to_array().unwrap().as_ref(), 0).unwrap()).unwrap(),
        "1984-10-11"
    );
    // Constant propagation must not accidentally turn a row-varying argument
    // into a clock or capture an arbitrary row during compilation.
    let query = "UNWIND ['1984-10-11','2000-01-01'] AS text RETURN date.realtime(text)";
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    assert!(
        RelBackend::new()
            .lower(&plan, &PropertyGraph::new())
            .is_err()
    );
}

#[tokio::test]
async fn temporal_transport_does_not_fence_unrelated_primitive_operations() {
    let query = "UNWIND [date('1984-10-11'),date('2000-01-01')] AS d WITH d, 2 AS n RETURN n + 1 AS result ORDER BY result LIMIT 1";
    assert_eq!(sql_scalar(query).await.to_string(), "3");
    let query =
        "UNWIND [[date('1984-10-11'),date('2000-01-01')]] AS dates RETURN size(dates) AS result";
    assert_eq!(sql_scalar(query).await.to_string(), "2");
    let query = "UNWIND [date('1984-10-11')] AS d RETURN d IS NULL AS result";
    assert_eq!(sql_scalar(query).await.to_string(), "false");
}

#[test]
fn temporal_payload_semantics_and_clocks_still_require_runtime() {
    for query in [
        "RETURN date.realtime()",
        "RETURN date.statement()",
        "RETURN date.transaction()",
        "RETURN date()",
        "UNWIND [duration('P1D'),duration('P2D')] AS t RETURN t ORDER BY t",
        "RETURN date.realtime(NULL, NULL)",
    ] {
        // A frontend rejection is also valid for invalid arity/type. A valid
        // runtime-clock or temporal comparison must not become ordinary SQL.
        if let Ok(ast) = parse_query(query) {
            if let Ok(plan) = CypherPlanner::new().plan(&ast) {
                assert!(
                    RelBackend::new()
                        .lower(&plan, &PropertyGraph::new())
                        .is_err(),
                    "{query}"
                );
            }
        }
    }
}

#[tokio::test]
async fn qualified_constructor_keeps_argument_and_arity_errors() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in [
        "RETURN date.realtime('not-a-date')",
        "RETURN date.statement(1)",
        "RETURN date.transaction(NULL, NULL)",
        "RETURN date.realtime(size(1))",
    ] {
        assert!(engine.cypher(query).await.is_err(), "{query}");
    }
}

#[tokio::test]
async fn finite_temporal_order_uses_semantics_and_preserves_payload() {
    for (values, expected) in [
        ("date('2000-01-01'), date('1910-05-06')", "1910-05-06"),
        (
            "localtime('12:00:00.000000002'),localtime('12:00:00.000000001')",
            "localtime|12:00:00.000000001",
        ),
        (
            "time('00:30-01:00'),time('00:30+01:00')",
            "time|00:30+01:00",
        ),
        (
            "localdatetime('2000-01-01T00:00:00.000000002'),localdatetime('2000-01-01T00:00:00.000000001')",
            "localdatetime|2000-01-01T00:00:00.000000001",
        ),
        (
            "datetime('2000-01-01T00:00+01:00'),datetime('1999-12-31T23:00Z')",
            "datetime|1999-12-31T23:00Z",
        ),
        (
            "date({year:1000000}),date({year:-1000000})",
            "date|-1000000-01-01",
        ),
    ] {
        let query = format!("UNWIND [{values}] AS t WITH t ORDER BY t LIMIT 1 RETURN t");
        let scalar = sql_scalar(&query).await;
        assert_eq!(
            orchiddb::ir::functions::domain::storage(&scalar)
                .map(|v| arrow::util::display::array_value_to_string(v.to_array().unwrap().as_ref(), 0).unwrap()).unwrap(),
            expected,
            "{query}"
        );
    }
    // Cypher ASC places null last and DESC first. Identical dictionary values
    // and comparator peers must not remove duplicate input occurrences.
    for (order, first_null) in [("ASC", false), ("DESC", true)] {
        let query = format!(
            "UNWIND [date('2000-01-01'),NULL,date('2000-01-01')] AS t WITH t ORDER BY t {order} LIMIT 1 RETURN t"
        );
        assert_eq!(sql_scalar(&query).await.is_null(), first_null, "{query}");
    }
}
