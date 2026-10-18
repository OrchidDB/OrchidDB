#![cfg(feature = "duckdb")]
use orchiddb::ir::{
    catalog::PropertyGraph,
    rel::{
        RelBackend,
        sql::{DuckDbExecutor, execute_lowered_sql},
    },
};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

async fn sql_rows(query: &str) -> Vec<String> {
    let graph = PropertyGraph::new();
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    // This backend leaves native value functions disabled: success proves the
    // actual primitive SQL path, rather than an engine runtime fallback.
    let lowered = RelBackend::new()
        .lower(&plan, &graph)
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    (0..result.batch.num_rows())
        .map(|i| {
            result
                .batch
                .columns()
                .iter()
                .map(|a| {
                    arrow::util::display::array_value_to_string(a, i)
                        .unwrap()
                        .replace(", ", ",")
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

#[tokio::test]
async fn primitive_conversions_keep_cypher_domains_and_precision() {
    for (query, expected) in [
        (
            "UNWIND [true,false,null] AS x RETURN toBoolean(x)",
            vec!["true", "false", ""],
        ),
        (
            "UNWIND [0,1,-2,null] AS x RETURN toBoolean(x)",
            vec!["false", "true", "true", ""],
        ),
        (
            "UNWIND ['TRUE',' false ','yes','1','','tru',null] AS x RETURN toBoolean(x)",
            vec!["true", "false", "", "", "", "", ""],
        ),
        (
            "UNWIND ['\u{00a0}TrUe\u{3000}','\tfalse\n'] AS x RETURN toBoolean(x)",
            vec!["true", "false"],
        ),
        (
            "UNWIND [82.9,-82.9,2.9,-2.9,null] AS x RETURN toInteger(x)",
            vec!["82", "-82", "2", "-2", ""],
        ),
        (
            "UNWIND [9007199254740993,9223372036854775807] AS x RETURN toInteger(x)",
            vec!["9007199254740993", "9223372036854775807"],
        ),
        (
            "UNWIND [1e30,-1e30] AS x RETURN toInteger(x)",
            vec!["9223372036854775807", "-9223372036854775808"],
        ),
        (
            "UNWIND [true,false,null] AS x RETURN toInteger(x)",
            vec!["1", "0", ""],
        ),
        (
            "WITH '0x2a' AS x,'9007199254740993' AS y,'2.9' AS z RETURN toInteger(x),toInteger(y),toInteger(z)",
            vec!["42|9007199254740993|2"],
        ),
        (
            "UNWIND ['2.9','-2e2','1_2','foo','', 'NaN', 'Infinity','1e999',null] AS x RETURN toFloat(x)",
            vec!["2.9", "-200.0", "12.0", "", "", "", "", "", ""],
        ),
    ] {
        assert_eq!(sql_rows(query).await, expected, "{query}");
    }
}

#[tokio::test]
async fn homogeneous_list_addition_preserves_nulls_order_and_duplicates() {
    for (query, expected) in [
        (
            "WITH [1,2,1] AS a,[3,1] AS b RETURN a+b",
            vec!["[1,2,1,3,1]"],
        ),
        (
            "WITH [1,2] AS a,3 AS b RETURN a+b,b+a",
            vec!["[1,2,3]|[3,1,2]"],
        ),
        (
            "WITH [1,null,2] AS a,[null,3] AS b RETURN a+b",
            vec!["[1,,2,,3]"],
        ),
        (
            "WITH [] AS a,[1,2] AS b RETURN a+b,b+a",
            vec!["[1,2]|[1,2]"],
        ),
        ("WITH [1,2] AS a,null AS b RETURN a+b,b+a", vec!["|"]),
    ] {
        assert_eq!(sql_rows(query).await, expected, "{query}");
    }
}

#[tokio::test]
async fn negative_contains_alias_retains_three_valued_logic() {
    assert_eq!(
        sql_rows("UNWIND ['abc','Abc',null] AS x RETURN NOT x CONTAINS 'a'").await,
        ["false", "true", ""]
    );
    assert_eq!(
        sql_rows("UNWIND ['abc','Abc',null] AS x WITH x WHERE NOT x CONTAINS null RETURN count(*)")
            .await,
        ["0"]
    );
}

#[test]
fn invalid_conversion_domains_and_mixed_lists_keep_runtime_boundary() {
    for query in [
        "WITH 1.5 AS x RETURN toBoolean(x)",
        "WITH true AS x RETURN toFloat(x)",
        "WITH [1] AS x RETURN toInteger(x)",
        "WITH [1] AS a,[1.5] AS b RETURN a+b",
        "WITH 1 AS x RETURN NOT x CONTAINS '1'",
    ] {
        let plan = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        assert!(
            RelBackend::new()
                .lower(&plan, &PropertyGraph::new())
                .is_err(),
            "{query}"
        );
    }
}
