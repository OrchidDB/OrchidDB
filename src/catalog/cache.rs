use super::{CatalogSnapshot, ResolvedCatalog};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::Mutex as AsyncMutex;

pub(super) struct Identity<'a> {
    pub endpoint: &'a str,
    pub scope: &'a str,
    pub graph: &'a str,
    pub revision: Option<i64>,
    pub interval: Duration,
}
struct Cached {
    snapshot: CatalogSnapshot,
    checked: Instant,
}
#[derive(Default)]
pub(super) struct Entry(AsyncMutex<Option<Cached>>);
type Entries = HashMap<[u8; 32], Arc<Entry>>;
static ENTRIES: OnceLock<Mutex<Entries>> = OnceLock::new();

pub(super) fn entry(identity: &Identity<'_>, credential: &[u8]) -> Result<Arc<Entry>, String> {
    let key: [u8; 32] = Sha256::digest(
        serde_json::to_vec(&(
            identity.endpoint,
            identity.scope,
            identity.graph,
            identity.revision,
            identity.interval.as_nanos(),
            credential,
        ))
        .map_err(|_| "invalid catalog cache identity")?,
    )
    .into();
    let mut entries = ENTRIES
        .get_or_init(Default::default)
        .lock()
        .map_err(|_| "catalog cache unavailable")?;
    entries.retain(|_, entry| match entry.0.try_lock() {
        Ok(cached) => {
            cached
                .as_ref()
                .is_some_and(|cached| cached.checked.elapsed() < Duration::from_secs(300))
                || Arc::strong_count(entry) > 1
        }
        Err(_) => true,
    });
    Ok(entries.entry(key).or_default().clone())
}

pub(super) async fn resolve<F, Fut>(
    entry: &Entry,
    identity: &Identity<'_>,
    force: bool,
    now: Instant,
    fetch: F,
) -> Result<CatalogSnapshot, String>
where
    F: FnOnce(Option<i64>) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    let waiting = Instant::now();
    let mut cached = entry.0.lock().await;
    let now = now + waiting.elapsed();
    if !force {
        if let Some(cached) = cached
            .as_ref()
            .filter(|cached| now.saturating_duration_since(cached.checked) < identity.interval)
        {
            return Ok(cached.snapshot.clone());
        }
    }
    let previous = cached.as_ref().map(|cached| &cached.snapshot);
    let response = fetch(previous.map(|snapshot| snapshot.revision)).await;
    let snapshot = response.and_then(|response| validate(response, previous, identity));
    match snapshot {
        Ok(snapshot) => {
            *cached = Some(Cached {
                snapshot: snapshot.clone(),
                checked: now,
            });
            Ok(snapshot)
        }
        Err(error) => {
            *cached = None;
            Err(error)
        }
    }
}
fn validate(
    response: Value,
    previous: Option<&CatalogSnapshot>,
    identity: &Identity<'_>,
) -> Result<CatalogSnapshot, String> {
    if response["unchanged"] == true {
        let mut snapshot = previous
            .cloned()
            .ok_or("catalog returned unchanged without cached metadata")?;
        if response["scope"] != identity.scope
            || response["graph"] != identity.graph
            || response["revision"].as_i64() != Some(snapshot.revision)
        {
            return Err("catalog response does not match cached revision".into());
        }
        snapshot.principal = Some(
            serde_json::from_value(response["principal"].clone())
                .map_err(|_| "invalid catalog principal response")?,
        );
        return Ok(snapshot);
    }
    let resolved: ResolvedCatalog =
        serde_json::from_value(response).map_err(|_| "invalid catalog manifest response")?;
    if resolved.manifest.scope != identity.scope
        || resolved.manifest.graph != identity.graph
        || identity
            .revision
            .is_some_and(|revision| revision != resolved.manifest.revision)
    {
        return Err("catalog response does not match the requested graph revision".into());
    }
    resolved.snapshot()
}

#[cfg(test)]
mod tests;
