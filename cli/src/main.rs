use arrow::{ipc::writer::StreamWriter, util::pretty::pretty_format_batches};
use duckdb::Connection;
use orchiddb_client::Statistics;
use serde_json::{Value, json};
use std::{
    error::Error,
    fs,
    io::{self, Write},
};

const HELP: &str = "OrchidDB: graph queries over DuckDB + Iceberg
Usage: orchiddb compile REQUEST.json
       orchiddb query REQUEST.json [--database FILE] [--init SQL_FILE] [--format arrow|table] [--no-iceberg]
       orchiddb statistics REQUEST.json --output SNAPSHOT.json [--database FILE] [--init SQL_FILE] [--no-iceberg]
       orchiddb compile REQUEST.json [--statistics SNAPSHOT.json] [--explain-json]
       orchiddb --version

REQUEST.json is compiler protocol v1, including query, mapping, and schema.
Iceberg loads by default for query; first use downloads the signed official extension.
--init executes application setup SQL (views, credentials, plugins, UDFs).
Arrow IPC is the default stdout format; diagnostics go to stderr.
--engines FILE supplies remote engine options by engine ID (endpoint, authentication,
timeouts and limits); credentials stay separate from the compiled query.
";
#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("orchiddb: {e}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let Some(command) = args.next() else {
        print!("{HELP}");
        return Ok(());
    };
    if command == "--help" || command == "-h" {
        print!("{HELP}");
        return Ok(());
    }
    if command == "--version" {
        println!("orchiddb {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if command != "query" && command != "compile" && command != "statistics" {
        return Err(format!("unknown command: {command}").into());
    }
    let request = args.next().ok_or("missing REQUEST.json")?;
    let mut database = None;
    let mut init = None;
    let mut format = "arrow".to_string();
    let mut iceberg = true;
    let mut statistics_path = None;
    let mut output_path = None;
    let mut explain_json = false;
    let mut engines_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--statistics" => statistics_path = Some(args.next().ok_or("missing statistics path")?),
            "--output" => output_path = Some(args.next().ok_or("missing output path")?),
            "--explain-json" => explain_json = true,
            "--engines" => {
                engines_path = Some(args.next().ok_or("missing engine configuration path")?)
            }
            "--database" => database = Some(args.next().ok_or("missing database path")?),
            "--init" => init = Some(args.next().ok_or("missing SQL file")?),
            "--format" => format = args.next().ok_or("missing output format")?,
            "--no-iceberg" => iceberg = false,
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if format != "arrow" && format != "table" {
        return Err("format must be arrow or table".into());
    }
    let request: Value = serde_json::from_str(&fs::read_to_string(request)?)?;
    let mut statistics = Statistics::default();
    if let Some(path) = statistics_path {
        statistics.load(path).await.map_err(io::Error::other)?;
    }
    if command == "compile" {
        let compiled = statistics
            .compile(request)
            .await
            .map_err(io::Error::other)?;
        if explain_json {
            println!("{}", serde_json::to_string_pretty(&compiled)?);
        } else {
            println!(
                "{}",
                compiled["sql"]
                    .as_str()
                    .ok_or("compiler did not return SQL")?
            );
        }
        statistics.clear().await.map_err(io::Error::other)?;
        return Ok(());
    }
    if request["dialect"] != "duckdb" {
        return Err("CLI executes only DuckDB SQL".into());
    }
    if command == "statistics" && output_path.is_none() {
        return Err("statistics requires --output SNAPSHOT.json".into());
    }
    let db = match database {
        Some(path) => Connection::open(path)?,
        None => Connection::open_in_memory()?,
    };
    if iceberg {
        if db.execute_batch("LOAD iceberg").is_err() {
            db.execute_batch("INSTALL iceberg; LOAD iceberg").map_err(|e| io::Error::other(format!("Iceberg could not be installed/loaded: {e}. First use requires network access to extensions.duckdb.org; use --no-iceberg only for queries that do not need Iceberg.")))?;
        }
        let loaded: bool = db.query_row(
            "SELECT loaded FROM duckdb_extensions() WHERE extension_name='iceberg'",
            [],
            |row| row.get(0),
        )?;
        if !loaded {
            return Err("Iceberg extension did not load".into());
        }
    }
    if let Some(path) = init {
        db.execute_batch(&fs::read_to_string(path)?)?;
    }
    if command == "statistics" {
        statistics
            .generate(request, |work| {
                let result = collect_statistics(&db, &work);
                async move { result }
            })
            .await
            .map_err(io::Error::other)?;
        statistics
            .save(output_path.unwrap())
            .map_err(io::Error::other)?;
        println!("{}", serde_json::to_string_pretty(&statistics.report())?);
        statistics.clear().await.map_err(io::Error::other)?;
        return Ok(());
    }
    let compiled = statistics
        .compile_plan(request)
        .await
        .map_err(io::Error::other)?;
    let sql = &compiled.sql;
    if explain_json {
        eprintln!("{}", serde_json::to_string_pretty(&compiled)?);
    }
    if !compiled.transfers.is_empty() {
        let plan = &compiled;
        let target = plan
            .execution_engine
            .as_ref()
            .ok_or("missing execution engine")?;
        let mut sessions: std::collections::BTreeMap<
            String,
            Box<dyn orchiddb_client::federation::Session>,
        > = std::collections::BTreeMap::from([(
            target.clone(),
            Box::new(LocalSession(db)) as Box<dyn orchiddb_client::federation::Session>,
        )]);
        let options: Value = match engines_path {
            Some(path) => serde_json::from_str(&fs::read_to_string(path)?)?,
            None => json!({}),
        };
        let mut routes = std::collections::BTreeMap::new();
        for transfer in &plan.transfers {
            routes.insert(
                transfer.source_engine.clone(),
                transfer.source_dialect.clone(),
            );
            if let Some(operation) = &transfer.request {
                routes.insert(operation.engine.clone(), operation.template.adapter.clone());
            }
            if let Some(operation) = &transfer.operation {
                routes.insert(operation.engine.clone(), operation.template.dialect.clone());
            }
        }
        for (id, adapter) in routes {
            if &id == target {
                continue;
            }
            #[cfg(any(feature = "quickwit", feature = "elasticsearch"))]
            {
                let config = options.get(&id).ok_or_else(|| {
                    format!("Supply options for engine `{id}` using --engines FILE")
                })?;
                let remote = orchiddb_client::remote::transport::HttpSession::from_json_options(
                    &adapter,
                    config.clone(),
                )
                .map_err(io::Error::other)?;
                sessions.insert(id, Box::new(remote));
            }
            #[cfg(not(any(feature = "quickwit", feature = "elasticsearch")))]
            {
                let _ = (&options, adapter);
                return Err(format!(
                    "Remote engine `{id}` requires the quickwit or elasticsearch feature"
                )
                .into());
            }
        }
        let batches = orchiddb_client::federation::execute(plan, &mut sessions)
            .await
            .map_err(io::Error::other)?;
        if format == "arrow" {
            let mut writer = StreamWriter::try_new(
                io::stdout().lock(),
                &batches
                    .first()
                    .ok_or("missing final result schema")?
                    .schema(),
            )?;
            for batch in batches {
                writer.write(&batch)?;
            }
            writer.finish()?;
        } else {
            println!("{}", pretty_format_batches(&batches)?);
        }
        return Ok(());
    }
    let mut statement = db.prepare(sql)?;
    let batches = statement.query_arrow([])?;
    if format == "arrow" {
        let stdout = io::stdout();
        let mut writer = StreamWriter::try_new(stdout.lock(), &batches.get_schema())?;
        for batch in batches {
            writer.write(&batch)?;
        }
        writer.finish()?;
    } else {
        // Human output is deliberately one batch at a time, avoiding full-result collection.
        let mut out = io::stdout().lock();
        for batch in batches {
            writeln!(out, "{}", pretty_format_batches(&[batch])?)?;
        }
    }
    Ok(())
}

struct LocalSession(Connection);
#[async_trait::async_trait(?Send)]
impl orchiddb_client::federation::Session for LocalSession {
    fn dialect(&self) -> &str {
        "duckdb"
    }
    async fn query(&mut self, sql: &str) -> Result<Vec<arrow::record_batch::RecordBatch>, String> {
        let mut statement = self.0.prepare(sql).map_err(|e| e.to_string())?;
        let rows = statement.query_arrow([]).map_err(|e| e.to_string())?;
        let schema = rows.get_schema();
        let mut batches: Vec<_> = rows.collect();
        if batches.is_empty() {
            batches.push(arrow::record_batch::RecordBatch::new_empty(schema));
        }
        Ok(batches)
    }
}

/// The CLI's application-owned DuckDB session adapter. The shared core chooses SQL.
fn collect_statistics(db: &Connection, work: &Value) -> Result<Value, String> {
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

#[cfg(test)]
mod statistics_adapter_tests {
    use super::*;
    #[test]
    fn transport_caps_and_deadlines_leave_the_session_usable() {
        let db = Connection::open_in_memory().unwrap();
        let mut request = json!({"dialect":"duckdb", "sql":"SELECT i FROM range(100) t(i)", "max_rows":2, "max_bytes":1024, "timeout_ms":30000});
        let rows = collect_statistics(&db, &request).unwrap();
        assert_eq!(rows["rows"].as_array().unwrap().len(), 2);
        request["max_bytes"] = json!(1);
        assert!(
            collect_statistics(&db, &request).unwrap()["rows"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        request["sql"] = json!("SELECT sum(sin(i)) FROM range(1000000000000) t(i)");
        request["timeout_ms"] = json!(1);
        assert!(collect_statistics(&db, &request).is_err());
        assert_eq!(
            db.query_row("SELECT 42", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            42
        );
    }
}
