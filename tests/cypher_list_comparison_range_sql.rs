#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, rel::{RelBackend, sql::{DuckDbExecutor, execute_lowered_sql}}};

async fn rows(query: &str) -> Vec<String> {
    rows_with_options(query, true).await
}

async fn rows_with_options(query: &str, language_functions: bool) -> Vec<String> {
    let plan = orchiddb::language::cypher::preparation::prepare(query, &Default::default(), Some(&Default::default())).unwrap();
    let lowered = RelBackend::new().with_language_functions(language_functions).lower(&plan, &PropertyGraph::new()).unwrap_or_else(|e| panic!("{query}: {e}"));
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap_or_else(|e| panic!("{query}: {e}"));
    (0..result.batch.num_rows()).map(|i| result.batch.columns().iter().map(|a| arrow::util::display::array_value_to_string(a,i).unwrap()).collect::<Vec<_>>().join("|")).collect()
}

#[tokio::test]
async fn generated_list_equality_preserves_ordering_conformance() {
    for query in [
        r#"WITH [true, false] AS values
WITH values, size(values) AS numOfValues
UNWIND values AS value
WITH size([ x IN values WHERE x < value ]) AS x, value, numOfValues
  ORDER BY value
WITH numOfValues, collect(x) AS orderedX
RETURN orderedX = range(0, numOfValues-1) AS equal"#,
        r#"WITH [351, -3974856, 93, -3, 123, 0, 3, -2, 20934587, 1, 20934585, 20934586, -10] AS values
WITH values, size(values) AS numOfValues
UNWIND values AS value
WITH size([ x IN values WHERE x < value ]) AS x, value, numOfValues
  ORDER BY value
WITH numOfValues, collect(x) AS orderedX
RETURN orderedX = range(0, numOfValues-1) AS equal"#,
        r#"WITH [351.5, -3974856.01, -3.203957, 123.0002, 123.0001, 123.00013, 123.00011, 0.0100000, 0.0999999, 0.00000001, 3.0, 209345.87, -10.654] AS values
WITH values, size(values) AS numOfValues
UNWIND values AS value
WITH size([ x IN values WHERE x < value ]) AS x, value, numOfValues
  ORDER BY value
WITH numOfValues, collect(x) AS orderedX
RETURN orderedX = range(0, numOfValues-1) AS equal"#,
        r#"WITH ['Sort', 'order', ' ', 'should', 'be', '', 'consistent', 'with', 'comparisons', ', ', 'where', 'comparisons are', 'defined', '!'] AS values
WITH values, size(values) AS numOfValues
UNWIND values AS value
WITH size([ x IN values WHERE x < value ]) AS x, value, numOfValues
  ORDER BY value
WITH numOfValues, collect(x) AS orderedX
RETURN orderedX = range(0, numOfValues-1) AS equal"#,
        r#"WITH [[2, 2], [2, -2], [1, 2], [], [1], [300, 0], [1, -20], [2, -2, 100]] AS values
WITH values, size(values) AS numOfValues
UNWIND values AS value
WITH size([ x IN values WHERE x < value ]) AS x, value, numOfValues
  ORDER BY value
WITH numOfValues, collect(x) AS orderedX
RETURN orderedX = range(0, numOfValues-1) AS equal"#,
] {
        assert_eq!(rows(query).await, vec!["true"], "{query}");
    }
}

#[tokio::test]
async fn generated_list_equality_preserves_null_and_mismatch() {
    assert_eq!(rows("UNWIND [[0,1],[0,null],[2,null],[],null] AS xs WITH xs, size(xs) AS n RETURN xs = range(0,n-1), xs <> range(0,n-1)").await,
        vec!["true|false", "|", "false|true", "true|false", "|"]);
}

#[test]
fn generated_list_equality_does_not_hide_throwing_bounds() {
    for query in [
        "UNWIND [-9223372036854775808] AS n RETURN [1] = range(0,n-1)",
        "UNWIND [0] AS n RETURN [1] = range(0,1,n)",
        "UNWIND [1] AS n RETURN [1] = range(0,n/0)",
    ] {
        let plan = orchiddb::language::cypher::preparation::prepare(query, &Default::default(), Some(&Default::default())).unwrap();
        assert!(RelBackend::new().with_language_functions(true).lower(&plan, &PropertyGraph::new()).is_err(), "{query}");
    }
}

#[tokio::test]
async fn primitive_list_ordering_stops_at_first_non_equal_pair() {
    assert_eq!(rows("UNWIND [[1,null],[2,null],[null,1],[1],[],[1,2],null] AS xs RETURN xs < [2,null], xs <= [2,null], xs > [2,null], xs >= [2,null]").await,
        vec!["true|true|false|false", "false|true|false|true", "|||", "true|true|false|false", "true|true|false|false", "true|true|false|false", "|||"]);
    assert_eq!(rows("UNWIND [[null,1],[1,null],[1],[1,2],[]] AS xs RETURN xs < [null,2], xs <= [null,2]").await,
        vec!["false|true", "|", "|", "|", "true|true"]);
    assert_eq!(rows("UNWIND [[1],[1,2],[1,2,3],[]] AS xs RETURN xs < [1,2], xs > [1,2]").await,
        vec!["true|false", "false|false", "false|true", "true|false"]);
}

#[tokio::test]
async fn primitive_list_ordering_matches_native_evaluator() {
    use orchiddb::ir::{expr::{IrExpr, Lit}, runtime::{eval, Row}, value::Value};
    let inputs: Vec<Vec<Option<i64>>> = vec![vec![Some(1), None], vec![Some(2), None], vec![None, Some(1)], vec![Some(1)], vec![], vec![Some(1), Some(2)]];
    let list = |items: &[Option<i64>]| IrExpr::List(items.iter().map(|item| IrExpr::Lit(item.map(Lit::Int).unwrap_or(Lit::Null))).collect());
    for (rhs, text) in [(vec![Some(2), None], "[2,null]"), (vec![None, Some(2)], "[null,2]"), (vec![Some(1), Some(2)], "[1,2]")] {
        let expected = inputs.iter().map(|lhs| {
            ["cypher_lt", "cypher_lte", "cypher_gt", "cypher_gte"].into_iter().map(|name| {
                match eval(&IrExpr::Call { name: name.into(), args: vec![list(lhs), list(&rhs)] }, &Row::default(), &PropertyGraph::new()).unwrap() {
                    Value::Bool(v) => v.to_string(), Value::Null => String::new(), other => panic!("unexpected comparison result {other:?}"),
                }
            }).collect::<Vec<_>>().join("|")
        }).collect::<Vec<_>>();
        let query = format!("UNWIND [[1,null],[2,null],[null,1],[1],[],[1,2]] AS xs RETURN xs < {text}, xs <= {text}, xs > {text}, xs >= {text}");
        assert_eq!(rows(&query).await, expected, "{query}");
    }
}

#[tokio::test]
async fn generated_list_comparison_without_language_functions_still_lowers() {
    let query = "WITH [3,1,2] AS values WITH values, size(values) AS n UNWIND values AS value WITH n, value ORDER BY value WITH n, collect(value) AS xs RETURN xs = range(1,n)";
    assert_eq!(rows_with_options(query, false).await, ["true"]);
}

#[tokio::test]
async fn primitive_list_comparison_is_compact_without_constant_folding() {
    use orchiddb::ir::{diagnostics::without_constant_folding, rel::sql::{unparse, SqlDialect}};
    without_constant_folding(async {
        let query = "RETURN [1, null] >= [1] AS result";
        let plan = orchiddb::language::cypher::preparation::prepare(query, &Default::default(), Some(&Default::default())).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &PropertyGraph::new()).unwrap();
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        assert!(sql.contains("__orchiddb_cypher_list_compare"), "{sql}");
        assert!(!sql.to_lowercase().contains("unnest"), "{sql}");
        assert!(sql.len() < 180, "{sql}");
        assert_eq!(rows(query).await, ["true"]);
        // A null in an equal prefix is unknown, while an earlier mismatch wins.
        assert_eq!(rows("RETURN [1,null] = [1,null], [1,null] >= [1,2], [2,null] > [1,2]").await, ["||true"]);
    }).await;
}

#[test]
fn list_comparison_accepts_duckdb_widened_integer_lists_losslessly() {
    let mut executor = DuckDbExecutor::default();
    executor.set_language_functions(true).unwrap();
    let connection = executor.connection().unwrap();
    for (left, right, expected) in [
        ("170141183460469231731687303715884105727", "170141183460469231731687303715884105726", true),
        ("-170141183460469231731687303715884105728", "-170141183460469231731687303715884105727", false),
        ("9007199254740993", "9007199254740992", true),
    ] {
        let sql = format!("SELECT __orchiddb_cypher_list_compare('gt', [CAST('{left}' AS HUGEINT)], [CAST('{right}' AS HUGEINT)])");
        let actual: bool = connection.query_row(&sql, [], |row| row.get(0)).unwrap();
        assert_eq!(actual, expected, "{sql}");
    }
}
