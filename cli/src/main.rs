use arrow::{ipc::writer::StreamWriter, util::pretty::pretty_format_batches};
use duckdb::Connection;
#[path = "../../clients/rust/src/statistics.rs"]
mod statistics;
use serde_json::{Value, json};
use statistics::Statistics;
use std::{
    error::Error,
    fs,
    io::{self, Write},
};

const HELP: &str = "OrchidDB: execute graph queries over DuckDB + Iceberg
Usage: orchiddb query QUERY --schema SCHEMA.json [--language cypher|gremlin|sparql]
       orchiddb query QUERY --catalog URL --scope SCOPE --graph GRAPH
       orchiddb query --file QUERY_FILE --schema SCHEMA.json
       orchiddb statistics --schema SCHEMA.json --output SNAPSHOT.json
       orchiddb --version

Schema files contain graph mappings and source metadata only, never query text.
--catalog URL selects Orchid Catalog instead of --schema.
--scope and --graph identify its graph; --revision pins a publication.
--token-env NAME selects the credential environment variable (default: ORCHID_CATALOG_TOKEN).
--auth bearer|client_credentials|token_exchange selects catalog authentication.
--token-file FILE reads a bearer token from a mounted file.
--client-id ID and --client-secret-env NAME (or --client-secret-file FILE) use OAuth.
--token-endpoint URL overrides the catalog token endpoint; --issuer URL uses OIDC discovery.
--oauth-scope ROLES selects OAuth roles (default: PRINCIPAL_ROLE:ALL).
--subject-token-env NAME (or --subject-token-file FILE) supplies a token to exchange.
--parameters FILE supplies a JSON object of query parameters.
--database FILE opens a persistent database; --init FILE runs setup SQL first.
--format arrow|table selects results (default: arrow).
--statistics FILE loads a saved statistics snapshot.
--engines FILE supplies remote connection options by engine ID.
Iceberg loads by default; --no-iceberg disables it.
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
    if command != "query" && command != "statistics" {
        return Err(format!("unknown command: {command}").into());
    }
    let mut query = None;
    let mut query_file = None;
    let mut schema_path = None;
    let mut catalog_endpoint = None;
    let mut catalog_scope = None;
    let mut catalog_graph = None;
    let mut catalog_revision: Option<i64> = None;
    let mut token_env = None;
    let mut auth_options = serde_json::Map::new();
    let mut language = "cypher".to_string();
    let mut parameters = serde_json::Map::new();
    let mut database = None;
    let mut init = None;
    let mut format = "arrow".to_string();
    let mut iceberg = true;
    let mut statistics_path = None;
    let mut output_path = None;
    let mut engines_path = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--catalog" => catalog_endpoint = Some(args.next().ok_or("missing catalog URL")?),
            "--scope" => catalog_scope = Some(args.next().ok_or("missing catalog scope")?),
            "--graph" => catalog_graph = Some(args.next().ok_or("missing graph name")?),
            "--revision" => catalog_revision = Some(args.next().ok_or("missing revision")?.parse()?),
            "--auth" | "--client-id" | "--client-secret-env" | "--client-secret-file" | "--token-endpoint" | "--issuer" | "--oauth-scope" | "--token-file" | "--subject-token-env" | "--subject-token-file" => {
                let key = arg.trim_start_matches("--").replace('-', "_");
                auth_options.insert(key, args.next().ok_or("missing authentication option value")?.into());
            }
            "--token-env" => token_env = Some(args.next().ok_or("missing token environment variable")?),
            "--statistics" => statistics_path = Some(args.next().ok_or("missing statistics path")?),
            "--output" => output_path = Some(args.next().ok_or("missing output path")?),
            "--schema" => schema_path = Some(args.next().ok_or("missing schema file")?),
            "--file" => query_file = Some(args.next().ok_or("missing query file")?),
            "--language" => language = args.next().ok_or("missing language")?,
            "--parameters" => {
                parameters = serde_json::from_str(&fs::read_to_string(
                    args.next().ok_or("missing parameters file")?,
                )?)?
            }
            "--engines" => {
                engines_path = Some(args.next().ok_or("missing engine configuration path")?)
            }
            "--database" => database = Some(args.next().ok_or("missing database path")?),
            "--init" => init = Some(args.next().ok_or("missing SQL file")?),
            "--format" => format = args.next().ok_or("missing output format")?,
            "--no-iceberg" => iceberg = false,
            _ if !arg.starts_with('-') && query.is_none() => query = Some(arg),
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if format != "arrow" && format != "table" {
        return Err("format must be arrow or table".into());
    }
    let schema = match (schema_path, catalog_endpoint) {
        (Some(path), None) => {
            if catalog_scope.is_some() || catalog_graph.is_some() || catalog_revision.is_some() || token_env.is_some() || !auth_options.is_empty() {
                return Err("catalog options require --catalog".into());
            }
            orchiddb::session::Schema::from_json(&fs::read_to_string(path)?)
        }
        (None, Some(endpoint)) => orchiddb::session::Schema::from_value(json!({"catalog": {
            "endpoint": endpoint,
            "scope": catalog_scope.ok_or("--catalog requires --scope")?,
            "graph": catalog_graph.ok_or("--catalog requires --graph")?,
            "token_env": token_env.clone().unwrap_or_else(|| "ORCHID_CATALOG_TOKEN".into()),
            "auth": catalog_auth(&auth_options, token_env.as_deref())?,
            "revision": catalog_revision,
        }})),
        _ => return Err("supply either --catalog or --schema".into()),
    }.map_err(io::Error::other)?;
    if query.is_some() && query_file.is_some() {
        return Err("supply query text or --file, not both".into());
    }
    if let Some(path) = query_file {
        query = Some(fs::read_to_string(path)?);
    }
    if command == "statistics" && query.is_some() {
        return Err("statistics takes a schema, not a query".into());
    }
    let text = if command == "statistics" {
        "RETURN 1".into()
    } else {
        query.ok_or("query text or --file is required")?
    };
    let mut query = orchiddb::session::Query::cypher(text);
    query.language = language;
    query.parameters = parameters.into_iter().collect();
    let schema = schema.resolve().await.map_err(io::Error::other)?;
    let request =
        serde_json::to_value(schema.request("duckdb", &query).map_err(io::Error::other)?)?;
    let mut statistics = Statistics::default();
    if let Some(path) = statistics_path {
        statistics.load(path).await.map_err(io::Error::other)?;
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
    if !compiled.transfers.is_empty() {
        let plan = &compiled;
        let target = plan
            .execution_engine
            .as_ref()
            .ok_or("missing execution engine")?;
        let mut sessions: std::collections::BTreeMap<
            String,
            Box<dyn orchiddb::federation::Session>,
        > = std::collections::BTreeMap::from([(
            target.clone(),
            Box::new(LocalSession(db)) as Box<dyn orchiddb::federation::Session>,
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
            #[cfg(any(feature = "quickwit", feature = "elasticsearch", feature = "weaviate"))]
            {
                let config = options.get(&id).ok_or_else(|| {
                    format!("Supply options for engine `{id}` using --engines FILE")
                })?;
                let remote = orchiddb::remote::transport::HttpSession::from_json_options(
                    &adapter,
                    config.clone(),
                )
                .map_err(io::Error::other)?;
                sessions.insert(id, Box::new(remote));
            }
            #[cfg(not(any(feature = "quickwit", feature = "elasticsearch", feature = "weaviate")))]
            {
                let _ = (&options, adapter);
                return Err(format!(
                    "Remote engine `{id}` requires the corresponding remote engine feature"
                )
                .into());
            }
        }
        let batches = orchiddb::federation::execute(plan, &mut sessions)
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
impl orchiddb::federation::Session for LocalSession {
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

fn catalog_auth(options: &serde_json::Map<String,Value>, token_env: Option<&str>) -> Result<Option<orchiddb::catalog::CatalogAuth>,Box<dyn Error>> {
    use orchiddb::catalog::{CatalogAuth,Credential};
    if options.is_empty() { return Ok(None); }
    let value = |name: &str|options.get(name).and_then(Value::as_str);
    let kind = value("auth").unwrap_or(if value("client_id").is_some() { "client_credentials" } else if value("subject_token_env").is_some() || value("subject_token_file").is_some() { "token_exchange" } else { "bearer" });
    let credential = |env: &str, file: &str| -> Result<Credential,Box<dyn Error>> {
        match (value(env),value(file)) {
            (Some(name),None) => Ok(Credential::env(name)),
            (None,Some(path)) => Ok(Credential::file(path)),
            _=>Err("supply exactly one credential environment variable or file".into()),
        }
    };
    let auth = match kind {
        "bearer" => {
            if value("token_file").is_some() && token_env.is_some() { return Err("choose --token-file or --token-env".into()); }
            CatalogAuth::bearer(value("token_file").map(Credential::file).unwrap_or_else(||Credential::env(token_env.unwrap_or("ORCHID_CATALOG_TOKEN"))))
        }
        "client_credentials" => CatalogAuth::client_credentials(value("client_id").ok_or("OAuth requires --client-id")?,credential("client_secret_env","client_secret_file")?),
        "token_exchange" => CatalogAuth::token_exchange(credential("subject_token_env","subject_token_file")?),
        _=>return Err("unknown catalog authentication type".into()),
    };
    for key in options.keys() {
        let allowed = match kind {
            "bearer" => matches!(key.as_str(),"auth"|"token_file"),
            "client_credentials" => matches!(key.as_str(),"auth"|"client_id"|"client_secret_env"|"client_secret_file"|"token_endpoint"|"issuer"|"oauth_scope"),
            _=>matches!(key.as_str(),"auth"|"subject_token_env"|"subject_token_file"|"token_endpoint"|"oauth_scope"),
        };
        if !allowed { return Err("authentication option does not apply to the selected method".into()); }
    }
    if kind != "bearer" && token_env.is_some() { return Err("--token-env applies only to bearer authentication".into()); }
    if value("issuer").is_some() && value("token_endpoint").is_some() { return Err("choose --issuer or --token-endpoint".into()); }
    let auth = if let Some(endpoint)=value("token_endpoint") { auth.with_token_endpoint(endpoint) } else { auth };
    let auth = if let Some(issuer)=value("issuer") { auth.with_issuer(issuer) } else { auth };
    Ok(Some(if let Some(scope)=value("oauth_scope") { auth.with_scope(scope) } else { auth }))
}
