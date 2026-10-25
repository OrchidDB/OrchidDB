#![cfg(feature = "duckdb")]
use orchiddb::ir::rel::sql::DuckDbExecutor;

#[test]
fn typed_helpers_preserve_nulls_empty_maps_and_row_local_elements() {
    let mut executor = DuckDbExecutor::default();
    let connection = executor.connection().unwrap();
    let checks:(bool,bool,bool)=connection.query_row(
        "SELECT orchid_cypher_pack(NULL) IS NULL, orchid_cypher_size([NULL,1])=2, orchid_cypher_eq(orchid_cypher_map(),orchid_cypher_map())",[],
        |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
    assert_eq!(checks, (true, true, true));
    let matches:i64=connection.query_row(
        "SELECT count(*) FILTER (WHERE orchid_cypher_eq(orchid_cypher_subscript(orchid_cypher_element(struct_pack(kind := 'node', id := 1, label := 'N', properties := struct_pack(v := i))), 'v'), i)) FROM range(4097) AS input(i)",[],|row|row.get(0)).unwrap();
    assert_eq!(
        matches, 4097,
        "element metadata must not leak between vector rows"
    );
}

#[test]
fn typed_scalar_errors_keep_language_classification() {
    let mut executor = DuckDbExecutor::default();
    let error = executor
        .connection()
        .unwrap()
        .query_row(
            "SELECT orchid_cypher_percentile_fraction(1000)",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_err()
        .to_string();
    assert!(error.contains("ORCHIDDB_QUERY_ERROR_V1:"), "{error}");
    assert!(error.contains("NumberOutOfRange"), "{error}");
}

#[test]
fn typed_nested_lists_can_exceed_duckdb_vector_size() {
    let mut executor = DuckDbExecutor::default();
    let size: i64 = executor
        .connection()
        .unwrap()
        .query_row(
            "SELECT orchid_cypher_size(orchid_cypher_items(range(5000)))",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(size, 5000);
}

#[tokio::test]
async fn constant_quantifier_on_typed_values_avoids_occurrence_reduction() {
    use orchiddb::ir::{catalog::PropertyGraph, diagnostics::without_constant_folding,
        rel::{RelBackend, RelBackendOptions, sql::{unparse, SqlDialect}}};
    use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
    without_constant_folding(async {
        for kind in ["all", "any", "none", "single"] {
            for predicate in ["true", "false", "null"] {
                let query=format!("UNWIND [[1,null],[],null] AS xs RETURN {kind}(x IN xs WHERE {predicate}) AS result");
                let plan=CypherPlanner::new().plan(&parse_query(&query).unwrap()).unwrap();
                let backend=RelBackend::with_options(RelBackendOptions {native_values:true,language_functions:true,..Default::default()});
                let lowered=backend.lower(&plan,&PropertyGraph::new()).unwrap();
                let sql=unparse(&lowered,SqlDialect::DuckDb).unwrap();
                assert!(!sql.contains("__w_quantifier_row_"),"{sql}");
                assert!(!sql.contains("__w_quantifier_true_"),"{sql}");
                assert!(sql.contains("__w_sql_cte_quantifier_constant_"),"{sql}");
            }
        }
    }).await;
}

#[tokio::test]
async fn row_local_quantifiers_use_lists_without_losing_null_semantics() {
    let mut engine=orchiddb::engine::GraphEngine::in_memory().unwrap();
    let result=engine.cypher("UNWIND [[1,null],[1,2],[],null] AS xs RETURN all(x IN xs WHERE x>0), any(x IN xs WHERE x>0), none(x IN xs WHERE x>0), single(x IN xs WHERE x>0)").await.unwrap();
    assert_eq!(result.stats.cost.sql_executions,1);
    assert_eq!(result.stats.cost.native_kernel_calls,0);
    assert!(!result.stats.sql_queries.join("\n").contains("__w_quantifier_row_"));
    let batch=&result.returned.batch;
    let mut rows=(0..batch.num_rows()).map(|row| batch.columns().iter().map(|a|arrow::util::display::array_value_to_string(a.as_ref(),row).unwrap()).collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
    rows.sort();
    let mut expected=vec!["|true|false|","true|true|false|false","true|false|true|false","|||"];
    expected.sort();assert_eq!(rows,expected);
}
