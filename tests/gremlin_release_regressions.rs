#![cfg(feature = "duckdb")]
use orchiddb::engine::{GraphEngine, QueryResult};

async fn modern() -> GraphEngine {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (m:person {name:'marko',age:29}), (v:person {name:'vadas',age:27}), (j:person {name:'josh',age:32}), (p:person {name:'peter',age:35}), (l:software {name:'lop',lang:'java'}), (r:software {name:'ripple',lang:'java'}), (m)-[:knows {weight:0.5}]->(v), (m)-[:knows {weight:1.0}]->(j), (m)-[:created {weight:0.4}]->(l), (j)-[:created {weight:1.0}]->(r), (j)-[:created {weight:0.4}]->(l), (p)-[:created {weight:0.2}]->(l)").await.unwrap();
    engine
}

fn cells(result: &QueryResult) -> Vec<String> {
    let mut rows = (0..result.returned.batch.num_rows())
        .map(|row| {
            arrow::util::display::array_value_to_string(result.returned.batch.column(0), row)
                .unwrap()
        })
        .collect::<Vec<_>>();
    rows.sort();
    rows
}

#[tokio::test]
async fn filtered_unwind_values() {
    let mut engine = modern().await;
    for (query, expected) in [
        ("g.V().values('age').is(32)", vec!["32"]),
        ("g.V().values('age').is(P.lte(30))", vec!["27", "29"]),
        (
            "g.V().values('age').is(P.gte(29)).is(P.lt(34))",
            vec!["29", "32"],
        ),
        (
            "g.V().has('age',__.is(P.gt(30))).values('name')",
            vec!["josh", "peter"],
        ),
        (
            "g.V().has('name',__.is('marko')).as('a').select('a').values('name')",
            vec!["marko"],
        ),
        (
            "g.V().as('a').out('created').where(__.as('a').values('name').is('josh')).in('created').values('name')",
            vec!["josh", "josh", "marko", "peter"],
        ),
    ] {
        let result = engine.gremlin(query).await.unwrap();
        assert_eq!(cells(&result), expected, "{query}");
        assert_eq!(
            result.stats.cost.native_kernel_calls, 0,
            "{query}: {:?}",
            result.stats
        );
    }
}

#[tokio::test]
async fn where_by_keeps_all_parent_occurrences() {
    let mut engine = modern().await;
    for (query, expected) in [
        (
            "g.V().as('a').out('created').in('created').as('b').where('a',P.gt('b')).by('age').select('a','b').by('name')",
            vec![
                r#"m[{"a":"josh","b":"marko"}]"#,
                r#"m[{"a":"peter","b":"josh"}]"#,
                r#"m[{"a":"peter","b":"marko"}]"#,
            ],
        ),
        (
            "g.V().as('a').outE('created').as('b').inV().as('c').where('a',P.gt('b').or(P.eq('b'))).by('age').by('weight').by('weight').select('a','c').by('name')",
            vec![
                r#"m[{"a":"josh","c":"lop"}]"#,
                r#"m[{"a":"josh","c":"ripple"}]"#,
                r#"m[{"a":"marko","c":"lop"}]"#,
                r#"m[{"a":"peter","c":"lop"}]"#,
            ],
        ),
    ] {
        let result = engine.gremlin(query).await.unwrap();
        assert_eq!(cells(&result), expected, "{query}");
        assert!(
            !result.stats.physical_plan.contains("LateralApply"),
            "{}",
            result.stats.physical_plan
        );
    }
}

#[tokio::test]
async fn repeat_last_keeps_latest_element_label() {
    let mut engine = modern().await;
    for (suffix, expected) in [
        (".select(Pop.last,'a')", vec!["v[lop]", "v[ripple]"]),
        (
            ".select(Pop.last,'a').values('name')",
            vec!["lop", "ripple"],
        ),
    ] {
        let query =
            format!("g.V().has('name','marko').as('a').repeat(__.out().as('a')).times(2){suffix}");
        let result = engine.gremlin(&query).await.unwrap();
        assert_eq!(cells(&result), expected);
    }
}

#[tokio::test]
async fn inject_null_preserves_numeric_values() {
    use arrow::array::{Array, Int64Array};
    let mut engine = modern().await;
    for (suffix, nulls) in [
        ("inject()", 0),
        ("inject(null)", 1),
        ("inject(null,null)", 2),
    ] {
        let result = engine
            .gremlin(&format!("g.V().has('name','marko').values('age').{suffix}"))
            .await
            .unwrap();
        let column = result
            .returned
            .batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .expect("numeric traverser type survives empty/null injection");
        assert_eq!(column.len(), nulls + 1);
        assert_eq!(column.null_count(), nulls);
        assert_eq!(column.iter().flatten().collect::<Vec<_>>(), vec![29]);
    }
}
