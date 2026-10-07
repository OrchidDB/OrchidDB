//! One-time statistics generation. All collection decisions live in the shared core.
use serde_json::{Value, json};
use std::future::Future;

pub use orchiddb::ir::rel::statistics::command;

/// Adapter on an existing session. It must enforce the coordinator's bounds.
#[allow(async_fn_in_trait)]
pub trait StatisticsCollector {
    async fn collect(&mut self, request: Value) -> Result<Value, String>;
}
impl<F, Fut> StatisticsCollector for F
where
    F: FnMut(Value) -> Fut,
    Fut: Future<Output = Result<Value, String>>,
{
    async fn collect(&mut self, request: Value) -> Result<Value, String> {
        self(request).await
    }
}

/// A retained immutable catalog. Save `snapshot()` as JSON to reuse across processes.
/// Call `clear()` when finished to release the native cache entry.
#[derive(Default)]
pub struct Statistics {
    catalog_id: Option<Value>,
    snapshot: Option<Value>,
    report: Option<Value>,
}
struct AnalysisGuard(String);
impl Drop for AnalysisGuard {
    fn drop(&mut self) {
        orchiddb::ir::rel::statistics::cancel_generation(&self.0);
    }
}
impl Drop for Statistics {
    fn drop(&mut self) {
        if let Some(id) = self.catalog_id.as_ref().and_then(Value::as_str) {
            orchiddb::ir::rel::statistics::release_catalog(id);
        }
    }
}
impl Statistics {
    async fn send(request: Value) -> Result<Value, String> {
        serde_json::from_str(&command(&request.to_string()).await?).map_err(|e| e.to_string())
    }
    pub fn snapshot(&self) -> Option<&Value> {
        self.snapshot.as_ref()
    }
    pub fn report(&self) -> Option<&Value> {
        self.report.as_ref()
    }
    /// Execute each collection request through the application's existing session.
    /// The callback must enforce the request's timeout, row and byte limits and
    /// return `{ "rows": [...] }` or `{ "ipc": "base64 Arrow stream" }`.
    /// Set `truncated: true` when a transport cap stops before EOF.
    /// Return an error if bounded execution is unsupported; it is recorded as coverage.
    pub async fn generate<C: StatisticsCollector>(
        &mut self,
        request: Value,
        mut collect: C,
    ) -> Result<(), String> {
        let mut state = Self::send(json!({"op":"begin","request":request})).await?;
        let id = state["id"].clone();
        let _analysis = AnalysisGuard(id.as_str().ok_or("Invalid analysis id")?.to_string());
        let result = async {
            while !state["request"].is_null() {
                let request = state["request"].clone();
                let mut submission = match collect.collect(request.clone()).await {
                    Ok(value) => value,
                    Err(error) => json!({"error":error}),
                };
                let object = submission
                    .as_object_mut()
                    .ok_or("Collector must return an object")?;
                object.insert("op".into(), json!("submit"));
                object.insert("id".into(), id.clone());
                object.insert("request_id".into(), request["id"].clone());
                state = Self::send(submission).await?;
            }
            Self::send(json!({"op":"finish","id":id})).await
        }
        .await;
        match result {
            Ok(done) => {
                self.clear().await?;
                self.catalog_id = Some(done["catalog_id"].clone());
                self.snapshot = Some(done["snapshot"].clone());
                self.report = Some(done["report"].clone());
                Ok(())
            }
            Err(error) => {
                let _ = Self::send(json!({"op":"cancel","id":id})).await;
                Err(error)
            }
        }
    }
    pub async fn install(&mut self, snapshot: Value) -> Result<(), String> {
        let installed = Self::send(json!({"op":"install","snapshot":snapshot})).await?;
        self.clear().await?;
        self.catalog_id = Some(installed["catalog_id"].clone());
        self.snapshot = Some(snapshot);
        Ok(())
    }
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let snapshot = self.snapshot.as_ref().ok_or("No statistics installed")?;
        std::fs::write(path, snapshot.to_string()).map_err(|e| e.to_string())
    }
    pub async fn load(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        self.install(serde_json::from_str(&text).map_err(|e| e.to_string())?)
            .await
    }
    /// Compile to the normal typed plan, ready for `execute(session, &plan)`.
    /// Reuses the cached Arc directly; statistics are not serialized per query.
    pub async fn compile_plan(
        &self,
        request: Value,
    ) -> Result<orchiddb::compiler::CompiledSql, String> {
        let catalog = match self.catalog_id.as_ref().and_then(Value::as_str) {
            Some(id) => {
                let catalog = orchiddb::ir::rel::statistics::catalog(id)?;
                if orchiddb::ir::rel::statistics::mapping_fingerprint(&request) != catalog.mapping {
                    return Err(
                        "statistics snapshot mapping mismatch; regenerate or clear statistics"
                            .into(),
                    );
                }
                Some(catalog)
            }
            None => None,
        };
        let mut request: orchiddb::compiler::CompileRequest =
            serde_json::from_value(request).map_err(|e| e.to_string())?;
        if catalog.is_some() {
            request.statistics = catalog;
        }
        orchiddb::compiler::compile(request).await
    }
    /// Compile to portable JSON, including all planner diagnostics.
    pub async fn compile(&self, request: Value) -> Result<Value, String> {
        serde_json::to_value(self.compile_plan(request).await?).map_err(|e| e.to_string())
    }
    pub async fn clear(&mut self) -> Result<(), String> {
        if let Some(id) = self.catalog_id.as_ref().and_then(Value::as_str) {
            orchiddb::ir::rel::statistics::release_catalog(id);
        }
        self.catalog_id = None;
        self.snapshot = None;
        self.report = None;
        Ok(())
    }
}
