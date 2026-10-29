#![cfg(feature = "duckdb")]
use orchiddb::engine::GraphEngine;

#[tokio::test]
async fn union_preserves_exact_mixed_numbers_and_language_equality() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (query, count) in [
        (
            "RETURN 9007199254740993 AS x UNION RETURN 9007199254740992.0 AS x",
            2,
        ),
        ("RETURN 1 AS x UNION RETURN 1.0 AS x", 1),
        ("RETURN 1 AS x UNION ALL RETURN 1.0 AS x", 2),
        ("RETURN null AS x UNION RETURN 1 AS x", 2),
    ] {
        let result = engine
            .cypher(query)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(
            result.returned.batch.num_rows(),
            count,
            "{query}\n{:?}\n{}",
            result.stats.sql_queries,
            result.stats.physical_plan
        );
    }
}

#[tokio::test]
async fn typed_predicates_do_not_borrow_sql_coercion() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in [
        "WITH [1,2,3] AS xs RETURN size([x IN xs WHERE 1])",
        "RETURN any(x IN [1,2] WHERE 'true')",
        "WITH 1 AS x WHERE x RETURN x",
        "RETURN CASE WHEN 1 THEN 42 ELSE 0 END",
    ] {
        assert!(engine.cypher(query).await.is_err(), "{query}");
    }
    for query in [
        "WITH [1,2] AS xs RETURN size([x IN xs WHERE null])",
        "RETURN all(x IN [true,null] WHERE x)",
    ] {
        engine.cypher(query).await.unwrap();
    }
}

#[tokio::test]
async fn predicate_validation_observes_collection_evaluation_scope() {
    use orchiddb::ir::diagnostics::{RuntimeDiagnosis, without_constant_folding};
    without_constant_folding(async {
        let mut engine = GraphEngine::in_memory().unwrap();
        for query in [
            "RETURN none(x IN [] WHERE x)",
            "UNWIND [null,null] AS x WITH collect(x) AS xs RETURN all(y IN xs WHERE y)",
            "RETURN all(x IN [] WHERE 1)",
            "RETURN CASE WHEN false THEN all(x IN [1] WHERE x) ELSE true END",
        ] {
            let result = engine
                .cypher(query)
                .await
                .unwrap_or_else(|e| panic!("{query}: {e}"));
            let column = result
                .returned
                .batch
                .column(0)
                .as_any()
                .downcast_ref::<arrow::array::BooleanArray>()
                .unwrap();
            assert_eq!(
                column.iter().collect::<Vec<_>>(),
                vec![Some(true)],
                "{query}"
            );
        }
        for query in [
            "RETURN all(x IN [1] WHERE x)",
            "RETURN all(x IN [1] WHERE 1)",
            "WITH [1,2,3] AS xs RETURN size([x IN xs WHERE 1])",
        ] {
            let plan =
                orchiddb::language::cypher::preparation::prepare(query, &Default::default(), None)
                    .unwrap();
            let error = engine
                .execute_plan_with_diagnostics(&plan)
                .await
                .unwrap_err();
            assert_eq!(
                error.diagnosis,
                Some(RuntimeDiagnosis::InvalidType),
                "{query}: {error}"
            );
        }
    })
    .await;
}

#[tokio::test]
async fn constant_invalid_bounds_fail_independently_of_input_rows() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for input in ["[]", "[1]"] {
        for clause in ["SKIP", "LIMIT"] {
            let query = format!("UNWIND {input} AS x RETURN x {clause} toInteger(-1)");
            let plan =
                orchiddb::language::cypher::preparation::prepare(&query, &Default::default(), None)
                    .unwrap();
            let error = engine
                .execute_plan_with_diagnostics(&plan)
                .await
                .unwrap_err();
            assert_eq!(
                error.diagnosis,
                Some(orchiddb::ir::diagnostics::RuntimeDiagnosis::NegativeIntegerArgument),
                "{query}: {error}"
            );
        }
    }
}

#[tokio::test]
async fn shortest_path_sql_preserves_ties_duplicates_and_zero_hops() {
    use orchiddb::ir::{
        Value,
        plan::{Direction, GraphPlan, LabelExpr, Node},
        policy::{GraphPlanPolicy, ResultForm},
    };
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (a:N {name:'a'}),(b:N {name:'b'}),(c:N {name:'c'}),(d:N {name:'d'}),(a)-[:R]->(b),(a)-[:R]->(c),(b)-[:R]->(d),(c)-[:R]->(d),(d)-[:R]->(a)").await.unwrap();
    let node = |id: i64| Value::Node {
        label: "N".into(),
        id: id.into(),
    };
    for (target, all_paths, include_edges, expected) in [
        (3_i64, false, true, 2),
        (3, true, true, 4),
        (0, true, true, 2),
        (3, false, false, 2),
    ] {
        let plan = GraphPlan::new(
            GraphPlanPolicy::cypher(),
            Node::GraphReturn {
                fields: vec!["p".into()],
                result_form: ResultForm::RowSet,
                input: Box::new(Node::GraphShortestPath {
                    source: "a".into(),
                    target: Some("b".into()),
                    direction: Direction::Out,
                    rel_types: LabelExpr::AnyOf(vec!["R".into()]),
                    max_distance: None,
                    include_edges,
                    output: "p".into(),
                    all_paths,
                    input: Box::new(Node::GraphValues {
                        bindings: vec!["a".into(), "b".into()],
                        rows: vec![vec![node(0_i64), node(target)]; 2],
                        bulk: None,
                    }),
                }),
            },
        );
        let result = engine.execute_plan(&plan).await.unwrap();
        assert_eq!(
            result.returned.batch.num_rows(),
            expected,
            "target={target}, all={all_paths}, edges={include_edges}"
        );
        assert_eq!(result.stats.cost.native_relational_kernel_calls, 0);
        assert!(
            result
                .stats
                .sql_queries
                .iter()
                .any(|sql| sql.contains("WITH RECURSIVE"))
        );
    }
}

#[tokio::test]
async fn temporal_sort_payload_does_not_fence_integer_aggregation() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine.cypher("WITH [date('1984-10-12'),date('1980-12-24')] AS values WITH values,size(values) AS n UNWIND values AS value WITH size([x IN values WHERE x < value]) AS x,value,n ORDER BY value WITH n,collect(x) AS xs RETURN xs=range(0,n-1) AS equal").await.unwrap();
    let column = result
        .returned
        .batch
        .column(0)
        .as_any()
        .downcast_ref::<arrow::array::BooleanArray>()
        .unwrap();
    assert_eq!(column.values().iter().collect::<Vec<_>>(), vec![true]);
    assert_eq!(result.stats.cost.native_relational_kernel_calls, 0);
}
