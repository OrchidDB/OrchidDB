//! Benchmark-only client for the fixture's close-delimited HTTP Arrow streams.
use super::*;
use arrow_ipc::reader::StreamReader;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
};

type Reader = StreamReader<BufReader<TcpStream>>;
type Batch = std::result::Result<RecordBatch, ArrowError>;
fn open(
    endpoint: &str,
    rows: usize,
    batch: usize,
    parts: usize,
    part: usize,
    latency: u64,
) -> std::result::Result<Reader, ArrowError> {
    let address = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| ArrowError::ParseError("fixture client requires http://host:port".into()))?;
    let mut socket = TcpStream::connect(address)?;
    socket.set_read_timeout(Some(Duration::from_secs(30)))?;
    socket.set_write_timeout(Some(Duration::from_secs(30)))?;
    socket.set_nodelay(true)?;
    write!(
        socket,
        "GET /bench/{rows}/{batch}/{parts}/{part}/{latency} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\n\r\n"
    )?;
    let mut input = BufReader::new(socket);
    let mut status = String::new();
    input.read_line(&mut status)?;
    if !status.starts_with("HTTP/1.1 200 ") {
        return Err(ArrowError::ParseError(status));
    }
    loop {
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Err(ArrowError::ParseError("truncated HTTP headers".into()));
        }
        if line == "\r\n" {
            break;
        }
    }
    StreamReader::try_new(input, None)
}

pub struct HttpBatches {
    endpoint: String,
    rows: usize,
    batch: usize,
    parts: usize,
    latency: u64,
    schema: SchemaRef,
    delivered: Arc<AtomicUsize>,
    reader: Option<Reader>,
    receiver: Option<Receiver<Batch>>,
    handles: Vec<JoinHandle<()>>,
}
impl HttpBatches {
    pub fn new(
        endpoint: String,
        rows: usize,
        batch: usize,
        parts: usize,
        latency: u64,
        delivered: Arc<AtomicUsize>,
        schema: SchemaRef,
    ) -> Self {
        Self {
            endpoint,
            rows,
            batch,
            parts,
            latency,
            schema,
            delivered,
            reader: None,
            receiver: None,
            handles: vec![],
        }
    }
}
impl Iterator for HttpBatches {
    type Item = Batch;
    fn next(&mut self) -> Option<Batch> {
        let result = if self.parts == 1 {
            if self.reader.is_none() {
                match open(&self.endpoint, self.rows, self.batch, 1, 0, self.latency) {
                    Ok(reader) => self.reader = Some(reader),
                    Err(error) => return Some(Err(error)),
                }
            }
            self.reader.as_mut().unwrap().next()?
        } else {
            if self.receiver.is_none() {
                let (tx, rx) = sync_channel(self.parts * 2);
                self.receiver = Some(rx);
                for part in 0..self.parts {
                    let tx = tx.clone();
                    let endpoint = self.endpoint.clone();
                    let (rows, batch, parts, latency) =
                        (self.rows, self.batch, self.parts, self.latency);
                    self.handles.push(thread::spawn(move || {
                        match open(&endpoint, rows, batch, parts, part, latency) {
                            Ok(reader) => {
                                for batch in reader {
                                    let failed = batch.is_err();
                                    if tx.send(batch).is_err() || failed {
                                        break;
                                    }
                                }
                            }
                            Err(error) => {
                                let _ = tx.send(Err(error));
                            }
                        }
                    }));
                }
            }
            self.receiver.as_ref().unwrap().recv().ok()?
        };
        if let Ok(batch) = &result {
            self.delivered
                .fetch_add(batch.num_rows(), Ordering::Relaxed);
        }
        Some(result)
    }
}
impl RecordBatchReader for HttpBatches {
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
}
impl Drop for HttpBatches {
    fn drop(&mut self) {
        self.receiver.take();
        for handle in self.handles.drain(..) {
            handle.join().expect("HTTP producer panicked");
        }
    }
}
