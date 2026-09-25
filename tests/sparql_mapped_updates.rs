#![cfg(feature = "duckdb")]
use arrow::datatypes::{DataType, Field, Schema};
use new_graph::ir::rel::mapping::schema_only_provider;
use new_graph::ir::rel::rdf::{IriQuadSource, RdfDatasetMapping, RdfTermColumns};
use new_graph::ir::rel::sql::DuckDbExecutor;
use new_graph::rdf_engine::{RdfGraphEngine, RdfTermValue, SparqlResults};
use std::sync::Arc;

fn engine() -> RdfGraphEngine {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    let columns = ["g", "s", "sk", "p", "pk", "o", "ok", "dt", "lang"];
    let schema = Arc::new(Schema::new(
        columns
            .iter()
            .map(|column| Field::new(*column, DataType::Utf8, true))
            .collect::<Vec<_>>(),
    ));
    let mut mapping = RdfDatasetMapping::new();
    for (table, predicate) in [("names", "urn:name"), ("ages", "urn:age")] {
        connection
            .execute_batch(&format!(
                "CREATE TABLE {table} ({})",
                columns
                    .iter()
                    .map(|c| format!("{c} VARCHAR"))
                    .collect::<Vec<_>>()
                    .join(",")
            ))
            .unwrap();
        mapping
            .register_table(table, schema_only_provider(schema.clone()))
            .map_typed_quads(
                "people",
                IriQuadSource::table(table, "s", "p", "o")
                    .graph_column("g")
                    .predicate(predicate)
                    .writable()
                    .typed_term_columns(
                        RdfTermColumns::new("s", "sk"),
                        RdfTermColumns::new("p", "pk"),
                        RdfTermColumns::new("o", "ok")
                            .datatype("dt")
                            .language("lang"),
                    ),
            );
    }
    connection
        .execute_batch("CREATE TABLE graphs (iri VARCHAR PRIMARY KEY)")
        .unwrap();
    mapping
        .register_table(
            "graphs",
            schema_only_provider(Arc::new(Schema::new(vec![Field::new(
                "iri",
                DataType::Utf8,
                false,
            )]))),
        )
        .map_writable_named_graphs("people", "graphs", "iri");
    RdfGraphEngine::new(
        DuckDbExecutor::from_connection(connection),
        Arc::new(mapping),
        "people",
    )
}

#[tokio::test]
async fn writes_use_both_mapped_tables_and_where_snapshot() {
    let mut engine = engine();
    engine
        .update(
            "INSERT DATA { <urn:a> <urn:name> 'Alice'; <urn:age> 30 }",
            None,
        )
        .await
        .unwrap();
    engine.update("DELETE { ?s <urn:age> ?old } INSERT { ?s <urn:age> ?new } WHERE { ?s <urn:age> ?old BIND(?old + 1 AS ?new) }",None).await.unwrap();
    assert_eq!(
        engine
            .query("SELECT ?age WHERE { <urn:a> <urn:age> ?age }")
            .await
            .unwrap(),
        SparqlResults::Solutions {
            variables: vec!["?age".into()],
            rows: vec![vec![Some(RdfTermValue::typed(
                "31",
                "http://www.w3.org/2001/XMLSchema#integer"
            ))]]
        }
    );
    engine
        .update(
            "INSERT DATA { <urn:a> <urn:name> 'Alice' }; DELETE DATA { <urn:a> <urn:age> 31 }",
            None,
        )
        .await
        .unwrap();
    let mut executor = engine.into_executor();
    let connection = executor.connection().unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM names", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM ages", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn request_rolls_back_all_tables_on_unmapped_or_database_error() {
    let mut engine = engine();
    assert!(engine.update("INSERT DATA { <urn:a> <urn:name> 'Alice'; <urn:age> 30 }; INSERT DATA { <urn:a> <urn:unknown> 1 }",None).await.is_err());
    assert_eq!(
        engine.query("ASK { ?s ?p ?o }").await.unwrap(),
        SparqlResults::Boolean(false)
    );
    let mapping = Arc::new(engine.mapping().clone());
    let mut executor = engine.into_executor();
    executor
        .connection()
        .unwrap()
        .execute_batch("CREATE UNIQUE INDEX one_age ON ages(s)")
        .unwrap();
    let mut engine = RdfGraphEngine::new(executor, mapping, "people");
    assert!(
        engine
            .update(
                "INSERT DATA { <urn:a> <urn:name> 'Alice'; <urn:age> 30,31 }",
                None
            )
            .await
            .is_err()
    );
    assert_eq!(
        engine.query("ASK { ?s ?p ?o }").await.unwrap(),
        SparqlResults::Boolean(false)
    );
}

#[tokio::test]
async fn graph_lifecycle_and_blank_nodes() {
    let mut engine = engine();
    engine.update("CREATE GRAPH <urn:g>; INSERT DATA { GRAPH <urn:g> { _:a <urn:name> 'A'; <urn:age> 1 } }",None).await.unwrap();
    engine
        .update("ADD GRAPH <urn:g> TO GRAPH <urn:h>", None)
        .await
        .unwrap();
    assert_eq!(
        engine
            .query("ASK { GRAPH <urn:g> { ?s <urn:name> 'A' } GRAPH <urn:h> { ?s <urn:age> 1 } }")
            .await
            .unwrap(),
        SparqlResults::Boolean(true)
    );
    engine.update("CLEAR GRAPH <urn:g>", None).await.unwrap();
    assert_eq!(
        engine.query("ASK { GRAPH <urn:g> {} }").await.unwrap(),
        SparqlResults::Boolean(true)
    );
    engine
        .update("DROP GRAPH <urn:g>; DROP SILENT GRAPH <urn:missing>", None)
        .await
        .unwrap();
    assert_eq!(
        engine.query("ASK { GRAPH <urn:g> {} }").await.unwrap(),
        SparqlResults::Boolean(false)
    );
    engine
        .update(
            "INSERT { _:a <urn:name> ?name } WHERE { VALUES ?name { 'A' 'B' } }",
            None,
        )
        .await
        .unwrap();
    let SparqlResults::Solutions { rows, .. } = engine
        .query("SELECT DISTINCT ?s WHERE { ?s <urn:name> ?name }")
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn with_changes_default_graph_without_hiding_explicit_named_graphs() {
    let mut engine = engine();
    engine.update("INSERT DATA { GRAPH <urn:g> { <urn:a> <urn:name> 'A'; <urn:age> 1 } GRAPH <urn:h> { <urn:a> <urn:name> 'B' } }",None).await.unwrap();
    engine.update("WITH <urn:h> DELETE { GRAPH <urn:g> { ?s <urn:age> ?age } } WHERE { GRAPH <urn:g> { ?s <urn:age> ?age } }",None).await.unwrap();
    assert_eq!(engine.query("ASK { GRAPH <urn:g> { ?s <urn:age> ?age } }").await.unwrap(),SparqlResults::Boolean(false));
    assert_eq!(engine.query("ASK { GRAPH <urn:h> { <urn:a> <urn:name> 'B' } }").await.unwrap(),SparqlResults::Boolean(true));
    engine.update("WITH <urn:h> DELETE { ?s <urn:name> ?name } WHERE { ?s <urn:name> ?name }",None).await.unwrap();
    assert_eq!(engine.query("ASK { GRAPH <urn:h> { ?s ?p ?o } }").await.unwrap(),SparqlResults::Boolean(false));
    assert_eq!(engine.query("ASK { GRAPH <urn:g> { <urn:a> <urn:name> 'A' } }").await.unwrap(),SparqlResults::Boolean(true));
}
