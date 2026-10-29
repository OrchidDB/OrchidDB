use arrow::array::{ArrayRef, Int64Array};
use orchiddb_client::{LazyDuckdb, LocalFunction, Query, Schema, SourceConnection};
use std::sync::Arc;

struct Double;
impl LocalFunction for Double {
    fn call(&self, arguments: &[ArrayRef], _: usize) -> Result<ArrayRef, String> {
        let input = arguments[0].as_any().downcast_ref::<Int64Array>().unwrap();
        Ok(Arc::new(
            input
                .iter()
                .map(|n| n.map(|n| n * 2))
                .collect::<Int64Array>(),
        ))
    }
}

#[tokio::test]
async fn source_connection_needs_no_engine_choice_and_elides_completion() {
    let schema = Schema::from_json(r#"{"functions":[{"name":"twice","target":"twice","parameters":["int64"],"returns":"int64","local":true}]}"#).unwrap();
    let mut connection = SourceConnection::new(schema, Default::default());
    connection.register_function("twice", Arc::new(Double));
    connection
        .set_duckdb(Box::new(LazyDuckdb::new(|| {
            panic!("completion must be optimized away")
        })))
        .unwrap();
    let batches = connection
        .query(Query::cypher("RETURN twice($n) AS answer").parameter("n", 21))
        .await
        .unwrap()
        .collect_batches()
        .await
        .unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(
        batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .values(),
        &[42]
    );
}
