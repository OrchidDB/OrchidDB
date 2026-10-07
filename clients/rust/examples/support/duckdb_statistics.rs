//! Example bounded adapter using an application-owned DuckDB connection.
use duckdb::Connection;
use serde_json::{Value, json};

pub fn collect_statistics(db: &Connection, work: &Value) -> Result<Value, String> {
    if work["dialect"] != "duckdb" {
        return Err("Wrong statistics SQL dialect".into());
    }
    let max_rows = work["max_rows"].as_u64().ok_or("Missing row bound")?;
    let max_bytes = work["max_bytes"].as_u64().ok_or("Missing byte bound")?;
    let timeout = work["timeout_ms"].as_u64().ok_or("Missing timeout")?;
    let sql = work["sql"].as_str().ok_or("Missing statistics SQL")?;
    let sql = format!("SELECT to_json(s) FROM ({sql}) s LIMIT {max_rows}");
    let interrupt = db.interrupt_handle();
    let (sender, receiver) = std::sync::mpsc::channel();
    let thread = std::thread::spawn(move || {
        if matches!(
            receiver.recv_timeout(std::time::Duration::from_millis(timeout)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ) {
            loop {
                interrupt.interrupt();
                // Preparation and execution can reset a prior interrupt. Keep
                // interrupting until this request has released its cursor.
                if !matches!(
                    receiver.recv_timeout(std::time::Duration::from_millis(10)),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                ) {
                    break;
                }
            }
        }
    });
    struct Deadline(
        Option<std::sync::mpsc::Sender<()>>,
        Option<std::thread::JoinHandle<()>>,
    );
    impl Drop for Deadline {
        fn drop(&mut self) {
            let _ = self.0.take().unwrap().send(());
            let _ = self.1.take().unwrap().join();
        }
    }
    let _deadline = Deadline(Some(sender), Some(thread));
    let mut statement = db.prepare(&sql).map_err(|e| e.to_string())?;
    let mut cursor = statement.query([]).map_err(|e| e.to_string())?;
    let mut rows = Vec::new();
    let mut bytes = 0;
    let mut truncated = false;
    while let Some(row) = cursor.next().map_err(|e| e.to_string())? {
        let text: String = row.get(0).map_err(|e| e.to_string())?;
        if bytes + text.len() as u64 > max_bytes {
            truncated = true;
            break;
        }
        bytes += text.len() as u64;
        rows.push(serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())?);
    }
    Ok(json!({"rows":rows,"truncated":truncated}))
}
