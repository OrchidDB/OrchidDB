#![cfg(feature = "duckdb")]

use orchiddb::ir::{catalog::PropertyGraph, value::Value, rel::{RelBackend, sql::{DuckDbExecutor, SqlDialect, execute_lowered_sql, unparse}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

#[tokio::test]
async fn graph_groups_keep_identity_payload_and_duplicate_multiplicity() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("A", BTreeMap::from([("k".into(), Value::Int(10))]));
    let b = graph.insert_node("B", BTreeMap::from([("k".into(), Value::Int(20))]));
    graph.insert_edge("T", &a, &b, BTreeMap::from([("w".into(), Value::Int(7))])).unwrap();
    graph.insert_edge("T", &a, &b, BTreeMap::from([("w".into(), Value::Int(8))])).unwrap();
    for (query, expected) in [
        ("MATCH (n) UNWIND [1,1] AS x WITH n,count(*) AS c RETURN n.k,c ORDER BY n.k", vec!["10|2", "20|2"]),
        ("MATCH (a:A)-[r:T]->(b) WITH a,r AS rel,b,count(*) AS c MATCH (a)-[rel]->(b) RETURN rel.w,c ORDER BY rel.w", vec!["7|1", "8|1"]),
        ("MATCH (n) OPTIONAL MATCH (n)-[:MISSING]->(x) WITH x,count(*) AS c RETURN x IS NULL,c", vec!["true|2"]),
        ("MATCH (n) WHERE false WITH n,count(*) AS c RETURN n.k,c", vec![]),
        ("MATCH (n) UNWIND [2,1,2] AS x WITH n,collect(DISTINCT x) AS xs RETURN n.k,xs ORDER BY n.k", vec!["10|[2, 1]", "20|[2, 1]"]),
        ("MATCH (n) UNWIND [1,1] AS x WITH n AS original,n AS copy,count(*) AS c RETURN original=copy,c ORDER BY original.k", vec!["true|2", "true|2"]),
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph)
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        assert!(!sql.contains("__orchiddb_value"), "{query}: {sql}");
        let returned = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await
            .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
        let rows = (0..returned.batch.num_rows()).map(|i| returned.batch.columns().iter().map(|a|
            arrow::util::display::array_value_to_string(a, i).unwrap()
        ).collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
        assert_eq!(rows, expected, "{query}\n{sql}");
    }
}

#[tokio::test]
async fn graph_keys_use_present_properties_without_internal_metadata() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("A", BTreeMap::from([
        ("id".into(), Value::Int(1)), ("present".into(), Value::Int(2)),
        ("absent".into(), Value::Null), ("list".into(), Value::List(vec![])),
    ]));
    let b = graph.insert_node("B", BTreeMap::from([("other".into(), Value::Int(3))]));
    graph.insert_edge("T", &a, &b, BTreeMap::from([("weight".into(), Value::Int(5))])).unwrap();
    for (query, expected) in [
        ("MATCH (n) UNWIND keys(n) AS k RETURN DISTINCT k ORDER BY k", vec!["id", "list", "other", "present"]),
        ("MATCH ()-[r:T]->() UNWIND keys(r) AS k RETURN k", vec!["weight"]),
        ("OPTIONAL MATCH (n:Missing) UNWIND keys(n) AS k RETURN k", vec![]),
        ("OPTIONAL MATCH (n:Missing) RETURN keys(n) IS NULL", vec!["true"]),
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph)
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        let returned = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await
            .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
        let rows = (0..returned.batch.num_rows()).map(|i|
            arrow::util::display::array_value_to_string(returned.batch.column(0), i).unwrap()
        ).collect::<Vec<_>>();
        assert_eq!(rows, expected, "{query}\n{sql}");
    }
}

#[tokio::test]
async fn graph_union_aligns_columns_and_deduplicates_identities() {
    let graph = PropertyGraph::new();
    graph.insert_node("A", BTreeMap::new());
    graph.insert_node("B", BTreeMap::new());
    for (query, expected_rows) in [
        ("MATCH (a:A) RETURN a UNION MATCH (b:B) RETURN b AS a", 2),
        ("MATCH (a:A) RETURN a UNION MATCH (b:A) RETURN b AS a", 1),
        ("MATCH (a:A) RETURN a UNION ALL MATCH (b:A) RETURN b AS a", 2),
        ("OPTIONAL MATCH (a:Missing) RETURN a UNION OPTIONAL MATCH (b:Missing) RETURN b AS a", 1),
        ("MATCH (a:A) RETURN a,1 AS x UNION MATCH (b:A) RETURN b AS a,2 AS x", 2),
    ] {
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph)
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        let returned = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await
            .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
        assert_eq!(returned.batch.num_rows(), expected_rows, "{query}\n{sql}");
        assert!(returned.batch.schema().field_with_name("a__id").is_ok(), "{query}");
    }
}

#[tokio::test]
async fn graph_union_distinct_preserves_duplicate_outer_occurrences() {
    let graph = PropertyGraph::new();
    graph.insert_node("A", BTreeMap::new());
    let query = "UNWIND [1,1,3] AS x RETURN x, COUNT { MATCH (a:A) WHERE x < 2 RETURN a UNION MATCH (a:A) WHERE x < 2 RETURN a } AS n ORDER BY x";
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph).unwrap();
    let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
    let returned = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await
        .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
    let rows = (0..returned.batch.num_rows()).map(|i| returned.batch.columns().iter().map(|a|
        arrow::util::display::array_value_to_string(a, i).unwrap()
    ).collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
    assert_eq!(rows, vec!["1|1", "1|1", "3|0"], "{sql}");
}
