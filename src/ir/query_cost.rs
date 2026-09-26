//! Measured query work for ranking SQL-lowering opportunities.
//!
//! This is a deterministic boundary-work metric, not an optimizer estimate of
//! DuckDB's internal scans or a prediction of elapsed time.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct QueryCost {
    /// Actual SQL island invocations, including repeated lateral invocations.
    pub sql_executions: u64,
    /// Rows and Arrow array memory bytes returned by SQL islands.
    pub sql_output_rows: u64,
    pub sql_output_bytes: u64,
    /// Input/output row visits across native graph kernels. SQL decoding is
    /// excluded; a row visited by two operators counts twice.
    pub native_input_rows: u64,
    pub native_output_rows: u64,
    pub native_kernel_calls: u64,
    /// Demand-driven database requests made by native graph kernels.
    pub native_source_queries: u64,
    pub native_source_rows: u64,
    /// Final result size, for comparing work with useful output.
    pub result_rows: u64,
    /// Observed execution duration; excluded from the deterministic score.
    pub elapsed_micros: u64,
}
impl QueryCost {
    /// Versioned heuristic: one unit per row visit or KiB transferred, plus
    /// 100 units per database request. The request weight flags N+1 access.
    pub const SCORE_VERSION: u32 = 1;
    pub fn work_units(&self) -> u64 {
        self.sql_output_rows
            .saturating_add(self.native_input_rows)
            .saturating_add(self.native_output_rows)
            .saturating_add(self.native_source_rows)
            .saturating_add(self.sql_output_bytes.div_ceil(1024))
            .saturating_add(
                self.sql_executions
                    .saturating_add(self.native_source_queries)
                    .saturating_mul(100),
            )
    }
    /// Work per result row, using one as the denominator for empty results.
    /// Pair with absolute work_units(): a legitimate aggregate can have high
    /// amplification even when its lowering is already efficient.
    pub fn work_per_result_row(&self) -> f64 {
        self.work_units() as f64 / self.result_rows.max(1) as f64
    }
    /// Serializable measurements plus the versioned ranking score.
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({"metric_version":Self::SCORE_VERSION,"coverage":"boundary_work","work_units":self.work_units(),"work_per_result_row":self.work_per_result_row(),"measurements":self})
    }
    pub(crate) fn add_work(&mut self, other: &Self) {
        self.sql_executions = self.sql_executions.saturating_add(other.sql_executions);
        self.sql_output_rows = self.sql_output_rows.saturating_add(other.sql_output_rows);
        self.sql_output_bytes = self.sql_output_bytes.saturating_add(other.sql_output_bytes);
        self.native_input_rows = self
            .native_input_rows
            .saturating_add(other.native_input_rows);
        self.native_output_rows = self
            .native_output_rows
            .saturating_add(other.native_output_rows);
        self.native_kernel_calls = self
            .native_kernel_calls
            .saturating_add(other.native_kernel_calls);
        self.native_source_queries = self
            .native_source_queries
            .saturating_add(other.native_source_queries);
        self.native_source_rows = self
            .native_source_rows
            .saturating_add(other.native_source_rows);
    }
}
