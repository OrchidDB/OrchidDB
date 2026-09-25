#![cfg(feature = "duckdb")]

use new_graph::engine::GraphEngine;

async fn rows(engine: &mut GraphEngine, query: &str) -> Vec<Vec<String>> {
    let result = engine
        .cypher(query)
        .await
        .unwrap_or_else(|error| panic!("{query}: {error}"));
    let batch = result.returned.batch;
    (0..batch.num_rows())
        .map(|row| {
            batch
                .columns()
                .iter()
                .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn upstream_heterogeneous_quantifier_collections() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        (
            "RETURN none(x IN [[1, 2, 3], ['a']] WHERE size(x) = 3) AS result",
            "false",
        ),
        (
            "RETURN any(x IN [['a'], [1, 2, 3]] WHERE size(x) = 3) AS result",
            "true",
        ),
        (
            "RETURN all(x IN [1, null, true, 4.5, 'abc', false] WHERE true) AS result",
            "true",
        ),
        (
            "RETURN none(x IN [1, null, true, 4.5, 'abc', false] WHERE false) AS result",
            "true",
        ),
    ] {
        assert_eq!(
            rows(&mut engine, query).await,
            vec![vec![expected]],
            "{query}"
        );
    }
}

#[tokio::test]
async fn upstream_order_by_projected_aggregate_subexpressions() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:Person {age: 30}), (:Person {age: 20}), (:Person {age: 20})")
        .await
        .unwrap();
    for query in [
        "MATCH (p:Person) RETURN p.age AS age, count(*) AS cnt ORDER BY age + count(*)",
        "MATCH (p:Person) RETURN p.age AS age, count(*) AS cnt ORDER BY p.age + count(*)",
        "MATCH (p:Person) WITH p.age AS age, count(*) AS cnt ORDER BY p.age + count(*) RETURN age, cnt",
    ] {
        assert_eq!(
            rows(&mut engine, query).await,
            vec![vec!["20", "2"], vec!["30", "1"]],
            "{query}"
        );
    }
    assert!(
        engine
            .cypher("MATCH (p:Person) RETURN count(*) AS cnt ORDER BY p.age")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn upstream_zero_based_index_and_exclusive_slice() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        ("RETURN [1, 2, 3][0]", "1"),
        ("RETURN [1, 2, 3][-1]", "3"),
        ("RETURN [1, 2, 3][3] IS NULL", "true"),
        ("RETURN [1, 2, 3][-4] IS NULL", "true"),
        ("RETURN [1, 2, 3][null] IS NULL", "true"),
        ("RETURN [1, 2, 3][0..1]", "[1]"),
        ("RETURN [1, 2, 3][..2]", "[1,2]"),
        ("RETURN [1, 2, 3][1..]", "[2,3]"),
        ("RETURN [1, 2, 3][-3..-1]", "[1,2]"),
        ("RETURN [1, 2, 3][-5..5]", "[1,2,3]"),
        ("RETURN [1, 2, 3][3..1]", "[]"),
        ("RETURN [1, 2, 3][0..0]", "[]"),
        ("RETURN [1, 2, 3][null..2] IS NULL", "true"),
        ("RETURN [1, 2, 3][1..null] IS NULL", "true"),
    ] {
        assert_eq!(
            rows(&mut engine, query).await,
            vec![vec![expected]],
            "{query}"
        );
    }
}

#[tokio::test]
async fn upstream_collection_values_survive_with() {
    let mut engine = GraphEngine::in_memory().unwrap();
    assert_eq!(
        rows(
            &mut engine,
            "WITH [[1, 2, 3], [4, 5, 6]] AS lol UNWIND lol AS x UNWIND x AS y RETURN y ORDER BY y"
        )
        .await,
        vec![
            vec!["1"],
            vec!["2"],
            vec!["3"],
            vec!["4"],
            vec!["5"],
            vec!["6"]
        ]
    );
    assert_eq!(
        rows(
            &mut engine,
            "WITH [1, 2, 3] AS xs RETURN xs[0], xs[-1], size(xs[1..3])"
        )
        .await,
        vec![vec!["1", "3", "2"]]
    );
    assert_eq!(
        rows(
            &mut engine,
            "WITH [1, 2, 3] AS xs RETURN any(x IN xs WHERE all(y IN xs WHERE x <= y))"
        )
        .await,
        vec![vec!["true"]]
    );
    assert_eq!(
        rows(
            &mut engine,
            "WITH range(1, 3) AS xs UNWIND xs AS x RETURN x ORDER BY x"
        )
        .await,
        vec![vec!["1"], vec!["2"], vec!["3"]]
    );
}

#[tokio::test]
async fn upstream_xor_three_valued_truth_table() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (a, b, expected) in [
        ("true", "true", "false"),
        ("true", "false", "true"),
        ("false", "true", "true"),
        ("false", "false", "false"),
        ("true", "null", "null"),
        ("false", "null", "null"),
        ("null", "true", "null"),
        ("null", "false", "null"),
        ("null", "null", "null"),
    ] {
        let expr = if expected == "null" {
            "(a XOR b) IS NULL"
        } else {
            "a XOR b"
        };
        let query = format!("UNWIND [{a}] AS a UNWIND [{b}] AS b RETURN {expr}");
        assert_eq!(
            rows(&mut engine, &query).await,
            vec![vec![if expected == "null" { "true" } else { expected }]],
            "{query}"
        );
    }
    assert_eq!(rows(&mut engine, "UNWIND [true, false, null] AS a UNWIND [true, false, null] AS b WITH a, b WHERE a IS NULL OR b IS NULL RETURN (a XOR b) IS NULL = (b XOR a) IS NULL AS result").await,
        vec![vec!["true"]; 5]);
}

#[tokio::test]
async fn upstream_to_integer_truncates_toward_zero() {
    let mut engine = GraphEngine::in_memory().unwrap();
    assert_eq!(
        rows(
            &mut engine,
            "UNWIND [82.9, -82.9, 0.9, -0.9] AS weight RETURN toInteger(weight) AS n ORDER BY n"
        )
        .await,
        vec![vec!["-82"], vec!["0"], vec!["0"], vec!["82"]]
    );
}

#[tokio::test]
async fn upstream_map_values_preserve_case_and_nulls_across_with() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        (
            "WITH {existing: 42, notMissing: null} AS m RETURN m.existing, m.notMissing IS NULL, m.missing IS NULL",
            vec!["42", "true", "true"],
        ),
        (
            "WITH {name: 'Mats', Name: 'Pontus'} AS m RETURN m.name, m.Name",
            vec!["Mats", "Pontus"],
        ),
        (
            "WITH {name: 'Mats', Name: 'Pontus'} AS m RETURN m['name'], m['Name'], m['nAMe'] IS NULL",
            vec!["Mats", "Pontus", "true"],
        ),
        (
            "WITH {name: {name2: 'baz'}} AS m RETURN m.name.name2",
            vec!["baz"],
        ),
    ] {
        assert_eq!(rows(&mut engine, query).await, vec![expected], "{query}");
    }
}

#[tokio::test]
async fn upstream_null_membership_and_nested_equality() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in [
        "RETURN (4 IN [1, null, 3]) IS NULL",
        "RETURN ([null] IN [[null]]) IS NULL",
        "RETURN ([null] IN [null]) IS NULL",
        "RETURN (null IN []) = false",
        "RETURN (1 IN [null, 1]) = true",
        "RETURN ([null] = [1]) IS NULL",
        "RETURN ([1, 2] = [null, 2]) IS NULL",
        "RETURN ([null, 1] = [null, 2]) = false",
        "RETURN ({a: null} = {a: 1}) IS NULL",
        "WITH [1, null, 3] AS xs RETURN (4 IN xs) IS NULL",
    ] {
        assert_eq!(
            rows(&mut engine, query).await,
            vec![vec!["true"]],
            "{query}"
        );
    }
}

#[tokio::test]
async fn upstream_keys_exposes_id_property_without_struct_metadata() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:TheLabel {id: 4611686018427387905})")
        .await
        .unwrap();
    assert_eq!(rows(&mut engine, "MATCH (n:TheLabel) RETURN size(keys(n)), 'id' IN keys(n), '__new_graph_struct_order' IN keys(n)").await,
        vec![vec!["1", "true", "false"]]);
}

#[tokio::test]
async fn upstream_equivalent_quantifiers_compose_as_boolean_values() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in [
        "RETURN none(x IN [1,2,3] WHERE x = 2) = all(x IN [1,2,3] WHERE NOT (x = 2))",
        "RETURN any(x IN [1,2,3] WHERE x = 2) = (NOT all(x IN [1,2,3] WHERE NOT (x = 2)))",
        "RETURN none(x IN [1,2,3] WHERE x = 2) = (size([x IN [1,2,3] WHERE x = 2 | x]) = 0)",
    ] {
        assert_eq!(
            rows(&mut engine, query).await,
            vec![vec!["true"]],
            "{query}"
        );
    }
}

#[tokio::test]
async fn upstream_graph_values_keep_identity_inside_collections_and_groups() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:A {num: 42})-[:T]->(:B {num: 42})")
        .await
        .unwrap();
    assert_eq!(rows(&mut engine, "MATCH (n)-[r]->(m) WITH [n,r,m] AS values RETURN (values[0]).num, type(values[1]), (values[2]).num").await,
        vec![vec!["42", "T", "42"]]);
    assert_eq!(rows(&mut engine, "MATCH (a:A), (b:B) WITH coalesce(a.num,b.num) AS foo, b.num AS bar, {name: count(b)} AS baz RETURN foo, bar, baz.name").await,
        vec![vec!["42", "42", "1"]]);
    assert_eq!(
        rows(
            &mut engine,
            "MATCH (a) WITH a, count(*) AS c RETURN a.num,c ORDER BY c"
        )
        .await,
        vec![vec!["42", "1"], vec!["42", "1"]]
    );
}

#[tokio::test]
async fn upstream_match_relationships_are_unique_across_variable_segments() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (n0:Node),(n1:Node),(n2:Node),(n3:Node),(n0)-[:EDGE]->(n1),(n1)-[:EDGE]->(n2),(n2)-[:EDGE]->(n3)").await.unwrap();
    assert_eq!(
        rows(
            &mut engine,
            "MATCH ()-[r:EDGE]-() MATCH p = (n)-[*0..1]-()-[r]-()-[*0..1]-(m) RETURN count(p)"
        )
        .await,
        vec![vec!["32"]]
    );
}

#[tokio::test]
async fn upstream_map_dot_lookup_is_case_sensitive() {
    let mut engine = GraphEngine::in_memory().unwrap();
    assert_eq!(
        rows(
            &mut engine,
            "WITH {name: 'Mats', Name: 'Pontus'} AS m RETURN m.name, m.Name, m.nAMe IS NULL"
        )
        .await,
        vec![vec!["Mats", "Pontus", "true"]]
    );
}

#[tokio::test]
async fn upstream_relationship_uniqueness_is_scoped_to_one_match_clause() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:N)-[:R]->(:N)").await.unwrap();
    assert_eq!(
        rows(
            &mut engine,
            "MATCH (a)-[r]->(b), (a)-[s]->(b) RETURN count(*)"
        )
        .await,
        vec![vec!["0"]]
    );
    assert_eq!(
        rows(
            &mut engine,
            "MATCH (a)-[r]->(b) MATCH (a)-[s]->(b) RETURN count(*)"
        )
        .await,
        vec![vec!["1"]]
    );
}

#[tokio::test]
async fn upstream_substring_is_zero_based_and_calendar_maps_construct_dates() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        (
            "RETURN substring('0123456789',1),substring('0123456789',1,3),substring('aé日z',1,2)",
            vec!["123456789", "123", "é日"],
        ),
        (
            "WITH date({year:1984,month:10,day:11}) AS d RETURN toString(d),date(toString(d)) = d",
            vec!["1984-10-11", "true"],
        ),
        (
            "WITH date({year:1980,month:12,day:24}) AS x,date({year:1984,month:10,day:11}) AS d RETURN x > d,x < d,x >= d,x <= d,x = d",
            vec!["false", "true", "false", "true", "false"],
        ),
        (
            "WITH datetime({year:1980,month:12,day:11,hour:12,minute:31,second:14,timezone:'+00:00'}) AS x,datetime({year:1984,month:10,day:11,hour:12,minute:31,second:14,timezone:'+05:00'}) AS d RETURN x > d,x < d,x >= d,x <= d,x = d",
            vec!["false", "true", "false", "true", "false"],
        ),
    ] {
        assert_eq!(rows(&mut engine, query).await, vec![expected], "{query}");
    }
    assert!(
        engine
            .cypher("RETURN date({year:2024,month:2,day:30})")
            .await
            .is_err()
    );
}

#[tokio::test]
async fn upstream_duration_preserves_calendar_and_clock_components() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        (
            "RETURN duration({days:14,hours:16,minutes:12}),duration('P14DT16H12M')",
            vec!["P14DT16H12M", "P14DT16H12M"],
        ),
        (
            "RETURN duration({years:12,months:5,days:14,hours:16,minutes:12,seconds:70})",
            vec!["P12Y5M14DT16H13M10S"],
        ),
        (
            "WITH duration({years:12,months:5,days:14,hours:16,minutes:12,seconds:70}) AS x,duration({years:12,months:5,days:13,hours:40,minutes:13,seconds:10}) AS d RETURN x = d",
            vec!["false"],
        ),
        (
            "RETURN duration({days:14,seconds:70,nanoseconds:1})",
            vec!["P14DT1M10.000000001S"],
        ),
        ("RETURN duration('PT70.001S')", vec!["PT1M10.001S"]),
        (
            "RETURN duration({hours:24}) = duration({days:1})",
            vec!["false"],
        ),
        (
            "RETURN duration({seconds:70}) = duration({minutes:1,seconds:10})",
            vec!["true"],
        ),
    ] {
        assert_eq!(rows(&mut engine, query).await, vec![expected], "{query}");
    }
    assert!(engine.cypher("RETURN duration('P1D1D')").await.is_err());
}


#[tokio::test]
async fn cypher_label_sets_preserve_storage_identity_and_gremlin_labels() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:A:B {k: 1}), (:A {k: 2}), ({k: 3})").await.unwrap();
    for (query, expected) in [
        ("MATCH (n) RETURN count(n)", "3"),
        ("MATCH (n:A:B) RETURN count(n)", "1"),
        ("MATCH (n:B) RETURN n.k", "1"),
        ("MATCH (n) WHERE n:B RETURN n.k", "1"),
        ("MATCH (n) WHERE size(labels(n)) = 0 RETURN n.k", "3"),
        ("MATCH (n:A:B) SET n:C REMOVE n:A RETURN n.k", "1"),
        ("MATCH (n:B:C) RETURN n.k", "1"),
        ("MATCH (n:A) RETURN n.k", "2"),
        ("MATCH (n:B) REMOVE n.k RETURN size(keys(n))", "0"),
    ] {
        assert_eq!(rows(&mut engine, query).await, vec![vec![expected]], "{query}");
    }
    let result = engine.gremlin("g.V().hasLabel('A').count()").await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0).unwrap(), "2");
}

#[tokio::test]
async fn cypher_label_sets_survive_incremental_reopen_and_checkpoint() {
    let path = std::env::temp_dir().join(format!("cypher-labels-{}.duckdb", std::process::id()));
    let _ = std::fs::remove_file(&path);
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        engine.cypher("CREATE (a:A:B {k: 1})-[:R]->(b {k: 2})").await.unwrap();
    }
    for checkpoint in [false, true] {
        let mut engine = GraphEngine::open(&path).unwrap();
        assert_eq!(rows(&mut engine, "MATCH (a:B)-[:R]->(b) RETURN a.k, b.k, size(labels(b))").await, vec![vec!["1", "2", "0"]]);
        if checkpoint { engine.checkpoint().unwrap(); }
    }
    {
        let mut engine = GraphEngine::open(&path).unwrap();
        assert_eq!(rows(&mut engine, "MATCH (a:A:B) RETURN count(*)").await, vec![vec!["1"]]);
    }
    std::fs::remove_file(path).unwrap();
}


#[tokio::test]
async fn cypher_collect_null_contract_and_undirected_loop_contract() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in ["UNWIND [null, null] AS x RETURN collect(x)",
        "UNWIND [null, null] AS x RETURN collect(DISTINCT x)",
        "UNWIND [] AS x RETURN collect(x)"] {
        assert_eq!(rows(&mut engine, query).await, vec![vec!["[]"]], "{query}");
    }
    engine.cypher("CREATE (a:A)-[:R]->(a)").await.unwrap();
    assert_eq!(rows(&mut engine, "MATCH p = (a:A)-[r]-(b) RETURN count(p)").await, vec![vec!["1"]]);
    let result = engine.gremlin("g.V().both().count()").await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0), 0).unwrap(), "2");
}
