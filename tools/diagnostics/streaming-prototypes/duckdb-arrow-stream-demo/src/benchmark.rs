//! Synthetic benchmark: real Arrow ingestion and SQL, simulated remote service.
use super::*;
mod http;
use std::{
    sync::mpsc::{Receiver, sync_channel},
    thread::{self, JoinHandle},
    time::Instant,
};

struct Batches {
    schema: SchemaRef,
    rows: usize,
    batch: usize,
    latency: Duration,
    workers: usize,
    offset: usize,
    receiver: Option<Receiver<RecordBatch>>,
    handles: Vec<JoinHandle<()>>,
    started: bool,
    delivered: Arc<AtomicUsize>,
}
fn make_batch(schema: SchemaRef, start: usize, end: usize) -> RecordBatch {
    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(Int64Array::from_iter_values(
                (start..end).map(|i| (i / 20) as i64),
            )),
            Arc::new(Int64Array::from_iter_values(
                (start..end).map(|i| (i % 100_000) as i64),
            )),
            Arc::new(Float64Array::from_iter_values(
                (start..end).map(|i| if i % 20 < 10 { 0.9 } else { 0.6 }),
            )),
        ],
    )
    .unwrap()
}
impl Iterator for Batches {
    type Item = std::result::Result<RecordBatch, ArrowError>;
    fn next(&mut self) -> Option<Self::Item> {
        let batch = if self.workers == 0 {
            if self.offset >= self.rows {
                return None;
            }
            thread::sleep(self.latency);
            let end = (self.offset + self.batch).min(self.rows);
            let batch = make_batch(self.schema.clone(), self.offset, end);
            self.offset = end;
            batch
        } else {
            if !self.started {
                self.started = true;
                // Queue and worker-local buffers are bounded independently of total rows.
                let (sender, receiver) = sync_channel(self.workers * 2);
                self.receiver = Some(receiver);
                let next = Arc::new(AtomicUsize::new(0));
                for _ in 0..self.workers {
                    let sender = sender.clone();
                    let next = next.clone();
                    let schema = self.schema.clone();
                    let (rows, size, latency) = (self.rows, self.batch, self.latency);
                    self.handles.push(thread::spawn(move || {
                        loop {
                            let start = next.fetch_add(size, Ordering::Relaxed);
                            if start >= rows {
                                break;
                            }
                            thread::sleep(latency);
                            let batch = make_batch(schema.clone(), start, (start + size).min(rows));
                            if sender.send(batch).is_err() {
                                break;
                            }
                        }
                    }));
                }
            }
            self.receiver.as_ref().unwrap().recv().ok()?
        };
        self.delivered
            .fetch_add(batch.num_rows(), Ordering::Relaxed);
        Some(Ok(batch))
    }
}
impl RecordBatchReader for Batches {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
}
impl Drop for Batches {
    fn drop(&mut self) {
        self.receiver.take(); // Disconnect first: wake producers blocked on send.
        for handle in self.handles.drain(..) {
            handle.join().expect("producer panicked");
        }
    }
}

pub fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 8 {
        return Err(
            "usage: --bench ROWS BATCH LATENCY_MS WORKERS THREADS scan|join|fixed|materialize"
                .into(),
        );
    }
    let rows: usize = args[2].parse()?;
    let batch: usize = args[3].parse()?;
    let latency: u64 = args[4].parse()?;
    let workers: usize = args[5].parse()?;
    let threads: usize = args[6].parse()?;
    let mode = args[7].as_str();
    if rows == 0
        || batch == 0
        || threads == 0
        || !["scan", "join", "fixed", "materialize"].contains(&mode)
    {
        return Err("invalid benchmark configuration".into());
    }
    let delivered = Arc::new(AtomicUsize::new(0));
    let reader = Batches {
        schema: RemoteMatches::new(Arc::new(AtomicUsize::new(0)), None).schema,
        rows,
        batch,
        latency: Duration::from_millis(latency),
        workers,
        offset: 0,
        receiver: None,
        handles: Vec::new(),
        started: false,
        delivered: delivered.clone(),
    };
    let reader: Box<dyn RecordBatchReader + Send> =
        if let Ok(endpoint) = std::env::var("ARROW_BENCH_HTTP") {
            Box::new(http::HttpBatches::new(
                endpoint,
                rows,
                batch,
                workers.max(1),
                latency,
                delivered.clone(),
                reader.schema.clone(),
            ))
        } else {
            Box::new(reader)
        };
    let mut stream = Box::new(FFI_ArrowArrayStream::new(reader));
    let database = Database::open()?;
    database.query(&format!(
        "SET threads={threads}; SET disabled_optimizers='filter_pushdown'; SET memory_limit='1GB'"
    ))?;
    database.query("CREATE TABLE documents AS SELECT i AS id FROM range(100000) t(i)")?;
    let name = CString::new("remote_matches")?;
    // SAFETY: same stable ABI pointer/lifetime contract as the main example.
    let status = unsafe {
        db::duckdb_arrow_scan(
            database.connection,
            name.as_ptr(),
            (&mut *stream as *mut FFI_ArrowArrayStream).cast(),
        )
    };
    if status != db::DuckDBSuccess {
        return Err("register failed".into());
    }
    assert_eq!(delivered.load(Ordering::Relaxed), 0);
    if mode == "fixed" {
        database
            .query("SET disabled_optimizers='filter_pushdown,join_order,build_side_probe_side'")?;
    }
    let start = Instant::now();
    let mut ingest_seconds = 0.0;
    let relation = if mode == "materialize" {
        database.query("CREATE TEMP TABLE staged AS SELECT * FROM remote_matches")?;
        ingest_seconds = start.elapsed().as_secs_f64();
        "staged"
    } else {
        "remote_matches"
    };
    let join = if mode == "scan" {
        ""
    } else {
        "JOIN documents d ON d.id=m.document_id"
    };
    let sql = format!(
        "SELECT count(*), sum(m.source_id)::BIGINT, sum(m.document_id)::BIGINT FROM {relation} m {join} WHERE m.score>=0.8"
    );
    if std::env::var_os("BENCH_EXPLAIN").is_some() {
        let mut plan = database.query(&format!("EXPLAIN {sql}"))?;
        unsafe {
            let value = db::duckdb_value_varchar(&mut plan.0, 1, 0);
            eprintln!("{}", CStr::from_ptr(value).to_string_lossy());
            db::duckdb_free(value.cast());
        }
    }
    let mut result = database.query(&sql)?;
    let seconds = start.elapsed().as_secs_f64();
    // Validate the complete result; no LIMIT or count-only shortcut can bypass the scan.
    let full = rows / 20;
    let tail = (rows % 20).min(10);
    let expected_count = full * 10 + tail;
    let expected_sum =
        10u128 * full as u128 * full.saturating_sub(1) as u128 / 2 + tail as u128 * full as u128;
    unsafe {
        assert_eq!(
            db::duckdb_value_int64(&mut result.0, 0, 0),
            expected_count as i64
        );
        assert_eq!(
            db::duckdb_value_int64(&mut result.0, 1, 0),
            expected_sum as i64
        );
    }
    let cycle_sum: usize = (0..100_000).filter(|i| i % 20 < 10).sum();
    let remainder_sum: usize = (0..rows % 100_000).filter(|i| i % 20 < 10).sum();
    unsafe {
        assert_eq!(
            db::duckdb_value_int64(&mut result.0, 2, 0),
            ((rows / 100_000) * cycle_sum + remainder_sum) as i64
        );
    }
    assert_eq!(delivered.load(Ordering::Relaxed), rows);
    println!(
        "{{\"rows\":{rows},\"batch\":{batch},\"latency_ms\":{latency},\"workers\":{workers},\"threads\":{threads},\"mode\":\"{mode}\",\"seconds\":{seconds:.6},\"ingest_seconds\":{ingest_seconds:.6},\"rows_per_second\":{:.0},\"validated\":true}}",
        rows as f64 / seconds
    );
    Ok(())
}
