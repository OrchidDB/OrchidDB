use arrow_array::{
    Float64Array, Int64Array, RecordBatch, RecordBatchReader, ffi_stream::FFI_ArrowArrayStream,
};
use arrow_schema::{ArrowError, DataType, Field, Schema, SchemaRef};
use libduckdb_sys as db;
use std::{
    ffi::{CStr, CString},
    ptr,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
mod benchmark;

/// A pull-based remote edge. One next() call performs one batched search.
/// No remote results are collected before handing this reader to DuckDB.
struct RemoteMatches {
    schema: SchemaRef,
    next_source: i64,
    calls: Arc<AtomicUsize>,
    fail_on_call: Option<usize>,
}

impl RemoteMatches {
    fn new(calls: Arc<AtomicUsize>, fail_on_call: Option<usize>) -> Self {
        Self {
            schema: Arc::new(Schema::new(vec![
                Field::new("source_id", DataType::Int64, false),
                Field::new("document_id", DataType::Int64, false),
                Field::new("score", DataType::Float64, false),
            ])),
            next_source: 1,
            calls,
            fail_on_call,
        }
    }
}

impl Iterator for RemoteMatches {
    type Item = std::result::Result<RecordBatch, ArrowError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next_source > 6 {
            return None;
        }
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fail_on_call == Some(call) {
            self.next_source = 7;
            return Some(Err(ArrowError::ComputeError(
                "simulated remote search failure".into(),
            )));
        }

        let a = self.next_source;
        let b = a + 1;
        self.next_source += 2;
        eprintln!("remote request {call}: search sources [{a}, {b}] (DuckDB requested a batch)");
        std::thread::sleep(Duration::from_millis(50));

        // Replace this fixture with a Weaviate request for these two source vectors.
        // Decode its response into these columns, preserving each source's ID.
        // Each source has two hits; SQL will reject the second, lower-scoring hit.
        Some(RecordBatch::try_new(
            self.schema.clone(),
            vec![
                Arc::new(Int64Array::from(vec![a, a, b, b])),
                Arc::new(Int64Array::from(vec![10, 20, 10, 20])),
                Arc::new(Float64Array::from(vec![0.95, 0.60, 0.90, 0.70])),
            ],
        ))
    }
}

impl RecordBatchReader for RemoteMatches {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
}

// Small RAII wrappers keep the unsafe C API plumbing separate from the reader.
struct Database {
    database: db::duckdb_database,
    connection: db::duckdb_connection,
}
impl Database {
    fn open() -> Result<Self> {
        let mut owner = Self {
            database: ptr::null_mut(),
            connection: ptr::null_mut(),
        };
        // SAFETY: valid output pointers; null path requests an in-memory database.
        unsafe {
            if db::duckdb_open(ptr::null(), &mut owner.database) != db::DuckDBSuccess {
                return Err("opening DuckDB failed".into());
            }
            if db::duckdb_connect(owner.database, &mut owner.connection) != db::DuckDBSuccess {
                return Err("connecting to DuckDB failed".into());
            }
        }
        Ok(owner)
    }

    fn query(&self, sql: &str) -> Result<QueryResult> {
        let sql = CString::new(sql)?;
        // SAFETY: DuckDB initializes the zeroed result, which we destroy even on error.
        let mut result = QueryResult(unsafe { std::mem::zeroed() });
        unsafe {
            if db::duckdb_query(self.connection, sql.as_ptr(), &mut result.0) != db::DuckDBSuccess {
                let error = db::duckdb_result_error(&mut result.0);
                let message = if error.is_null() {
                    "DuckDB query failed".into()
                } else {
                    CStr::from_ptr(error).to_string_lossy().into_owned()
                };
                return Err(message.into());
            }
        }
        Ok(result)
    }
}
impl Drop for Database {
    fn drop(&mut self) {
        // SAFETY: these handles belong to this owner and are destroyed exactly once.
        unsafe {
            if !self.connection.is_null() {
                db::duckdb_disconnect(&mut self.connection);
            }
            if !self.database.is_null() {
                db::duckdb_close(&mut self.database);
            }
        }
    }
}
struct QueryResult(db::duckdb_result);
impl Drop for QueryResult {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns this initialized result.
        unsafe {
            db::duckdb_destroy_result(&mut self.0);
        }
    }
}

#[derive(Debug, PartialEq)]
struct Match {
    source_id: i64,
    title: String,
    score: f64,
}

fn run(fail_on_call: Option<usize>) -> Result<Vec<Match>> {
    let calls = Arc::new(AtomicUsize::new(0));
    let reader = RemoteMatches::new(calls.clone(), fail_on_call);

    // Box pins the ABI struct's address. Declare it before the connection so it
    // outlives the view, queries, and connection, including on error unwinding.
    let mut stream = Box::new(FFI_ArrowArrayStream::new(Box::new(reader)));
    let database = Database::open()?;
    database.query("SET threads = 1")?;
    // The legacy C Arrow bridge advertises filter pushdown, but its factory
    // ignores filter parameters. Keep predicates in DuckDB for this demo.
    database.query("SET disabled_optimizers = 'filter_pushdown'")?;
    database.query(
        "CREATE TABLE documents(id BIGINT, title VARCHAR);
                    INSERT INTO documents VALUES (10, 'Arrow streaming'), (20, 'Other document')",
    )?;
    let name = CString::new("remote_matches")?;
    // SAFETY: Arrow's FFI stream implements the Arrow C Stream ABI expected by
    // DuckDB. Its address and producer remain valid until the connection drops.
    // The registered view is consumed exactly once, synchronously below.
    let status = unsafe {
        db::duckdb_arrow_scan(
            database.connection,
            name.as_ptr(),
            (&mut *stream as *mut FFI_ArrowArrayStream).cast(),
        )
    };
    if status != db::DuckDBSuccess {
        return Err("registering Arrow stream failed".into());
    }
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "registration must not fetch remote data"
    );
    eprintln!("Registered stream: 0 remote requests. Executing SQL now.");

    // remote_matches is a VIEW over the live stream, not a populated temp table.
    // DuckDB owns all joins/filtering/sorting. ORDER BY is intentionally blocking;
    // duckdb_query materializes the final result, not the input before execution.
    let mut result = database.query(
        "SELECT m.source_id, d.title, m.score
         FROM remote_matches m JOIN documents d ON d.id = m.document_id
         WHERE m.score >= 0.8
         ORDER BY m.source_id",
    )?;
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "six inputs should require three remote batches"
    );

    let mut matches = Vec::new();
    // SAFETY: the query fixes these column types and the row range is in bounds.
    unsafe {
        for row in 0..db::duckdb_row_count(&mut result.0) {
            let title = db::duckdb_value_varchar(&mut result.0, 1, row);
            if title.is_null() {
                return Err("unexpected NULL title".into());
            }
            let text = CStr::from_ptr(title).to_string_lossy().into_owned();
            db::duckdb_free(title.cast());
            matches.push(Match {
                source_id: db::duckdb_value_int64(&mut result.0, 0, row),
                title: text,
                score: db::duckdb_value_double(&mut result.0, 2, row),
            });
        }
    }
    Ok(matches)
}

fn main() -> Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--bench") {
        return benchmark::run();
    }
    // SAFETY: DuckDB returns a static, NUL-terminated version string.
    let version = unsafe { CStr::from_ptr(db::duckdb_library_version()) };
    eprintln!("DuckDB {}", version.to_string_lossy());
    for hit in run(None)? {
        println!("{} | {} | {:.2}", hit.source_id, hit.title, hit.score);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duckdb_pulls_batches_and_executes_the_join_and_filter() {
        let rows = run(None).unwrap();
        let expected: Vec<_> = (1..=6)
            .map(|id| Match {
                source_id: id,
                title: "Arrow streaming".into(),
                score: if id % 2 == 1 { 0.95 } else { 0.90 },
            })
            .collect();
        assert_eq!(rows, expected);
    }

    #[test]
    fn remote_stream_errors_fail_the_sql_query() {
        let error = run(Some(2)).unwrap_err().to_string();
        assert!(error.contains("simulated remote search failure"), "{error}");
    }
}
