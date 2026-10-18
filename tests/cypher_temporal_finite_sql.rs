#![cfg(feature = "duckdb")]
use orchiddb::ir::{
    Value,
    catalog::PropertyGraph,
    rel::{
        RelBackend,
        sql::{DuckDbExecutor, execute_lowered_sql},
    },
};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

fn graph(kind: &str, values: &[&str]) -> PropertyGraph {
    let graph = PropertyGraph::new();
    for text in values {
        graph.insert_node(
            "T",
            BTreeMap::from([(
                "v".into(),
                Value::Temporal(orchiddb::ir::temporal::parse(kind, text).unwrap()),
            )]),
        );
    }
    graph
}
async fn rows(graph: &PropertyGraph, query: &str) -> Vec<String> {
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    let lowered = RelBackend::new()
        .with_language_functions(true)
        .lower(&plan, graph)
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    (0..result.batch.num_rows())
        .map(|row| {
            let value =
                datafusion::common::ScalarValue::try_from_array(result.batch.column(0), row)
                    .unwrap();
            if value.is_null() {
                "NULL".into()
            } else {
                orchiddb::ir::functions::domain::storage(&value)
                    .map_or_else(|_| value.to_string(), |v| arrow::util::display::array_value_to_string(v.to_array().unwrap().as_ref(), 0).unwrap())
            }
        })
        .collect()
}
#[tokio::test]
async fn snapshot_temporal_order_is_semantic_and_keeps_duplicates_and_nulls() {
    let graph = graph("time", &["00:30-01:00", "00:30+01:00", "00:30+01:00"]);
    graph.insert_node("T", BTreeMap::new());
    assert_eq!(
        rows(&graph, "MATCH (n:T) WITH n.v AS v RETURN v ORDER BY v").await,
        [
            "time|00:30+01:00",
            "time|00:30+01:00",
            "time|00:30-01:00",
            "NULL"
        ]
    );
}
#[tokio::test]
async fn snapshot_temporal_properties_arithmetic_and_comparison_use_reference_semantics() {
    let graph = graph("date", &["2000-01-31", "1999-12-31"]);
    assert_eq!(
        rows(&graph, "MATCH (n:T) WITH n.v AS d RETURN d.year ORDER BY d").await,
        ["1999", "2000"]
    );
    assert_eq!(
        rows(
            &graph,
            "MATCH (n:T) RETURN n.v + duration('P1M') AS v ORDER BY v"
        )
        .await,
        ["2000-01-31", "2000-02-29"]
    );
    assert_eq!(
        rows(
            &graph,
            "MATCH (n:T) RETURN n.v < date('2000-01-01') AS v ORDER BY v"
        )
        .await,
        ["false", "true"]
    );
}
#[tokio::test]
async fn relowering_reads_new_snapshot_without_reusing_old_dictionary() {
    let graph = graph("date", &["2000-01-01"]);
    assert_eq!(
        rows(&graph, "MATCH (n:T) WITH n.v AS d RETURN d.year").await,
        ["2000"]
    );
    graph.insert_node(
        "T",
        BTreeMap::from([(
            "v".into(),
            Value::Temporal(orchiddb::ir::temporal::parse("date", "1984-10-11").unwrap()),
        )]),
    );
    assert_eq!(
        rows(
            &graph,
            "MATCH (n:T) WITH n.v AS d RETURN d.year AS y ORDER BY y"
        )
        .await,
        ["1984", "2000"]
    );
}
#[test]
fn large_domains_and_undefined_duration_order_remain_runtime() {
    let graph = graph("duration", &["P1D", "P2D"]);
    let plan = CypherPlanner::new()
        .plan(&parse_query("MATCH (n:T) RETURN n.v ORDER BY n.v").unwrap())
        .unwrap();
    assert!(RelBackend::new().lower(&plan, &graph).is_err());
    let graph = PropertyGraph::new();
    for year in 1900..1965 {
        graph.insert_node(
            "T",
            BTreeMap::from([(
                "v".into(),
                Value::Temporal(
                    orchiddb::ir::temporal::parse("date", &format!("{year}-01-01")).unwrap(),
                ),
            )]),
        );
    }
    let plan = CypherPlanner::new()
        .plan(&parse_query("MATCH (n:T) WITH n.v AS d RETURN d.year").unwrap())
        .unwrap();
    assert!(RelBackend::new().lower(&plan, &graph).is_err());
}

#[tokio::test]
async fn compiled_temporal_dictionary_and_rows_share_one_immutable_snapshot() {
    let graph = graph("date", &["2000-01-01"]);
    let plan = CypherPlanner::new()
        .plan(&parse_query("MATCH (n:T) WITH n.v AS d RETURN d.year").unwrap())
        .unwrap();
    let lowered = RelBackend::new().lower(&plan, &graph).unwrap();
    graph.insert_node(
        "T",
        BTreeMap::from([(
            "v".into(),
            Value::Temporal(orchiddb::ir::temporal::parse("date", "1984-10-11").unwrap()),
        )]),
    );
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await
        .unwrap();
    assert_eq!(result.batch.num_rows(), 1);
    assert_eq!(
        datafusion::common::ScalarValue::try_from_array(result.batch.column(0), 0)
            .map(|v| arrow::util::display::array_value_to_string(v.to_array().unwrap().as_ref(), 0).unwrap()).unwrap(),
        "2000"
    );
}

#[tokio::test]
async fn temporal_case_without_else_includes_null_in_its_domain() {
    let graph = PropertyGraph::new();
    assert_eq!(
        rows(&graph, "UNWIND [1,2] AS k WITH CASE WHEN k=1 THEN date('2020-01-01') END AS d RETURN (d=date('2020-01-01')) OR true AS value").await,
        ["true", "true"]
    );
}

#[test]
fn volatile_case_does_not_prove_repeatable_temporal_input() {
    let query = "WITH CASE WHEN rand()<0.5 THEN date('2020-01-01') ELSE date('2021-01-01') END AS d RETURN d.year,d=d";
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    assert!(RelBackend::new().with_language_functions(true).lower(&plan, &PropertyGraph::new()).is_err());
}
