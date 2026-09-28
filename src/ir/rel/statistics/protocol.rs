use super::*;
use base64::Engine;
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::Cursor,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};
static NEXT: AtomicU64 = AtomicU64::new(1);
#[derive(Default)]
struct Registry {
    generators: HashMap<String, Generator>,
    catalogs: HashMap<String, Arc<StatisticsSnapshot>>,
}
fn registry() -> &'static Mutex<Registry> {
    static R: OnceLock<Mutex<Registry>> = OnceLock::new();
    R.get_or_init(Default::default)
}
fn id() -> String {
    format!("statistics-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}
fn required<'a>(v: &'a Value, name: &str) -> Result<&'a str, String> {
    v[name]
        .as_str()
        .ok_or_else(|| format!("missing statistics {name}"))
}
/// Shared stateful statistics protocol. The returned JSON is the result, without an ABI envelope.
pub async fn command(input: &str) -> Result<String, String> {
    if input.len() > MAX_BYTES * 2 {
        return Err("statistics command exceeds input budget".into());
    }
    let v: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    // Do not retain the registry lock while planning or awaiting anything.
    if v["op"] == "compile" {
        let catalog = {
            let r = registry()
                .lock()
                .map_err(|_| "statistics registry poisoned")?;
            r.catalogs
                .get(required(&v, "catalog_id")?)
                .cloned()
                .ok_or("unknown statistics catalog")?
        };
        let mut request: crate::compiler::CompileRequest =
            serde_json::from_value(v["request"].clone()).map_err(|e| e.to_string())?;
        if mapping_fingerprint(&v["request"]) != catalog.mapping {
            return Err(
                "statistics snapshot mapping mismatch; regenerate or clear statistics".into(),
            );
        }
        request.statistics = Some(catalog);
        return serde_json::to_string(&crate::compiler::compile(request).await?)
            .map_err(|e| e.to_string());
    }
    let decoded_rows = if v["op"] == "submit" {
        Some(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || -> Result<Vec<BTreeMap<String, Value>>, String> {
                    let rows = if let Some(ipc) = v["ipc"].as_str() {
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(ipc)
                            .map_err(|e| e.to_string())?;
                        if bytes.len() > MAX_BYTES {
                            return Err("IPC batch exceeds statistics budget".into());
                        }
                        let reader =
                            arrow::ipc::reader::StreamReader::try_new(Cursor::new(bytes), None)
                                .map_err(|e| e.to_string())?;
                        let mut rows = Vec::new();
                        let mut decoded = 0usize;
                        for batch in reader {
                            let batch = batch.map_err(|e| e.to_string())?;
                            decoded += batch.get_array_memory_size();
                            if decoded > MAX_BYTES || rows.len() + batch.num_rows() > MAX_ROWS {
                                return Err("decoded IPC exceeds statistics budget".into());
                            }
                            let mut out = Vec::new();
                            {
                                let mut writer = arrow_json::ArrayWriter::new(&mut out);
                                writer.write(&batch).map_err(|e| e.to_string())?;
                                writer.finish().map_err(|e| e.to_string())?;
                            }
                            let chunk: Vec<BTreeMap<String, Value>> =
                                serde_json::from_slice(&out).map_err(|e| e.to_string())?;
                            rows.extend(chunk);
                        }
                        rows
                    } else {
                        serde_json::from_value(v.get("rows").cloned().unwrap_or(json!([])))
                            .map_err(|e| e.to_string())?
                    };
                    Ok(rows)
                },
            ))
            .map_err(|_| "invalid Arrow IPC statistics batch".to_string())??,
        )
    } else {
        None
    };
    let mut r = registry()
        .lock()
        .map_err(|_| "statistics registry poisoned")?;
    let output = match required(&v, "op")? {
        "begin" => {
            if r.generators.len() >= 16 {
                return Err("too many active statistics generations".into());
            }
            let mut g = Generator::new(v["request"].clone())?;
            let next = g.next();
            let id = id();
            r.generators.insert(id.clone(), g);
            json!({"id":id,"request":next})
        }
        "next" => {
            let id = required(&v, "id")?;
            let g = r
                .generators
                .get_mut(id)
                .ok_or("unknown statistics generation")?;
            json!({"id":id,"request":g.next()})
        }
        "submit" => {
            let id = required(&v, "id")?;
            let g = r
                .generators
                .get_mut(id)
                .ok_or("unknown statistics generation")?;
            if v.get("ipc").is_some() && v.get("rows").is_some() {
                return Err("submit rows or IPC, not both".into());
            }
            let rows = decoded_rows.unwrap_or_default();
            if v["truncated"].as_bool().unwrap_or(false) {
                g.submit(required(&v, "request_id")?, rows, None, false)?;
                g.submit(
                    required(&v, "request_id")?,
                    vec![],
                    Some("request transport budget reached; partial sample".into()),
                    v["done"].as_bool().unwrap_or(true),
                )?;
            } else {
                g.submit(
                    required(&v, "request_id")?,
                    rows,
                    v["error"].as_str().map(str::to_string),
                    v["done"].as_bool().unwrap_or(true),
                )?;
            }
            json!({"id":id,"request":g.next()})
        }
        "finish" => {
            if r.catalogs.len() >= 64 {
                return Err("statistics catalog capacity reached; release unused handles".into());
            }
            let g = r
                .generators
                .remove(required(&v, "id")?)
                .ok_or("unknown statistics generation")?;
            let snapshot = g.finish();
            snapshot.validate()?;
            let id = id();
            let report = snapshot.report.clone();
            r.catalogs.insert(id.clone(), Arc::new(snapshot.clone()));
            json!({"catalog_id":id,"snapshot":snapshot,"report":report})
        }
        "cancel" => {
            r.generators.remove(required(&v, "id")?);
            json!({"cancelled":true})
        }
        "install" => {
            if r.catalogs.len() >= 64 {
                return Err("statistics catalog capacity reached; release unused handles".into());
            }
            let snapshot: StatisticsSnapshot =
                serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
            snapshot.validate()?;
            let id = id();
            r.catalogs.insert(id.clone(), Arc::new(snapshot));
            json!({"catalog_id":id})
        }
        "release" => {
            r.catalogs.remove(required(&v, "catalog_id")?);
            json!({"released":true})
        }
        _ => return Err("unknown statistics operation".into()),
    };
    Ok(output.to_string())
}
/// Release a catalog from RAII owners without entering an async runtime.
pub fn release_catalog(id: &str) {
    if let Ok(mut r) = registry().lock() {
        r.catalogs.remove(id);
    }
}
/// Cancel an unfinished collection from RAII owners.
pub fn cancel_generation(id: &str) {
    if let Ok(mut r) = registry().lock() {
        r.generators.remove(id);
    }
}
/// Borrow a retained immutable catalog for direct typed Rust compilation.
pub fn catalog(id: &str) -> Result<Arc<StatisticsSnapshot>, String> {
    registry()
        .lock()
        .map_err(|_| "statistics registry poisoned".to_string())?
        .catalogs
        .get(id)
        .cloned()
        .ok_or_else(|| "unknown statistics catalog".into())
}
