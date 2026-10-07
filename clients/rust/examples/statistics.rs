#[path = "support/duckdb_statistics.rs"]
mod adapter;
use duckdb::arrow::{
    array::StringArray,
    datatypes::SchemaRef,
    error::ArrowError,
    record_batch::{RecordBatch, RecordBatchReader},
};
use duckdb::{Connection, Statement};
struct Batches<'a>(duckdb::Arrow<'a>);
impl Iterator for Batches<'_> {
    type Item = Result<RecordBatch, ArrowError>;
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(Ok)
    }
}
impl RecordBatchReader for Batches<'_> {
    fn schema(&self) -> SchemaRef {
        self.0.get_schema()
    }
}
use orchiddb_client::{
    Connection as OrchidConnection, Query, Schema, SqlDialect, SqlSession, SqlWork as CompiledSql,
};

// Borrows the application's existing connection; never closes it or commits.
struct BorrowedDuckDb<'connection> {
    connection: &'connection Connection,
    statement: Option<Statement<'connection>>,
}
impl SqlSession for BorrowedDuckDb<'_> {
    type Error = duckdb::Error;
    type Output<'session>
        = Batches<'session>
    where
        Self: 'session;
    fn dialect(&self) -> SqlDialect {
        SqlDialect::DuckDb
    }
    async fn query<'session>(
        &'session mut self,
        query: &CompiledSql,
    ) -> Result<Batches<'session>, Self::Error> {
        self.statement = Some(self.connection.prepare(&query.sql)?);
        self.statement
            .as_mut()
            .unwrap()
            .query_arrow([])
            .map(Batches)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = Connection::open_in_memory()?;
    database.execute_batch("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1,'Ada'),(2,'Grace')")?;
    let schema = Schema::from_json(r#"{"tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]}],"nodes":[{"label":"Person","table":"people","id":"id","properties":{"name":"name"}}]}"#)
        .map_err(std::io::Error::other)?;
    let session = BorrowedDuckDb {
        connection: &database,
        statement: None,
    };
    let mut graph = OrchidConnection::new(session, schema);
    let report = graph
        .generate_statistics(|work| {
            let result = adapter::collect_statistics(&database, &work);
            async move { result }
        })
        .await
        .map_err(std::io::Error::other)?;
    println!(
        "Sampled {} rows for query planning",
        report["accepted_rows"]
    );
    let mut result = graph
        .query(
            Query::cypher("MATCH (p:Person) WHERE p.name=$name RETURN p.name")
                .parameter("name", "Ada"),
        )
        .await?;
    for batch in &mut result {
        let batch = batch?;
        let names = batch
            .column(0)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap();
        for name in names.iter().flatten() {
            println!("{name}");
        }
    }
    Ok(())
}
