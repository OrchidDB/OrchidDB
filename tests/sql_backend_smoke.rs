//! Real-database SQL execution smoke tests.
//!
//! Each test plans a query, lowers it through `RelBackend`, unparses the
//! lowered plan to dialect SQL, materializes the fixture graph's scan tables
//! into DuckDB, executes the SQL there, and compares the rows against both
//! expected values and the in-process DataFusion execution of the same plan.
//!
//! A Postgres variant of the round-trip is `#[ignore]`d and only runs when
//! the `postgres` feature is enabled and `GRAPH_PG_URL` points at a server.

use std::collections::HashMap;
use std::sync::Arc;

use arrow::array::{
    Array, ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray,
};
use arrow::datatypes::{DataType, Field, Schema};

use new_graph::ir::bridge::gremlin as gb;
use new_graph::ir::catalog::{NodeTable, PropertyGraph, edges_from_columns, nodes_from_columns};
use new_graph::ir::expr::Lit;
use new_graph::ir::plan::GraphPlan;
use new_graph::ir::rel::sql::{self, DuckDbExecutor, SqlDialect, SqlExecutor};
use new_graph::ir::rel::{RelBackend, RelBackendOptions, execute_lowered};
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

fn long_chain_graph() -> PropertyGraph {
    let names: ArrayRef = Arc::new(StringArray::from(
        (0..9).map(|index| format!("n{index}")).collect::<Vec<_>>(),
    ));
    let person = nodes_from_columns("Person", vec![("name", names)]);
    let mut graph = PropertyGraph::new();
    graph.add_nodes(person);
    graph
        .add_edges(edges_from_columns(
            "NEXT",
            "Person",
            "Person",
            (0..8).collect(),
            (1..9).collect(),
            Vec::new(),
        ))
        .unwrap();
    graph
}

fn mixed_path_graph() -> PropertyGraph {
    let people: ArrayRef = Arc::new(StringArray::from(vec!["alice", "bob"]));
    let cities: ArrayRef = Arc::new(StringArray::from(vec!["paris"]));
    let mut graph = PropertyGraph::new();
    graph.add_nodes(nodes_from_columns("Person", vec![("name", people)]));
    graph.add_nodes(nodes_from_columns("City", vec![("name", cities)]));
    graph
        .add_edges(edges_from_columns(
            "KNOWS",
            "Person",
            "Person",
            vec![0],
            vec![1],
            Vec::new(),
        ))
        .unwrap();
    graph
        .add_edges(edges_from_columns(
            "LIVES_IN",
            "Person",
            "City",
            vec![1],
            vec![0],
            Vec::new(),
        ))
        .unwrap();
    graph
}

fn union_property_graph() -> PropertyGraph {
    let field = Field::new("grade", DataType::Utf8, true).with_metadata(HashMap::from([(
        "new_graph.value_type".to_string(),
        "value".to_string(),
    )]));
    let values: ArrayRef = Arc::new(StringArray::from(vec![
        r#"Map({"__tag": String("grade1"), "__value": Float(3.5)})"#,
        r#"Map({"__tag": String("grade2"), "__value": Int(4)})"#,
    ]));
    let batch = RecordBatch::try_new(Arc::new(Schema::new(vec![field])), vec![values]).unwrap();
    let mut graph = PropertyGraph::new();
    graph.add_nodes(NodeTable {
        label: "Movie".to_string(),
        batch,
    });
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
async fn duckdb_and_datafusion(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    ordered: bool,
) -> Vec<String> {
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
    let plan = cypher_plan("MATCH (p:Person) WHERE p.age + 1 >= 31 RETURN p.name ORDER BY p.name");
    let rows = duckdb_and_datafusion(&plan, &fixture_graph(), true).await;
    assert_eq!(rows, vec!["alice", "carol"]);
}

#[tokio::test]
async fn cypher_stored_union_tag_on_duckdb() {
    let plan = cypher_plan("MATCH (m:Movie) RETURN union_tag(m.grade) AS tag ORDER BY tag");
    let rows = duckdb_and_datafusion(&plan, &union_property_graph(), true).await;
    assert_eq!(rows, vec!["grade1", "grade2"]);
}

#[tokio::test]
async fn cypher_unbounded_varlen_uses_recursive_cte_on_duckdb() {
    let graph = fixture_graph();
    let plan = cypher_plan(
        "MATCH (p:Person)-[:KNOWS*1..]->(f) \
         WHERE p.name = 'alice' \
         RETURN f.name, count(*) AS paths ORDER BY f.name",
    );
    let lowered = RelBackend::new().lower(&plan, &graph).expect("lower");
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb)
        .await
        .expect("prepare sql");
    assert!(
        prepared.query.starts_with("WITH RECURSIVE"),
        "query did not use a recursive CTE:\n{}",
        prepared.query
    );
    assert!(
        prepared
            .setup
            .iter()
            .all(|statement| !statement.contains("__graph_varlen_")),
        "CTE work table was incorrectly materialized: {:?}",
        prepared.setup
    );

    let mut executor = DuckDbExecutor::new();
    let returned = sql::execute_prepared(&mut executor, &prepared)
        .unwrap_or_else(|err| panic!("duckdb execute: {err}\nquery: {}", prepared.query));
    assert_eq!(batch_lines(&returned.batch), vec!["bob|1", "carol|2"]);
}

#[tokio::test]
async fn cypher_unbounded_varlen_is_not_silently_capped() {
    let plan = cypher_plan(
        "MATCH (p:Person)-[:NEXT*1..]->(f) \
         WHERE p.name = 'n0' RETURN count(*)",
    );
    let lowered = RelBackend::new()
        .lower(&plan, &long_chain_graph())
        .expect("lower");
    let mut executor = DuckDbExecutor::new();
    let returned = sql::execute_lowered_sql(&mut executor, &lowered)
        .await
        .expect("execute recursive path");
    assert_eq!(batch_lines(&returned.batch), vec!["8"]);
}

#[tokio::test]
async fn cypher_varlen_optional_ceiling_is_explicit() {
    let plan = cypher_plan(
        "MATCH (p:Person)-[:NEXT*1..]->(f) \
         WHERE p.name = 'n0' RETURN count(*)",
    );
    let backend = RelBackend::with_options(RelBackendOptions {
        varlen_recursive_ceiling: Some(3),
        ..RelBackendOptions::default()
    });
    let lowered = backend.lower(&plan, &long_chain_graph()).expect("lower");
    let mut executor = DuckDbExecutor::new();
    let returned = sql::execute_lowered_sql(&mut executor, &lowered)
        .await
        .expect("execute guarded recursive path");
    assert_eq!(batch_lines(&returned.batch), vec!["3"]);
}

#[tokio::test]
async fn cypher_varlen_path_and_undirected_length_on_duckdb() {
    let plan = cypher_plan(
        "MATCH (p:Person)-[e:KNOWS*2..2]-(f) \
         WHERE p.name = 'alice' \
         RETURN f.name, length(e), e ORDER BY f.name",
    );
    let lowered = RelBackend::new()
        .lower(&plan, &fixture_graph())
        .expect("lower");
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb)
        .await
        .expect("prepare sql");
    let mut executor = DuckDbExecutor::new();
    let returned = sql::execute_prepared(&mut executor, &prepared)
        .unwrap_or_else(|err| panic!("duckdb execute: {err}\nquery: {}", prepared.query));
    let rows = batch_lines(&returned.batch);
    assert_eq!(rows.len(), 2);
    assert!(rows[0].starts_with("bob|2|{_NODES: ["));
    assert!(rows[1].starts_with("carol|2|{_NODES: ["));
    assert!(rows.iter().all(|row| row.contains("_RELS: [")));
}

#[tokio::test]
async fn cypher_count_distinct_varlen_paths_on_duckdb() {
    let plan = cypher_plan(
        "MATCH (p:Person)-[e:KNOWS*1..]->(f) \
         WHERE p.name = 'alice' RETURN count(DISTINCT e)",
    );
    let lowered = RelBackend::new()
        .lower(&plan, &fixture_graph())
        .expect("lower");
    let mut executor = DuckDbExecutor::new();
    let returned = sql::execute_lowered_sql(&mut executor, &lowered)
        .await
        .expect("execute recursive paths");
    assert_eq!(batch_lines(&returned.batch), vec!["3"]);
}

#[tokio::test]
async fn cypher_named_path_spans_varlen_and_fixed_segments() {
    let plan = cypher_plan(
        "MATCH p = (a:Person)-[:KNOWS*1..1]->(:Person)-[:LIVES_IN]->(:City) \
         WHERE a.name = 'alice' RETURN p",
    );
    let lowered = RelBackend::new()
        .lower(&plan, &mixed_path_graph())
        .expect("lower mixed path");
    let mut executor = DuckDbExecutor::new();
    let returned = sql::execute_lowered_sql(&mut executor, &lowered)
        .await
        .expect("execute mixed path");
    let rows = batch_lines(&returned.batch);
    assert_eq!(rows.len(), 1);
    let path = &rows[0];
    assert_eq!(path.matches("_LABEL: Person").count(), 2, "{path}");
    assert_eq!(path.matches("_LABEL: City").count(), 1, "{path}");
    assert_eq!(path.matches("_LABEL: KNOWS").count(), 1, "{path}");
    assert_eq!(path.matches("_LABEL: LIVES_IN").count(), 1, "{path}");
}

#[tokio::test]
async fn cypher_dynamic_list_and_unwind_on_duckdb() {
    let plan = cypher_plan(
        "MATCH (p:Person) WHERE p.name = 'alice' \
         UNWIND [p.age, p.age + 1] AS age RETURN age ORDER BY age",
    );
    let rows = duckdb_and_datafusion(&plan, &fixture_graph(), true).await;
    assert_eq!(rows, vec!["30", "31"]);
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
    let rows = executor
        .run(&statements, "SELECT count(*) FROM \"node_person\"")
        .expect("query reused materialized graph");
    assert_eq!(
        rows,
        vec![vec![sql::SqlValue::Int(3)]],
        "reapplying a prepared setup must not duplicate rows"
    );
}

#[test]
fn duckdb_preserves_hugeint_results_exactly() {
    let mut executor = DuckDbExecutor::new();
    let value = "170141183460469231731687303715884105727";
    let rows = executor
        .run(&[], &format!("SELECT CAST('{value}' AS HUGEINT)"))
        .expect("hugeint query");
    assert_eq!(
        rows,
        vec![vec![sql::SqlValue::ExactNumber(value.to_string())]]
    );
}

#[test]
fn duckdb_replaces_changed_materialization_blocks() {
    let mut executor = DuckDbExecutor::new();
    let first = vec![
        "CREATE OR REPLACE TABLE \"items\" (\"value\" BIGINT)".to_string(),
        "INSERT INTO \"items\" VALUES (1)".to_string(),
    ];
    let second = vec![
        "CREATE OR REPLACE TABLE \"items\" (\"value\" BIGINT)".to_string(),
        "INSERT INTO \"items\" VALUES (2)".to_string(),
    ];
    assert_eq!(
        executor
            .run(&first, "SELECT sum(value) FROM items")
            .unwrap(),
        vec![vec![sql::SqlValue::ExactNumber("1".to_string())]]
    );
    assert_eq!(
        executor
            .run(&second, "SELECT sum(value) FROM items")
            .unwrap(),
        vec![vec![sql::SqlValue::ExactNumber("2".to_string())]]
    );
    assert_eq!(
        executor.run(&second, "SELECT count(*) FROM items").unwrap(),
        vec![vec![sql::SqlValue::Int(1)]]
    );
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
