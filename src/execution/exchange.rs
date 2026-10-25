//! Query-scoped, bounded Arrow exchanges consumed by executable engines.
//!
//! A registry contains stream factories, never collected result batches. Each
//! partition states its replay semantics through `BatchSource`. Leases revoke
//! access and cancel active readers on drop. The optional HTTP server is a
//! transport for Trino workers; it does not execute relational operations.
use super::stream::BatchSource;
use crate::ir::rel::sql::DialectAdapter;
use arrow::{datatypes::SchemaRef, ipc::writer::StreamWriter};
use futures::StreamExt;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, BufWriter, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Default)]
pub struct ExchangeRegistry {
    entries: Arc<Mutex<BTreeMap<String, Arc<Entry>>>>,
}
struct Entry {
    schema: SchemaRef,
    partitions: Vec<BatchSource>,
    estimated_rows: Option<u64>,
    cancelled: AtomicBool,
}
/// Holds execution identity and input streams until their consuming query ends.
pub struct ExchangeLease {
    id: String,
    registry: ExchangeRegistry,
}
impl ExchangeLease {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl Drop for ExchangeLease {
    fn drop(&mut self) {
        self.registry.revoke(&self.id);
    }
}
#[derive(Debug, Serialize)]
pub struct ExchangeMetadata {
    pub id: String,
    pub partitions: usize,
    pub estimated_rows: Option<u64>,
    pub replayable: bool,
    pub columns: Vec<ExchangeColumn>,
}
#[derive(Debug, Serialize)]
pub struct ExchangeColumn {
    pub name: String,
    pub sql_type: String,
    pub nullable: bool,
}
impl ExchangeRegistry {
    pub fn register(
        &self,
        partitions: Vec<BatchSource>,
        estimated_rows: Option<u64>,
    ) -> Result<ExchangeLease, String> {
        let schema = partitions
            .first()
            .ok_or("exchange requires at least one partition")?
            .schema();
        if partitions.iter().any(|p| p.schema() != schema) {
            return Err("exchange partition schemas differ".into());
        }
        let mut names = std::collections::BTreeSet::new();
        for field in schema.fields() {
            if field.name().is_empty() || !names.insert(field.name().to_lowercase()) {
                return Err(
                    "Trino exchange requires nonempty, case-insensitively unique column names"
                        .into(),
                );
            }
            crate::ir::rel::sql::trino::TRINO
                .sql_type(field.data_type())
                .map_err(|e| e.to_string())?;
        }
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| "exchange registry lock poisoned")?;
        let id = loop {
            let id = format!("e{:032x}", rand::random::<u128>());
            if !entries.contains_key(&id) {
                break id;
            }
        };
        entries.insert(
            id.clone(),
            Arc::new(Entry {
                schema,
                partitions,
                estimated_rows,
                cancelled: AtomicBool::new(false),
            }),
        );
        Ok(ExchangeLease {
            id,
            registry: self.clone(),
        })
    }
    fn entry(&self, id: &str) -> Result<Arc<Entry>, String> {
        self.entries
            .lock()
            .map_err(|_| "exchange registry lock poisoned")?
            .get(id)
            .cloned()
            .ok_or_else(|| "exchange expired or unknown".into())
    }
    pub fn metadata(&self, id: &str) -> Result<ExchangeMetadata, String> {
        let entry = self.entry(id)?;
        let columns = entry
            .schema
            .fields()
            .iter()
            .map(|field| {
                Ok(ExchangeColumn {
                    name: field.name().clone(),
                    sql_type: crate::ir::rel::sql::trino::TRINO
                        .sql_type(field.data_type())
                        .map_err(|e| e.to_string())?,
                    nullable: field.is_nullable(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(ExchangeMetadata {
            id: id.into(),
            partitions: entry.partitions.len(),
            estimated_rows: entry.estimated_rows,
            replayable: entry.partitions.iter().all(BatchSource::is_replayable),
            columns,
        })
    }
    pub fn revoke(&self, id: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.remove(id) {
                entry.cancelled.store(true, Ordering::Release);
            }
        }
    }
}

pub struct ExchangeServerOptions {
    pub bind: SocketAddr,
    /// Shared with the Trino connector, never included in generated SQL or URLs.
    pub bearer_token: String,
    pub max_connections: usize,
    pub max_batch_bytes: usize,
    pub io_timeout: Duration,
}
impl ExchangeServerOptions {
    pub fn local(bearer_token: String) -> Self {
        Self {
            bind: "127.0.0.1:0".parse().unwrap(),
            bearer_token,
            max_connections: 16,
            max_batch_bytes: 64 * 1024 * 1024,
            io_timeout: Duration::from_secs(30),
        }
    }
}
/// Dropping the server closes admission and cancels active transport readers.
pub struct ExchangeServer {
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
}
impl ExchangeServer {
    pub fn bind(
        registry: ExchangeRegistry,
        options: ExchangeServerOptions,
    ) -> Result<Self, String> {
        if options.bearer_token.len() < 32
            || options.bearer_token.bytes().any(|b| b <= 32 || b >= 127)
            || options.max_connections == 0
            || options.max_batch_bytes == 0
            || options.io_timeout.is_zero()
        {
            return Err("invalid exchange server limits or bearer token (at least 32 printable, non-space characters required)".into());
        }
        let listener = TcpListener::bind(options.bind).map_err(|e| e.to_string())?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let address = listener.local_addr().map_err(|e| e.to_string())?;
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = stopped.clone();
        let options = Arc::new(options);
        let active = Arc::new(AtomicUsize::new(0));
        let handle = thread::spawn(move || {
            while !stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        if active.load(Ordering::Acquire) >= options.max_connections {
                            let _ = respond(&mut socket, 503, b"exchange concurrency limit");
                            continue;
                        }
                        active.fetch_add(1, Ordering::AcqRel);
                        let (active, stop, registry, options) = (
                            active.clone(),
                            stop.clone(),
                            registry.clone(),
                            options.clone(),
                        );
                        thread::spawn(move || {
                            struct Active(Arc<AtomicUsize>);
                            impl Drop for Active {
                                fn drop(&mut self) {
                                    self.0.fetch_sub(1, Ordering::AcqRel);
                                }
                            }
                            let _active = Active(active);
                            let _ = serve(socket, registry, options, stop);
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            address,
            stopped,
            listener: Some(handle),
        })
    }
    pub fn address(&self) -> SocketAddr {
        self.address
    }
}
impl Drop for ExchangeServer {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}
fn respond(socket: &mut TcpStream, status: u16, body: &[u8]) -> std::io::Result<()> {
    write!(
        socket,
        "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    socket.write_all(body)
}
fn serve(
    mut socket: TcpStream,
    registry: ExchangeRegistry,
    options: Arc<ExchangeServerOptions>,
    stopped: Arc<AtomicBool>,
) -> Result<(), String> {
    // BSD sockets inherit the nonblocking listener flag after accept.
    socket.set_nonblocking(false).map_err(|e| e.to_string())?;
    socket
        .set_read_timeout(Some(options.io_timeout))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(options.io_timeout))
        .map_err(|e| e.to_string())?;
    socket.set_nodelay(true).map_err(|e| e.to_string())?;
    let mut input = BufReader::new(socket.try_clone().map_err(|e| e.to_string())?);
    let mut request = String::new();
    let mut authorization = String::new();
    let mut remaining = 16_384usize;
    loop {
        // Read through a bounded buffer so an untrusted header cannot allocate indefinitely.
        let available = input.fill_buf().map_err(|e| e.to_string())?;
        if available.is_empty() {
            return Err("truncated HTTP request".into());
        }
        let count = available
            .iter()
            .position(|b| *b == b'\n')
            .map_or(available.len(), |n| n + 1);
        if count > remaining {
            let _ = respond(&mut socket, 431, b"headers too large");
            return Ok(());
        }
        remaining -= count;
        request.push_str(
            std::str::from_utf8(&available[..count]).map_err(|_| "invalid HTTP headers")?,
        );
        input.consume(count);
        if request.ends_with("\r\n\r\n") {
            break;
        }
    }
    let mut lines = request.lines();
    let first = lines.next().unwrap_or("");
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("authorization") {
                authorization = value.trim().to_string();
            }
        }
    }
    let expected = format!("Bearer {}", options.bearer_token);
    let matched = authorization.len() == expected.len()
        && authorization
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |v, (a, b)| v | (a ^ b))
            == 0;
    if !matched {
        respond(&mut socket, 401, b"unauthorized").map_err(|e| e.to_string())?;
        return Ok(());
    }
    let parts = first.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 3 || parts[0] != "GET" {
        respond(&mut socket, 405, b"GET required").map_err(|e| e.to_string())?;
        return Ok(());
    }
    let path = parts[1]
        .split('/')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>();
    if path.len() < 3 || path[0] != "v1" || path[1] != "exchanges" {
        respond(&mut socket, 404, b"unknown path").map_err(|e| e.to_string())?;
        return Ok(());
    }
    let id = path[2];
    if path.len() == 3 {
        match registry.metadata(id) {
            Ok(metadata) => respond(
                &mut socket,
                200,
                &serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
            ),
            Err(_) => respond(&mut socket, 404, b"exchange expired or unknown"),
        }
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    if path.len() != 5 || path[3] != "partitions" {
        respond(&mut socket, 404, b"unknown path").map_err(|e| e.to_string())?;
        return Ok(());
    }
    let Some((entry, partition)) = registry.entry(id).ok().and_then(|entry| {
        path[4]
            .parse::<usize>()
            .ok()
            .filter(|&p| p < entry.partitions.len())
            .map(|p| (entry, p))
    }) else {
        respond(&mut socket, 404, b"unknown partition").map_err(|e| e.to_string())?;
        return Ok(());
    };
    let mut stream = match entry.partitions[partition].open() {
        Ok(stream) => stream,
        Err(_) => {
            respond(
                &mut socket,
                409,
                b"partition consumed; replay not available",
            )
            .map_err(|e| e.to_string())?;
            return Ok(());
        }
    };
    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/vnd.apache.arrow.stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").map_err(|e| e.to_string())?;
    let mut writer = StreamWriter::try_new(
        BufWriter::with_capacity(64 * 1024, Chunked(socket)),
        &entry.schema,
    )
    .map_err(|e| e.to_string())?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        loop {
            let batch = tokio::select! {
                batch = stream.next() => batch,
                _ = async { while !entry.cancelled.load(Ordering::Acquire) && !stopped.load(Ordering::Acquire) { tokio::time::sleep(Duration::from_millis(10)).await; } } => return Err("exchange cancelled".to_string()),
            };
            let Some(batch) = batch else { break; };
            let batch = batch?;
            if batch.schema() != entry.schema { return Err("exchange schema changed".into()); }
            if batch.get_array_memory_size() > options.max_batch_bytes { return Err("exchange batch exceeds byte limit".into()); }
            writer.write(&batch).map_err(|e| e.to_string())?;
            writer.get_mut().flush().map_err(|e| e.to_string())?;
        }
        writer.finish().map_err(|e| e.to_string())?;
        writer.get_mut().flush().map_err(|e| e.to_string())?;
        writer.get_mut().get_mut().0.write_all(b"0\r\n\r\n").map_err(|e| e.to_string())
    })
    // On error deliberately omit the terminating HTTP chunk: consumers must fail,
    // not mistake a partially delivered result for successful Arrow EOF.
}
struct Chunked(TcpStream);
impl Write for Chunked {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        write!(self.0, "{:x}\r\n", bytes.len())?;
        self.0.write_all(bytes)?;
        self.0.write_all(b"\r\n")?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}
