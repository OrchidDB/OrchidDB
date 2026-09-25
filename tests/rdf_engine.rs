#![cfg(feature = "duckdb")]

use std::sync::Arc;

use arrow::array::StringArray;
use arrow::datatypes::{DataType, Field, Schema};

use orchiddb::ir::rel::mapping::schema_only_provider;
use orchiddb::ir::rel::rdf::{IriQuadSource, RdfDatasetMapping};
use orchiddb::ir::rel::sql::DuckDbExecutor;
use orchiddb::rdf_engine::RdfGraphEngine;

#[tokio::test]
async fn public_engine_queries_external_quads_without_changing_source() {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE user_quads (graph VARCHAR, subject VARCHAR, predicate VARCHAR, object VARCHAR);
            INSERT INTO user_quads VALUES
                (NULL, 'https://example.com/alice', 'https://example.com/knows', 'https://example.com/bob'),
                (NULL, 'https://example.com/bob', 'https://example.com/knows', 'https://example.com/cara'),
                (NULL, 'https://example.com/alice', 'https://example.com/likes', 'https://example.com/dana');
            "#,
        )
        .unwrap();

    let schema = Arc::new(Schema::new(vec![
        Field::new("graph", DataType::Utf8, true),
        Field::new("subject", DataType::Utf8, false),
        Field::new("predicate", DataType::Utf8, false),
        Field::new("object", DataType::Utf8, false),
    ]));
    let mut mapping = RdfDatasetMapping::new();
    mapping
        .register_table("user_quads", schema_only_provider(schema))
        .map_iri_quads(
            "people",
            IriQuadSource::table("user_quads", "subject", "predicate", "object")
                .graph_column("graph"),
        );

    let executor = DuckDbExecutor::from_connection(connection);
    let mut engine = RdfGraphEngine::new(executor, Arc::new(mapping), "people");
    let output = engine
        .sparql(
            r#"
            PREFIX ex: <https://example.com/>
            SELECT ?predicate ?friend WHERE {
              ex:alice ?predicate ?middle .
              ?middle ex:knows ?friend .
            }
            "#,
        )
        .await
        .unwrap();
    assert_eq!(output.batch.num_rows(), 1);
    assert_eq!(output.fields, vec!["?predicate", "?friend"]);
    let predicate = output
        .batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    let friend = output
        .batch
        .column(1)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    assert_eq!(predicate.value(0), "https://example.com/knows");
    assert_eq!(friend.value(0), "https://example.com/cara");

    let mut executor = engine.into_executor();
    let connection = executor.connection().unwrap();
    let rows: i64 = connection
        .query_row("SELECT count(*) FROM user_quads", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 3);
}
