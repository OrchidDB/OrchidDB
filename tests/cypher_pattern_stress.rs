//! End-to-end pattern stress cases execute the compiled DataFusion plans.
#[path = "common/execution.rs"]
mod datafusion_test;
use datafusion_test::execute;
use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::language::cypher::parser::parse_query;
use orchiddb::language::cypher::planner::CypherPlanner;

fn rows(graph: &PropertyGraph, query: &str) -> Result<Vec<String>, String> {
    let parsed = parse_query(query).map_err(|e| format!("parse: {e}"))?;
    let plan = CypherPlanner::new()
        .plan(&parsed)
        .map_err(|e| format!("plan: {e}"))?;
    let returned = execute(&plan, graph)?;
    let batch = returned.batch;
    Ok((0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|col| {
                    arrow::util::display::array_value_to_string(batch.column(col), row).unwrap()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect())
}

fn fixture() -> PropertyGraph {
    let graph = PropertyGraph::new();
    rows(&graph, "CREATE (a:P {name:'a', k:1}), (b:P {name:'b', k:2}), (c:P {name:'c', k:3}), (d:P {name:'d', k:4}), (e:P {name:'e', k:5}), (a)-[:R {w:1}]->(b), (b)-[:R {w:2}]->(c), (c)-[:R {w:3}]->(a), (a)-[:R {w:4}]->(d), (b)-[:S]->(d), (d)-[:R {w:5}]->(d)").unwrap();
    graph
}

#[test]
fn direction_reuse_optional_and_exists_matrix() {
    let graph = fixture();
    let cases: &[(&str, &[&str])] = &[
        (
            "MATCH (a:P {name:'a'})-[:R]->(b) RETURN b.name ORDER BY b.name",
            &["b", "d"],
        ),
        ("MATCH (a:P {name:'a'})<-[:R]-(b) RETURN b.name", &["c"]),
        (
            "MATCH (a:P {name:'a'})-[:R]-(b) RETURN b.name ORDER BY b.name",
            &["b", "c", "d"],
        ),
        ("MATCH (a)-[r:R]->(a) RETURN a.name, r.w", &["d|5"]),
        ("MATCH (a)-[r:R]-(a) RETURN a.name, r.w", &["d|5"]),
        (
            "MATCH (a {name:'a'})-[:R]->(b)-[:R]->(c) RETURN b.name, c.name ORDER BY b.name",
            &["b|c", "d|d"],
        ),
        (
            "MATCH (a {name:'a'})-[:R]->(b), (b)-[:R]->(c) RETURN b.name, c.name ORDER BY b.name",
            &["b|c", "d|d"],
        ),
        (
            "MATCH (a {name:'d'})-[r:R]->(b)-[s:R]->(c) RETURN count(*)",
            &["0"],
        ),
        (
            "MATCH (a {name:'d'})-[r:R]->(b) MATCH (b)-[s:R]->(c) RETURN count(*)",
            &["1"],
        ),
        (
            "MATCH (a:P) OPTIONAL MATCH (a)-[:R]->(b) RETURN a.name, b.name ORDER BY a.name, b.name",
            &["a|b", "a|d", "b|c", "c|a", "d|d", "e|"],
        ),
        (
            "MATCH (a:P) OPTIONAL MATCH (a)-[:R]->(b) WHERE b.name='c' RETURN a.name, b.name ORDER BY a.name",
            &["a|", "b|c", "c|", "d|", "e|"],
        ),
        (
            "MATCH (a:P) WHERE EXISTS { MATCH (a)-[:R]->(b) WHERE b.name='c' } RETURN a.name",
            &["b"],
        ),
        (
            "MATCH (a:P) WHERE NOT EXISTS { MATCH (a)-[:R]->(b) } RETURN a.name",
            &["e"],
        ),
        (
            "MATCH (a:P) WHERE EXISTS { MATCH (a)-[:R]->(b)-[:R]->(c) WHERE c=a } RETURN a.name",
            &[],
        ),
        (
            "MATCH (a:P) WHERE EXISTS { MATCH (a)-[:R]->(b)-[:R]->(c)-[:R]->(a) } RETURN a.name ORDER BY a.name",
            &["a", "b", "c"],
        ),
        (
            "MATCH (a {name:'a'}), (b {name:'d'}) OPTIONAL MATCH (a)-[:S]->(b) RETURN a.name, b.name",
            &["a|d"],
        ),
        (
            "MATCH (a {name:'a'}), (b {name:'d'}) MATCH (a)-[:R]->(b) RETURN a.name, b.name",
            &["a|d"],
        ),
        (
            "MATCH (a {name:'a'}) WITH a OPTIONAL MATCH (a)-[r:R]->(b) WHERE r.w > 2 RETURN b.name",
            &["d"],
        ),
        (
            "MATCH (a {name:'a'})-[:R|S]->(b) RETURN b.name ORDER BY b.name",
            &["b", "d"],
        ),
        (
            "MATCH (a {name:'b'})-[:R|S]->(b) RETURN b.name ORDER BY b.name",
            &["c", "d"],
        ),
        (
            "MATCH (a:P) WHERE (a)-[:R]->() RETURN a.name ORDER BY a.name",
            &["a", "b", "c", "d"],
        ),
        ("MATCH (a:P) WHERE NOT (a)-[:R]->() RETURN a.name", &["e"]),
        (
            "MATCH (a:P) OPTIONAL MATCH (a)-[:R]->(b) OPTIONAL MATCH (b)-[:S]->(c) RETURN a.name, b.name, c.name ORDER BY a.name,b.name",
            &["a|b|d", "a|d|", "b|c|", "c|a|", "d|d|", "e||"],
        ),
        (
            "MATCH (a:P {name:'e'}) OPTIONAL MATCH (a)-[:R]->(b), (b)-[:S]->(c) RETURN a.name,b.name,c.name",
            &["e||"],
        ),
        (
            "MATCH (a:P {name:'a'}) OPTIONAL MATCH (a)-[:R]->(b), (b)-[:S]->(c) RETURN a.name,b.name,c.name",
            &["a|b|d"],
        ),
        (
            "MATCH (a:P {name:'a'}) OPTIONAL MATCH (a)-[:R]->(b), (b)-[:MISSING]->(c) RETURN a.name,b.name,c.name",
            &["a||"],
        ),
        (
            "MATCH (a:P)-[:R]->(b {k:a.k}) RETURN a.name,b.name",
            &["d|d"],
        ),
        (
            "MATCH (a:P)-[r:R]->(b) MATCH (b)<-[r]-(a) RETURN count(*)",
            &["5"],
        ),
        (
            "MATCH (a:P)-[:R]->(b) WHERE EXISTS { MATCH (b)-[:R]->(c) WHERE c.k > a.k } RETURN a.name,b.name ORDER BY a.name,b.name",
            &["a|b", "a|d", "c|a"],
        ),
    ];
    let mut failures = Vec::new();
    for (query, expected) in cases {
        match rows(&graph, query) {
            Ok(actual) if actual == *expected => {}
            actual => failures.push(format!("{query}\nexpected {expected:?}\nactual {actual:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn variable_length_zero_hops_cycles_and_bound_endpoints() {
    let graph = fixture();
    let cases: &[(&str, &[&str])] = &[
        ("MATCH (a {name:'a'})-[:R*0..0]->(b) RETURN b.name", &["a"]),
        (
            "MATCH (a {name:'a'})-[:R*0..1]->(b) RETURN b.name ORDER BY b.name",
            &["a", "b", "d"],
        ),
        (
            "MATCH (a {name:'a'})-[:R*1..2]->(b) RETURN b.name ORDER BY b.name",
            &["b", "c", "d", "d"],
        ),
        ("MATCH (a {name:'a'})-[:R*3..3]->(b) RETURN b.name", &["a"]),
        ("MATCH (a {name:'a'})-[:R*1..3]->(a) RETURN a.name", &["a"]),
        ("MATCH (a {name:'d'})-[:R*1..3]->(b) RETURN b.name", &["d"]),
        (
            "MATCH (a {name:'a'})<-[:R*1..2]-(b) RETURN b.name ORDER BY b.name",
            &["b", "c"],
        ),
        (
            "MATCH (a {name:'e'}) OPTIONAL MATCH (a)-[:R*1..3]->(b) RETURN a.name,b.name",
            &["e|"],
        ),
        (
            "MATCH (a {name:'a'}), (b {name:'c'}) MATCH (a)-[:R*2..2]->(b) RETURN a.name,b.name",
            &["a|c"],
        ),
        (
            "MATCH p=(a {name:'a'})-[:R*0..2]->(b) RETURN b.name,length(p) ORDER BY length(p),b.name",
            &["a|0", "b|1", "d|1", "c|2", "d|2"],
        ),
        (
            "MATCH (a {name:'a'})-[rs:R*0..2]->(b) RETURN b.name,size(rs) ORDER BY size(rs),b.name",
            &["a|0", "b|1", "d|1", "c|2", "d|2"],
        ),
        (
            "MATCH (a {name:'a'})-[:R*1..2 {w:1}]->(b) RETURN b.name",
            &["b"],
        ),
    ];
    let mut failures = Vec::new();
    for (query, expected) in cases {
        match rows(&graph, query) {
            Ok(actual) if actual == *expected => {}
            actual => failures.push(format!("{query}\nexpected {expected:?}\nactual {actual:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn variable_length_matches_edge_distinct_reference_walks() {
    // Parallel edges, a self-loop, cycles and a disconnected vertex exercise
    // multiplicity and relationship uniqueness independently of the planner.
    let edges = [
        (0, 1),
        (0, 1),
        (1, 2),
        (2, 0),
        (0, 3),
        (3, 3),
        (2, 4),
        (4, 1),
    ];
    let graph = PropertyGraph::new();
    let mut setup = (0..6)
        .map(|id| format!("(n{id}:N {{i:{id}}})"))
        .collect::<Vec<_>>();
    setup.extend(edges.iter().map(|(a, b)| format!("(n{a})-[:R]->(n{b})")));
    rows(&graph, &format!("CREATE {}", setup.join(","))).unwrap();
    fn walks(
        edges: &[(usize, usize)],
        node: usize,
        direction: usize,
        min: usize,
        max: usize,
        used: &mut Vec<usize>,
        endpoints: &mut Vec<String>,
    ) {
        if used.len() >= min {
            endpoints.push(node.to_string());
        }
        if used.len() == max {
            return;
        }
        for (id, &(a, b)) in edges.iter().enumerate() {
            if used.contains(&id) {
                continue;
            }
            let next = match direction {
                0 if a == node => Some(b),
                1 if b == node => Some(a),
                2 if a == node => Some(b),
                2 if b == node => Some(a),
                _ => None,
            };
            if let Some(next) = next {
                used.push(id);
                walks(edges, next, direction, min, max, used, endpoints);
                used.pop();
            }
        }
    }
    for start in 0..6 {
        for direction in 0..3 {
            for (min, max) in [(0, 0), (0, 1), (1, 1), (1, 2), (0, 3), (2, 4)] {
                let rel = format!("[:R*{min}..{max}]");
                let segment = match direction {
                    0 => format!("-{rel}->"),
                    1 => format!("<-{rel}-"),
                    _ => format!("-{rel}-"),
                };
                let query = format!("MATCH (a:N {{i:{start}}}){segment}(b:N) RETURN b.i");
                let mut expected = Vec::new();
                walks(
                    &edges,
                    start,
                    direction,
                    min,
                    max,
                    &mut Vec::new(),
                    &mut expected,
                );
                let mut actual = rows(&graph, &query).unwrap_or_else(|e| panic!("{query}: {e}"));
                actual.sort();
                expected.sort();
                assert_eq!(actual, expected, "{query}");
            }
        }
    }
}

#[test]
fn pattern_matrix_compiles_for_mapped_duckdb_and_postgres() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    use orchiddb::compiler::compile_json;
    use serde_json::{Value, json};
    let queries: &[(&str, &[&str])] = &[
        (
            "MATCH (a:P {name:'a'})-[:R]->(b) RETURN b.name AS name",
            &["b", "d"],
        ),
        (
            "MATCH (a:P {name:'a'})<-[:R]-(b) RETURN b.name AS name",
            &["c"],
        ),
        (
            "MATCH (a:P {name:'a'})-[:R]-(b) RETURN b.name AS name",
            &["b", "c", "d"],
        ),
        (
            "MATCH (a)-[r:R]->(a) RETURN a.name AS name,r.w AS w",
            &["d|5"],
        ),
        (
            "MATCH (a:P) OPTIONAL MATCH (a)-[:R]->(b) RETURN a.name AS a,b.name AS b ORDER BY a,b",
            &["a|b", "a|d", "b|c", "c|a", "d|d", "e|"],
        ),
        (
            "MATCH (a:P) OPTIONAL MATCH (a)-[:R]->(b) WHERE b.name='c' RETURN a.name AS a,b.name AS b",
            &["a|", "b|c", "c|", "d|", "e|"],
        ),
        (
            "MATCH (a:P) WHERE EXISTS { MATCH (a)-[:R]->(b) WHERE b.name='c' } RETURN a.name AS name",
            &["b"],
        ),
        (
            "MATCH (a:P) WHERE NOT EXISTS { MATCH (a)-[:R]->(b) } RETURN a.name AS name",
            &["e"],
        ),
        (
            "MATCH (a:P)-[:R]->(b) WHERE EXISTS { MATCH (b)-[:R]->(c) WHERE c.k>a.k } RETURN a.name AS a,b.name AS b",
            &["a|b", "a|d", "c|a"],
        ),
        (
            "MATCH (a:P {name:'a'}) OPTIONAL MATCH (a)-[:R]->(b), (b)-[:S]->(c) RETURN a.name AS a,b.name AS b,c.name AS c",
            &["a|b|d"],
        ),
        (
            "MATCH (a:P)-[:R]->(b {k:a.k}) RETURN a.name AS a,b.name AS b",
            &["d|d"],
        ),
        (
            "MATCH (a:P {name:'a'})-[:R*0..0]->(b) RETURN b.name AS name",
            &["a"],
        ),
        (
            "MATCH (a:P {name:'a'})-[:R*0..3]->(b) RETURN b.name AS name",
            &["a", "a", "b", "c", "d", "d"],
        ),
        (
            "MATCH (a:P {name:'a'})-[:R*1..3]->(a) RETURN a.name AS name",
            &["a"],
        ),
        (
            "MATCH (a:P {name:'a'})<-[:R*1..2]-(b) RETURN b.name AS name",
            &["b", "c"],
        ),
        (
            "MATCH (a:P {name:'a'})-[:R*1..3]-(b) RETURN b.name AS name",
            &["a", "a", "b", "b", "c", "c", "d", "d"],
        ),
        (
            "MATCH (a:P {name:'a'})-[:R*1..2]->(b)-[:R]->(c) RETURN c.name AS name",
            &["a", "c", "d"],
        ),
        (
            "MATCH (a:P {name:'a'})-[:R]->(b), (b)-[:R*1..2]->(c) RETURN c.name AS name",
            &["a", "c", "d"],
        ),
    ];
    let mut failures = Vec::new();
    let graph = fixture();
    let mut exports = Vec::new();
    for dialect in ["duckdb", "postgres"] {
        for &(query, expected) in queries {
            let request = json!({"version":1,"dialect":dialect,"language":"cypher","query":query,
            "tables":[
                {"name":"nodes","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"},{"name":"k","data_type":"int64"}]},
                {"name":"edges_r","columns":[{"name":"id","data_type":"int64"},{"name":"src","data_type":"int64"},{"name":"dst","data_type":"int64"},{"name":"w","data_type":"int64"}]},
                {"name":"edges_s","columns":[{"name":"id","data_type":"int64"},{"name":"src","data_type":"int64"},{"name":"dst","data_type":"int64"}]}
            ],
            "nodes":[{"label":"P","table":"nodes","id":"id","properties":{"name":"name","k":"k"}}],
            "edges":[
                {"label":"R","table":"edges_r","id":"id","source":"src","target":"dst","source_label":"P","target_label":"P","properties":{"w":"w"}},
                {"label":"S","table":"edges_s","id":"id","source":"src","target":"dst","source_label":"P","target_label":"P"}
            ]});
            match runtime.block_on(compile_json(&request.to_string())) {
                Ok(compiled) => {
                    let compiled: Value = serde_json::from_str(&compiled).unwrap();
                    assert!(!compiled["sql"].as_str().unwrap().is_empty(), "{query}");
                    assert!(
                        !compiled["fields"].as_array().unwrap().is_empty(),
                        "{query}"
                    );
                    let mut actual = rows(&graph, query).unwrap();
                    let mut expected_rows = expected
                        .iter()
                        .map(|row| row.to_string())
                        .collect::<Vec<_>>();
                    if !query.contains("ORDER BY") {
                        actual.sort();
                        expected_rows.sort();
                    }
                    assert_eq!(actual, expected_rows, "{query}");
                    exports.push(json!({"query":query,"dialect":dialect,"sql":compiled["sql"],"expected":expected}));
                }
                Err(error) => failures.push(format!("{dialect}: {query}\n{error}")),
            }
        }
    }
    if let Ok(path) = std::env::var("ORCHIDDB_PATTERN_SQL_EXPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&exports).unwrap()).unwrap();
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
