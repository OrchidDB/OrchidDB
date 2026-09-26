#![cfg(feature = "duckdb")]

//! Gremlin traverser semantics (package G1) executed through the relational
//! backend on DuckDB: per-traverser child traversals, productivity, labels,
//! property objects, and local/global scope. Each test pins a TinkerPop
//! expectation in corpus notation (`v[marko]`, `d[3].l`, ...).
//!
//! `GREMLIN_G1_PROBE="g.V()...;g.inject(1)..."` with the ignored `probe`
//! test prints DataFusion DAG and DuckDB output side by side.

#[path = "common/execution.rs"]
mod datafusion_test;
mod gremlin_case_runner;

use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, RecordBatch};
use arrow::datatypes::{DataType, Field, Schema};
use gremlin_case_runner::{compare, dataset, format};
use orchiddb::ir::catalog::PropertyGraph;
use crate::datafusion_test::execute_async as interpret;
use orchiddb::ir::rel::RelBackend;
use orchiddb::ir::rel::sql::{self, DuckDbExecutor, SqlExecutor, SqlValue, TableData};
use orchiddb::language::gremlin::planner::GremlinPlanner;

fn plan(query: &str) -> orchiddb::ir::plan::GraphPlan {
    let traversal =
        gremlin_case_runner::parse::gremlin_with_case(query, "").expect("parse gremlin");
    GremlinPlanner::new()
        .plan(&traversal)
        .expect("plan gremlin")
}

async fn dag_lines(query: &str, graph: &PropertyGraph) -> Result<Vec<String>, String> {
    interpret(&plan(query), graph).await
        .map(|batches| format::lines_from_batch(&batches))
        .map_err(|error| error.to_string())
}

async fn duckdb_lines(query: &str, graph: &PropertyGraph) -> Result<Vec<String>, String> {
    let lowered = RelBackend::new()
        .lower(&plan(query), graph)
        .map_err(|error| format!("lower: {error}"))?;
    let mut executor = DuckDbExecutor::new();
    sql::execute_lowered_sql(&mut executor, &lowered)
        .await
        .map(|batches| format::lines_from_batch(&batches))
        .map_err(|error| format!("execute: {error}"))
}

/// Run on DuckDB and require a match with `expected`.
async fn assert_duckdb_with(query: &str, graph: &PropertyGraph, ordered: bool, expected: &[&str]) {
    let actual = duckdb_lines(query, graph)
        .await
        .unwrap_or_else(|error| panic!("{query}: {error}"));
    let expected = expected.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    match compare::matches(&actual, &expected, ordered, "rows") {
        compare::Verdict::Match => {}
        compare::Verdict::Mismatch { reason } => {
            panic!("{query}: {reason}\n actual={actual:?}\n expected={expected:?}")
        }
    }
}

async fn assert_duckdb(query: &str, graph: &PropertyGraph, expected: &[&str]) {
    assert_duckdb_with(query, graph, false, expected).await;
}

fn modern() -> PropertyGraph {
    dataset::modern_graph()
}

#[tokio::test]
#[ignore]
async fn probe() {
    let graph = match std::env::var("GREMLIN_G1_DATASET").as_deref() {
        Ok("crew") => dataset::crew_graph(),
        _ => modern(),
    };
    let queries = std::env::var("GREMLIN_G1_PROBE").unwrap_or_default();
    for query in queries.split(';').map(str::trim).filter(|q| !q.is_empty()) {
        println!("=== {query}");
        println!("DAG: {:?}", dag_lines(query, &graph).await);
        println!("duckdb: {:?}", duckdb_lines(query, &graph).await);
        if std::env::var("GREMLIN_G1_SHOW").is_ok_and(|v| v == "1") {
            let plan = plan(query);
            println!("{}", orchiddb::ir::plan::explain(&plan));
            if std::env::var("GREMLIN_G1_DEBUG").is_ok_and(|v| v == "1") {
                println!("{:#?}", plan.root);
            }
            if let Ok(lowered) = RelBackend::new().lower(&plan, &graph) {
                println!("{lowered:?}");
            }
        }
    }
}

/// One DuckDB session reuses scan tables by content. Two scans with equal
/// content but different binding column names must not share a stale view.
#[test]
fn scan_view_is_rebuilt_when_columns_change_over_same_content() {
    let table = |column: &str| {
        let schema = Arc::new(Schema::new(vec![Field::new(
            column,
            DataType::Int64,
            false,
        )]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![Arc::new(Int64Array::from(vec![1_i64, 2])) as ArrayRef],
        )
        .expect("batch");
        TableData {
            name: "__graph_rel_nodes_2".into(),
            schema,
            batches: vec![batch],
        }
    };
    let mut executor = DuckDbExecutor::new();
    let first = executor
        .run_with_tables(
            &[table("current__id")],
            &[],
            "SELECT sum(\"current__id\") FROM \"__graph_rel_nodes_2\"",
        )
        .expect("first scan");
    let second = executor
        .run_with_tables(
            &[table("v_0__id")],
            &[],
            "SELECT sum(\"v_0__id\") FROM \"__graph_rel_nodes_2\"",
        )
        .expect("second scan must see its own column names");
    assert_eq!(first, second);
    assert!(
        matches!(second[0][0], SqlValue::Int(3) | SqlValue::ExactNumber(_)),
        "{second:?}"
    );
}

/// `by()` modulators run once per traverser. A global LIMIT 1 over the
/// child would give every traverser the first traverser's key.
#[tokio::test]
async fn by_modulator_is_evaluated_per_traverser() {
    let graph = modern();
    assert_duckdb(
        "g.V().groupCount().by('age')",
        &graph,
        &[r#"m[{"d[27].i":"d[1].l", "d[29].i":"d[1].l", "d[32].i":"d[1].l", "d[35].i":"d[1].l"}]"#],
    )
    .await;
    assert_duckdb(
        "g.V().has('lang').group().by('lang').by(__.count())",
        &graph,
        &[r#"m[{"java":"d[2].l"}]"#],
    )
    .await;
    assert_duckdb(
        "g.V().group().by(__.values('name').substring(0,1)).by(__.constant(1))",
        &graph,
        &[r#"m[{"j":"d[1].i", "l":"d[1].i", "m":"d[1].i", "p":"d[1].i", "r":"d[1].i", "v":"d[1].i"}]"#],
    )
    .await;
}

/// A reducing child emits its empty-input value per traverser: `count()` of
/// an empty child stream is 0 rather than a dropped traverser.
#[tokio::test]
async fn local_count_keeps_traversers_with_empty_children() {
    let graph = modern();
    assert_duckdb(
        "g.V().local(__.outE().count())",
        &graph,
        &["d[3].l", "d[0].l", "d[0].l", "d[2].l", "d[0].l", "d[1].l"],
    )
    .await;
    assert_duckdb(
        "g.V().where(__.outE().count().is(0)).values('name')",
        &graph,
        &["vadas", "lop", "ripple"],
    )
    .await;
    assert_duckdb(
        "g.V().hasLabel('person').choose(__.local(__.out('knows').count())).option(0, __.constant('noFriends')).option(Pick.none, __.constant('hasFriends'))",
        &graph,
        &["hasFriends", "noFriends", "noFriends", "noFriends"],
    )
    .await;
}

/// A child that replaces `current` owns the traverser's value after the
/// apply; the input element must not shadow it.
#[tokio::test]
async fn child_output_replaces_current_per_traverser() {
    let graph = modern();
    assert_duckdb(
        "g.V().local(__.out().local(__.count()))",
        &graph,
        &["d[1].l", "d[1].l", "d[1].l", "d[1].l", "d[1].l", "d[1].l"],
    )
    .await;
    assert_duckdb(
        "g.V(vid1).out().map(__.values('name')).map(__.length())",
        &graph,
        &["d[3].i", "d[4].i", "d[5].i"],
    )
    .await;
}

/// Branches that keep the element on some traversers and produce a scalar
/// on others yield one heterogeneous stream.
#[tokio::test]
async fn choose_mixes_element_and_scalar_traversers() {
    let graph = modern();
    assert_duckdb(
        "g.V().choose(__.has('name','vadas'), __.values('name'))",
        &graph,
        &[
            "v[marko]",
            "vadas",
            "v[lop]",
            "v[josh]",
            "v[ripple]",
            "v[peter]",
        ],
    )
    .await;
    assert_duckdb(
        "g.V().choose(__.values('age').is(P.lte(30)), __.values('name'), __.identity())",
        &graph,
        &[
            "marko",
            "vadas",
            "v[lop]",
            "v[josh]",
            "v[ripple]",
            "v[peter]",
        ],
    )
    .await;
}

#[tokio::test]
async fn edge_endpoints_preserve_vertex_properties_and_labels() {
    let graph = modern();
    for query in [
        "g.V().outE().inV().values('name')",
        "g.V().outE().as('e').inV().as('v').select('v').values('name')",
        "g.E().inV().values('name')",
    ] {
        assert_duckdb(
            query,
            &graph,
            &["vadas", "josh", "lop", "ripple", "lop", "lop"],
        )
        .await;
    }
    assert_duckdb(
        "g.E().outV().values('name')",
        &graph,
        &["marko", "marko", "marko", "josh", "josh", "peter"],
    )
    .await;
    assert_duckdb(
        "g.V(vid1).outE('knows').inV().out('created').values('name')",
        &graph,
        &["ripple", "lop"],
    )
    .await;
}

#[tokio::test]
async fn select_preserves_element_shape_and_rejects_missing_label_history() {
    let graph = modern();
    assert_duckdb(
        "g.V(vid1).as('a').out('knows').select('a').values('name')",
        &graph,
        &["marko", "marko"],
    )
    .await;
    assert_duckdb(
        "g.V(vid1).as('a').out('knows').as('a').select(Pop.last,'a').values('name')",
        &graph,
        &["vadas", "josh"],
    )
    .await;
    let error = duckdb_lines(
        "g.V(vid1).as('a').out('knows').as('a').select(Pop.first,'a')",
        &graph,
    )
    .await
    .expect_err("repeated label history must not silently select the last value");
    assert!(error.contains("label history"), "{error}");
}

#[tokio::test]
async fn null_traversers_are_productive() {
    let graph = modern();
    assert_duckdb("g.inject(null,1,null)", &graph, &["null", "d[1].i", "null"]).await;
    assert_duckdb(
        "g.inject(null).coalesce(__.identity(),__.constant('fallback'))",
        &graph,
        &["null"],
    )
    .await;
}

#[tokio::test]
async fn utf16_strings_preserve_surrogate_boundaries_in_sql_and_datafusion() {
    let graph = PropertyGraph::new();
    graph.insert_node("text", [("name".into(), orchiddb::ir::Value::String("A😀B".into()))].into());
    for (suffix, expected) in [
        ("length()", "d[4].i"),
        ("substring(1,3)", "😀"),
        ("substring(1,2)", "�"),
        ("substring(2,3)", "�"),
        ("substring(-3,-1)", "😀"),
        ("substring(3,2)", ""),
        ("substring(2,2)", ""),
    ] {
        let query = format!("g.V().values('name').{suffix}");
        assert_duckdb(&query, &graph, &[expected]).await;
        let result = RelBackend::new().execute(&plan(&query), &graph).await.unwrap();
        if suffix == "length()" {
            assert_eq!(result.batch.column(0).data_type(), &DataType::Int32);
            assert_eq!(format::lines_from_batch(&result), vec!["4"], "DataFusion: {query}");
        } else {
            let values = result.batch.column(0).as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
            assert_eq!(values.value(0), expected, "DataFusion: {query}");
        }
    }
}

#[tokio::test]
async fn public_ids_are_not_storage_offsets_or_coerced_strings() {
    let graph = modern();
    assert_duckdb("g.V(1).values('name')", &graph, &["marko"]).await;
    assert_duckdb("g.V(1).id()", &graph, &["d[1].i"]).await;
    assert_duckdb("g.V('person#0').values('name')", &graph, &[]).await;
}

#[tokio::test]
async fn group_keys_preserve_quotes_and_backslashes() {
    let graph = PropertyGraph::new();
    graph.insert_node("text", [("name".into(), orchiddb::ir::Value::String("a\"b\\c".into()))].into());
    assert_duckdb("g.V().groupCount().by('name')", &graph,
        &[r#"m[{"a\"b\\c":"d[1].l"}]"#]).await;
}
