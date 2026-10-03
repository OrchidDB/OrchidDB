//! Local adversarial coverage of projection scopes and query boundaries.
use arrow::array::{ArrayRef, Int64Array, StringArray};
use orchiddb::ir::catalog::{PropertyGraph, nodes_from_columns};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::sync::Arc;

fn graph() -> PropertyGraph {
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns(
        "Person",
        vec![
            (
                "name",
                Arc::new(StringArray::from(vec!["Ada", "Bob", "Cy", "Dee"])) as ArrayRef,
            ),
            (
                "age",
                Arc::new(Int64Array::from(vec![20, 20, 30, 40])) as ArrayRef,
            ),
        ],
    ));
    graph
}

async fn rows(query: &str) -> Result<Vec<String>, String> {
    let ast = parse_query(query).map_err(|e| e.to_string())?;
    let plan = CypherPlanner::new().plan(&ast).map_err(|e| e.to_string())?;
    let (result, _) = orchiddb::ir::rel::runtime::execute(&plan, &graph(), None).await?;
    Ok((0..result.batch.num_rows())
        .map(|row| {
            result
                .batch
                .columns()
                .iter()
                .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect())
}

#[tokio::test]
async fn distinct_top_k_keeps_limits_when_window_order_already_satisfies_sort() {
    for direction in ["ASC", "DESC"] {
        let sorted = if direction == "ASC" {
            [1, 3, 5]
        } else {
            [5, 3, 1]
        };
        for skip in 0..=3 {
            for limit in 0..=4 {
                let query = format!(
                    "UNWIND [5,1,3,1,5,3] AS x WITH DISTINCT x ORDER BY x {direction} \
                     SKIP {skip} LIMIT {limit} RETURN x"
                );
                let expected = sorted
                    .iter()
                    .skip(skip)
                    .take(limit)
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                assert_eq!(rows(&query).await.unwrap(), expected, "{query}");
            }
        }
    }
}

#[tokio::test]
async fn projection_scope_and_aggregation_matrix() {
    let cases: &[(&str, &[&str])] = &[
        ("WITH 1 AS x WITH x + 1 AS x RETURN x", &["2"]),
        (
            "WITH 1 AS x, 2 AS y WITH y AS x, x AS y RETURN x, y",
            &["2|1"],
        ),
        ("WITH 1 AS x WITH *, x + 1 AS y RETURN x, y", &["1|2"]),
        (
            "UNWIND [3,1,2] AS x WITH x AS y ORDER BY y SKIP 1 LIMIT 1 RETURN y",
            &["2"],
        ),
        (
            "UNWIND [3,1,2] AS x RETURN x + 1 AS x ORDER BY x",
            &["2", "3", "4"],
        ),
        (
            "UNWIND [3,1,2] AS x RETURN x + 1 AS y ORDER BY x",
            &["2", "3", "4"],
        ),
        (
            "UNWIND [3,1,2] AS x WITH x + 1 AS y ORDER BY x RETURN y",
            &["2", "3", "4"],
        ),
        (
            "UNWIND [3,1,2] AS x WITH x + 1 AS y WHERE x > 1 RETURN y ORDER BY y",
            &["3", "4"],
        ),
        (
            "UNWIND [3,1,2] AS x WITH x + 1 AS y WHERE y > 2 RETURN y ORDER BY y",
            &["3", "4"],
        ),
        (
            "UNWIND [1,2,2] AS x RETURN DISTINCT x ORDER BY x DESC SKIP 1",
            &["1"],
        ),
        (
            "UNWIND [1,2,2] AS x WITH DISTINCT x AS y RETURN sum(y)",
            &["3"],
        ),
        (
            "UNWIND [1,2,2] AS x RETURN x, count(*) AS n ORDER BY n DESC, x",
            &["2|2", "1|1"],
        ),
        (
            "UNWIND [1,2,2] AS x WITH x, count(*) AS n WHERE n > 1 RETURN x, n",
            &["2|2"],
        ),
        (
            "UNWIND [1,2,2] AS x RETURN count(DISTINCT x), sum(x), min(x), max(x)",
            &["2|5|1|2"],
        ),
        (
            "UNWIND [] AS x RETURN count(*), count(x), sum(x)",
            &["0|0|0"],
        ),
        (
            "UNWIND [null,null] AS x RETURN count(*), count(x), count(DISTINCT x)",
            &["2|0|0"],
        ),
        (
            "MATCH (p:Person) WITH p.age AS age, count(*) AS n RETURN age, n ORDER BY age",
            &["20|2", "30|1", "40|1"],
        ),
        (
            "MATCH (p:Person) RETURN p.age AS age, count(*) AS n ORDER BY count(*) DESC, age",
            &["20|2", "30|1", "40|1"],
        ),
        (
            "MATCH (p:Person) WITH p.age AS age, count(*) AS n ORDER BY n DESC LIMIT 1 RETURN age, n",
            &["20|2"],
        ),
        ("MATCH (p:Person) RETURN count(*) + 1 AS n", &["5"]),
        ("RETURN 1 AS x UNION RETURN 1 AS x", &["1"]),
        ("RETURN 1 AS x UNION ALL RETURN 1 AS x", &["1", "1"]),
        (
            "RETURN 1 AS x UNION ALL RETURN 2 AS x UNION ALL RETURN 3 AS x",
            &["1", "2", "3"],
        ),
        (
            "RETURN 1 AS x UNION RETURN 2 AS x UNION RETURN 1 AS x",
            &["1", "2"],
        ),
        (
            "WITH 1 AS x RETURN x UNION WITH 2 AS x RETURN x",
            &["1", "2"],
        ),
        (
            "UNWIND [2,1] AS x RETURN x ORDER BY x LIMIT 1 UNION ALL RETURN 3 AS x",
            &["1", "3"],
        ),
        (
            "UNWIND [3,1,2] AS x WITH x + 1 AS y ORDER BY x RETURN collect(y)",
            &["[2,3,4]"],
        ),
        (
            "UNWIND [3,1,2] AS x WITH x + 1 AS y ORDER BY x RETURN [z IN range(1,y) | z]",
            &["[1,2]", "[1,2,3]", "[1,2,3,4]"],
        ),
        (
            "WITH 1 AS x RETURN EXISTS { WITH x RETURN x } AS yes",
            &["true"],
        ),
    ];
    let mut errors = Vec::new();
    for (query, expected) in cases {
        match rows(query).await {
            Ok(actual) if actual.iter().map(String::as_str).collect::<Vec<_>>() == *expected => {}
            actual => errors.push(format!(
                "{query}\nexpected: {expected:?}\nactual: {actual:?}"
            )),
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n\n"));
}

#[tokio::test]
async fn mapped_scope_sql_compiles_for_both_backends() {
    use orchiddb::compiler::compile_json;
    use serde_json::{Value, json};
    let cases: &[(&str, &[&str])] = &[
        (
            "MATCH (p:Person) RETURN p.name AS name ORDER BY p.age DESC, name",
            &["Dee", "Cy", "Ada", "Bob"],
        ),
        (
            "MATCH (p:Person) WITH p.age + 1 AS age, p.name AS name RETURN name, age ORDER BY age DESC, name",
            &["Dee|41", "Cy|31", "Ada|21", "Bob|21"],
        ),
        (
            "MATCH (p:Person) WITH p.name AS name ORDER BY p.age DESC, name SKIP 1 LIMIT 2 RETURN name",
            &["Cy", "Ada"],
        ),
        (
            "MATCH (p:Person) WITH p.age AS age, count(*) AS n WHERE n > 1 RETURN age, n",
            &["20|2"],
        ),
        (
            "MATCH (p:Person) RETURN p.age AS age, count(*) AS n ORDER BY n DESC, age DESC",
            &["20|2", "40|1", "30|1"],
        ),
        (
            "MATCH (p:Person) RETURN DISTINCT p.age AS age ORDER BY age DESC SKIP 1 LIMIT 1",
            &["30"],
        ),
        (
            "MATCH (p:Person) WITH p.age AS age WHERE p.name <> 'Ada' RETURN age ORDER BY age",
            &["20", "30", "40"],
        ),
        (
            "MATCH (p:Person) WITH p.age + 1 AS age WHERE age > 30 RETURN age ORDER BY age",
            &["31", "41"],
        ),
        (
            "MATCH (p:Person) RETURN count(DISTINCT p.age), sum(p.age), min(p.age), max(p.age)",
            &["3|110|20|40"],
        ),
        ("MATCH (p:Person) WHERE p.age < 0 RETURN count(*)", &["0"]),
        (
            "MATCH (p:Person) WHERE p.age = 30 RETURN p.name AS name UNION ALL MATCH (p:Person) WHERE p.age = 40 RETURN p.name AS name",
            &["Cy", "Dee"],
        ),
        ("MATCH (p:Person) RETURN count(*) + 1 AS n", &["5"]),
        (
            "WITH 1 AS x, 2 AS y WITH y AS x, x AS y RETURN x, y",
            &["2|1"],
        ),
        ("WITH 1 AS x WITH *, x + 1 AS y RETURN x, y", &["1|2"]),
    ];
    let mut exports = Vec::new();
    let mut errors = Vec::new();
    for dialect in ["duckdb", "postgres"] {
        for (query, expected) in cases {
            let request = json!({"version":1,"dialect":dialect,"language":"cypher","query":query,
                "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"},{"name":"age","data_type":"int64"}]}],
                "nodes":[{"label":"Person","table":"people","id":"id","properties":{"name":"name","age":"age"}}]});
            match compile_json(&request.to_string()).await {
                Ok(result) => {
                    let result: Value = serde_json::from_str(&result).unwrap();
                    assert!(!result["sql"].as_str().unwrap().is_empty());
                    exports.push(json!({"query":query,"dialect":dialect,"sql":result["sql"],"expected":expected}));
                }
                Err(error) => errors.push(format!("{dialect}: {query}: {error}")),
            }
        }
    }
    if let Some(path) = std::env::var_os("ORCHIDDB_STRESS_SCOPE_SQL_OUTPUT") {
        std::fs::write(path, serde_json::to_vec_pretty(&exports).unwrap()).unwrap();
    }
    assert!(errors.is_empty(), "{}", errors.join("\n\n"));
}

#[tokio::test]
async fn ordering_survives_alias_shadowing_and_hidden_keys_for_every_permutation() {
    let mut errors = Vec::new();
    for values in ["1,2,3", "1,3,2", "2,1,3", "2,3,1", "3,1,2", "3,2,1"] {
        for (tail, expected) in [
            ("RETURN -x AS x ORDER BY x", vec!["-3", "-2", "-1"]),
            ("RETURN -x AS x ORDER BY x DESC", vec!["-1", "-2", "-3"]),
            ("RETURN -x AS y ORDER BY x", vec!["-1", "-2", "-3"]),
            ("RETURN -x AS y ORDER BY x DESC", vec!["-3", "-2", "-1"]),
            ("WITH -x AS y ORDER BY x RETURN y", vec!["-1", "-2", "-3"]),
            (
                "WITH -x AS y ORDER BY x DESC RETURN y",
                vec!["-3", "-2", "-1"],
            ),
            ("RETURN -x AS y ORDER BY x SKIP 1 LIMIT 1", vec!["-2"]),
            ("RETURN x AS y ORDER BY x % 2, x DESC", vec!["2", "3", "1"]),
        ] {
            let query = format!("UNWIND [{values}] AS x {tail}");
            match rows(&query).await {
                Ok(actual) if actual.iter().map(String::as_str).collect::<Vec<_>>() == expected => {
                }
                actual => errors.push(format!("{query}: expected {expected:?}, got {actual:?}")),
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
}
