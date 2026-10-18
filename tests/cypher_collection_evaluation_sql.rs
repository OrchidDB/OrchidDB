#![cfg(feature = "duckdb")]

use orchiddb::ir::{
    catalog::PropertyGraph,
    rel::{
        RelBackend,
        sql::{DuckDbExecutor, execute_lowered_sql},
    },
};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

#[tokio::test]
async fn filtered_size_uses_plain_sql_and_preserves_scope_and_occurrences() {
    for (query, expected) in [
        ("UNWIND [3,1,2] AS k WITH k,[1,2,3] AS xs ORDER BY k DESC WITH k,size([x IN xs WHERE x<k]) AS n RETURN collect(k) AS ks", vec!["[3, 2, 1]"]),
        (
            "WITH [3,1,2,null] AS xs RETURN size([x IN xs WHERE x < 3]) AS n",
            vec!["2"],
        ),
        (
            "WITH [] AS xs RETURN size([x IN xs WHERE x = 1]) AS n",
            vec!["0"],
        ),
        (
            "WITH [1,2,3] AS xs RETURN size([x IN xs WHERE null]) AS n",
            vec!["0"],
        ),
        (
            "WITH 9 AS x, [1,2,3] AS xs RETURN x, size([x IN xs WHERE x < 3]) AS n",
            vec!["9|2"],
        ),
        (
            "UNWIND [1,1,2] AS k WITH k,[1,2,3] AS xs RETURN k,size([x IN xs WHERE x <= k]) AS n ORDER BY k",
            vec!["1|1", "1|1", "2|2"],
        ),
        (
            "WITH [true,false] AS xs UNWIND xs AS value RETURN value,size([x IN xs WHERE x < value]) AS n ORDER BY value",
            vec!["false|0", "true|1"],
        ),
        (
            "WITH [1,2,3] AS xs RETURN size([x IN xs WHERE x < 2]) AS a,size([x IN xs WHERE x > 1]) AS b",
            vec!["1|2"],
        ),
    ] {
        let graph = PropertyGraph::new();
        let ir = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        let lowered = RelBackend::new()
            .with_language_functions(true)
            .lower(&ir, &graph)
            .unwrap_or_else(|error| panic!("{query}: {error}"));
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await
            .unwrap_or_else(|error| panic!("{query}: {error}"));
        let actual = (0..result.batch.num_rows())
            .map(|row| {
                result
                    .batch
                    .columns()
                    .iter()
                    .map(|array| arrow::util::display::array_value_to_string(array, row).unwrap())
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{query}");
    }
}

#[test]
fn volatile_filter_stays_out_of_pure_sql_cardinality_rewrite() {
    let graph = PropertyGraph::new();
    let ir = CypherPlanner::new()
        .plan(
            &parse_query("WITH [1,2,3] AS xs RETURN size([x IN xs WHERE rand() < 0.5]) AS n")
                .unwrap(),
        )
        .unwrap();
    assert!(
        RelBackend::new()
            .with_language_functions(true)
            .lower(&ir, &graph)
            .is_err()
    );
}

#[test]
fn invalid_filter_predicate_and_untyped_null_keep_native_validation() {
    let graph = PropertyGraph::new();
    for query in [
        "WITH [1,2,3] AS xs RETURN size([x IN xs WHERE 1]) AS n",
        "WITH null AS xs RETURN size([x IN xs WHERE x = 1]) AS n",
    ] {
        let ir = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        assert!(
            RelBackend::new()
                .with_language_functions(true)
                .lower(&ir, &graph)
                .is_err(),
            "{query}"
        );
    }
}

#[tokio::test]
async fn dynamic_integer_ranges_are_inclusive_without_endpoint_overflow() {
    for (query, expected) in [
        ("WITH 0 AS start, [1, 2, 500, 1000, 1500] AS stopList, [-1000, -3, -2, -1, 1, 2, 3, 1000] AS stepList UNWIND stopList AS stop UNWIND stepList AS step WITH start,stop,step,range(start,stop,step) AS list WITH start,stop,step,list,sign(stop-start) <> sign(step) AS empty RETURN ALL(ok IN collect((size(list)=0)=empty) WHERE ok) AS okay", vec!["true"]),
        ("UNWIND [-2, 1, 2] AS step RETURN range(0,3,step) AS xs ORDER BY step", vec!["[]", "[0, 1, 2, 3]", "[0, 2]"]),
        ("WITH [2,3] AS stops, [-1,2] AS steps UNWIND stops AS stop UNWIND steps AS step RETURN range(0,stop,step) AS xs ORDER BY stop,step", vec!["[]", "[0, 2]", "[]", "[0, 2]"]),
        (
            "UNWIND [1,3] AS start RETURN range(start,3) AS xs ORDER BY start",
            vec!["[1, 2, 3]", "[3]"],
        ),
        (
            "UNWIND [1,3] AS start RETURN range(start,1,-1) AS xs ORDER BY start",
            vec!["[1]", "[3, 2, 1]"],
        ),
        ("WITH 3 AS start RETURN range(start,1) AS xs", vec!["[]"]),
        (
            "UNWIND [9223372036854775806] AS start RETURN range(start,9223372036854775807) AS xs",
            vec!["[9223372036854775806, 9223372036854775807]"],
        ),
        (
            "UNWIND [-9223372036854775807] AS start RETURN range(start,-9223372036854775808,-1) AS xs",
            vec!["[-9223372036854775807, -9223372036854775808]"],
        ),
        (
            "UNWIND [2,3] AS stop UNWIND range(1,stop) AS n RETURN stop,n ORDER BY stop,n",
            vec!["2|1", "2|2", "3|1", "3|2", "3|3"],
        ),
    ] {
        let graph = PropertyGraph::new();
        let ir = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        let lowered = RelBackend::new()
            .with_language_functions(true)
            .lower(&ir, &graph)
            .unwrap_or_else(|error| panic!("{query}: {error}"));
        if query.contains("stepList") {
            let sql = orchiddb::ir::rel::sql::unparse(&lowered, orchiddb::ir::rel::sql::SqlDialect::DuckDb).unwrap();
            assert_eq!(sql.matches("array_agg(").count(), 1, "collection must be aggregated once: {sql}");
            assert!(sql.contains("ORDER BY"));
        }
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await
            .unwrap_or_else(|error| panic!("{query}: {error}"));
        let actual = (0..result.batch.num_rows())
            .map(|row| {
                result
                    .batch
                    .columns()
                    .iter()
                    .map(|array| arrow::util::display::array_value_to_string(array, row).unwrap())
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{query}");
    }
}

#[tokio::test]
async fn filtered_size_preserves_a_typed_null_list_from_optional_match() {
    use orchiddb::ir::value::Value;
    let graph = PropertyGraph::new();
    graph.insert_node(
        "N",
        std::collections::BTreeMap::from([(
            "xs".into(),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        )]),
    );
    let query = "MATCH (n:N) OPTIONAL MATCH (n)-[:Missing]->(m:N) RETURN size([x IN m.xs WHERE x > 1]) AS n";
    let ir = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    let lowered = RelBackend::new()
        .with_language_functions(true)
        .lower(&ir, &graph)
        .unwrap();
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await
        .unwrap();
    assert_eq!(result.batch.num_rows(), 1);
    assert!(result.batch.column(0).is_null(0));
}

#[test]
fn zero_or_null_dynamic_range_steps_remain_native() {
    for query in ["UNWIND [0,1] AS step RETURN range(0,3,step)", "UNWIND [null,1] AS step RETURN range(0,3,step)"] {
        let ir = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        assert!(RelBackend::new().with_language_functions(true).lower(&ir, &PropertyGraph::new()).is_err(), "{query}");
    }
}

#[tokio::test]
async fn ordered_filtered_size_collect_and_scalar_subquery_execute_together() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    for values in ["[true,false]", "[351,-3,0,3,1]", "[3.1,-2.1,0.0]", "['Sort','order','','!']", "[[2,2],[2,-2],[1,2],[],[1]]"] {
        let query = format!("WITH {values} AS values WITH values,size(values) AS numOfValues UNWIND values AS value WITH size([x IN values WHERE x < value]) AS x,value,numOfValues ORDER BY value WITH numOfValues,collect(x) AS orderedX RETURN orderedX = range(0,numOfValues-1) AS equal");
        let result = engine.cypher(&query).await.unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(result.returned.batch.num_rows(), 1, "{query}");
        assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(), "true", "{query}");
    }
}

#[tokio::test]
async fn ordered_collect_alias_can_be_reused_by_unwind() {
    let query = "UNWIND RANGE(1,2) AS row WITH collect(row) AS rows UNWIND rows AS x RETURN x";
    let ir = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    let lowered = RelBackend::new().with_language_functions(true).lower(&ir, &PropertyGraph::new()).unwrap();
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap();
    assert_eq!(result.batch.num_rows(), 2);
    assert_eq!(arrow::util::display::array_value_to_string(result.batch.column(0),0).unwrap(), "1");
    assert_eq!(arrow::util::display::array_value_to_string(result.batch.column(0),1).unwrap(), "2");
}
