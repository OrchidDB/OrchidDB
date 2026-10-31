//! Host-only policy handles; query/schema JSON cannot attach or replace a policy.
use orchiddb::opa::{Client, Config, Identity, Policy};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    policy_uri: String,
    user: String,
    #[serde(default)]
    groups: BTreeSet<String>,
    batched_uri: Option<String>,
    row_filters_uri: Option<String>,
    column_masking_uri: Option<String>,
    batch_column_masking_uri: Option<String>,
    timeout_ms: Option<u64>,
    max_concurrent_requests: Option<usize>,
    max_batch_size: Option<usize>,
    max_response_bytes: Option<usize>,
    default_catalog: Option<String>,
    default_schema: Option<String>,
    #[serde(default)]
    context: BTreeMap<String, String>,
}
#[derive(Default)]
struct Registry {
    next: u64,
    policies: BTreeMap<u64, Policy>,
}
fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}
pub async fn command(input: &str) -> Result<Option<String>, String> {
    let value: Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    let result = match value["op"].as_str() {
        Some("opa_create") => {
            let options: Options =
                serde_json::from_value(value["config"].clone()).map_err(|e| e.to_string())?;
            let mut config = Config::new(options.policy_uri);
            config.batched_uri = options.batched_uri;
            config.row_filters_uri = options.row_filters_uri;
            config.column_masking_uri = options.column_masking_uri;
            config.batch_column_masking_uri = options.batch_column_masking_uri;
            if let Some(v) = options.timeout_ms {
                config.timeout = Duration::from_millis(v);
            }
            if let Some(v) = options.max_concurrent_requests {
                config.max_concurrent_requests = v;
            }
            if let Some(v) = options.max_batch_size {
                config.max_batch_size = v;
            }
            if let Some(v) = options.max_response_bytes {
                config.max_response_bytes = v;
            }
            if let Some(v) = options.default_catalog {
                config.default_catalog = v;
            }
            if let Some(v) = options.default_schema {
                config.default_schema = v;
            }
            config.context = options.context;
            let policy = Policy::new(
                Arc::new(Client::new(config)?),
                Identity {
                    user: options.user,
                    groups: options.groups,
                },
            )?;
            let mut registry = registry().lock().map_err(|_| "OPA registry unavailable")?;
            registry.next = registry
                .next
                .checked_add(1)
                .ok_or("OPA handles exhausted")?;
            let handle = registry.next;
            registry.policies.insert(handle, policy);
            json!(handle).to_string()
        }
        Some("opa_close") => {
            let handle = value["handle"].as_u64().ok_or("invalid OPA handle")?;
            registry()
                .lock()
                .map_err(|_| "OPA registry unavailable")?
                .policies
                .remove(&handle);
            "null".into()
        }
        Some("opa_compile") => {
            let handle = value["handle"].as_u64().ok_or("invalid OPA handle")?;
            let policy = registry()
                .lock()
                .map_err(|_| "OPA registry unavailable")?
                .policies
                .get(&handle)
                .cloned()
                .ok_or("OPA policy is closed or unavailable")?;
            let mut request: orchiddb::compiler::CompileRequest =
                serde_json::from_value(value["request"].clone()).map_err(|e| e.to_string())?;
            if let Some(snapshot) = &request.statistics {
                snapshot.validate()?;
            }
            request.opa = Some(policy);
            serde_json::to_string(&orchiddb::compiler::compile(request).await?)
                .map_err(|e| e.to_string())?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}
