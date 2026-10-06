//! Optional, explicitly generated statistics. Estimation never authorizes a rewrite.
mod collect;
pub(crate) mod access;
mod estimate;
mod optimize;
mod protocol;
mod provider;
pub use collect::{CollectionRequest, Generator};
pub(crate) use estimate::source_access_cost;
pub use estimate::{PlanEstimate, estimate, explain};
pub use optimize::{OptimizerDecision, optimize};
pub use protocol::{cancel_generation, catalog, command, release_catalog};
pub use provider::{StatisticsProvider, statistics_provider};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const VERSION: u32 = 1;
pub const MAX_ROWS: usize = 100_000;
pub const MAX_ELEMENTS: usize = 250_000;
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RETAINED_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_REQUESTS: usize = 32;
pub const TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ColumnStatistics {
    pub data_type: String,
    pub observations: u64,
    pub nulls: u64,
    pub nan: u64,
    /// Observed distinct values. Never represented as exact population NDV.
    pub sample_distinct: u64,
    pub estimated_distinct: Option<f64>,
    pub frequent: Vec<(String, u64)>,
    pub histogram: Vec<String>,
    pub minimum: Option<String>,
    pub maximum: Option<String>,
    pub average_width: f64,
    pub list: Option<ListStatistics>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ListStatistics {
    pub parents: u64,
    pub null_lists: u64,
    pub empty_lists: u64,
    pub observed_elements: u64,
    pub total_lengths: u64,
    pub truncated: bool,
    pub lengths: Vec<u64>,
    pub elements: BTreeMap<String, ColumnStatistics>,
    pub parents_containing: BTreeMap<String, u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupStatistics {
    pub columns: Vec<String>,
    pub observations: u64,
    pub sample_distinct: u64,
    pub frequent: Vec<(Vec<String>, u64)>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceStatistics {
    pub schema: String,
    pub estimated_rows: Option<f64>,
    pub estimated_bytes: Option<f64>,
    pub sample_rows: u64,
    pub method: String,
    pub columns: BTreeMap<String, ColumnStatistics>,
    pub groups: Vec<GroupStatistics>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RelationshipStatistics {
    pub name: String,
    pub source: String,
    pub source_columns: Vec<String>,
    pub target_columns: Vec<String>,
    pub sampled_edges: u64,
    pub source_distinct: u64,
    pub target_distinct: u64,
    pub pair_distinct: u64,
    pub outgoing_heavy_hitters: Vec<(Vec<String>, u64)>,
    pub incoming_heavy_hitters: Vec<(Vec<String>, u64)>,
    pub method: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RdfStatistics {
    pub source: String,
    pub dataset: String,
    pub predicate: String,
    pub sampled_statements: u64,
    pub sampled_subjects: u64,
    pub sampled_objects: u64,
    pub object_kind: String,
    pub datatype: Option<String>,
    pub language: Option<String>,
    pub method: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CollectionReport {
    pub requests: usize,
    pub accepted_rows: usize,
    pub accepted_bytes: usize,
    pub elapsed_ms: u64,
    pub complete: bool,
    pub stopped: String,
    pub notes: Vec<String>,
    pub skipped: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticsSnapshot {
    pub version: u32,
    pub revision: String,
    pub mapping: String,
    pub collected_at: String,
    pub sources: BTreeMap<String, SourceStatistics>,
    pub relationships: Vec<RelationshipStatistics>,
    #[serde(default)]
    pub rdf: Vec<RdfStatistics>,
    pub report: CollectionReport,
}
impl StatisticsSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err("unsupported statistics snapshot version".into());
        }
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err("statistics snapshot exceeds size budget".into());
        }
        self.validate_estimates()
    }
    /// Cheap validation for a retained immutable snapshot; no JSON serialization.
    pub(crate) fn validate_estimates(&self) -> Result<(), String> {
        if self.version != VERSION {
            return Err("unsupported statistics snapshot version".into());
        }
        for s in self.sources.values() {
            if [s.estimated_rows, s.estimated_bytes]
                .into_iter()
                .flatten()
                .any(|x| !x.is_finite() || x < 0.0)
            {
                return Err("invalid statistics estimate".into());
            }
        }
        Ok(())
    }
}
pub(crate) fn digest(value: &impl Serialize) -> String {
    format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).unwrap_or_default())
    )
}
pub fn mapping_fingerprint(request: &serde_json::Value) -> String {
    use serde_json::{Value, json};
    let mut m: BTreeMap<String, Value> = BTreeMap::new();
    let mut tables = BTreeMap::new();
    for t in request["tables"].as_array().into_iter().flatten() {
        let columns=t["columns"].as_array().into_iter().flatten().map(|c|json!({"name":c["name"],"data_type":c["data_type"],"nullable":c["nullable"].as_bool().unwrap_or(true)})).collect::<Vec<_>>();
        tables.insert(t["name"].as_str().unwrap_or(""), columns);
    }
    m.insert("tables".into(), json!(tables));
    for name in [
        "logical_sources",
        "collection_sources",
        "representation_sources",
    ] {
        let v = request.get(name).cloned().unwrap_or(json!([]));
        let canonical = match name {
            "logical_sources" => {
                serde_json::from_value::<Vec<super::layout::LogicalSource>>(v.clone())
                    .ok()
                    .and_then(|x| serde_json::to_value(x).ok())
            }
            "collection_sources" => {
                serde_json::from_value::<Vec<super::collection_source::CollectionSource>>(v.clone())
                    .ok()
                    .and_then(|x| serde_json::to_value(x).ok())
            }
            _ => serde_json::from_value::<Vec<super::representation::RepresentationSource>>(
                v.clone(),
            )
            .ok()
            .and_then(|x| serde_json::to_value(x).ok()),
        }
        .unwrap_or(v);
        m.insert(name.into(), canonical);
    }
    digest(&m)
}
pub fn schema_fingerprint(schema: &arrow::datatypes::Schema) -> String {
    digest(
        &schema
            .fields()
            .iter()
            .map(|f| (f.name(), format!("{:?}", f.data_type()), f.is_nullable()))
            .collect::<Vec<_>>(),
    )
}
/// Portable schema spelling accepted by the compiler and collection protocol.
pub fn type_name(dt: &arrow::datatypes::DataType) -> Result<String, String> {
    use arrow::datatypes::DataType::*;
    Ok(match dt {
        Boolean => "boolean".into(),
        Int8 => "int8".into(),
        Int16 => "int16".into(),
        Int32 => "int32".into(),
        Int64 => "int64".into(),
        UInt8 => "uint8".into(),
        UInt16 => "uint16".into(),
        UInt32 => "uint32".into(),
        UInt64 => "uint64".into(),
        Float32 => "float32".into(),
        Float64 => "float64".into(),
        Utf8 => "string".into(),
        Binary => "binary".into(),
        Date32 => "date".into(),
        Timestamp(arrow::datatypes::TimeUnit::Microsecond, None) => "timestamp".into(),
        Time64(arrow::datatypes::TimeUnit::Microsecond) => "time".into(),
        Duration(arrow::datatypes::TimeUnit::Microsecond) => "duration".into(),
        Interval(arrow::datatypes::IntervalUnit::MonthDayNano) => "interval".into(),
        Decimal128(p, s) => format!("decimal:{p}:{s}"),
        List(f) => format!("list:{}", type_name(f.data_type())?),
        Struct(fields) => format!(
            "struct:{}",
            serde_json::to_string(
                &fields
                    .iter()
                    .map(|f| Ok((f.name().clone(), type_name(f.data_type())?)))
                    .collect::<Result<BTreeMap<_, _>, String>>()?
            )
            .map_err(|e| e.to_string())?
        ),
        _ => return Err(format!("statistics schema type {dt:?} is not supported")),
    })
}
/// Explicit collection on a caller-owned DuckDB connection. Does not commit or change settings.
#[cfg(feature = "duckdb")]
pub fn generate_duckdb(
    connection: &duckdb::Connection,
    request: serde_json::Value,
) -> Result<StatisticsSnapshot, String> {
    let mut generator = Generator::new(request)?;
    while let Some(task) = generator.next() {
        let interrupt = connection.interrupt_handle();
        let (send, recv) = std::sync::mpsc::channel();
        let timeout = std::time::Duration::from_millis(task.timeout_ms.max(1));
        let timer = std::thread::spawn(move || {
            if recv.recv_timeout(timeout).is_err() {
                interrupt.interrupt();
            }
        });
        let result = (|| -> Result<(), String> {
            let mut stmt = connection.prepare(&task.sql).map_err(|e| e.to_string())?;
            let reader = stmt.query_arrow([]).map_err(|e| e.to_string())?;
            let mut bytes = 0usize;
            let mut count = 0usize;
            for batch in reader {
                bytes = bytes.saturating_add(batch.get_array_memory_size());
                count += batch.num_rows();
                if bytes > task.max_bytes || count > task.max_rows {
                    return Err("request transport budget reached".into());
                }
                let mut json = Vec::new();
                {
                    let mut w = arrow_json::ArrayWriter::new(&mut json);
                    w.write(&batch).map_err(|e| e.to_string())?;
                    w.finish().map_err(|e| e.to_string())?;
                }
                let rows = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
                generator.submit(&task.id, rows, None, false)?;
            }
            Ok(())
        })();
        let _ = send.send(());
        let _ = timer.join();
        generator.submit(&task.id, Vec::new(), result.err(), true)?;
    }
    let snapshot = generator.finish();
    snapshot.validate()?;
    Ok(snapshot)
}
