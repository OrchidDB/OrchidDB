#![cfg(feature = "duckdb")]
use orchiddb::engine::{GraphEngine, QueryResult};
fn rows(result: &QueryResult) -> Vec<String> {
    let batch = &result.returned.batch;
    (0..batch.num_rows())
        .map(|r| {
            batch
                .columns()
                .iter()
                .map(|a| arrow::util::display::array_value_to_string(a, r).unwrap())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}
#[tokio::test]
async fn integer_range_is_a_streaming_sql_table_function() {
    let mut e = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        (
            "UNWIND range(1000000,2000000) AS i WITH i LIMIT 3000 RETURN sum(i)",
            "3004498500",
        ),
        ("UNWIND range(7,-5,-3) AS i RETURN sum(i)", "5"),
        ("UNWIND range(3,1) AS i RETURN count(i)", "0"),
        (
            "UNWIND range(9223372036854775806,9223372036854775807) AS i RETURN count(i)",
            "2",
        ),
    ] {
        let r = e.cypher(query).await.unwrap();
        assert_eq!(rows(&r), [expected], "{query}");
        assert_eq!(
            r.stats.cost.native_kernel_calls, 0,
            "{query}: {:?}\n{}",
            r.stats.cost, r.stats.physical_plan
        );
        assert_eq!(r.stats.cost.sql_output_rows, 1, "{query}");
        assert!(
            r.stats
                .sql_queries
                .iter()
                .any(|q| q.contains("generate_series")),
            "{:?}",
            r.stats.sql_queries
        );
    }
}
#[tokio::test]
async fn element_count_stays_in_sql_and_handles_optional_nulls() {
    let mut e = GraphEngine::in_memory().unwrap();
    e.cypher("CREATE (:P),(:P),(:Q)").await.unwrap();
    for (query, expected) in [
        ("MATCH (n) RETURN count(n)", "3"),
        ("MATCH (n) WITH n.missing AS n RETURN count(n),count(DISTINCT n)","0|0"),
        ("MATCH (n:P) OPTIONAL MATCH (n)-[:R]->(m) RETURN count(DISTINCT m)","0"),
        ("MATCH (n) UNWIND [1,2] AS x RETURN count(DISTINCT n)", "3"),
        (
            "MATCH (n:P) OPTIONAL MATCH (n)-[r:R]->(m) RETURN count(m),count(r),count(n)",
            "0|0|2",
        ),
    ] {
        let r = e.cypher(query).await.unwrap();
        assert_eq!(rows(&r), [expected]);
        assert_eq!(
            r.stats.cost.native_kernel_calls, 0,
            "{query}: {:?}\n{}",
            r.stats.cost, r.stats.physical_plan
        );
        assert_eq!(r.stats.cost.sql_output_rows, 1);
    }
}
#[tokio::test]
async fn quantifiers_preserve_multiple_bindings_and_three_valued_logic() {
    let mut e = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        (
            "WITH 7 AS x,[1,2] AS xs RETURN x,any(x IN xs WHERE x=2)",
            "7|true",
        ),
        (
            "WITH [1,2,null] AS xs RETURN all(x IN xs WHERE x > 0), any(x IN xs WHERE x = 2), none(x IN xs WHERE x = 2), single(x IN xs WHERE x = 2)",
            "|true|false|",
        ),
        (
            "WITH [] AS xs RETURN all(x IN xs WHERE x > 0), any(x IN xs WHERE x > 0), none(x IN xs WHERE x > 0), single(x IN xs WHERE x > 0)",
            "true|false|true|false",
        ),
        (
            "UNWIND [1,1,2] AS n WITH n, [1,2] AS xs RETURN n,all(x IN xs WHERE x >= n),any(x IN xs WHERE x = n)",
            "1|true|true;1|true|true;2|false|true",
        ),
    ] {
        let r = e.cypher(query).await.unwrap();
        let mut actual = rows(&r);
        actual.sort();
        assert_eq!(actual.join(";"), expected, "{query}");
        assert_eq!(
            r.stats.cost.native_kernel_calls, 0,
            "{query}: {:?}\n{}\n{:?}",
            r.stats.cost, r.stats.physical_plan, r.stats.sql_queries
        );
    }
}
#[tokio::test]
async fn heterogeneous_constant_quantifiers_keep_null_and_empty_semantics() {
    let mut e = GraphEngine::in_memory().unwrap();
    for (list, expected) in [
        (
            "[1,null,true,4.5,'abc',[234,false],{a:null}]",
            "false|true|false|false|",
        ),
        ("[]", "true|false|true|false|false"),
        ("null", "||||"),
    ] {
        let query = format!(
            "WITH {list} AS xs RETURN all(x IN xs WHERE false),any(x IN xs WHERE true),none(x IN xs WHERE true),single(x IN xs WHERE true),single(x IN xs WHERE null)"
        );
        let r = e.cypher(&query).await.unwrap();
        assert_eq!(rows(&r), [expected], "{query}");
    }
}

#[tokio::test]
async fn fused_mixed_collection_kernels_preserve_order_and_shadowing() {
    let mut e = GraphEngine::in_memory().unwrap();
    let query = "WITH 42 AS x, [1,true,'z',null] AS xs UNWIND xs AS item RETURN x,item,any(x IN xs WHERE x = item)";
    let r = e.cypher(query).await.unwrap();
    assert_eq!(rows(&r), ["42|1|true", "42|true|true", "42|z|true", "42||"]);
    assert!(r.stats.physical_plan.contains("Fused("), "{}", r.stats.physical_plan);
}

#[tokio::test]
async fn scalar_collect_lowers_with_order_null_and_empty_semantics() {
    let mut e = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        ("UNWIND [3,null,1,3] AS x RETURN collect(x)", "[3,1,3]"),
        ("UNWIND [3,1,2] AS x WITH x ORDER BY x DESC RETURN collect(x)", "[3,2,1]"),
        ("UNWIND [3,1,2] AS x WITH x ORDER BY x DESC WITH x * 10 AS y RETURN collect(y)", "[30,20,10]"),
        ("UNWIND [3,1,2] AS x WITH x ORDER BY x DESC LIMIT 2 RETURN collect(x)", "[3,2]"),
        ("UNWIND [3,null,1,2] AS x WITH x ORDER BY x DESC LIMIT 2 RETURN collect(x)", "[3]"),
        ("UNWIND [3,1,2] AS x WITH x ORDER BY x DESC WITH x AS y WHERE y > 1 RETURN collect(y)", "[3,2]"),
        ("UNWIND [null,null] AS x RETURN collect(x)", "[]"),
        ("UNWIND [] AS x RETURN collect(x)", "[]"),
        ("UNWIND [1,2,3] AS x RETURN collect(x),collect(x > 1)", "[1,2,3]|[False,True,True]"),
        ("UNWIND [1,2,3] AS x WITH x WHERE false RETURN collect(x), count(*)", "[]|0"),
        ("UNWIND [1,2,3] AS x WITH collect(x) AS xs RETURN all(x IN xs WHERE x > 0)", "true"),
    ] {
        let result = e.cypher(query).await.unwrap();
        assert_eq!(rows(&result), [expected], "{query}\n{:?}", result.stats.sql_queries);
        assert_eq!(result.stats.cost.native_kernel_calls, 0, "{query}\n{}", result.stats.physical_plan);
    }
    let result = e.cypher("UNWIND [3,1,2,4] AS x WITH x, x % 2 AS g ORDER BY x DESC RETURN g,collect(x) ORDER BY g").await.unwrap();
    assert_eq!(rows(&result), ["0|[4,2]", "1|[3,1]"]);
    assert_eq!(result.stats.cost.native_kernel_calls, 0);
}

#[tokio::test]
async fn uniform_union_uses_sql_set_operations_without_coercing_mixed_values() {
    let mut e = GraphEngine::in_memory().unwrap();
    let result = e.cypher("UNWIND [2,1,2] AS x RETURN x UNION UNWIND [2,3] AS x RETURN x").await.unwrap();
    let mut actual = rows(&result);
    actual.sort();
    assert_eq!(actual, ["1", "2", "3"]);
    assert_eq!(result.stats.cost.native_kernel_calls, 0, "{}", result.stats.physical_plan);
    assert!(result.stats.sql_queries.iter().any(|sql| sql.contains("UNION")));
    assert!(!result.stats.sql_queries.iter().any(|sql| sql.to_lowercase().contains("row_number")));
    assert!(e.cypher("RETURN 1 AS x UNION RETURN '1' AS x").await.is_err());
    for query in [
        "RETURN 9007199254740993 AS x UNION RETURN 9007199254740992.0 AS x",
    ] {
        let result = e.cypher(query).await.unwrap();
        assert_eq!(result.returned.batch.num_rows(), 2, "{query}");
        assert!(result.stats.cost.native_kernel_calls > 0, "{query}");
    }
}

#[tokio::test]
async fn literal_quantifiers_need_no_window_or_unnest() {
    let mut e = GraphEngine::in_memory().unwrap();
    for (list, predicate, expected) in [
        ("[]", "true", "true|false|true|false"),
        ("[]", "false", "true|false|true|false"),
        ("[]", "null", "true|false|true|false"),
        ("[null]", "true", "true|true|false|true"),
        ("[null]", "false", "false|false|true|false"),
        ("[null]", "null", "|||"),
        ("[1,null]", "true", "true|true|false|false"),
        ("[1,null]", "false", "false|false|true|false"),
        ("[1,null]", "null", "|||"),
    ] {
        let query = format!("WITH {list} AS xs RETURN all(x IN xs WHERE {predicate}),any(x IN xs WHERE {predicate}),none(x IN xs WHERE {predicate}),single(x IN xs WHERE {predicate})");
        let result = e.cypher(&query).await.unwrap();
        assert_eq!(rows(&result), [expected], "{query}");
        assert_eq!(result.stats.cost.native_kernel_calls, 0, "{query}\n{}", result.stats.physical_plan);
        assert!(!result.stats.sql_queries.iter().any(|sql| {
            let sql = sql.to_lowercase(); sql.contains("row_number") || sql.contains("unnest")
        }), "{query}\n{:?}", result.stats.sql_queries);
    }
}

#[tokio::test]
async fn collection_cardinality_uses_count_without_building_a_list() {
    let mut e = GraphEngine::in_memory().unwrap();
    e.cypher("CREATE (:P),(:P)").await.unwrap();
    for (query, expected) in [
        ("MATCH (n:P) RETURN size(collect(n))", "2"),
        ("MATCH (n:P) UNWIND [1,2] AS i RETURN size(collect(DISTINCT n))", "2"),
        ("MATCH (n:Missing) RETURN size(collect(n))", "0"),
        ("UNWIND [1,null,1,2] AS n RETURN size(collect(n)),size(collect(DISTINCT n))", "3|2"),
    ] {
        let result = e.cypher(query).await.unwrap();
        assert_eq!(rows(&result), [expected], "{query}");
        assert_eq!(result.stats.cost.native_kernel_calls, 0, "{query}\n{}", result.stats.physical_plan);
        assert!(!result.stats.sql_queries.iter().any(|sql| sql.to_lowercase().contains("array_agg")), "{query}");
    }
}

#[tokio::test]
async fn collected_properties_do_not_coerce_scalar_types() {
    let mut e = GraphEngine::in_memory().unwrap();
    e.cypher("CREATE (:P {v:1}),(:P {v:'1'})").await.unwrap();
    for query in [
        "MATCH (n:P) RETURN collect(n.v)",
        "MATCH (n:P) WITH n.v AS x RETURN collect(x)",
        "MATCH (n:P) RETURN collect(DISTINCT n.v)",
    ] {
        let result = e.cypher(query).await.unwrap();
        let typed: serde_json::Value = serde_json::from_str(&result.returned.batch.schema().metadata()["orchiddb.cypher.typed_rows.v1"]).unwrap();
        let items = typed[0][0]["value"].as_array().unwrap();
        assert_eq!(items.len(), 2);
        assert_ne!(items[0]["type"], items[1]["type"], "{query}: {typed}");
        assert!(result.stats.cost.native_kernel_calls > 0);
    }
}

#[tokio::test]
async fn absent_quantifier_items_do_not_evaluate_the_predicate() {
    let mut e = GraphEngine::in_memory().unwrap();
    for (list, expected) in [("[]", "true|false|true|false"), ("null", "|||")] {
        let query = format!("RETURN all(x IN {list} WHERE 1 / 0 > 0),any(x IN {list} WHERE 1 / 0 > 0),none(x IN {list} WHERE 1 / 0 > 0),single(x IN {list} WHERE 1 / 0 > 0)");
        let result = e.cypher(&query).await.unwrap();
        assert_eq!(rows(&result), [expected]);
        assert_eq!(result.stats.cost.native_kernel_calls, 0);
        assert!(!result.stats.sql_queries.iter().any(|sql| sql.to_lowercase().contains("row_number")));
    }
}

#[tokio::test]
async fn distinct_scalar_collect_preserves_order_groups_and_sibling_aggregates() {
    let mut e = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        ("UNWIND [3,null,1,3,2] AS x RETURN collect(DISTINCT x),count(*)", "[3,1,2]|5"),
        ("UNWIND [3,1,3,2] AS x WITH x ORDER BY x DESC RETURN collect(DISTINCT x),collect(x)", "[3,2,1]|[3,3,2,1]"),
        ("UNWIND [3,1,3,2] AS x WITH x, x % 2 AS y ORDER BY x DESC RETURN collect(DISTINCT x),collect(DISTINCT y),count(*)", "[3,2,1]|[1,0]|4"),
        ("UNWIND [4,1,3,2] AS x WITH x % 2 AS v, x ORDER BY x DESC WITH v RETURN collect(DISTINCT v)", "[0,1]"),
        ("UNWIND [null,null] AS x RETURN collect(DISTINCT x),count(*)", "[]|2"),
        ("UNWIND [] AS x RETURN collect(DISTINCT x),count(*)", "[]|0"),
    ] {
        let result = e.cypher(query).await.unwrap();
        assert_eq!(rows(&result), [expected], "{query}\n{:?}", result.stats.sql_queries);
        assert_eq!(result.stats.cost.native_kernel_calls, 0, "{query}\n{}", result.stats.physical_plan);
    }
    let result = e.cypher("UNWIND [3,1,3,2,2] AS x WITH x, x % 2 AS g ORDER BY x DESC RETURN g,collect(DISTINCT x),count(*) ORDER BY g").await.unwrap();
    assert_eq!(rows(&result), ["0|[2]|2", "1|[3,1]|3"]);
    assert_eq!(result.stats.cost.native_kernel_calls, 0);
    e.cypher("CREATE (:P {g:1}),(:P {g:1}),(:P {g:2,v:3})").await.unwrap();
    let result = e.cypher("MATCH (n:P) RETURN n.g AS g,collect(DISTINCT n.v),count(*) ORDER BY g").await.unwrap();
    assert_eq!(rows(&result), ["1|[]|2", "2|[3]|1"]);
    assert_eq!(result.stats.cost.native_kernel_calls, 0);
}

#[tokio::test]
async fn deterministic_slice_bounds_lower_and_invalid_bounds_still_fail() {
    let mut e = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        ("UNWIND [3,1,2] AS x RETURN x ORDER BY x LIMIT toInteger(ceil(1.7))", vec!["1", "2"]),
        ("UNWIND [3,1,2] AS x RETURN x ORDER BY x SKIP toInteger(ceil(1.7))", vec!["3"]),
        ("UNWIND [3,1,2] AS x RETURN x ORDER BY x SKIP 1 + 0 LIMIT 1 + 0", vec!["2"]),
    ] {
        let result = e.cypher(query).await.unwrap();
        assert_eq!(rows(&result), expected, "{query}");
        assert_eq!(result.stats.cost.native_kernel_calls, 0, "{query}\n{}", result.stats.physical_plan);
    }
    assert!(e.cypher("UNWIND [] AS x RETURN x LIMIT toInteger(-1)").await.is_err());
    assert!(e.cypher("UNWIND [1] AS x RETURN x LIMIT toInteger(-1)").await.is_err());
}
