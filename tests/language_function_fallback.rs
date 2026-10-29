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
    },
};

#[tokio::test]
async fn primitive_grouping_uses_native_sql_on_postgres_and_duckdb() {
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
    let prepared = sql::prepare(&lowered, SqlDialect::Postgres).await.unwrap();
    assert!(!prepared.query.contains("orchid_cypher_"), "{}", prepared.query);
}
