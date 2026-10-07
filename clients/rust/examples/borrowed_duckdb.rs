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
    SqlDialect, SqlSession,
    compiler::{CompiledSql, compile},
    execute,
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
    // The application configures its connection, extensions, caches and UDFs.
    let connection = Connection::open_in_memory()?;
    connection.execute_batch(
        "CREATE TABLE people(id BIGINT, name VARCHAR);
         INSERT INTO people VALUES (1, 'Ada');
         CREATE MACRO decorate_name(s) AS upper(s);
         BEGIN;
         INSERT INTO people VALUES (2, 'Grace');",
    )?;
    let request = serde_json::from_value(serde_json::json!({
        "version": 1, "dialect": "duckdb", "language": "cypher",
        "query": "MATCH (p:Person) RETURN decorate(p.name) AS name ORDER BY name",
        "tables": [{"name": "people", "columns": [
            {"name": "id", "data_type": "int64", "nullable": false},
            {"name": "name", "data_type": "string"}
        ]}],
        "nodes": [{"label": "Person", "table": "people", "id": "id",
                   "properties": {"name": "name"}}],
        "functions": [{"name": "decorate", "target": "decorate_name",
                       "parameters": ["string"], "returns": "string"}]
    }))?;
    let compiled = compile(request).await.map_err(std::io::Error::other)?;
    let mut session = BorrowedDuckDb {
        connection: &connection,
        statement: None,
    };
    {
        let mut rows = execute(&mut session, &compiled).await?;
        let mut names = Vec::new();
        for batch in &mut rows {
            let batch = batch?;
            let column = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            for name in column.iter().flatten() {
                println!("{name}");
                names.push(name.to_owned());
            }
        }
        assert_eq!(names, ["ADA", "GRACE"]);
    } // Cursor releases its borrow. No result buffering is imposed by OrchidDB.
    drop(session);
    // The caller still owns the open transaction; OrchidDB did not commit it.
    connection.execute_batch("ROLLBACK")?;
    let count: i64 = connection.query_row("SELECT count(*) FROM people", [], |r| r.get(0))?;
    assert_eq!(count, 1);
    Ok(())
}
