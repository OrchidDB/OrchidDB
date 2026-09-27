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
