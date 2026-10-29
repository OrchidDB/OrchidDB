#![cfg(feature = "duckdb")]
use orchiddb::engine::GraphEngine;
use serde_json::Value;

fn native(result: &orchiddb::engine::QueryResult, language: &str) -> Value {
    let key = format!("orchiddb.{language}.typed_rows.v1");
    let schema = result.returned.batch.schema();
    serde_json::from_str(schema.metadata().get(&key).expect("typed result boundary")).unwrap()
}



#[tokio::test]
async fn sql_islands_do_not_reinterpret_literal_graph_text() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine.cypher("WITH 'v[not-a-vertex]' AS s RETURN s").await.unwrap();
    let rows = native(&result, "cypher");
    assert_eq!(rows[0][0]["type"], "string");
    assert_eq!(rows[0][0]["value"], "v[not-a-vertex]");
}





#[tokio::test]
async fn sql_preserves_composed_null_predicate_precedence() {
    let query = "UNWIND [true,false,null] AS a UNWIND [true,false,null] AS b UNWIND [true,false,null] AS c RETURN (a OR (b AND c)) IS NULL = ((a OR b) AND (a OR c)) IS NULL AS result";
    let graph = orchiddb::ir::catalog::PropertyGraph::new();
    let parsed = orchiddb::language::cypher::parser::parse_query(query).unwrap();
    let plan = orchiddb::language::cypher::planner::CypherPlanner::new().plan(&parsed).unwrap();
    let lowered = orchiddb::ir::rel::RelBackend::new().lower(&plan, &graph).unwrap();
    let sql = orchiddb::ir::rel::sql::unparse(&lowered, orchiddb::ir::rel::sql::SqlDialect::DuckDb).unwrap();
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine.cypher(query).await.unwrap();
    let rows = native(&result, "cypher");
    assert!(rows.as_array().unwrap().iter().all(|r| r[0]["value"] == true), "{sql}\n{rows}");
    let result = engine.cypher("UNWIND [true,false,null] AS a RETURN (NOT a) IS NULL = a IS NULL AS result").await.unwrap();
    let rows = native(&result, "cypher");
    assert!(rows.as_array().unwrap().iter().all(|r| r[0]["value"] == true), "{rows}");
}







#[tokio::test]
async fn sql_islands_preserve_nested_empty_and_null_lists() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for expr in ["[]", "[null]", "[[]]", "[[[[[[[]]]]]]]", "range(0,-1)", "keys({})"] {
        let result = engine.cypher(&format!("RETURN {expr} AS value")).await.unwrap();
        assert_eq!(result.returned.batch.num_rows(), 1, "{expr}");
        assert!(result.stats.islands > 0, "{expr}");
    }
}
