use orchiddb::ir::{catalog::PropertyGraph, rel::{RelBackend, sql::{SqlDialect, unparse}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

fn lower(graph: &PropertyGraph, query: &str) -> orchiddb::ir::rel::LoweredPlan {
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    RelBackend::new().with_language_functions(true).lower(&plan, graph).unwrap()
}

#[tokio::test]
async fn simple_scan_count_has_no_aggregate_scaffolding() {
    let graph = PropertyGraph::new();
    graph.insert_node("P", BTreeMap::new());
    for query in [
        "MATCH () RETURN count(*) AS columnName",
        "MATCH (n:P) RETURN count(*) AS columnName",
        "MATCH (n) WHERE false RETURN count(*) AS columnName",
    ] {
        let lowered = lower(&graph, query);
        let sql = orchiddb::ir::rel::sql::prepare(&lowered, SqlDialect::DuckDb).await.unwrap().query;
        assert!(sql.contains("count(*) AS \"columnName\""), "{query}: {sql}");
        assert!(sql.contains(" FROM "), "{query}: {sql}");
        assert!(!sql.contains("__w_sql_cte_aggregate") && !sql.contains("__w_cte_guard"), "{query}: {sql}");
    }
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn counts_preserve_empty_filtered_joined_and_optional_rows() {
    use orchiddb::ir::rel::sql::{DuckDbExecutor, execute_lowered_sql};
    for size in [0, 3] {
        let graph = PropertyGraph::new();
        for _ in 0..size { graph.insert_node("P", BTreeMap::new()); }
        for (query, expected) in [
            ("MATCH () RETURN count(*) AS columnName", size),
            ("MATCH (n:P) RETURN count(*) AS columnName", size),
            ("MATCH (n) WHERE false RETURN count(*) AS columnName", 0),
            ("MATCH (a),(b) RETURN count(*) AS columnName", size * size),
            ("MATCH (n) UNWIND [1,2] AS x RETURN count(*) AS columnName", size * 2),
            ("OPTIONAL MATCH (n) RETURN count(*) AS columnName", size.max(1)),
            ("UNWIND [null,null] AS x RETURN count(*) AS columnName", 2),
            ("MATCH (n) WITH n LIMIT 1 RETURN count(*) AS columnName", size.min(1)),
            ("MATCH (n) RETURN count(DISTINCT 1) AS columnName", size.min(1)),
            ("MATCH (n) RETURN count(null) AS columnName", 0),
        ] {
            let lowered = lower(&graph, query);
            let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
            let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await
                .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
            assert_eq!(result.batch.num_rows(), 1, "{query}: {sql}");
            assert_eq!(arrow::util::display::array_value_to_string(result.batch.column(0), 0).unwrap(), expected.to_string(), "{query}: {sql}");
        }
    }
}

#[tokio::test]
async fn same_name_output_alias_does_not_wrap_aggregate_computation() {
    let graph = PropertyGraph::new();
    graph.insert_node("Person", BTreeMap::from([("age".into(), orchiddb::ir::value::Value::Int(20))]));
    let query = "MATCH (person) WITH 38 + avg(person.age) - 1000 AS agg RETURN *";
    let lowered = lower(&graph, query);
    let sql = orchiddb::ir::rel::sql::prepare(&lowered, SqlDialect::DuckDb).await.unwrap().query;
    assert!(!sql.starts_with("SELECT \"agg\" AS \"agg\" FROM"), "{sql}");
    assert!(sql.contains("avg("), "{sql}");
    #[cfg(feature = "duckdb")]
    {
        let result = orchiddb::ir::rel::sql::execute_lowered_sql(
            &mut orchiddb::ir::rel::sql::DuckDbExecutor::default(), &lowered).await.unwrap();
        assert_eq!(arrow::util::display::array_value_to_string(result.batch.column(0),0).unwrap(), "-942.0");
    }
}
