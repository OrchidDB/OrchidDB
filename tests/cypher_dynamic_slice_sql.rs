#![cfg(feature = "duckdb")]
use orchiddb::ir::{
    catalog::PropertyGraph,
    diagnostics::without_constant_folding,
    rel::{
        RelBackend,
        sql::{self, DuckDbExecutor, SqlDialect},
    },
};
use std::collections::BTreeMap;

#[tokio::test]
async fn random_skip_is_once_per_query_and_not_a_row_filter() {
    without_constant_folding(async {
        let query =
            "UNWIND range(1,10) AS n WITH n SKIP toInteger(rand()*9) RETURN count(*) AS count";
        let plan = orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), None)
            .unwrap();
        // The shared native runtime must retain a valid execution route even
        // though DataFusion's physical Limit itself only supports literals.
        let (native, _) = orchiddb::ir::rel::runtime::execute(&plan, &PropertyGraph::new(), None).await.unwrap();
        let native_count = arrow::util::display::array_value_to_string(native.batch.column(0),0).unwrap().parse::<i64>().unwrap();
        assert!((2..=10).contains(&native_count));
        let lowered = RelBackend::new()
            .with_language_functions(true)
            .lower(&plan, &PropertyGraph::new())
            .unwrap();
        let prepared = sql::prepare(&lowered, SqlDialect::DuckDb).await.unwrap();
        assert_eq!(
            prepared.query.to_lowercase().matches("random()").count(),
            1,
            "{}",
            prepared.query
        );
        assert!(
            prepared.query.to_lowercase().contains("offset"),
            "{}",
            prepared.query
        );
        let mut executor = DuckDbExecutor::default();
        for _ in 0..30 {
            let result = sql::execute_prepared(&mut executor, &prepared).unwrap();
            let value = arrow::util::display::array_value_to_string(result.batch.column(0), 0)
                .unwrap()
                .parse::<i64>()
                .unwrap();
            assert!((2..=10).contains(&value), "{value}: {}", prepared.query);
        }
        for (query, calls, max) in [
            ("UNWIND range(1,10) AS n WITH n LIMIT toInteger(rand()*9) RETURN count(*) AS count",1,8),
            ("UNWIND range(1,10) AS n WITH n SKIP toInteger(rand()*9) LIMIT toInteger(rand()*3) RETURN count(*) AS count",2,2),
            ("UNWIND [] AS n WITH n SKIP toInteger(rand()*9) RETURN count(*) AS count",1,0),
        ] {
            let plan=orchiddb::language::cypher::preparation::prepare(query,&BTreeMap::new(),None).unwrap();
            let lowered=RelBackend::new().with_language_functions(true).lower(&plan,&PropertyGraph::new()).unwrap();
            let prepared=sql::prepare(&lowered,SqlDialect::DuckDb).await.unwrap();
            assert_eq!(prepared.query.to_lowercase().matches("random()").count(),calls,"{}",prepared.query);
            for _ in 0..10 {
                let result=sql::execute_prepared(&mut executor,&prepared).unwrap();
                let value=arrow::util::display::array_value_to_string(result.batch.column(0),0).unwrap().parse::<i64>().unwrap();
                assert!((0..=max).contains(&value),"{value}: {}",prepared.query);
            }
        }
        for query in [
            "UNWIND [1,2] AS n WITH n SKIP toInteger(rand()*9)-10 RETURN n",
            "UNWIND [1,2] AS n WITH n LIMIT toInteger(rand()/0) RETURN n",
        ] {
            let plan =
                orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), None)
                    .unwrap();
            assert!(
                RelBackend::new()
                    .with_language_functions(true)
                    .lower(&plan, &PropertyGraph::new())
                    .is_err()
            );
        }
    })
    .await;
}
