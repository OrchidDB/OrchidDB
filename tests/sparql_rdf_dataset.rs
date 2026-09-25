#![cfg(feature = "duckdb")]
use std::collections::BTreeSet;
use std::sync::Arc;

use arrow::array::{ArrayRef, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::datasource::MemTable;

use orchiddb::ir::PropertyGraph;
use orchiddb::ir::df::{from_logical_plan, to_logical_plan};
use orchiddb::ir::rel::rdf::{IriQuadSource, RdfDatasetMapping};
use orchiddb::ir::rel::sql::{self, DuckDbExecutor, SqlDialect};
use orchiddb::ir::rel::{RelBackend, RelBackendOptions};
use orchiddb::language::sparql::SparqlPlanner;

const EX: &str = "https://example.com/";

fn backend() -> RelBackend {
    let batch = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("g", DataType::Utf8, true),
            Field::new("s", DataType::Utf8, false),
            Field::new("p", DataType::Utf8, false),
            Field::new("o", DataType::Utf8, false),
        ])),
        vec![
            Arc::new(StringArray::from(vec![
                None,
                None,
                None,
                Some("https://example.com/graph"),
                Some("https://example.com/graph2"),
            ])) as ArrayRef,
            Arc::new(StringArray::from(vec![
                format!("{EX}alice"),
                format!("{EX}alice"),
                format!("{EX}bob"),
                format!("{EX}alice"),
                format!("{EX}alice"),
            ])) as ArrayRef,
            Arc::new(StringArray::from(vec![format!("{EX}knows"); 5])) as ArrayRef,
            Arc::new(StringArray::from(vec![
                format!("{EX}bob"),
                format!("{EX}bob"),
                format!("{EX}cara"),
                format!("{EX}dana"),
                format!("{EX}dana"),
            ])) as ArrayRef,
        ],
    )
    .unwrap();
    let mut mapping = RdfDatasetMapping::new();
    mapping
        .register_table(
            "iri_quads",
            Arc::new(MemTable::try_new(batch.schema(), vec![vec![batch]]).unwrap()),
        )
        .map_iri_quads(
            "default",
            IriQuadSource::table("iri_quads", "s", "p", "o").graph_column("g"),
        );
    RelBackend::with_options(RelBackendOptions {
        rdf_datasets: Some(Arc::new(mapping)),
        ..RelBackendOptions::default()
    })
}

async fn duckdb_rows(query: &str) -> Vec<Vec<String>> {
    let plan = SparqlPlanner::default().plan_str(query).unwrap();
    let lowered = backend().lower(&plan, &PropertyGraph::new()).unwrap();
    let sql = sql::unparse(&lowered, SqlDialect::DuckDb).unwrap();
    assert!(sql.contains("iri_quads"), "{sql}");
    let prepared = sql::prepare_with_external(
        &lowered,
        SqlDialect::DuckDb,
        &BTreeSet::from(["iri_quads".to_string()]),
    )
    .await
    .unwrap();
    let mut executor = DuckDbExecutor::from_connection(connection());
    let output = sql::execute_prepared(&mut executor, &prepared)
        .unwrap_or_else(|error| panic!("{error}: {sql}"));
    (0..output.batch.num_rows())
        .map(|row| {
            // Visible fields come first; RDF term identity columns follow.
            (0..output.fields.len())
                .map(|column| {
                    output
                        .batch
                        .column(column)
                        .as_any()
                        .downcast_ref::<StringArray>()
                        .unwrap()
                        .value(row)
                        .to_string()
                })
                .collect()
        })
        .collect()
}

fn connection() -> duckdb::Connection {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE iri_quads(g VARCHAR, s VARCHAR, p VARCHAR, o VARCHAR);
            INSERT INTO iri_quads VALUES
                (NULL, 'https://example.com/alice', 'https://example.com/knows', 'https://example.com/bob'),
                (NULL, 'https://example.com/alice', 'https://example.com/knows', 'https://example.com/bob'),
                (NULL, 'https://example.com/bob', 'https://example.com/knows', 'https://example.com/cara'),
                ('https://example.com/graph', 'https://example.com/alice', 'https://example.com/knows', 'https://example.com/dana'),
                ('https://example.com/graph2', 'https://example.com/alice', 'https://example.com/knows', 'https://example.com/dana');
            "#,
        )
        .unwrap();
    connection
}

#[tokio::test]
async fn variable_predicate_and_two_pattern_join_run_on_duckdb() {
    let rows = duckdb_rows(
        "PREFIX ex: <https://example.com/> SELECT ?p ?friend WHERE { ex:alice ?p ?middle . ?middle ex:knows ?friend }",
    ).await;
    assert_eq!(rows, vec![vec![format!("{EX}knows"), format!("{EX}cara")]]);
}

#[tokio::test]
async fn named_graph_variable_and_default_scope_are_distinct() {
    let mut rows = duckdb_rows(
        "PREFIX ex: <https://example.com/> SELECT ?g ?o WHERE { GRAPH ?g { ex:alice ex:knows ?o } }",
    ).await;
    rows.sort();
    assert_eq!(
        rows,
        vec![
            vec![format!("{EX}graph"), format!("{EX}dana")],
            vec![format!("{EX}graph2"), format!("{EX}dana")],
        ]
    );

    let rows =
        duckdb_rows("PREFIX ex: <https://example.com/> SELECT ?o WHERE { ex:alice ex:knows ?o }")
            .await;
    assert_eq!(rows, vec![vec![format!("{EX}bob")]]);

    let rows = duckdb_rows(
        "PREFIX ex: <https://example.com/> SELECT ?o WHERE { GRAPH ex:graph { ex:alice ex:knows ?o } }",
    ).await;
    assert_eq!(rows, vec![vec![format!("{EX}dana")]]);
}

#[tokio::test]
async fn from_merges_named_graphs_into_a_set_valued_default_graph() {
    let query = "PREFIX ex: <https://example.com/> SELECT ?o FROM ex:graph FROM ex:graph2 WHERE { ex:alice ex:knows ?o }";
    let plan = SparqlPlanner::default().plan_str(query).unwrap();
    assert!(!orchiddb::ir::explain(&plan).contains("SparqlDataset"));
    assert_eq!(duckdb_rows(query).await, vec![vec![format!("{EX}dana")]]);

    // Declaring a default graph alone makes no named graph visible to GRAPH.
    let rows = duckdb_rows(
        "PREFIX ex: <https://example.com/> SELECT ?o FROM ex:graph WHERE { GRAPH ex:graph { ex:alice ex:knows ?o } }",
    )
    .await;
    assert!(rows.is_empty());
}

#[tokio::test]
async fn from_named_restricts_graph_selection_and_leaves_default_empty() {
    let rows = duckdb_rows(
        "PREFIX ex: <https://example.com/> SELECT ?g ?o FROM NAMED ex:graph WHERE { GRAPH ?g { ex:alice ex:knows ?o } }",
    )
    .await;
    assert_eq!(rows, vec![vec![format!("{EX}graph"), format!("{EX}dana")]]);

    let rows = duckdb_rows(
        "PREFIX ex: <https://example.com/> SELECT ?o FROM NAMED ex:graph WHERE { ex:alice ex:knows ?o }",
    )
    .await;
    assert!(rows.is_empty());

    let rows = duckdb_rows(
        "PREFIX ex: <https://example.com/> SELECT ?o FROM NAMED ex:graph WHERE { GRAPH ex:graph2 { ex:alice ex:knows ?o } }",
    )
    .await;
    assert!(rows.is_empty());

    let rows = duckdb_rows(
        "PREFIX ex: <https://example.com/> SELECT ?o FROM NAMED ex:missing WHERE { GRAPH ex:missing { ex:alice ex:knows ?o } }",
    )
    .await;
    assert!(rows.is_empty());
}

#[tokio::test]
async fn mixed_dataset_scopes_survive_ir_round_trip_and_execute() {
    let query = "PREFIX ex: <https://example.com/> SELECT ?default ?g ?named FROM ex:graph2 FROM NAMED ex:graph WHERE { ex:alice ex:knows ?default . GRAPH ?g { ex:alice ex:knows ?named } }";
    let plan = SparqlPlanner::default().plan_str(query).unwrap();
    let logical = to_logical_plan(&plan).unwrap();
    assert_eq!(from_logical_plan(&logical).unwrap(), plan);
    assert_eq!(
        duckdb_rows(query).await,
        vec![vec![
            format!("{EX}dana"),
            format!("{EX}graph"),
            format!("{EX}dana"),
        ]]
    );
}

#[tokio::test]
async fn repeated_variables_and_missing_predicates_filter_correctly() {
    let rows =
        duckdb_rows("PREFIX ex: <https://example.com/> SELECT ?x WHERE { ?x ex:knows ?x }").await;
    assert!(rows.is_empty());
    let rows =
        duckdb_rows("PREFIX ex: <https://example.com/> SELECT ?x WHERE { ?x ex:missing ex:bob }")
            .await;
    assert!(rows.is_empty());
}

#[test]
fn typed_literal_cannot_be_compared_as_iri_text() {
    let plan = SparqlPlanner::default()
        .plan_str("PREFIX ex: <https://example.com/> SELECT ?s WHERE { ?s ex:age 42 }")
        .unwrap();
    let error = backend().lower(&plan, &PropertyGraph::new()).unwrap_err();
    assert!(
        error.to_string().contains("typed RDF term source"),
        "{error}"
    );
}

#[tokio::test]
async fn graph_variable_groups_and_optional_scopes_execute() {
    let rows = duckdb_rows(
        "SELECT ?g (COUNT(*) AS ?c) WHERE { GRAPH ?g { ?s ?p ?o } } GROUP BY ?g ORDER BY ?g",
    )
    .await;
    assert_eq!(
        rows,
        vec![
            vec![format!("{EX}graph"), "1".to_string()],
            vec![format!("{EX}graph2"), "1".to_string()],
        ]
    );
    // The default graph is a set: the duplicated source row counts once.
    let rows = duckdb_rows("SELECT (COUNT(*) AS ?c) WHERE { ?s ?p ?o }").await;
    assert_eq!(rows, vec![vec!["2".to_string()]]);
    let rows = duckdb_rows(
        "PREFIX ex: <https://example.com/> SELECT ?s ?g WHERE { ?s ex:knows ?o OPTIONAL { GRAPH ?g { ?s ex:knows ex:dana } } } ORDER BY ?s ?g",
    )
    .await;
    assert_eq!(
        rows,
        vec![
            vec![format!("{EX}alice"), format!("{EX}graph")],
            vec![format!("{EX}alice"), format!("{EX}graph2")],
            vec![format!("{EX}bob"), String::new()],
        ]
    );
}
