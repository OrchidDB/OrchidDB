#![cfg(feature = "duckdb")]
#[path = "../conformance/runner/src/relational_fixture.rs"]
mod relational_fixture;
use orchiddb::ir::{diagnostics::without_constant_folding, plan::GraphPlan};
fn prepare(query: &str) -> GraphPlan {
    orchiddb::language::cypher::preparation::prepare(
        query,
        &Default::default(),
        Some(&Default::default()),
    )
    .unwrap()
}

#[tokio::test]
async fn optional_cardinality_uses_real_schema_and_preserves_duplicates() {
    without_constant_folding(async {
        for (setup, query, expected_rows, no_window) in [
            ("", "OPTIONAL MATCH ()-[r]->() RETURN r.missing", 1, true),
            (
                "",
                "UNWIND [1,1] AS x OPTIONAL MATCH ()-[r]->() RETURN x, r.missing",
                2,
                false,
            ),
            ("", "MATCH ()-[r]->() RETURN r.missing", 0, true),
            (
                "CREATE (:P)-[:R]->(:P), (:P)-[:R]->(:P)",
                "OPTIONAL MATCH ()-[r]->() RETURN r.missing",
                2,
                true,
            ),
            (
                "CREATE (:P)-[:R]->(:P), (:P)-[:R]->(:P)",
                "UNWIND [1,1] AS x OPTIONAL MATCH ()-[r]->() RETURN x, r.missing",
                4,
                false,
            ),
            (
                "CREATE (:P)-[:R]->(:P)",
                "UNWIND [1,2] AS x WITH x LIMIT 1 OPTIONAL MATCH ()-[r]->() RETURN x, r.missing",
                1,
                true,
            ),
        ] {
            let mut fixture = relational_fixture::RelationalFixture::new().unwrap();
            let target = prepare(query);
            let mut plans = if setup.is_empty() {
                vec![]
            } else {
                vec![prepare(setup)]
            };
            plans.push(target.clone());
            fixture.configure_cypher(&plans).unwrap();
            if !setup.is_empty() {
                fixture.prepare(&plans[0]).unwrap();
                fixture.engine.execute_plan(&plans[0]).await.unwrap();
            }
            let report = fixture.diagnostic_sql(&target).await.unwrap();
            if no_window {
                assert!(
                    !report["logical_plan"]
                        .as_str()
                        .unwrap()
                        .contains("row_number"),
                    "{query}: {report}"
                );
            }
            if setup.is_empty() && query == "OPTIONAL MATCH ()-[r]->() RETURN r.missing" {
                assert_eq!(report["regions"][0]["sql"], "SELECT NULL AS \"__region_0\"");
            }
            let result = fixture.engine.execute_plan(&target).await.unwrap();
            assert_eq!(
                result.returned.batch.num_rows(),
                expected_rows,
                "{setup}; {query}"
            );
            let last = result.returned.batch.columns().last().unwrap();
            assert_eq!(
                last.null_count(),
                expected_rows,
                "missing property must remain NULL: {query}"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn terminal_distinct_order_demand_survives_sql_island_selection() {
    without_constant_folding(async {
        let target = prepare("UNWIND [1,1,2,null,null] AS x RETURN DISTINCT x ORDER BY x");
        let mut fixture = relational_fixture::RelationalFixture::new().unwrap();
        fixture.configure_cypher(&[target.clone()]).unwrap();
        let report = fixture.diagnostic_sql(&target).await.unwrap();
        let regions = report["regions"].as_array().unwrap();
        assert!(!regions.is_empty(), "{report}");
        for region in regions {
            assert!(
                !region["sql"]
                    .as_str()
                    .unwrap()
                    .to_lowercase()
                    .contains("row_number"),
                "{report}"
            );
        }
        let result = fixture.engine.execute_plan(&target).await.unwrap();
        let values = (0..result.returned.batch.num_rows())
            .map(|i| {
                arrow::util::display::array_value_to_string(result.returned.batch.column(0), i)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(values, ["1", "2", ""]);
    })
    .await;
}

#[tokio::test]
async fn mapped_optional_direct_joins_preserve_parallel_edges_and_null_rows() {
    without_constant_folding(async {
        for (query, expected) in [
            ("UNWIND [1,1] AS k MATCH (a:A) OPTIONAL MATCH (a)-[r:R]->(b:B) RETURN k, b.v", vec!["1|2", "1|2", "1|2", "1|2"]),
            ("UNWIND [1,1] AS k MATCH (a:A), (b:B) OPTIONAL MATCH (a)-[r:R]->(b) RETURN k, b.v", vec!["1|2", "1|2", "1|2", "1|2"]),
            ("MATCH (a:A) OPTIONAL MATCH (a)-[r:R]->(b:Missing) RETURN b.v", vec![""]),
            ("MATCH (a:A), (b:B) OPTIONAL MATCH (a)-[r:R]->(b) WITH b, r WHERE r IS NOT NULL RETURN b.v", vec!["2", "2"]),
            ("MATCH (a:A), (b:B) OPTIONAL MATCH (a)-[r:R]->(b) WITH b, r WHERE r IS NULL RETURN b.v", vec![]),
        ] {
            let setup = prepare("CREATE (a:A {v:1}), (b:B {v:2}), (a)-[:R]->(b), (a)-[:R]->(b)");
            let target = prepare(query);
            let mut fixture = relational_fixture::RelationalFixture::new().unwrap();
            fixture.configure_cypher(&[setup.clone(), target.clone()]).unwrap();
            fixture.prepare(&setup).unwrap();
            fixture.engine.execute_plan(&setup).await.unwrap();
            let report=fixture.diagnostic_sql(&target).await.unwrap();
            assert!(report["error"].is_null(), "{query}: {report}");
            let result=fixture.engine.execute_plan(&target).await.unwrap();
            let batch=result.returned.batch;
            let mut actual=(0..batch.num_rows()).map(|i|batch.columns().iter().map(|a|
                arrow::util::display::array_value_to_string(a,i).unwrap()).collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
            actual.sort();
            assert_eq!(actual,expected,"{query}: {report}");
        }
    }).await;
}

#[tokio::test]
async fn shared_partition_scans_preserve_binding_aliases_and_cartesian_multiplicity() {
    without_constant_folding(async {
        let setup = prepare("CREATE (:A {v:1}), (:B {v:2})");
        let target = prepare("MATCH (a), (b) RETURN a.v, b.v");
        let mut fixture = relational_fixture::RelationalFixture::new().unwrap();
        fixture
            .configure_cypher(&[setup.clone(), target.clone()])
            .unwrap();
        fixture.prepare(&setup).unwrap();
        fixture.engine.execute_plan(&setup).await.unwrap();
        let report = fixture.diagnostic_sql(&target).await.unwrap();
        assert!(report.to_string().contains("shared_scan"), "{report}");
        let result = fixture.engine.execute_plan(&target).await.unwrap();
        let batch = result.returned.batch;
        let mut actual = (0..batch.num_rows())
            .map(|i| {
                batch
                    .columns()
                    .iter()
                    .map(|a| arrow::util::display::array_value_to_string(a, i).unwrap())
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect::<Vec<_>>();
        actual.sort();
        assert_eq!(actual, ["1|1", "1|2", "2|1", "2|2"]);
    })
    .await;
}

#[tokio::test]
async fn fixed_expand_property_filters_preserve_trails_and_new_bindings() {
    without_constant_folding(async {
        let setup=prepare("CREATE (a:A {id:1}), (b:B {id:3}), (c:C {id:2}), (a)-[:R]->(b), (a)-[:R]->(b), (b)-[:R]->(c)");
        for query in [
            "MATCH (a)--(b)--(c) WHERE a.id=1 AND c.id=2 RETURN b.id",
            "MATCH (a)--(b)--(a) WHERE a.id=1 RETURN b.id",
            "MATCH (a)--(b)--(c) WITH a,b,c WHERE a.id=1 AND c.id=2 RETURN b.id",
        ] {
            let target=prepare(query);
            let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
            fixture.configure_cypher(&[setup.clone(),target.clone()]).unwrap();
            fixture.prepare(&setup).unwrap();
            fixture.engine.execute_plan(&setup).await.unwrap();
            let result=fixture.engine.execute_plan(&target).await.unwrap_or_else(|e|panic!("{query}: {e}"));
            let batch=result.returned.batch;
            assert_eq!(batch.num_rows(),2,"{query}");
            for i in 0..batch.num_rows() {
                assert_eq!(arrow::util::display::array_value_to_string(batch.column(0),i).unwrap(),"3","{query}");
            }
        }
    }).await;
}

#[tokio::test]
async fn deleting_a_collected_relationship_keeps_its_storage_identity() {
    let setup=prepare("CREATE (u:User) CREATE (u)-[:FRIEND]->() CREATE (u)-[:FRIEND]->() CREATE (u)-[:FRIEND]->() CREATE (u)-[:FRIEND]->()");
    let delete=prepare("MATCH (:User)-[r:FRIEND]->() WITH collect(r) AS friendships DETACH DELETE friendships[1]");
    let count=prepare("MATCH ()-[r]->() RETURN count(r)");
    let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
    fixture.configure_cypher(&[setup.clone(),delete.clone(),count.clone()]).unwrap();
    fixture.prepare(&setup).unwrap();
    fixture.engine.execute_plan(&setup).await.unwrap();
    fixture.prepare(&delete).unwrap();
    fixture.engine.execute_plan(&delete).await.unwrap();
    let result=fixture.engine.execute_plan(&count).await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(),"3");
}

#[tokio::test]
async fn merge_matches_existing_list_properties() {
    let setup=prepare("CREATE (a:A), (b:B) CREATE (a)-[:T {numbers:[42,43]}]->(b)");
    let merge=prepare("MATCH (a:A), (b:B) MERGE (a)-[r:T {numbers:[42,43]}]->(b) RETURN count(*)");
    let check=prepare("MATCH ()-[r:T]->() RETURN r.numbers=[42,43] AS eq");
    let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
    fixture.configure_cypher(&[setup.clone(),merge.clone(),check.clone()]).unwrap();
    fixture.prepare(&setup).unwrap();
    fixture.engine.execute_plan(&setup).await.unwrap();
    let result=fixture.engine.execute_plan(&check).await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(),"true");
    fixture.prepare(&merge).unwrap();
    fixture.engine.execute_plan(&merge).await.unwrap();
    let result=fixture.engine.execute_plan(&check).await.unwrap();
    assert_eq!(result.returned.batch.num_rows(),1);
}

#[tokio::test]
async fn temporal_properties_keep_their_type_in_mapped_storage() {
    for (constructor, literal) in [("date", "1910-05-06"), ("localtime", "12:30:00"), ("duration", "P1D")] {
        let setup=prepare(&format!("CREATE (:Event {{value: {constructor}('{literal}'), values: [{constructor}('{literal}')]}})"));
        let check=prepare(&format!("MATCH (e:Event) RETURN e.value = {constructor}('{literal}') AS eq, e.values[0] = {constructor}('{literal}') AS list_eq"));
        let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
        fixture.configure_cypher(&[setup.clone(),check.clone()]).unwrap();
        fixture.prepare(&setup).unwrap();
        fixture.engine.execute_plan(&setup).await.unwrap();
        let result=fixture.engine.execute_plan(&check).await.unwrap();
        assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(),"true", "{constructor}");
        assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(1),0).unwrap(),"true", "list {constructor}");
        if constructor == "date" {
            let fields = prepare("MATCH (e:Event) WITH e.value AS d RETURN d.year, d.month, d.day");
            let result = fixture.engine.execute_plan(&fields).await.unwrap();
            for (column, expected) in ["1910", "5", "6"].into_iter().enumerate() {
                assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(column),0).unwrap(),expected);
            }
        }
    }
}

#[tokio::test]
async fn mapped_and_nested_list_comparisons_use_the_shared_semantic_helper() {
    without_constant_folding(async {
        for (query, expected) in [
            ("MATCH ()-[r:T]->() RETURN r.numbers=[42,43] AS eq", "true"),
            ("RETURN ['a',null] = ['a',null] AS eq", ""),
            ("RETURN [[1],[2]] = [[1.0],[2.0]] AS eq", "true"),
            ("RETURN [9007199254740993] = [9007199254740992.0] AS eq", "false"),
            ("RETURN ['a'] < ['b'] AS eq", "true"),
        ] {
            let setup=prepare("CREATE ()-[:T {numbers:[42,43]}]->()");
            let target=prepare(query);
            let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
            fixture.configure_cypher(&[setup.clone(),target.clone()]).unwrap();
            fixture.prepare(&setup).unwrap();
            fixture.engine.execute_plan(&setup).await.unwrap();
            let report=fixture.diagnostic_sql(&target).await.unwrap();
            assert!(report.to_string().contains("__orchiddb_cypher_list_compare"), "{query}: {report}");
            let result=fixture.engine.execute_plan(&target).await.unwrap();
            assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(),expected,"{query}");
        }
    }).await;
}

#[tokio::test]
async fn mapped_writes_span_multiple_duckdb_vectors_and_zero_limit_is_empty() {
    let setup=prepare("UNWIND range(0,4096) AS i CREATE (:Person {num:i})");
    let count=prepare("MATCH (p:Person) RETURN count(p)");
    let empty=prepare("MATCH (p:Person) RETURN p.num ORDER BY p.num LIMIT 0");
    let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
    fixture.configure_cypher(&[setup.clone(),count.clone(),empty.clone()]).unwrap();
    fixture.prepare(&setup).unwrap();
    fixture.engine.execute_plan(&setup).await.unwrap();
    let result=fixture.engine.execute_plan(&count).await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(),"4097");
    let result=fixture.engine.execute_plan(&empty).await.unwrap();
    assert_eq!(result.returned.batch.num_rows(),0);
}

#[tokio::test]
async fn entity_key_membership_is_a_property_presence_test() {
    without_constant_folding(async {
        for (setup, query, expected) in [
            ("CREATE (:P {exists:1})", "MATCH (n:P) RETURN 'exists' IN keys(n), 'missing' IN keys(n)", vec!["true", "false"]),
            ("CREATE (:P)", "OPTIONAL MATCH (n:Missing) RETURN 'exists' IN keys(n)", vec![""]),
            ("CREATE ()-[:T {exists:1}]->()", "MATCH ()-[r:T]->() RETURN 'exists' IN keys(r), 'missing' IN keys(r)", vec!["true", "false"]),
        ] {
            let setup=prepare(setup);let target=prepare(query);
            let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
            fixture.configure_cypher(&[setup.clone(),target.clone()]).unwrap();
            fixture.prepare(&setup).unwrap();fixture.engine.execute_plan(&setup).await.unwrap();
            let report=fixture.diagnostic_sql(&target).await.unwrap();
            assert!(!report.to_string().contains("array_position"),"{report}");
            let result=fixture.engine.execute_plan(&target).await.unwrap();
            for (column,expected) in expected.into_iter().enumerate() {
                assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(column),0).unwrap(),expected,"{query}");
            }
        }
    }).await;
}

#[tokio::test]
async fn shared_undirected_scans_preserve_self_loops_and_parallel_edge_trails() {
    without_constant_folding(async {
        let setup=prepare("CREATE (a:A), (b:B), (a)-[:T]->(a), (a)-[:T]->(b), (a)-[:T]->(b)");
        let target=prepare("MATCH (a)--(b)--(c) RETURN count(*)");
        let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
        fixture.configure_cypher(&[setup.clone(),target.clone()]).unwrap();
        fixture.prepare(&setup).unwrap();
        fixture.engine.execute_plan(&setup).await.unwrap();
        let report=fixture.diagnostic_sql(&target).await.unwrap();
        assert!(report.to_string().contains("shared_scan"), "{report}");
        let result=fixture.engine.execute_plan(&target).await.unwrap();
        assert_eq!(arrow::util::display::array_value_to_string(result.returned.batch.column(0),0).unwrap(),"8");
    }).await;
}

#[tokio::test]
async fn aesthetic_collection_and_fixed_paths_preserve_results() {
    without_constant_folding(async {
        for (setup, query, expected) in [
            ("", "RETURN single(x IN [] WHERE x) AS result", vec!["false"]),
            ("", "RETURN none(x IN [null] WHERE x) AS result", vec![""]),
            ("", "UNWIND [[1,2,2],[],null] AS xs RETURN size([x IN xs WHERE x > 1]) AS result", vec!["2", "0", ""]),
            ("CREATE (a:P)-[:R]->(b:P), (a)-[:R]->(b), (b)-[:R]->(b)", "MATCH ()-[r:R]->()-[s:R]->() RETURN count(*) AS result", vec!["2"]),
            ("CREATE (a:P)-[:R]->(b:P), (a)-[:S]->(b), (b)-[:S]->(b)", "MATCH ()-[:R]->()-[:S]->() RETURN count(*) AS result", vec!["1"]),
        ] {
            let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
            let target=prepare(query);
            let mut plans=if setup.is_empty() {vec![]} else {vec![prepare(setup)]};
            plans.push(target.clone()); fixture.configure_cypher(&plans).unwrap();
            if !setup.is_empty() {fixture.prepare(&plans[0]).unwrap();fixture.engine.execute_plan(&plans[0]).await.unwrap();}
            let diagnostic=fixture.diagnostic_sql(&target).await.unwrap();
            let sql=diagnostic["regions"].as_array().unwrap().iter().map(|r|r["sql"].as_str().unwrap()).collect::<Vec<_>>().join("\n");
            if query.contains("size([") {assert!(!sql.contains("row_number"),"{sql}");}
            if query.starts_with("MATCH") {assert!(!sql.contains("array_has(\"__cypher_match_history"),"{sql}");}
            let result=fixture.engine.execute_plan(&target).await.unwrap();
            let values=(0..result.returned.batch.num_rows()).map(|i|arrow::util::display::array_value_to_string(result.returned.batch.column(0),i).unwrap()).collect::<Vec<_>>();
            assert_eq!(values,expected,"{query}\n{sql}");
        }
    }).await;
}

#[tokio::test]
async fn conformance_plan_audit_explains_real_tables_without_query_results() {
    let mut fixture = relational_fixture::RelationalFixture::new().unwrap();
    let setup = prepare("CREATE (:P {num: 1}), (:P {num: 2})");
    let target = prepare("MATCH (n:P) WHERE n.num > 0 RETURN n.num AS value");
    fixture.configure_cypher(&[setup.clone(), target.clone()]).unwrap();
    fixture.prepare(&setup).unwrap();
    fixture.engine.execute_plan(&setup).await.unwrap();
    let plain = fixture.diagnostic_sql(&target).await.unwrap();
    assert!(plain.get("sql_plans").is_none());
    let report = fixture.diagnostic_sql_with_estimates(&target, true).await.unwrap();
    assert_eq!(report["capture"], "prepared_not_executed");
    assert_eq!(report["regions"], plain["regions"]);
    let plans = report["sql_plans"].as_array().unwrap();
    assert!(!plans.is_empty());
    for plan in plans {
        assert!(plan.get("error").is_none(), "{plan}");
        assert!(plan["plan"].is_array(), "{plan}");
    }
    let encoded = report["sql_plans"].to_string();
    assert!(encoded.contains("Estimated Cardinality"), "{encoded}");
    assert!(encoded.contains("SEQ_SCAN"), "{encoded}");
}

#[tokio::test]
async fn mapped_correlations_bind_and_execute_in_both_folding_modes() {
    for nofold in [false,true] {
        let work=async {
            let mut fixture=relational_fixture::RelationalFixture::new().unwrap();
            let setup=prepare("CREATE (a:P {num: 1, xs: [1,2]}), (b:P {num: 2, xs: []}), (:P {num: 3}), (a)-[:R]->(b)");
            let cases=[
                ("all(x IN n.xs WHERE x > 0)", vec!["true","true",""]),
                ("any(x IN n.xs WHERE x = 2)", vec!["true","false",""]),
                ("none(x IN n.xs WHERE x < 0)", vec!["true","true",""]),
                ("single(x IN n.xs WHERE x = 2)", vec!["true","false",""]),
                ("size([x IN n.xs WHERE x > 1])",vec!["1","0",""]),
                ("size([(n)-[:R]->(m) | 1]) > 0",vec!["true","false","false"]),
            ];
            let plans=cases.iter().map(|(expr,_)|prepare(&format!("MATCH (n:P) RETURN {expr} AS result ORDER BY n.num"))).collect::<Vec<_>>();
            let mut all=vec![setup.clone()];all.extend(plans.clone());
            fixture.configure_cypher(&all).unwrap();fixture.prepare(&setup).unwrap();fixture.engine.execute_plan(&setup).await.unwrap();
            for (plan,(_,expected)) in plans.iter().zip(cases) {
                let report=fixture.diagnostic_sql_with_estimates(plan,true).await.unwrap();
                for sql in report["sql_plans"].as_array().unwrap() {assert!(sql.get("error").is_none(),"{sql}");}
                let result=fixture.engine.execute_plan(plan).await.unwrap();
                let values=(0..result.returned.batch.num_rows()).map(|i|arrow::util::display::array_value_to_string(result.returned.batch.column(0),i).unwrap()).collect::<Vec<_>>();
                assert_eq!(values,expected,"{report}");
            }
        };
        if nofold {without_constant_folding(work).await;} else {work.await;}
    }
}
