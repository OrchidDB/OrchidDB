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
fn test_connection() -> duckdb::Connection {
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
    orchiddb::ir::rel::sql::language_functions::register(&con).unwrap();
    con
}
async fn run(query: &str) -> Vec<Vec<i64>> {
    let compiled: Value =
        serde_json::from_str(&compile_json(&request(query).to_string()).await.unwrap()).unwrap();
    let sql = compiled["sql"].as_str().unwrap();
    let con = test_connection();
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
async fn observed_paths_and_relationship_lists_retain_trail_identity() {
    use orchiddb::ir::value::Value as GraphValue;
    let mut expected=vec![];
    for start in 1..=4 {
        for depth in 1..=3 {
            let mut paths=vec![];
            trails(start,depth,false,&mut vec![],&mut paths);
            expected.extend(paths.into_iter().map(|(_,edges)| edges.len()));
        }
    }
    expected.sort();
    for (query,is_path) in [
        ("MATCH p=(a:N)-[*1..3]->(b:N) RETURN p AS value",true),
        ("MATCH (a:N)-[r*1..3]->(b:N) RETURN r AS value",false),
    ] {
        let compiled:Value=serde_json::from_str(&compile_json(&request(query).to_string()).await.unwrap()).unwrap();
        let con=test_connection();
        let mut statement=con.prepare(compiled["sql"].as_str().unwrap()).unwrap();
        let batches=statement.query_arrow([]).unwrap().collect::<Vec<_>>();
        let mut lengths=vec![];
        for batch in batches {
            let rows=orchiddb::ir::rel::native_values::decode_rows(&batch,&["value".into()],&orchiddb::ir::catalog::PropertyGraph::new()).unwrap();
            for row in rows {
                let values=match row.get("value") {
                    GraphValue::Path(values) if is_path => {
                        assert_eq!(values.len()%2,1);
                        assert!(values.iter().step_by(2).all(|value|matches!(value,GraphValue::Node{..})));
                        values.into_iter().skip(1).step_by(2).collect::<Vec<_>>()
                    },
                    GraphValue::List(values) if !is_path => values,
                    value=>panic!("{query}: {value:?}"),
                };
                assert!(values.iter().all(|value|matches!(value,GraphValue::Edge{..})));
                let unique=values.iter().map(|value|format!("{value:?}")).collect::<std::collections::BTreeSet<_>>();
                assert_eq!(unique.len(),values.len(),"a trail cannot repeat a physical relationship");
                lengths.push(values.len());
            }
        }
        lengths.sort();assert_eq!(lengths,expected,"{query}");
    }
    let query="MATCH (a:N)-[*1..3 {missing:1}]->(b:N) RETURN b.id";
    assert!(run(query).await.is_empty(),"missing relationship properties must filter every trail");
}

#[tokio::test]
async fn postgres_fixed_relationship_uniqueness_uses_scalar_keys() {
    let mut req = request("MATCH (a:N)-[]->(b:N)-[]->(c:N) RETURN c.id");
    req["dialect"] = json!("postgres");
    let compiled: Value =
        serde_json::from_str(&compile_json(&req.to_string()).await.unwrap()).unwrap();
    let sql = compiled["sql"].as_str().unwrap();
    assert!(sql.contains("<>") && sql.contains("__e0_label") && sql.contains("__e0_id"), "{sql}");
    assert!(!sql.contains("array_position"), "fixed paths need no list history: {sql}");
    assert!(!sql.contains("array_has("), "{sql}");
    datafusion::sql::sqlparser::parser::Parser::parse_sql(
        &datafusion::sql::sqlparser::dialect::PostgreSqlDialect {},
        sql,
    )
    .unwrap();
}

#[tokio::test]
async fn internal_looking_cypher_path_names_preserve_nodes_and_relationships() {
    use orchiddb::ir::value::Value as GraphValue;
    for (path, target) in [("p", "b"), ("__path", "b"), ("__path", "__edge_other"), ("p", "__edge_other")] {
        for hops in [1, 2] {
            let pattern = if hops == 1 {
                format!("(a:N)-[r]->({target}:N)")
            } else {
                format!("(a:N)-[r]->(middle:N)-[s]->({target}:N)")
            };
            let query = format!("MATCH {path}={pattern} RETURN {path} AS value");
            let compiled: Value = serde_json::from_str(&compile_json(&request(&query).to_string()).await.unwrap()).unwrap();
            let con = test_connection();
            let mut statement = con.prepare(compiled["sql"].as_str().unwrap()).unwrap();
            let batches = statement.query_arrow([]).unwrap().collect::<Vec<_>>();
            let mut actual_rows = 0;
            for batch in batches {
                let rows = orchiddb::ir::rel::native_values::decode_rows(&batch, &["value".into()],
                    &orchiddb::ir::catalog::PropertyGraph::new()).unwrap();
                actual_rows += rows.len();
                for row in rows {
                    let GraphValue::Path(items) = row.get("value") else { panic!("expected path: {query}") };
                    assert_eq!(items.len(), 2 * hops + 1, "{query}: {items:?}");
                    assert!(items.iter().step_by(2).all(|item| matches!(item, GraphValue::Node { .. })));
                    assert!(items.iter().skip(1).step_by(2).all(|item| matches!(item, GraphValue::Edge { .. })));
                }
            }
            let expected_rows = (1..=4).map(|start| {
                let mut expected = vec![];
                trails(start, hops, false, &mut vec![], &mut expected);
                expected.len()
            }).sum::<usize>();
            assert_eq!(actual_rows, expected_rows, "{query}");
        }
    }
}
