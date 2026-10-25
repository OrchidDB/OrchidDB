//! Snapshot the completed statement before setup/cleanup can replace its profile.
use crate::ir::query_cost::EngineCost;
use duckdb::{Connection, profiling::ProfilingInfo};
use std::sync::{Arc, Mutex};
pub(crate) struct Profile<'a> {
    conn: &'a Connection,
    restore: bool,
    target: Arc<Mutex<EngineCost>>,
    bytes: u64,
}
impl<'a> Profile<'a> {
    pub fn start(
        conn: &'a Connection,
        enabled: bool,
        target: Arc<Mutex<EngineCost>>,
        sql: &str,
    ) -> Result<Option<Self>, String> {
        target.lock().map_err(|e| e.to_string())?.statements += 1;
        if !enabled {
            return Ok(None);
        }
        let old: Option<String> = conn
            .query_row("SELECT current_setting('enable_profiling')", [], |row| {
                row.get(0)
            })
            .map_err(|e| e.to_string())?;
        let restore = old.as_deref().is_none_or(|value| value.is_empty());
        if restore {
            conn.execute_batch("PRAGMA enable_profiling='no_output'")
                .map_err(|e| e.to_string())?;
        }
        Ok(Some(Self {
            conn,
            restore,
            target,
            bytes: sql.len() as u64,
        }))
    }
    pub fn finish(&self) {
        if let Some(info) = self.conn.get_profiling_info() {
            let mut cost = read(&info);
            cost.sql_bytes = self.bytes;
            if let Ok(mut target) = self.target.lock() {
                target.merge(&cost);
            }
        }
    }
}
impl Drop for Profile<'_> {
    fn drop(&mut self) {
        if self.restore {
            let _ = self.conn.execute_batch("PRAGMA disable_profiling");
        }
    }
}
fn metric(info: &ProfilingInfo, name: &str) -> u64 {
    info.metrics
        .get(name)
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}
fn nanos(info: &ProfilingInfo, name: &str) -> u64 {
    info.metrics
        .get(name)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|n| n.is_finite() && *n >= 0.)
        .map(|n| (n * 1e9).round() as u64)
        .unwrap_or(0)
}
fn complete_metrics(info: &ProfilingInfo) -> bool {
    fn operators(info: &ProfilingInfo) -> bool {
        info.metrics.contains_key("OPERATOR_CARDINALITY")
            && info.metrics.contains_key("OPERATOR_ROWS_SCANNED")
            && info.children.iter().all(operators)
    }
    info.metrics.contains_key("CPU_TIME") && info.metrics.contains_key("LATENCY")
        && info.children.iter().all(operators)
}
fn read(info: &ProfilingInfo) -> EngineCost {
    let mut cost = EngineCost {
        // Caller-supplied profiling configurations may disable required
        // metrics. Preserve that configuration, but report partial coverage.
        profiled_statements: u64::from(complete_metrics(info)),
        cpu_nanos: nanos(info, "CPU_TIME"),
        latency_nanos: nanos(info, "LATENCY"),
        peak_memory_bytes: metric(info, "SYSTEM_PEAK_BUFFER_MEMORY"),
        peak_spill_bytes: metric(info, "SYSTEM_PEAK_TEMP_DIR_SIZE"),
        ..Default::default()
    };
    fn walk(info: &ProfilingInfo, depth: u64, cost: &mut EngineCost) {
        cost.operators += 1;
        cost.max_plan_depth = cost.max_plan_depth.max(depth);
        cost.rows_scanned = cost
            .rows_scanned
            .saturating_add(metric(info, "OPERATOR_ROWS_SCANNED"));
        cost.operator_rows = cost
            .operator_rows
            .saturating_add(metric(info, "OPERATOR_CARDINALITY"));
        for child in &info.children {
            walk(child, depth + 1, cost);
        }
    }
    for child in &info.children {
        walk(child, 1, &mut cost);
    }
    cost
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn counts_each_operator_once_and_takes_peaks() {
        let leaf = ProfilingInfo {
            metrics: [
                ("OPERATOR_ROWS_SCANNED".into(), "10".into()),
                ("OPERATOR_CARDINALITY".into(), "3".into()),
            ]
            .into(),
            children: vec![],
        };
        let root = ProfilingInfo {
            metrics: [
                ("CPU_TIME".into(), "0.25".into()),
                ("CUMULATIVE_CARDINALITY".into(), "999".into()),
                ("SYSTEM_PEAK_BUFFER_MEMORY".into(), "4096".into()),
            ]
            .into(),
            children: vec![leaf],
        };
        let mut cost = read(&root);
        assert_eq!(cost.work_units(), 13);
        assert_eq!(cost.cpu_nanos, 250_000_000);
        cost.merge(&read(&root));
        assert_eq!(cost.work_units(), 26);
        assert_eq!(cost.peak_memory_bytes, 4096);
    }
    #[test]
    fn profiles_once_and_restores_disabled_setting() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE SEQUENCE calls").unwrap();
        let cost = Arc::new(Mutex::new(EngineCost::default()));
        {
            let profile = Profile::start(&conn, true, cost.clone(), "SELECT nextval('calls')")
                .unwrap()
                .unwrap();
            let value: i64 = conn
                .query_row("SELECT nextval('calls')", [], |r| r.get(0))
                .unwrap();
            assert_eq!(value, 1);
            profile.finish();
        }
        assert_eq!(cost.lock().unwrap().profiled_statements, 1);
        let next: i64 = conn
            .query_row("SELECT nextval('calls')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(next, 2);
        let setting: Option<String> = conn
            .query_row("SELECT current_setting('enable_profiling')", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert!(setting.is_none_or(|s| s.is_empty()));
    }
}
