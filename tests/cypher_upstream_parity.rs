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


#[tokio::test]
async fn cypher_identity_is_unique_across_storage_groups_and_stable_after_writes() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine.cypher("CREATE (a:B:A:D), (b:B:C), (c:D:E:B)").await.unwrap();
    assert_eq!(result.returned.batch.num_rows(), 0);
    assert_eq!(rows(&mut engine, "MATCH (n) RETURN count(DISTINCT id(n))").await, vec![vec!["3"]]);
    let before = rows(&mut engine, "MATCH (n:D:E) RETURN id(n)").await;
    engine.cypher("CREATE (:A), (:Z)").await.unwrap();
    engine.cypher("MATCH (n:E) SET n:Other REMOVE n:D").await.unwrap();
    assert_eq!(rows(&mut engine, "MATCH (n:Other) RETURN id(n)").await, before);
    engine.checkpoint().unwrap();
    assert_eq!(rows(&mut engine, "MATCH (n:Other) RETURN id(n)").await, before);
}


#[tokio::test]
async fn cypher_bound_optional_relationship_and_constant_sort() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:A)-[:T]->(:B)").await.unwrap();
    assert_eq!(rows(&mut engine, "MATCH (a1)-[r]->() WITH r, a1 LIMIT 1 OPTIONAL MATCH (a1)<-[r]-(b2) RETURN b2 IS NULL").await, vec![vec!["true"]]);
    assert_eq!(rows(&mut engine, "MATCH (a)-->(b) RETURN DISTINCT b ORDER BY b.name").await.len(), 1);
}


#[tokio::test]
async fn cypher_compound_comparison_and_overloaded_addition() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        ("RETURN [1, 2] >= [1, null]", ""),
        ("RETURN 0.0 / 0.0 < 1", "false"),
        ("WITH 'a' AS a, 'b' AS b RETURN a + b", "ab"),
        ("WITH [1, 2] AS a RETURN a + [3]", "[1,2,3]"),
    ] { assert_eq!(rows(&mut engine, query).await, vec![vec![expected]], "{query}"); }
}

#[tokio::test]
async fn cypher_temporal_values_survive_query_parts_and_properties() {
    let mut engine = GraphEngine::in_memory().unwrap();
    rows(&mut engine, "CREATE (:Event {date: date('201507'), times: [localtime('12:31:14.645876123')]})").await;
    assert_eq!(rows(&mut engine, "MATCH (e:Event) WITH e.date AS d, e.times[0] AS t RETURN d.year, d.month, t.nanosecond, d = date('2015-07-01')").await,
        vec![vec!["2015", "7", "645876123", "true"]]);
    assert_eq!(rows(&mut engine, "WITH datetime('1984-10-11T12:31:14.645876123+01:00') AS d RETURN datetime(toString(d)) = d, d.offsetSeconds").await,
        vec![vec!["true", "3600"]]);
}

#[tokio::test]
async fn cypher_temporal_calendar_arithmetic_and_duration_precision() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        ("RETURN toString(date('2024-01-31') + duration('P1M'))", "2024-02-29"),
        ("RETURN toString(datetime('2024-03-30T12:00+01:00[Europe/Stockholm]') + duration('P1D'))", "2024-03-31T12:00+02:00[Europe/Stockholm]"),
        ("RETURN toString(duration({seconds: 9007199254740993, nanoseconds: 1}))", "PT2501999792983H36M33.000000001S"),
        ("RETURN toString(duration({years: 12, months: 5, days: 14, hours: 16, minutes: 12, seconds: 70, nanoseconds: 1}) / 2)", "P6Y2M22DT13H21M8S"),
    ] {
        assert_eq!(rows(&mut engine,query).await,vec![vec![expected]],"{query}");
    }
}

#[tokio::test]
async fn cypher_temporal_duration_difference_and_truncation() {
    let mut engine = GraphEngine::in_memory().unwrap();
    assert_eq!(rows(&mut engine, "WITH duration.between(localdatetime('2018-01-02T10:00:00.1'), localdatetime('2018-01-01T10:00:00.2')) AS d RETURN toString(d), d.seconds, d.nanosecondsOfSecond").await,
        vec![vec!["PT-23H-59M-59.9S", "-86400", "100000000"]]);
    assert_eq!(rows(&mut engine, "RETURN toString(duration.between(date('1984-10-11'), date('2015-06-24'))), toString(date.truncate('month', date('2024-02-17')))").await,
        vec![vec!["P30Y8M13D", "2024-02-01"]]);
}

#[tokio::test]
async fn cypher_temporal_projection_preserves_instant_and_overlap_offset() {
    let mut engine = GraphEngine::in_memory().unwrap();
    assert_eq!(rows(&mut engine, "WITH time('12:31:14.645876+01:00') AS t RETURN toString(time({time:t, timezone:'+05:00', second:42}))").await,
        vec![vec!["16:31:42.645876+05:00"]]);
    assert_eq!(rows(&mut engine, "WITH datetime('2017-10-29T02:30+01:00[Europe/Stockholm]') AS d RETURN datetime(toString(d)) = d, d.offsetSeconds").await,
        vec![vec!["true", "3600"]]);
}

#[tokio::test]
async fn cypher_mixed_values_keep_types_through_unwind_and_aggregation() {
    let mut engine = GraphEngine::in_memory().unwrap();
    assert_eq!(rows(&mut engine, "UNWIND [1, 'a', null, [1,2], 0.2, 'b'] AS x RETURN max(x), min(x)").await,
        vec![vec!["1", "[1,2]"]]);
    assert_eq!(rows(&mut engine, "UNWIND [1, 'a', [1,2]] AS x WITH collect(x) AS xs RETURN xs[0] = 1, xs[1] = 'a', xs[2] = [1,2]").await,
        vec![vec!["true", "true", "true"]]);
}

#[tokio::test]
async fn cypher_temporal_projection_and_truncation_overrides() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, expected) in [
        ("RETURN toString(localtime({hour:12,minute:31,second:14,millisecond:123,microsecond:456,nanosecond:789}))", "12:31:14.123456789"),
        ("RETURN toString(time({time:localtime('12:31')}))", "12:31Z"),
        ("RETURN toString(datetime.truncate('month', date('2024-02-29')))", "2024-02-01T00:00Z"),
        ("RETURN toString(datetime.truncate('hour', datetime('1984-10-11T12:31-01:00'), {timezone:'Europe/Stockholm'}))", "1984-10-11T12:00+01:00[Europe/Stockholm]"),
        ("RETURN toString(localtime.truncate('microsecond',localtime('12:31:14.645876123'),{nanosecond:2}))", "12:31:14.645876002"),
    ] { assert_eq!(rows(&mut engine,query).await,vec![vec![expected]],"{query}"); }
}

#[tokio::test]
async fn cypher_percentile_errors_retain_runtime_diagnosis() {
    use new_graph::language::cypher::{parser::parse_query, planner::CypherPlanner};
    use new_graph::ir::diagnostics::RuntimeDiagnosis;
    let mut engine = GraphEngine::in_memory().unwrap();
    rows(&mut engine,"CREATE (:Price {v:10})").await;
    for function in ["percentileCont","percentileDisc"] {
        for fraction in ["-1","1.1","1000"] {
            let query=parse_query(&format!("MATCH (n:Price) RETURN {function}(n.v,{fraction})")).unwrap();
            let plan=CypherPlanner::new().plan(&query).unwrap();
            let error=engine.execute_plan_with_diagnostics(&plan).await.unwrap_err();
            assert_eq!(error.diagnosis,Some(RuntimeDiagnosis::NumberOutOfRange),"{error}");
        }
    }
    assert_eq!(rows(&mut engine,"UNWIND [1,2,3,4] AS v RETURN percentileDisc(v,0.25)").await,vec![vec!["1"]]);
}

#[tokio::test]
async fn cypher_nan_respects_operand_comparability() {
    let mut engine=GraphEngine::in_memory().unwrap();
    assert_eq!(rows(&mut engine,"RETURN (0.0/0.0 < 'a') IS NULL, (0.0/0.0 >= 'a') IS NULL, 0.0/0.0 > 1").await,
        vec![vec!["true","true","false"]]);
}

#[tokio::test]
async fn cypher_duration_differences_respect_clock_and_inherited_zone() {
    let mut engine=GraphEngine::in_memory().unwrap();
    for (query,expected) in [
        ("RETURN toString(duration.between(datetime('2014-07-21T21:40:36.143+0200'),datetime('2015-07-21T21:40:32.142+0100')))","P1YT59M55.999S"),
        ("RETURN toString(duration.inMonths(date('2018-07-21'),datetime('2016-07-21T21:40:32.142+0100')))","P-1Y-11M"),
        ("RETURN toString(duration.inDays(datetime('2014-07-21T21:40:36.143+0200'),date('2015-06-24')))","P337D"),
        ("RETURN toString(duration.inSeconds(datetime('2017-10-29T00:00+02:00[Europe/Stockholm]'),localdatetime('2017-10-29T04:00')))","PT5H"),
    ] { assert_eq!(rows(&mut engine,query).await,vec![vec![expected]],"{query}"); }
}

#[tokio::test]
async fn diagnosed_runtime_failure_rolls_back_writes() {
    use new_graph::language::cypher::{parser::parse_query,planner::CypherPlanner};
    use new_graph::ir::diagnostics::RuntimeDiagnosis;
    let mut engine=GraphEngine::in_memory().unwrap();
    let query=parse_query("CREATE (n:Price {v:10}) RETURN percentileCont(n.v,2)").unwrap();
    let plan=CypherPlanner::new().plan(&query).unwrap();
    let error=engine.execute_plan_with_diagnostics(&plan).await.unwrap_err();
    assert_eq!(error.diagnosis,Some(RuntimeDiagnosis::NumberOutOfRange));
    assert_eq!(rows(&mut engine,"MATCH (n:Price) RETURN count(n)").await,vec![vec!["0"]]);
}

#[tokio::test]
async fn cypher_integer_boundaries_and_dynamic_property_types() {
    let mut engine=GraphEngine::in_memory().unwrap();
    assert_eq!(rows(&mut engine,"RETURN -9223372036854775808, 9223372036854775807").await,
        vec![vec!["-9223372036854775808","9223372036854775807"]]);
    rows(&mut engine,"CREATE (:Item {name:42, age:'young', foo:'present'})").await;
    assert_eq!(rows(&mut engine,"MATCH (n:Item) RETURN n.name+1, n.age+'!', n.foo, n.missing IS NULL").await,
        vec![vec!["43","young!","present","true"]]);
}

#[tokio::test]
async fn cypher_coalesce_preserves_selected_type_and_short_circuits() {
    let mut engine=GraphEngine::in_memory().unwrap();
    assert_eq!(rows(&mut engine,"RETURN coalesce(1,'text') = 1, coalesce(null,[1,true]) = [1,true], coalesce(1,1/0), {a:1}.missing IS NULL").await,
        vec![vec!["true","true","1","true"]]);
    rows(&mut engine,"CREATE (:N)").await;
    assert_eq!(rows(&mut engine,"MATCH (n:N) RETURN id(n)+1").await,vec![vec!["1"]]);
}

#[tokio::test]
async fn cypher_subscript_rejects_coercions_and_preserves_nulls() {
    use new_graph::language::cypher::{parser::parse_query,planner::CypherPlanner};
    use new_graph::ir::diagnostics::RuntimeDiagnosis;
    let mut engine=GraphEngine::in_memory().unwrap();
    for (query,diagnosis) in [
        ("RETURN [1,2][true]",RuntimeDiagnosis::InvalidType),
        ("RETURN [1,2][1.5]",RuntimeDiagnosis::InvalidType),
        ("RETURN [1,2]['0']",RuntimeDiagnosis::InvalidType),
        ("RETURN 'abc'[0]",RuntimeDiagnosis::InvalidType),
        ("RETURN {a:1}[0]",RuntimeDiagnosis::MapKeyType),
    ] {
        let plan=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let error=engine.execute_plan_with_diagnostics(&plan).await.unwrap_err();
        assert_eq!(error.diagnosis,Some(diagnosis),"{query}: {error}");
    }
    assert_eq!(rows(&mut engine,"RETURN [1,2][-1], [1,2][4] IS NULL, [1,2][null] IS NULL, null[0] IS NULL").await,
        vec![vec!["2","true","true","true"]]);
}

#[tokio::test]
async fn cypher_range_validates_types_and_includes_integer_endpoints() {
    use new_graph::language::cypher::{parser::parse_query,planner::CypherPlanner};
    use new_graph::ir::diagnostics::RuntimeDiagnosis;
    let mut engine=GraphEngine::in_memory().unwrap();
    for (query,diagnosis) in [
        ("RETURN range(1,3,0)",RuntimeDiagnosis::NumberOutOfRange),
        ("RETURN range(true,3)",RuntimeDiagnosis::ArgumentType),
        ("RETURN range(1,'3')",RuntimeDiagnosis::ArgumentType),
        ("RETURN range(1,3,1.5)",RuntimeDiagnosis::ArgumentType),
    ] {
        let plan=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let error=engine.execute_plan_with_diagnostics(&plan).await.unwrap_err();
        assert_eq!(error.diagnosis,Some(diagnosis),"{query}: {error}");
    }
    assert_eq!(rows(&mut engine,"RETURN range(9223372036854775806,9223372036854775807) = [9223372036854775806,9223372036854775807], range(1,3) = [1,2,3]").await,
        vec![vec!["true","true"]]);
}

#[tokio::test]
async fn cypher_conversions_distinguish_invalid_types_from_invalid_text() {
    use new_graph::language::cypher::{parser::parse_query,planner::CypherPlanner};
    use new_graph::ir::diagnostics::RuntimeDiagnosis;
    let mut engine=GraphEngine::in_memory().unwrap();
    for expression in ["toBoolean([])","toBoolean(1.5)","toInteger({})", "toFloat(true)","toString([1])"] {
        let query=format!("RETURN {expression}");
        let plan=CypherPlanner::new().plan(&parse_query(&query).unwrap()).unwrap();
        let error=engine.execute_plan_with_diagnostics(&plan).await.unwrap_err();
        assert_eq!(error.diagnosis,Some(RuntimeDiagnosis::InvalidValue),"{query}: {error}");
    }
    assert_eq!(rows(&mut engine,"RETURN toInteger('bad') IS NULL, toFloat('bad') IS NULL, toBoolean('bad') IS NULL, toInteger('2.9'), toString(date('2026-09-24'))").await,
        vec![vec!["true","true","true","2","2026-09-24"]]);
}
