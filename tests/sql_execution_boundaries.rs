#![cfg(feature = "duckdb")]

use orchiddb::engine::{GraphEngine, QueryResult};

fn rows(result: QueryResult) -> Vec<Vec<String>> {
    let batch = result.returned.batch;
    (0..batch.num_rows()).map(|row| batch.columns().iter()
        .map(|col| arrow::util::display::array_value_to_string(col, row).unwrap())
        .collect()).collect()
}

#[tokio::test]
async fn empty_grouped_aggregate_does_not_become_global() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in [
        "MATCH (me:Person)--(you:Person) RETURN me.age, me.age + count(you.age)",
        "MATCH (me:Person)--(you:Person) RETURN me.age AS age, count(you.age) AS cnt ORDER BY age, age + count(you.age)",
    ] {
        assert!(rows(engine.cypher(query).await.unwrap()).is_empty(), "{query}");
    }
    assert_eq!(rows(engine.cypher("MATCH (n:Person) RETURN count(n)").await.unwrap()), vec![vec!["0"]]);
    engine.cypher("CREATE (:Person)-[:KNOWS]->(:Person)").await.unwrap();
    assert_eq!(rows(engine.cypher("MATCH (me:Person)-->(you:Person) RETURN me.age, count(you)").await.unwrap()), vec![vec!["", "1"]]);
}

#[tokio::test]
async fn ordered_offset_keeps_exact_limit_and_projected_alias_scope() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:A {name:'E', num2:4}), (:A {name:'D', num2:2}), (:A {name:'C', num2:0}), (:A {name:'B', num2:3}), (:A {name:'A', num2:1})").await.unwrap();
    for query in [
        "MATCH (n) RETURN n.name ORDER BY n.name SKIP 2 LIMIT 2",
        "MATCH (n) WITH n ORDER BY n.name SKIP 2 LIMIT 2 RETURN n.name",
    ] {
        assert_eq!(rows(engine.cypher(query).await.unwrap()), vec![vec!["C"], vec!["D"]], "{query}");
    }
    assert_eq!(rows(engine.cypher("MATCH (a:A) WITH a.num2 AS x WITH x % 3 AS x ORDER BY x * -1 LIMIT 3 RETURN x").await.unwrap()), vec![vec!["2"], vec!["1"], vec!["1"]]);
}

#[tokio::test]
async fn scalar_child_guard_does_not_mask_list_step_errors() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for step in ["combine", "difference", "disjunct", "intersect", "merge", "product"] {
        let error = engine.gremlin(&format!("g.inject(null).{step}(__.inject(1))")).await.err().expect("null input must fail");
        assert!(error.to_string().contains(&format!("Incoming traverser for {step} step can't be null")), "{step}: {error}");
    }
}

#[tokio::test]
async fn wildcard_values_keeps_heterogeneous_properties() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:Person {name:'b', age:3}), (:Person {name:'a', age:2})").await.unwrap();
    assert_eq!(rows(engine.gremlin("g.V().values().order()").await.unwrap()), vec![vec!["2"], vec!["3"], vec!["a"], vec!["b"]]);
    assert_eq!(rows(engine.gremlin("g.V().values('name').order()").await.unwrap()), vec![vec!["a"], vec!["b"]]);
}
