#![cfg(feature = "duckdb")]
use orchiddb::engine::{GraphEngine, QueryResult};
fn rows(r: &QueryResult) -> Vec<String> {
    let b = &r.returned.batch;
    (0..b.num_rows())
        .map(|i| {
            b.columns()
                .iter()
                .map(|a| arrow::util::display::array_value_to_string(a, i).unwrap())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

#[tokio::test]
async fn subquery_sql_capability_matrix() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher(
            "CREATE (a:P {k:1}),(b:P {k:2}),(c:P {k:3}),(a)-[:R]->(b),(a)-[:R]->(c),(b)-[:R]->(c)",
        )
        .await
        .unwrap();
    let cases: &[(&str, &[&str])] = &[
        (
            "MATCH (a:P) RETURN a.k, COUNT { MATCH (a)-[:R]->(b) } AS n ORDER BY a.k",
            &["1|2", "2|1", "3|0"],
        ),
        (
            "MATCH (a:P) RETURN a.k, EXISTS { MATCH (a)-[:R]->(b) } AS yes ORDER BY a.k",
            &["1|true", "2|true", "3|false"],
        ),
        (
            "UNWIND [1,1,3] AS x RETURN x, COUNT { MATCH (a:P) WHERE a.k > x } AS n ORDER BY x",
            &["1|2", "1|2", "3|0"],
        ),
        (
            "MATCH (a:P) WHERE NOT EXISTS { MATCH (a)-[:R]->(b) } RETURN a.k",
            &["3"],
        ),
        (
            "MATCH (a:P) RETURN a.k, COUNT { MATCH (a)-[:R]->(b) RETURN b.k ORDER BY b.k DESC LIMIT 1 } AS n ORDER BY a.k",
            &["1|1", "2|1", "3|0"],
        ),
        (
            "MATCH (a:P) RETURN a.k, COUNT { MATCH (a)-[:R]->(b) RETURN count(*) AS n } AS n ORDER BY a.k",
            &["1|1", "2|1", "3|1"],
        ),
        (
            "MATCH (a:P) RETURN a.k, EXISTS { MATCH (a)-[:R]->(b) WHERE EXISTS { MATCH (b)-[:R]->(c) } } AS yes ORDER BY a.k",
            &["1|true", "2|false", "3|false"],
        ),
        (
            "WITH 1 AS x RETURN COUNT { UNWIND [1,2,2] AS y RETURN DISTINCT y } AS n, x",
            &["2|1"],
        ),
        (
            "MATCH (a:P) RETURN a.k, COUNT { MATCH (a)-[:R]->(b) RETURN b.k AS k UNION MATCH (a)-[:R]->(b) RETURN b.k AS k } AS n ORDER BY a.k",
            &["1|2", "2|1", "3|0"],
        ),
        (
            "UNWIND [1,1,3] AS x RETURN x, COUNT { MATCH (a:P) WHERE a.k > x RETURN a.k AS k ORDER BY k DESC SKIP 1 LIMIT 1 } AS n ORDER BY x",
            &["1|1", "1|1", "3|0"],
        ),
        (
            "UNWIND [1,1,3] AS x RETURN x, COUNT { MATCH (a:P) WHERE a.k > x RETURN a.k AS k UNION MATCH (a:P) WHERE a.k > x RETURN a.k AS k } AS n ORDER BY x",
            &["1|2", "1|2", "3|0"],
        ),
        (
            "MATCH (a:P) RETURN a.k, COUNT { MATCH (a)-[:R]->(b) RETURN b.k AS k UNION ALL MATCH (a)-[:R]->(b) RETURN b.k AS k } AS n ORDER BY a.k",
            &["1|4", "2|2", "3|0"],
        ),
        (
            "MATCH (a:P) RETURN a.k, EXISTS { MATCH (a)-[:R]->(b) RETURN count(*) AS n } AS yes ORDER BY a.k",
            &["1|true", "2|true", "3|true"],
        ),
        (
            "MATCH (a:P) RETURN a.k, COUNT { MATCH (a)-[:R]->(b) WITH b.k AS k, count(*) AS n WHERE n > 0 RETURN k } AS n ORDER BY a.k",
            &["1|2", "2|1", "3|0"],
        ),
        (
            "MATCH (a:P) RETURN a.k, COUNT { MATCH (a)-[:R]->(b) WHERE COUNT { MATCH (b)-[:R]->(c) } > 0 } AS n ORDER BY a.k",
            &["1|1", "2|0", "3|0"],
        ),
        (
            "MATCH (a:P) RETURN a.k, CASE WHEN EXISTS { MATCH (a)-[:R]->(b) } THEN COUNT { MATCH (a)-[:R]->(b) } ELSE -1 END AS n ORDER BY a.k",
            &["1|2", "2|1", "3|-1"],
        ),
        (
            "MATCH (a:P) RETURN a.k, COUNT { MATCH (a)-[:R]->(b) WITH b.k AS k ORDER BY k DESC LIMIT 1 WITH k WHERE k = 2 RETURN k } AS n ORDER BY a.k",
            &["1|0", "2|0", "3|0"],
        ),
        (
            "MATCH (a:P) RETURN a.k, COUNT { MATCH (a)-[:R]->(b) WITH b.k AS k ORDER BY k ASC LIMIT 1 WITH k WHERE k = 2 RETURN k } AS n ORDER BY a.k",
            &["1|1", "2|0", "3|0"],
        ),
    ];
    let mut failures = Vec::new();
    for (query, expected) in cases {
        match engine.cypher(query).await {
            Err(e) => failures.push(format!("{query}\nERROR: {e}")),
            Ok(r) => {
                if rows(&r) != *expected {
                    failures.push(format!(
                        "{query}\nexpected {expected:?}, got {:?}\n{:?}",
                        rows(&r),
                        r.stats.sql_queries
                    ));
                }
                if r.stats.cost.native_relational_kernel_calls != 0 {
                    failures.push(format!(
                        "{query}\nresidual kernels: {}\n{}",
                        r.stats.cost.native_kernel_calls, r.stats.physical_plan
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
