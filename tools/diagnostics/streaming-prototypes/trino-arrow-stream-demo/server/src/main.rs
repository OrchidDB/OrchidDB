use arrow_array::{Float64Array, Int64Array, RecordBatch};
use arrow_ipc::writer::StreamWriter;
use arrow_schema::{DataType, Field, Schema};
use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    net::{TcpListener, TcpStream},
    sync::Arc,
    time::Duration,
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
fn serve(mut socket: TcpStream) -> Result<()> {
    socket.set_read_timeout(Some(Duration::from_secs(10)))?;
    socket.set_write_timeout(Some(Duration::from_secs(10)))?;
    let mut request = String::new();
    let mut input = BufReader::new(socket.try_clone()?);
    input.read_line(&mut request)?;
    // Consume request headers before closing the response socket (avoid TCP reset).
    let mut header_bytes = request.len();
    loop {
        let mut header = String::new();
        let length = input.read_line(&mut header)?;
        header_bytes += length;
        if header_bytes > 16384 {
            return Err("request headers too large".into());
        }
        if length == 0 || header == "\r\n" || header == "\n" {
            break;
        }
    }
    let path = request.split_whitespace().nth(1).unwrap_or("");
    if path == "/health" {
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")?;
        return Ok(());
    }
    if path.starts_with("/bench/") {
        return benchmark_stream(socket, path);
    }
    let partition = match path {
        "/matches/0" => 0,
        "/matches/1" => 1,
        _ => {
            socket.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")?;
            return Ok(());
        }
    };
    // Close-delimited HTTP body: each batch is flushed as it is produced.
    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/vnd.apache.arrow.stream\r\nConnection: close\r\n\r\n")?;
    let schema = Arc::new(Schema::new(vec![
        Field::new("source_id", DataType::Int64, false),
        Field::new("document_id", DataType::Int64, false),
        Field::new("score", DataType::Float64, false),
    ]));
    let mut writer = StreamWriter::try_new(&mut socket, &schema)?;
    let sources: Vec<i64> = (1..=6).filter(|id| (id - 1) % 2 == partition).collect();
    for (batch_index, batch_sources) in sources.chunks(2).enumerate() {
        // Replace this deterministic fixture with one batched search request.
        std::thread::sleep(Duration::from_millis(50));
        let mut ids = Vec::new();
        let mut documents = Vec::new();
        let mut scores = Vec::new();
        for &id in batch_sources {
            ids.extend([id, id]);
            documents.extend([10, 20]);
            scores.extend([if id % 2 == 1 { 0.95 } else { 0.90 }, 0.60]);
        }
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from(ids)),
                Arc::new(Int64Array::from(documents)),
                Arc::new(Float64Array::from(scores)),
            ],
        )?;
        writer.write(&batch)?;
        writer.get_mut().flush()?;
        eprintln!(
            "partition={partition} batch={batch_index} rows={}",
            batch.num_rows()
        );
    }
    writer.finish()?;
    Ok(())
}
// Same values as the DuckDB benchmark, partitioned into disjoint ranges.
fn benchmark_stream(mut socket: TcpStream, path: &str) -> Result<()> {
    let args = path
        .trim_start_matches("/bench/")
        .split('/')
        .map(str::parse::<usize>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if args.len() != 5 {
        return Err("expected rows/batch/partitions/partition/latency_ms".into());
    }
    let (rows, batch_size, parts, part, latency) = (args[0], args[1], args[2], args[3], args[4]);
    if rows == 0
        || rows > 1_000_000_000
        || batch_size == 0
        || batch_size > 65536
        || parts == 0
        || parts > 16
        || part >= parts
        || latency > 1000
    {
        return Err("invalid benchmark range".into());
    }
    socket.set_nodelay(true)?;
    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/vnd.apache.arrow.stream\r\nConnection: close\r\n\r\n")?;
    let schema = Arc::new(Schema::new(vec![
        Field::new("source_id", DataType::Int64, false),
        Field::new("document_id", DataType::Int64, false),
        Field::new("score", DataType::Float64, false),
    ]));
    let mut writer = StreamWriter::try_new(BufWriter::with_capacity(256 * 1024, socket), &schema)?;
    let start = rows * part / parts;
    let end = rows * (part + 1) / parts;
    for offset in (start..end).step_by(batch_size) {
        let limit = (offset + batch_size).min(end);
        if latency != 0 {
            std::thread::sleep(Duration::from_millis(latency as u64));
        }
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(Int64Array::from_iter_values(
                    (offset..limit).map(|i| (i / 20) as i64),
                )),
                Arc::new(Int64Array::from_iter_values(
                    (offset..limit).map(|i| (i % 100000) as i64),
                )),
                Arc::new(Float64Array::from_iter_values(
                    (offset..limit).map(|i| if i % 20 < 10 { 0.9 } else { 0.6 }),
                )),
            ],
        )?;
        writer.write(&batch)?;
        writer.get_mut().flush()?;
    }
    writer.finish()?;
    writer.get_mut().flush()?;
    eprintln!(
        "bench rows={rows} batch={batch_size} partition={part}/{parts} delivered={}",
        end - start
    );
    Ok(())
}
fn main() -> Result<()> {
    let address = std::env::var("ARROW_LISTEN").unwrap_or("0.0.0.0:18881".into());
    let listener = TcpListener::bind(&address)?;
    eprintln!("Arrow fixture listening on {address}");
    for socket in listener.incoming() {
        let socket = socket?;
        std::thread::spawn(move || {
            if let Err(error) = serve(socket) {
                eprintln!("stream closed/failed: {error}");
            }
        });
    }
    Ok(())
}
