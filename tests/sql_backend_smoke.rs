//! Real-database SQL execution smoke tests.
//!
//! Each test plans a query, lowers it through `RelBackend`, unparses the
//! lowered plan to dialect SQL, materializes the fixture graph's scan tables
//! into DuckDB, executes the SQL there, and compares the rows against both
//! expected values and the in-process DataFusion execution of the same plan.
//!
//! A Postgres variant of the round-trip is `#[ignore]`d and only runs when
//! the `postgres` feature is enabled and `GRAPH_PG_URL` points at a server.

use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray,
};
use arrow::datatypes::DataType;

use new_graph::ir::bridge::gremlin as gb;
use new_graph::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use new_graph::ir::expr::Lit;
use new_graph::ir::plan::GraphPlan;
use new_graph::ir::rel::sql::{self, DuckDbExecutor, SqlDialect, SqlExecutor};
use new_graph::ir::rel::{RelBackend, execute_lowered};
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner as AstCypherPlanner;
use new_graph::planner::GremlinPlanner;

fn fixture_graph() -> PropertyGraph {
    let names: ArrayRef = Arc::new(StringArray::from(vec!["alice", "bob", "carol"]));
    let ages: ArrayRef = Arc::new(Int64Array::from(vec![30, 28, 41]));
    let person = nodes_from_columns("Person", vec![("name", names), ("age", ages)]);

    let mut graph = PropertyGraph::new();
    graph.add_nodes(person);

    let edges = edges_from_columns(
        "KNOWS",
        "Person",
        "Person",
        vec![0, 0, 1],
        vec![1, 2, 2],
        Vec::new(),
    );
    graph.add_edges(edges).unwrap();
    graph
}

fn cypher_plan(query: &str) -> GraphPlan {
    let parsed = parse_query(query).expect("parse");
    AstCypherPlanner::new().plan(&parsed).expect("plan")
}

fn batch_lines(batch: &RecordBatch) -> Vec<String> {
    (0..batch.num_rows())
        .map(|row| {
            (0..batch.num_columns())
                .map(|col| cell_to_string(batch.column(col), row))
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

fn cell_to_string(array: &ArrayRef, row: usize) -> String {
    if array.is_null(row) {
        return "null".into();
    }
    match array.data_type() {
        DataType::Utf8 => array
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Int64 => array
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(row)
            .to_string(),
        DataType::Float64 => {
            let value = array
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap()
                .value(row);
            format!("{value}")
        }
        DataType::Boolean => array
            .as_any()
            .downcast_ref::<BooleanArray>()
            .unwrap()
            .value(row)
            .to_string(),
        other => panic!("unhandled result type in smoke test: {other}"),
    }
}

/// Run the plan through DuckDB and through in-process DataFusion, assert they
/// agree, and return the DuckDB rows.
async fn duckdb_and_datafusion(plan: &GraphPlan, graph: &PropertyGraph, ordered: bool) -> Vec<String> {
    let backend = RelBackend::new();
    let lowered = backend.lower(plan, graph).expect("lower");
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb)
        .await
        .expect("prepare sql");
    let mut executor = DuckDbExecutor::new();
    let from_duckdb = sql::execute_prepared(&mut executor, &prepared)
        .unwrap_or_else(|err| panic!("duckdb execute: {err}\nquery: {}", prepared.query));
    let from_datafusion = execute_lowered(backend.lower(plan, graph).expect("lower"))
        .await
        .expect("datafusion execute");

    let mut duckdb_lines = batch_lines(&from_duckdb.batch);
    let mut datafusion_lines = batch_lines(&from_datafusion.batch);
    if !ordered {
        duckdb_lines.sort();
        datafusion_lines.sort();
    }
    assert_eq!(
        duckdb_lines, datafusion_lines,
        "duckdb and datafusion disagree\nquery: {}",
        prepared.query
    );
    duckdb_lines
}

#[tokio::test]
async fn cypher_match_filter_expand_order_on_duckdb() {
    let plan = cypher_plan(
        "MATCH (p:Person)-[:KNOWS]->(f) WHERE p.name = 'alice' RETURN f.name ORDER BY f.name",
    );
    let rows = duckdb_and_datafusion(&plan, &fixture_graph(), true).await;
    assert_eq!(rows, vec!["bob", "carol"]);
}

#[tokio::test]
async fn cypher_aggregates_on_duckdb() {
    let plan =
        cypher_plan("MATCH (p:Person) RETURN min(p.age) AS lo, max(p.age) AS hi, sum(p.age) AS s");
    let rows = duckdb_and_datafusion(&plan, &fixture_graph(), true).await;
    assert_eq!(rows, vec!["28|41|99"]);
}

#[tokio::test]
async fn cypher_order_desc_limit_on_duckdb() {
    let plan = cypher_plan("MATCH (p:Person) RETURN p.name ORDER BY p.age DESC LIMIT 2");
    let rows = duckdb_and_datafusion(&plan, &fixture_graph(), true).await;
    assert_eq!(rows, vec!["carol", "alice"]);
}

#[tokio::test]
async fn cypher_arithmetic_filter_on_duckdb() {
    let plan =
        cypher_plan("MATCH (p:Person) WHERE p.age + 1 >= 31 RETURN p.name ORDER BY p.name");
    let rows = duckdb_and_datafusion(&plan, &fixture_graph(), true).await;
    assert_eq!(rows, vec!["alice", "carol"]);
}

#[tokio::test]
async fn gremlin_out_values_on_duckdb() {
    let traversal = gb::GremlinTraversal {
        steps: vec![
            gb::Step::V {
                label: Some("Person".into()),
            },
            gb::Step::Has {
                key: "name".into(),
                value: Lit::String("alice".into()),
            },
            gb::Step::Out {
                rel_types: vec!["KNOWS".into()],
            },
            gb::Step::Values("name".into()),
        ],
    };
    let plan = GremlinPlanner::new().plan(&traversal);
    let rows = duckdb_and_datafusion(&plan, &fixture_graph(), false).await;
    assert_eq!(rows, vec!["bob", "carol"]);
}

#[tokio::test]
async fn gremlin_count_on_duckdb() {
    let traversal = gb::GremlinTraversal {
        steps: vec![
            gb::Step::V {
                label: Some("Person".into()),
            },
            gb::Step::Count,
        ],
    };
    let plan = GremlinPlanner::new().plan(&traversal);
    let rows = duckdb_and_datafusion(&plan, &fixture_graph(), true).await;
    assert_eq!(rows, vec!["3"]);
}

#[tokio::test]
async fn graph_setup_sql_materializes_catalog_tables() {
    let statements =
        sql::graph_setup_sql(SqlDialect::DuckDb, &fixture_graph()).expect("graph setup sql");
    let script = statements.join(";\n");
    assert!(script.contains("\"node_person\""));
    assert!(script.contains("\"edge_knows\""));

    let mut executor = DuckDbExecutor::new();
    let rows = executor
        .run(&statements, "SELECT count(*) FROM \"node_person\"")
        .expect("query materialized graph");
    assert_eq!(rows, vec![vec![sql::SqlValue::Int(3)]]);
}

#[cfg(feature = "postgres")]
#[tokio::test]
#[ignore = "needs a live Postgres server via GRAPH_PG_URL"]
async fn cypher_match_filter_expand_order_on_postgres() {
    let Some(mut executor) = sql::PostgresExecutor::from_env().expect("postgres connect") else {
        eprintln!("GRAPH_PG_URL unset; skipping postgres smoke test");
        return;
    };
    let graph = fixture_graph();
    let plan = cypher_plan(
        "MATCH (p:Person)-[:KNOWS]->(f) WHERE p.name = 'alice' RETURN f.name ORDER BY f.name",
    );
    let lowered = RelBackend::new().lower(&plan, &graph).expect("lower");
    let returned = sql::execute_lowered_sql(&mut executor, &lowered)
        .await
        .expect("postgres execute");
    assert_eq!(batch_lines(&returned.batch), vec!["bob", "carol"]);
}
