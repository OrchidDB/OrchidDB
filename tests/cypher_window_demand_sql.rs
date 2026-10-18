#![cfg(feature = "duckdb")]
use orchiddb::ir::{
    catalog::PropertyGraph,
    diagnostics::without_constant_folding,
    rel::{
        RelBackend,
        sql::{DuckDbExecutor, SqlDialect, execute_lowered_sql, unparse},
    },
};

#[tokio::test]
async fn collection_order_demand_and_singleton_quantifiers() {
    without_constant_folding(async {
        for (query, expected, window) in [
            ("UNWIND [true,false,null] AS x WITH collect(x) AS xs RETURN all(y IN xs WHERE y) AS a, any(y IN xs WHERE y) AS b", "false|true", false),
            ("UNWIND [null,null] AS x WITH collect(x) AS xs RETURN all(y IN xs WHERE y) AS a, any(y IN xs WHERE y) AS b", "true|false", false),
            ("UNWIND [true,false] AS x WITH collect(x) AS xs WITH xs AS renamed RETURN any(y IN renamed WHERE y)", "true", false),
            ("RETURN none(x IN [] WHERE x)", "true", false),
            ("RETURN single(x IN [20.0,3.4,50.2,-2.1] WHERE x = 2.1)", "false", false),
            ("UNWIND [null,1,null] AS x RETURN collect(DISTINCT x)", "[1]", false),
            ("UNWIND [true,false] AS x WITH x ORDER BY x DESC WITH collect(x) AS xs RETURN xs, any(y IN xs WHERE y)", "[true, false]|true", true),
            ("UNWIND [true,false] AS x WITH x ORDER BY x DESC WITH collect(x) AS xs RETURN xs[0], any(y IN xs WHERE y)", "true|true", true),
        ] {
            let plan = orchiddb::language::cypher::preparation::prepare(query, &Default::default(), Some(&Default::default())).unwrap();
            let lowered = RelBackend::new().with_language_functions(true).lower(&plan,&PropertyGraph::new()).unwrap();
            let sql = unparse(&lowered,SqlDialect::DuckDb).unwrap();
            assert_eq!(sql.to_lowercase().contains("row_number"),window,"{query}\n{sql}");
            let result=execute_lowered_sql(&mut DuckDbExecutor::default(),&lowered).await.unwrap_or_else(|e|panic!("{query}: {e}\n{sql}"));
            assert_eq!(result.batch.num_rows(),1,"{query}");
            let values=result.batch.columns().iter().map(|a|arrow::util::display::array_value_to_string(a,0).unwrap()).collect::<Vec<_>>().join("|");
            assert_eq!(values,expected,"{query}\n{sql}");
        }
    }).await;
}

#[tokio::test]
async fn terminal_distinct_and_duplicate_quantifier_rows() {
    without_constant_folding(async {
        for (query, expected, no_windows) in [
            (
                "UNWIND [1,1,2,null,null] AS x RETURN DISTINCT x ORDER BY x",
                vec!["1", "2", ""],
                true,
            ),
            (
                "UNWIND [0,2,1,2,0,1] AS x WITH DISTINCT x ORDER BY x DESC LIMIT 1 RETURN x",
                vec!["2"],
                true,
            ),
            (
                "UNWIND [[true,false],[true,false],null,[]] AS xs RETURN any(x IN xs WHERE x)",
                vec!["true", "true", "", "false"],
                false,
            ),
        ] {
            let plan = orchiddb::language::cypher::preparation::prepare(
                query,
                &Default::default(),
                Some(&Default::default()),
            )
            .unwrap();
            let lowered = RelBackend::new()
                .with_language_functions(true)
                .lower(&plan, &PropertyGraph::new())
                .unwrap();
            let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
            if no_windows {
                assert!(!sql.to_lowercase().contains("row_number"), "{query}\n{sql}");
            }
            let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
                .await
                .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
            let actual = (0..result.batch.num_rows())
                .map(|i| {
                    arrow::util::display::array_value_to_string(result.batch.column(0), i).unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{query}\n{sql}");
        }
    })
    .await;
}
