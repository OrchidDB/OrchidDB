use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use arrow::{datatypes::SchemaRef, record_batch::RecordBatch};
use orchiddb::ir::rel::sql::{SqlDialect, SqlError, SqlResult, region::RegionSession};

#[derive(Debug)]
struct Worker {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}

impl Worker {
    pub fn new() -> Result<Self, String> {
        let python = std::env::var("CONFORMANCE_PYTHON").unwrap_or_else(|_| "python3".into());
        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../upstream/starrocks_session.py");
        let mut child = Command::new(python).arg(script).stdin(Stdio::piped()).stdout(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut session = Self { child, input, output };
        let mut ready = String::new();
        session.output.read_line(&mut ready).map_err(|e| e.to_string())?;
        if ready.trim() != "ready" { return Err(format!("StarRocks session failed: {ready}")); }
        Ok(session)
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug)]
pub struct Session(Arc<Mutex<Worker>>);

impl Session {
    pub fn new() -> Result<Self, String> {
        static SHARED: OnceLock<Mutex<Weak<Mutex<Worker>>>> = OnceLock::new();
        let mut shared = SHARED.get_or_init(|| Mutex::new(Weak::new())).lock().map_err(|e| e.to_string())?;
        let worker = match shared.upgrade() {
            Some(worker) => worker,
            None => {
                let worker = Arc::new(Mutex::new(Worker::new()?));
                *shared = Arc::downgrade(&worker);
                worker
            }
        };
        Ok(Self(worker))
    }
}

impl RegionSession for Session {
    fn dialect(&self) -> SqlDialect { SqlDialect::resolve("starrocks").unwrap() }
    fn query(&mut self, sql: &str, schema: SchemaRef) -> SqlResult<RecordBatch> {
        self.0.lock().map_err(|e| SqlError::Execution(e.to_string()))?.query(sql, schema)
    }
}

impl Worker {
    fn query(&mut self, sql: &str, schema: SchemaRef) -> SqlResult<RecordBatch> {
        let mut bytes = Vec::new();
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, &schema)?;
        writer.finish()?;
        drop(writer);
        let request = serde_json::json!({"sql":sql,"schema_ipc":bytes});
        writeln!(self.input, "{request}").and_then(|_| self.input.flush()).map_err(|e| SqlError::Execution(e.to_string()))?;
        let mut header = String::new();
        self.output.read_line(&mut header).map_err(|e| SqlError::Execution(e.to_string()))?;
        let header: serde_json::Value = serde_json::from_str(&header).map_err(|e| SqlError::Execution(e.to_string()))?;
        if let Some(error) = header["error"].as_str() { return Err(SqlError::Execution(error.into())); }
        let size = header["bytes"].as_u64().ok_or_else(|| SqlError::Conversion("Missing IPC length".into()))? as usize;
        let mut bytes = vec![0; size];
        self.output.read_exact(&mut bytes).map_err(|e| SqlError::Execution(e.to_string()))?;
        let batches = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)?.collect::<Result<Vec<_>, _>>()?;
        Ok(arrow::compute::concat_batches(&schema, &batches)?)
    }
}
