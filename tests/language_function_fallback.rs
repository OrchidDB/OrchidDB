use orchiddb::{
    ir::{
        catalog::PropertyGraph,
        rel::{
            RelBackend,
            sql::{self, SqlDialect},
        },
        value::Value,
    },
    language::{
        cypher::{parse_query, planner::CypherPlanner},
        gremlin::{GremlinPlanner, parse_traversal},
    },
};

#[tokio::test]
async fn language_kernels_execute_without_duckdb_and_reject_other_sql_dialects() {
    let graph = PropertyGraph::new();
    graph.insert_node(
        "P",
        [
            ("name".into(), Value::String(" a😀 ".into())),
            ("x".into(), Value::Int(1)),
        ]
        .into(),
    );
    let backend = RelBackend::new().with_language_functions(true);
    let plan = CypherPlanner::new()
        .plan(&parse_query("MATCH (p:P) RETURN p.x, count(*)").unwrap())
        .unwrap();
    let result = backend.execute(&plan, &graph).await.unwrap();
    assert_eq!(result.batch.num_rows(), 1);
    assert_eq!(
        arrow::util::display::array_value_to_string(result.batch.column(1), 0).unwrap(),
        "1"
    );
    let lowered = backend.lower(&plan, &graph).unwrap();
    assert!(sql::prepare(&lowered, SqlDialect::Postgres).await.is_err());
    let plan = GremlinPlanner::new()
        .plan(&parse_traversal("g.V().values('name').trim().length()").unwrap())
        .unwrap();
    let result = backend.execute(&plan, &graph).await.unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(result.batch.column(0), 0).unwrap(),
        "3"
    );
}
