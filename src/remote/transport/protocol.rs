//! Foreign-client access to the same validated HTTP sessions used by Rust.
//! Connections are explicitly opened and closed; credentials never enter plans.
use super::*;
use base64::Engine as _;
use serde::Deserialize;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

type Handle = Arc<tokio::sync::Mutex<Option<HttpSession>>>;
static SESSIONS: OnceLock<Mutex<BTreeMap<String, Handle>>> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
fn sessions() -> &'static Mutex<BTreeMap<String, Handle>> {
    SESSIONS.get_or_init(Default::default)
}
fn handle(id: &str) -> Result<Handle, String> {
    sessions()
        .lock()
        .map_err(|_| "remote session registry unavailable")?
        .get(id)
        .cloned()
        .ok_or_else(|| "unknown or closed remote session".into())
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
enum Auth {
    None,
    Basic { username: String, password: String },
    Bearer { token: String },
    ApiKey { token: String },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    endpoint: String,
    authentication: Option<Auth>,
    connect_timeout_ms: Option<u64>,
    request_timeout_ms: Option<u64>,
    page_size: Option<usize>,
    batch_size: Option<usize>,
    max_request_bytes: Option<usize>,
    max_response_bytes: Option<usize>,
    max_rows: Option<usize>,
    scroll_keep_alive: Option<String>,
}
impl Options {
    fn into_http(self) -> HttpOptions {
        let mut o = HttpOptions::new(self.endpoint);
        o.authentication = match self.authentication.unwrap_or(Auth::None) {
            Auth::None => Authentication::None,
            Auth::Basic { username, password } => Authentication::Basic { username, password },
            Auth::Bearer { token } => Authentication::Bearer(token),
            Auth::ApiKey { token } => Authentication::ApiKey(token),
        };
        if let Some(n) = self.connect_timeout_ms {
            o.connect_timeout = Duration::from_millis(n);
        }
        if let Some(n) = self.request_timeout_ms {
            o.request_timeout = Duration::from_millis(n);
        }
        if let Some(n) = self.page_size {
            o.page_size = n;
        }
        if let Some(n) = self.batch_size {
            o.batch_size = n;
        }
        if let Some(n) = self.max_request_bytes {
            o.max_request_bytes = n;
        }
        if let Some(n) = self.max_response_bytes {
            o.max_response_bytes = n;
        }
        if let Some(n) = self.scroll_keep_alive {
            o.scroll_keep_alive = n;
        }
        o.max_rows = self.max_rows;
        o
    }
}

/// Execute an HTTP session lifecycle or typed request command. This is separate
/// from compilation: only an explicit `execute` command performs network I/O.
pub async fn command(input: &str) -> Result<String, String> {
    let input: Value = serde_json::from_str(input).map_err(|_| "invalid remote command JSON")?;
    let op = input["op"].as_str().ok_or("missing remote command op")?;
    let output = if op == "open" {
        let adapter = input["adapter"].as_str().ok_or("missing remote adapter")?;
        let session = new_session(adapter, input["options"].clone())?;
        let id = format!("remote-{}", NEXT.fetch_add(1, Ordering::Relaxed));
        sessions()
            .lock()
            .map_err(|_| "remote session registry unavailable")?
            .insert(id.clone(), Arc::new(tokio::sync::Mutex::new(Some(session))));
        json!({"id":id,"adapter":adapter})
    } else {
        let id = input["id"].as_str().ok_or("missing remote session id")?;
        if op == "close" {
            let removed = sessions()
                .lock()
                .map_err(|_| "remote session registry unavailable")?
                .remove(id);
            if let Some(session) = removed {
                session.lock().await.take();
            }
            json!({"closed":true})
        } else {
            let shared = handle(id)?;
            let mut guard = shared.lock().await;
            let session = guard.as_mut().ok_or("closed remote session")?;
            match op {
                "clear_metadata_cache" => {
                    session.clear_metadata_cache();
                    json!({"cleared":true})
                }
                "execute" => {
                    let format = input["format"].as_str().unwrap_or("ipc");
                    if !matches!(format, "ipc" | "rows") {
                        return Err("unsupported remote result format".into());
                    }
                    let requests = input["requests"]
                        .as_array()
                        .ok_or("missing remote requests")?;
                    let columns: Vec<TransferColumn> =
                        serde_json::from_value(input["columns"].clone())
                            .map_err(|e| format!("invalid remote result schema: {e}"))?;
                    let schema = Arc::new(Schema::new(
                        columns
                            .iter()
                            .map(|c| {
                                Ok(Field::new(
                                    &c.name,
                                    crate::compiler::data_type(&c.data_type)?,
                                    c.nullable,
                                ))
                            })
                            .collect::<Result<Vec<_>, String>>()?,
                    ));
                    let results = session.execute_many(requests, &columns).await?;
                    if results.len() != requests.len() {
                        return Err("remote result count mismatch".into());
                    }
                    let batches: Vec<_> = results.into_iter().flatten().collect();
                    if format == "ipc" {
                        let mut bytes = vec![];
                        {
                            let mut writer =
                                arrow::ipc::writer::StreamWriter::try_new(&mut bytes, &schema)
                                    .map_err(|e| e.to_string())?;
                            for batch in &batches {
                                writer.write(batch).map_err(|e| e.to_string())?;
                            }
                            writer.finish().map_err(|e| e.to_string())?;
                        }
                        json!({"ipc":base64::engine::general_purpose::STANDARD.encode(bytes)})
                    } else {
                        let mut rows = vec![];
                        for batch in batches {
                            for row in 0..batch.num_rows() {
                                let values = batch
                                    .columns()
                                    .iter()
                                    .map(|a| {
                                        let value = ScalarValue::try_from_array(a, row)
                                            .map_err(|e| e.to_string())?;
                                        crate::federation::scalar_json(&value)
                                            .map(lossless_numbers)
                                            .map_err(|e| e.to_string())
                                    })
                                    .collect::<Result<Vec<_>, String>>()?;
                                rows.push(values);
                            }
                        }
                        json!({"rows":rows})
                    }
                }
                _ => return Err("unknown remote command".into()),
            }
        }
    };
    Ok(output.to_string())
}

// Typed exchange readers accept numeric text. Keep it exact even when the host
// language's JSON parser would otherwise turn a 64-bit ID into a double.
fn lossless_numbers(value: Value) -> Value {
    match value {
        Value::Number(n) => Value::String(n.to_string()),
        Value::Array(a) => Value::Array(a.into_iter().map(lossless_numbers).collect()),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| (k, lossless_numbers(v)))
                .collect(),
        ),
        v => v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn lifecycle_empty_results_and_invalid_options() {
        let adapter = if cfg!(feature = "quickwit") {
            "quickwit"
        } else {
            "elasticsearch"
        };
        let open =
            json!({"op":"open","adapter":adapter,"options":{"endpoint":"http://127.0.0.1:1"}});
        let result: Value =
            serde_json::from_str(&command(&open.to_string()).await.unwrap()).unwrap();
        let id = &result["id"];
        let execute = json!({"op":"execute","id":id,"requests":[],"columns":[{"name":"id","data_type":"int64","nullable":false}],"format":"rows"});
        assert_eq!(
            command(&execute.to_string()).await.unwrap(),
            r#"{"rows":[]}"#
        );
        command(&json!({"op":"close","id":id}).to_string())
            .await
            .unwrap();
        command(&json!({"op":"close","id":id}).to_string())
            .await
            .unwrap();
        assert!(
            command(&execute.to_string())
                .await
                .unwrap_err()
                .contains("closed")
        );
        let invalid = json!({"op":"open","adapter":adapter,"options":{"endpoint":"http://localhost","authentication":{"type":"bearer","token":"secret","extra":"secret"}}});
        let error = command(&invalid.to_string()).await.unwrap_err();
        assert!(!error.contains("secret"));
    }
    #[test]
    fn typed_json_exchange_does_not_round_numbers() {
        assert_eq!(
            lossless_numbers(json!([
                9007199254740993u64,
                true,
                null,
                "{\"n\":9007199254740993}"
            ])),
            json!(["9007199254740993", true, null, "{\"n\":9007199254740993}"])
        );
    }
}

pub(super) fn new_session(adapter: &str, options: Value) -> Result<HttpSession, String> {
    let engine = match adapter {
        #[cfg(feature = "weaviate")]
        "weaviate" => Engine::Weaviate,
        #[cfg(feature = "quickwit")]
        "quickwit" => Engine::Quickwit,
        #[cfg(feature = "elasticsearch")]
        "elasticsearch" => Engine::Elasticsearch,
        _ => return Err("unsupported or disabled remote adapter".into()),
    };
    // Do not include raw option values (potential credentials) in errors.
    let options: Options =
        serde_json::from_value(options).map_err(|_| "invalid remote HTTP options")?;
    HttpSession::new(engine, options.into_http(), None)
}
