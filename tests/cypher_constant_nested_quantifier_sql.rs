#![cfg(feature = "duckdb")]
use orchiddb::{engine::GraphEngine, ir::{catalog::PropertyGraph, rel::{RelBackend, sql::{SqlDialect, unparse}}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

#[tokio::test]
async fn closed_nested_quantifiers_emit_and_execute_scalar_literals() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (expression, expected) in [
        ("(single(x IN [1,2,3,4,5,6,7,8,9] WHERE x % 2 = 0) OR all(x IN [1,2,3,4,5,6,7,8,9] WHERE x % 2 = 0)) <= any(x IN [1,2,3,4,5,6,7,8,9] WHERE x % 2 = 0)", Some(true)),
        ("any(x IN [1,null] WHERE x = 2)", None),
        ("all(x IN [2,null,3] WHERE x = 2)", Some(false)),
        ("none(x IN [] WHERE x % 2 = 0)", Some(true)),
        ("single(x IN [-2,3] WHERE abs(x) = 2)", Some(true)),
        ("single(x IN [['abc'], ['abc', 'def']] WHERE single(y IN x WHERE y = 'def'))", Some(true)),
        ("single(x IN [[null], [true]] WHERE single(y IN x WHERE y))", None),
        ("single(x IN [[true], [true], [null]] WHERE single(y IN x WHERE y))", Some(false)),
        ("all(x IN [[null], [false]] WHERE any(y IN x WHERE y))", Some(false)),
        ("any(x IN [[null], [true]] WHERE any(y IN x WHERE y))", Some(true)),
        ("none(x IN [[null], [false]] WHERE any(y IN x WHERE y))", None),
        ("all(x IN [] WHERE single(y IN x WHERE y))", Some(true)),
        ("single(x IN [] WHERE single(y IN x WHERE y))", Some(false)),
        ("single(x IN null WHERE single(y IN x WHERE y))", None),
        ("all(x IN [[true], [false]] WHERE single(x IN x WHERE x))", Some(false)),
        ("all(x IN [[true]] WHERE single(y IN x WHERE y AND all(z IN [y] WHERE z)))", Some(true)),
        ("all(x IN [[true], [null]] WHERE single(y IN x WHERE y IS NULL OR NOT y))", Some(false)),
    ] {
        let query = format!("RETURN {expression} AS result");
        let plan = CypherPlanner::new().plan(&parse_query(&query).unwrap()).unwrap();
        assert!(!(format!("{plan:?}").contains("GraphApply") || format!("{plan:?}").contains("GraphQuantifier")), "{query}: {plan:?}");
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &PropertyGraph::new()).unwrap();
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        let literal = expected.map(|value| value.to_string()).unwrap_or("NULL".into());
        assert_eq!(sql, format!("SELECT {literal} AS \"result\""), "{query}");
        let result = engine.cypher(&query).await.unwrap();
        assert_eq!(result.returned.batch.num_rows(),1,"{query}");
        let value = result.returned.batch.column(0);
        if let Some(expected) = expected {
            assert_eq!(arrow::util::display::array_value_to_string(value,0).unwrap(),expected.to_string(),"{query}");
        } else {
            assert!(value.is_null(0),"{query}");
        }
    }
}

#[test]
fn effects_outer_bindings_and_invalid_literals_are_not_folded_away() {
    for query in [
        "RETURN single(x IN [[1]] WHERE single(y IN x WHERE rand() < 0.5)) AS result",
        "RETURN single(x IN [[1]] WHERE single(y IN x WHERE 1 / 0 = y)) AS result",
        "UNWIND [true,false] AS outside RETURN single(x IN [[true],[false]] WHERE single(y IN x WHERE y = outside)) AS result",
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        assert!((format!("{plan:?}").contains("GraphApply") || format!("{plan:?}").contains("GraphQuantifier")),"{query}: {plan:?}");
    }
    let query = "RETURN all(x IN [] WHERE single(y IN [9223372036854775808] WHERE true)) AS result";
    assert!(CypherPlanner::new().plan(&parse_query(query).unwrap()).is_err());
}

#[tokio::test]
async fn folded_expression_preserves_upstream_row_multiplicity() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine.cypher("UNWIND [1,1,2] AS occurrence RETURN occurrence,single(x IN [[true],[false]] WHERE single(y IN x WHERE y)) AS result").await.unwrap();
    assert_eq!(result.returned.batch.num_rows(),3);
    for row in 0..3 {
        assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(1),row).unwrap(),"true");
    }
}

#[tokio::test]
async fn with_constant_lists_and_checked_arithmetic_collapse_long_nested_queries() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for predicate in ["x + y < 5", "x % y = 1", "x <= y + 1", "2 * x + y > 25"] {
        let query = format!("WITH [1,2,3,4,5,6,7,8,9] AS list RETURN single(x IN list WHERE single(y IN list WHERE {predicate})) AS result");
        let expected = (1_i64..=9).filter(|x| (1_i64..=9).filter(|y| match predicate {
            "x + y < 5" => x + y < 5,
            "x % y = 1" => x % y == 1,
            "x <= y + 1" => *x <= y + 1,
            _ => 2 * x + y > 25,
        }).count() == 1).count() == 1;
        let plan = CypherPlanner::new().plan(&parse_query(&query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan,&PropertyGraph::new()).unwrap();
        assert_eq!(unparse(&lowered,SqlDialect::DuckDb).unwrap(),format!("SELECT {expected} AS \"result\""),"{query}");
        let result = engine.cypher(&query).await.unwrap();
        assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(),expected.to_string(),"{query}");
    }
}

#[tokio::test]
async fn with_alias_scopes_rebindings_and_empty_inputs_keep_their_meaning() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (prefix,expected,rows) in [
        ("WITH [[true],[false]] AS old WITH old AS list", "true",1),
        ("WITH [[false]] AS list WITH [[true],[false]] AS list", "true",1),
        ("WITH [[true]] AS list WITH [[false]] AS list,list AS old WITH old AS list", "true",1),
        ("WITH [[true],[false]] AS list UNWIND [1,1,2] AS occurrence", "true",3),
        ("WITH [[true],[false]] AS list UNWIND [] AS occurrence", "true",0),
        ("WITH [[true],[false]] AS list WITH list WHERE false", "true",0),
    ] {
        let query = format!("{prefix} RETURN single(x IN list WHERE single(y IN x WHERE y)) AS result");
        let result = engine.cypher(&query).await.unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(result.returned.batch.num_rows(),rows,"{query}");
        for row in 0..rows {
            assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),row).unwrap(),expected,"{query}");
        }
    }
    let query = "WITH [[true],[false]] AS list UNWIND [[[false]],[[true]]] AS candidate WITH candidate AS list RETURN single(x IN list WHERE single(y IN x WHERE y)) AS result";
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    assert!((format!("{plan:?}").contains("GraphApply") || format!("{plan:?}").contains("GraphQuantifier")));
    let result = engine.cypher(query).await.unwrap();
    assert_eq!(result.returned.batch.num_rows(),2);
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(),"false");
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),1).unwrap(),"true");
    assert!(CypherPlanner::new().plan(&parse_query("WITH [[true]] AS list WITH 1 AS unrelated RETURN single(x IN list WHERE single(y IN x WHERE y)) AS result").unwrap()).is_err());
}

#[test]
fn arithmetic_errors_and_computed_aliases_remain_runtime_work() {
    for query in [
        "WITH [0] AS list RETURN single(x IN list WHERE single(y IN list WHERE 1 / y = 1)) AS result",
        "WITH [0] AS list RETURN single(x IN list WHERE single(y IN list WHERE 1 % y = 1)) AS result",
        "WITH [9223372036854775807] AS list RETURN single(x IN list WHERE single(y IN list WHERE x + y > 0)) AS result",
        "WITH [1e308] AS list RETURN single(x IN list WHERE single(y IN list WHERE x * y > 0)) AS result",
        "WITH 1 / 0 AS danger RETURN single(x IN [[true]] WHERE single(y IN x WHERE danger = y)) AS result",
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        assert!((format!("{plan:?}").contains("GraphApply") || format!("{plan:?}").contains("GraphQuantifier")),"{query}");
    }
}

#[tokio::test]
async fn nested_absolute_values_fold_without_losing_numeric_or_null_semantics() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (prefix, expression, expected) in [
        ("WITH [1,2,3,4,5,6,7,8,9] AS list ", "none(x IN list WHERE single(y IN list WHERE abs(x - y) < 3))", Some(true)),
        ("WITH [1,2,3,4,5,6,7,8,9] AS list ", "all(x IN list WHERE all(y IN list WHERE abs(x - y) < 10))", Some(true)),
        ("", "single(x IN [[-2.5]] WHERE single(y IN x WHERE ABS(y) = 2.5))", Some(true)),
        ("", "single(x IN [[0]] WHERE single(y IN x WHERE abs(y) = 0))", Some(true)),
        ("", "single(x IN [[9223372036854775807]] WHERE single(y IN x WHERE abs(y) = y))", Some(true)),
        ("", "single(x IN [[null]] WHERE single(y IN x WHERE abs(y) = 1))", None),
    ] {
        let query = format!("{prefix}RETURN {expression} AS result");
        let plan = CypherPlanner::new().plan(&parse_query(&query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan,&PropertyGraph::new()).unwrap();
        assert_eq!(unparse(&lowered,SqlDialect::DuckDb).unwrap(),format!("SELECT {} AS \"result\"",expected.map(|v|v.to_string()).unwrap_or("NULL".into())),"{query}");
        let result = engine.cypher(&query).await.unwrap();
        assert_eq!(result.returned.batch.num_rows(),1);
        if let Some(expected) = expected {
            assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(),expected.to_string(),"{query}");
        } else { assert!(result.returned.batch.column(0).is_null(0)); }
    }
}

#[test]
fn abs_overflow_type_errors_and_effects_are_not_folded() {
    for query in [
        "WITH [-9223372036854775808] AS list RETURN all(x IN list WHERE all(y IN list WHERE abs(y) > 0))",
        "RETURN all(x IN [['a']] WHERE all(y IN x WHERE abs(y) > 0))",
        "RETURN all(x IN [[1]] WHERE all(y IN x WHERE abs(rand()) > 0))",
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        assert!((format!("{plan:?}").contains("GraphApply") || format!("{plan:?}").contains("GraphQuantifier")),"{query}");
    }
}

#[test]
fn ordinary_quantifier_folding_preserves_errors_effects_and_dynamic_inputs() {
    for query in [
        "RETURN any(x IN [0] WHERE 1 / x = 1) AS result",
        "RETURN any(x IN [0] WHERE 1 % x = 1) AS result",
        "RETURN any(x IN [9223372036854775807] WHERE x + 1 > 0) AS result",
        "RETURN any(x IN [-9223372036854775808] WHERE abs(x) > 0) AS result",
        "RETURN any(x IN [1] WHERE rand() < 0.5) AS result",
        "UNWIND [1,2] AS outside RETURN any(x IN [1,2] WHERE x = outside) AS result",
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        assert!(format!("{plan:?}").contains("GraphQuantifier"), "{query}: {plan:?}");
    }
    assert!(CypherPlanner::new().plan(&parse_query(
        "RETURN all(x IN [] WHERE x = 9223372036854775808) AS result"
    ).unwrap()).is_err());
}
