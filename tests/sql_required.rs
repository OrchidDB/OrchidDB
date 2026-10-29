#![cfg(feature = "duckdb")]
use orchiddb::{engine::GraphEngine, ir::rel::runtime::policy::LoweringPolicy};
#[tokio::test]
async fn cypher_sql_required() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:Person {name:'Alice', age:30}), (:Person {name:'Bob', age:20})")
        .await
        .unwrap();
    engine.set_lowering_policy(LoweringPolicy::SqlRequired);
    for query in [
        "RETURN 1 AS n",
        "UNWIND [1,2,3] AS n RETURN sum(n) AS n",
        "MATCH (p:Person) WHERE p.age > 20 RETURN p.name AS name",
        "MATCH (p:Person) WITH p WHERE p.age = 30 RETURN count(*) AS n",
        "MATCH (p:Person) RETURN p LIMIT 0",
        "MATCH (p:Person), (q:Person) WHERE p.age < q.age RETURN p.name, q.name",
        "UNWIND [1,1,2] AS n RETURN n, count(*) AS c ORDER BY n",
        "RETURN duration.between(date('2000-01-01'),date('2001-01-01')) AS d",
        "UNWIND [1, null, 'x'] AS n RETURN collect(n) AS xs",
    ] {
        engine.set_lowering_policy(LoweringPolicy::Hybrid);
        let expected = engine.cypher(query).await.unwrap();
        engine.set_lowering_policy(LoweringPolicy::SqlRequired);
        let result = engine
            .cypher(query)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let rows = |b: &arrow::record_batch::RecordBatch| {
            (0..b.num_rows())
                .map(|i| {
                    b.columns()
                        .iter()
                        .map(|c| arrow::util::display::array_value_to_string(c, i).unwrap())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            rows(&result.returned.batch),
            rows(&expected.returned.batch),
            "{query}"
        );
        assert!(result.stats.islands > 0 || query.ends_with("LIMIT 0"), "{query}: {:?}", result.stats);
    }
}

#[derive(Debug)]
struct Observe(std::sync::Arc<std::sync::atomic::AtomicUsize>);
impl orchiddb::ir::rel::runtime::policy::BoundaryExecutable for Observe {
    fn execute(
        &self,
        boundary: &orchiddb::ir::rel::runtime::CompiledKernel,
        inputs: Vec<Vec<orchiddb::ir::runtime::Row>>,
        state: &mut orchiddb::ir::rel::runtime::KernelState,
    ) -> Result<Vec<orchiddb::ir::runtime::Row>, orchiddb::ir::diagnostics::QueryExecutionError>
    {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        boundary.invoke_default(inputs, state)
    }
}



#[tokio::test]
async fn sql_native_aggregate_semantics_are_independent_of_placement() {
    use arrow::array::Array;
    use orchiddb::ir::rules::CypherAggregatePolicy;
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.set_lowering_policy(LoweringPolicy::SqlRequired);
    let query = "UNWIND [1] AS n WITH n WHERE false RETURN sum(n) AS total";
    let conformant = engine.cypher(query).await.unwrap();
    assert!(!conformant.returned.batch.column(0).is_null(0));
    engine.set_cypher_aggregate_policy(CypherAggregatePolicy::SqlNative);
    engine.set_language_functions(false).unwrap();
    let relaxed = engine.cypher(query).await.unwrap();
    assert!(relaxed.returned.batch.column(0).is_null(0));
    for query in [
        "UNWIND [1.5,1.5,2.5] AS n RETURN n, count(*) AS c ORDER BY n",
        "UNWIND [1] AS n WITH n WHERE false RETURN collect(n) AS xs, stDev(n) AS d",
    ] {
        let result = engine.cypher(query).await.unwrap();
        assert!(
            result
                .stats
                .sql_queries
                .iter()
                .all(|sql| !sql.contains("__orchiddb_lang_cypher_key")
                    && !sql.contains("__orchiddb_value_aggregate"))
        );
    }
}



#[test]
fn sql_required_rejects_relational_kernels_at_compile_time() {
    use orchiddb::ir::{
        catalog::PropertyGraph,
        rel::runtime::{CompileOptions, compile_for_host},
    };
    let plan = orchiddb::language::cypher::preparation::prepare(
        "UNWIND [1,2] AS n RETURN n + 1 AS x",
        &Default::default(),
        None,
    )
    .unwrap();
    let mut options = CompileOptions::default();
    options.sql_islands = false;
    options.rule_context.lowering_policy = LoweringPolicy::SqlRequired;
    let error = compile_for_host(&plan, &PropertyGraph::new(), options).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("rejected native relational kernel"),
        "{error}"
    );
}

#[tokio::test]
async fn effect_barrier_preserves_writes_when_sql_returns_no_rows() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.set_lowering_policy(LoweringPolicy::SqlRequired);
    let empty = engine.cypher("CREATE (n:N {x:42}) RETURN n LIMIT 0").await.unwrap();
    assert_eq!(empty.returned.batch.num_rows(), 0);
    let count = engine.cypher("MATCH (n:N) RETURN count(*) AS c").await.unwrap();
    assert_eq!(arrow::util::display::array_value_to_string(count.returned.batch.column(0), 0).unwrap(), "1");
}
