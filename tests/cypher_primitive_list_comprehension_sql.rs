#![cfg(feature = "duckdb")]
use orchiddb::ir::{
    catalog::PropertyGraph,
    diagnostics::without_constant_folding,
    rel::{
        RelBackend,
        sql::{DuckDbExecutor, SqlDialect, execute_lowered_sql, unparse},
    },
};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
async fn run(query: &str) -> (String, Vec<String>) {
    without_constant_folding(async {
        let plan = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        let lowered = RelBackend::new()
            .with_language_functions(true)
            .lower(&plan, &PropertyGraph::new())
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
        let rows = (0..result.batch.num_rows())
            .map(|i| {
                result
                    .batch
                    .columns()
                    .iter()
                    .map(|a| arrow::util::display::array_value_to_string(a, i).unwrap())
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect();
        (sql, rows)
    })
    .await
}
#[tokio::test]
async fn identity_filter_preserves_order_duplicates_empty_null_and_unknown_predicates() {
    let (sql, mut rows) = run(
        "UNWIND [[3,2,1,2,null],[],null,[null]] AS xs RETURN [x IN xs WHERE x % 2 = 0] AS result",
    )
    .await;
    rows.sort();
    assert_eq!(rows, vec!["", "[2, 2]", "[]", "[]"]);
    assert!(sql.contains("% 2"), "{sql}");
    assert!(sql.contains("ORDER BY"), "{sql}");
    assert!(!sql.to_lowercase().contains("row_number"), "{sql}");
}
#[tokio::test]
async fn maps_keep_null_elements_and_source_nullability() {
    let (_, mut rows) =
        run("UNWIND [[3,2,1,null,2],[],null] AS xs RETURN [x IN xs | x % 2] AS result").await;
    rows.sort();
    assert_eq!(rows, vec!["", "[1, 0, 1, , 0]", "[]"]);
    let (_, rows) =
        run("WITH [3,null,1,null] AS xs RETURN [x IN xs WHERE x IS NULL | x] AS result").await;
    assert_eq!(rows, vec!["[, ]"]);
}
#[tokio::test]
async fn outer_bindings_shadowing_and_duplicate_outer_rows() {
    for item in ["x", "xs", "__comprehension_index", "__comprehension_item"] {
        let query = format!(
            "UNWIND [2,2,3] AS k WITH k,[3,2,1,2,null] AS xs RETURN k,[{item} IN xs WHERE {item} < k | {item}] AS result"
        );
        let (_, mut rows) = run(&query).await;
        rows.sort();
        assert_eq!(rows, vec!["2|[1]", "2|[1]", "3|[2, 1, 2]"], "{query}");
    }
}
#[tokio::test]
async fn primitive_strings_booleans_and_modulo_minimum_integer() {
    assert_eq!(
        run("WITH ['b',null,'a','b'] AS xs RETURN [x IN xs WHERE x = 'b'] AS result")
            .await
            .1,
        vec!["[b, b]"]
    );
    assert_eq!(
        run("WITH [true,null,false,true] AS xs RETURN [x IN xs | x IS NULL] AS result")
            .await
            .1,
        vec!["[false, true, false, false]"]
    );
    assert_eq!(
        run(
            "WITH [-9223372036854775808,2,3,null] AS xs RETURN [x IN xs WHERE x % 2 = 0] AS result"
        )
        .await
        .1,
        vec!["[-9223372036854775808, 2]"]
    );
}

#[tokio::test]
async fn effectful_throwing_and_nested_correlations_keep_runtime_fallback() {
    without_constant_folding(async {
        for query in [
            "WITH [1,2] AS xs RETURN [x IN xs | rand()] AS result",
            "WITH [rand()] AS xs RETURN [x IN xs WHERE x > 0] AS result",
            "WITH [1,2] AS xs RETURN [x IN xs WHERE x % 0 = 0] AS result",
            "WITH [-9223372036854775808] AS xs RETURN [x IN xs WHERE x % -1 = 0] AS result",
            "WITH [9223372036854775807] AS xs RETURN [x IN xs | x + 1] AS result",
            "WITH [1] AS xs, [1,2] AS ys RETURN [x IN xs WHERE ys = ys] AS result",
        ] {
            let plan = CypherPlanner::new()
                .plan(&parse_query(query).unwrap())
                .unwrap();
            assert!(
                RelBackend::new()
                    .with_language_functions(true)
                    .lower(&plan, &PropertyGraph::new())
                    .is_err(),
                "{query}"
            );
        }
    })
    .await;
}
