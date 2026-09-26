//! Execute compiler-only SQL against mapped tables; compare to an independent trail enumerator.
#![cfg(feature = "duckdb")]
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

// Type-local IDs deliberately overlap. The first label contains separators.
const EDGES: &[(usize, i64, i64, i64)] = &[
    (0, 1, 1, 2),
    (0, 2, 2, 1),
    (0, 3, 2, 3),
    (0, 4, 2, 2),
    (0, 5, 1, 2),
    (1, 1, 3, 1),
    (1, 2, 2, 3),
];
fn request(query: &str) -> Value {
    json!({"version":1,"dialect":"duckdb","language":"cypher","query":query,
    "tables":[
        {"name":"nodes","columns":[{"name":"id","data_type":"int64"}]},
        {"name":"ea","columns":[{"name":"id","data_type":"int64"},{"name":"src","data_type":"int64"},{"name":"dst","data_type":"int64"}]},
        {"name":"eb","columns":[{"name":"id","data_type":"int64"},{"name":"src","data_type":"int64"},{"name":"dst","data_type":"int64"}]}
    ],
    "nodes":[{"label":"N","table":"nodes","id":"id","properties":{"id":"id"}}],
    "edges":[
        {"label":"x,:7","table":"ea","id":"id","source":"src","target":"dst","source_label":"N","target_label":"N"},
        {"label":"B","table":"eb","id":"id","source":"src","target":"dst","source_label":"N","target_label":"N"}
    ]})
}
async fn run(query: &str) -> Vec<Vec<i64>> {
    let compiled: Value =
        serde_json::from_str(&compile_json(&request(query).to_string()).await.unwrap()).unwrap();
    let sql = compiled["sql"].as_str().unwrap();
    let con = duckdb::Connection::open_in_memory().unwrap();
    con.execute_batch("CREATE TABLE nodes(id BIGINT); INSERT INTO nodes VALUES (1),(2),(3),(4); CREATE TABLE ea(id BIGINT,src BIGINT,dst BIGINT); CREATE TABLE eb(id BIGINT,src BIGINT,dst BIGINT);").unwrap();
    for &(kind, id, src, dst) in EDGES {
        con.execute(
            &format!(
                "INSERT INTO {} VALUES (?,?,?)",
                if kind == 0 { "ea" } else { "eb" }
            ),
            [id, src, dst],
        )
        .unwrap();
    }
    let mut stmt = con
        .prepare(sql)
        .unwrap_or_else(|e| panic!("{query}\n{e}\n{sql}"));
    let width = compiled["fields"].as_array().unwrap().len();
    let mut rows = stmt
        .query([])
        .unwrap_or_else(|e| panic!("{query}\n{e}\n{sql}"));
    let mut result = vec![];
    while let Some(row) = rows.next().unwrap() {
        result.push((0..width).map(|i| row.get(i).unwrap()).collect());
    }
    result.sort();
    result
}
fn trails(
    node: i64,
    depth: usize,
    both: bool,
    used: &mut Vec<usize>,
    output: &mut Vec<(i64, Vec<usize>)>,
) {
    if depth == 0 {
        output.push((node, used.clone()));
        return;
    }
    for (edge, &(_, _, src, dst)) in EDGES.iter().enumerate() {
        if used.contains(&edge) {
            continue;
        }
        let next = if src == node {
            Some(dst)
        } else if both && dst == node {
            Some(src)
        } else {
            None
        };
        if let Some(next) = next {
            used.push(edge);
            trails(next, depth - 1, both, used, output);
            used.pop();
        }
    }
}
#[tokio::test]
async fn fixed_patterns_use_distinct_physical_relationships() {
    for both in [false, true] {
        let arrow = if both { "-" } else { "->" };
        let query = format!(
            "MATCH (a:N)-[]{}(b:N)-[]{}(c:N) RETURN a.id AS a, c.id AS c",
            arrow, arrow
        );
        let mut expected = vec![];
        for start in 1..=4 {
            let mut found = vec![];
            trails(start, 2, both, &mut vec![], &mut found);
            expected.extend(found.into_iter().map(|(end, _)| vec![start, end]));
        }
        expected.sort();
        assert_eq!(run(&query).await, expected, "{query}");
    }
}
#[tokio::test]
async fn bounded_paths_include_zero_hops_and_never_reuse_edges() {
    for both in [false, true] {
        let arrow = if both { "-" } else { "->" };
        let query = format!(
            "MATCH (a:N)-[*0..4]{}(b:N) RETURN a.id AS a, b.id AS b",
            arrow
        );
        let mut expected = vec![];
        for start in 1..=4 {
            for depth in 0..=4 {
                let mut found = vec![];
                trails(start, depth, both, &mut vec![], &mut found);
                expected.extend(found.into_iter().map(|(end, _)| vec![start, end]));
            }
        }
        expected.sort();
        assert_eq!(run(&query).await, expected, "{query}");
    }
}
#[tokio::test]
async fn history_survives_variable_and_fixed_pattern_segments() {
    for (first_min, first_max, second_min, second_max) in [(0, 2, 1, 1), (1, 1, 0, 2), (0, 2, 0, 2)]
    {
        let query = format!(
            "MATCH (a:N)-[*{first_min}..{first_max}]->(b:N)-[*{second_min}..{second_max}]->(c:N) RETURN a.id AS a,b.id AS b,c.id AS c"
        );
        let mut expected = vec![];
        for start in 1..=4 {
            for depth in first_min..=first_max {
                let mut first = vec![];
                trails(start, depth, false, &mut vec![], &mut first);
                for (mid, mut used) in first {
                    for next_depth in second_min..=second_max {
                        let mut second = vec![];
                        trails(mid, next_depth, false, &mut used, &mut second);
                        expected.extend(second.into_iter().map(|(end, _)| vec![start, mid, end]));
                    }
                }
            }
        }
        expected.sort();
        assert_eq!(run(&query).await, expected, "{query}");
    }
}
#[tokio::test]
async fn comma_patterns_share_history_but_separate_match_clauses_do_not() {
    let distinct = run("MATCH (a:N)-[r]->(b:N), (c:N)-[s]->(d:N) RETURN count(*) AS n").await;
    assert_eq!(
        distinct,
        vec![vec![(EDGES.len() * (EDGES.len() - 1)) as i64]]
    );
    let repeatable =
        run("MATCH (a:N)-[r]->(b:N) MATCH (c:N)-[s]->(d:N) RETURN count(*) AS n").await;
    assert_eq!(repeatable, vec![vec![(EDGES.len() * EDGES.len()) as i64]]);
}
#[tokio::test]
async fn observed_paths_and_relationship_properties_are_not_silently_dropped() {
    for query in [
        "MATCH p=(a:N)-[*1..3]->(b:N) RETURN p",
        "MATCH (a:N)-[r*1..3]->(b:N) RETURN r",
        "MATCH (a:N)-[*1..3 {missing:1}]->(b:N) RETURN b.id",
    ] {
        assert!(
            compile_json(&request(query).to_string()).await.is_err(),
            "{query}"
        );
    }
}

#[tokio::test]
async fn postgres_relationship_membership_uses_its_native_array_function() {
    let mut req = request("MATCH (a:N)-[]->(b:N)-[]->(c:N) RETURN c.id");
    req["dialect"] = json!("postgres");
    let compiled: Value =
        serde_json::from_str(&compile_json(&req.to_string()).await.unwrap()).unwrap();
    let sql = compiled["sql"].as_str().unwrap();
    assert!(sql.contains("array_position"), "{sql}");
    assert!(!sql.contains("array_has("), "{sql}");
    datafusion::sql::sqlparser::parser::Parser::parse_sql(
        &datafusion::sql::sqlparser::dialect::PostgreSqlDialect {},
        sql,
    )
    .unwrap();
}
