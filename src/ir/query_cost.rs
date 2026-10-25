//! Measured query work for ranking SQL-lowering opportunities.
//!
//! Engine work and Orchid boundary work are disjoint components of a versioned
//! ranking heuristic. CPU, latency and memory remain separate measurements.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct QueryCost {
    pub engine: EngineCost,
    pub source_engine: EngineCost,
    pub write_engine: EngineCost,
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
    /// Observed execution duration; excluded from the work score.
    pub elapsed_micros: u64,
}
impl QueryCost {
    /// Versioned heuristic: one unit per row visit or KiB transferred, plus
    /// 100 units per database request. The request weight flags N+1 access.
    pub const SCORE_VERSION: u32 = 2;
    pub fn work_units(&self) -> u64 {
        self.engine_work_units()
            .saturating_add(self.orchid_work_units())
    }
    pub fn engine_work_units(&self) -> u64 {
        self.engine
            .work_units()
            .saturating_add(self.source_engine.work_units())
            .saturating_add(self.write_engine.work_units())
    }
    pub fn orchid_work_units(&self) -> u64 {
        self.sql_output_rows
            .saturating_add(self.native_input_rows)
            .saturating_add(self.native_output_rows)
            .saturating_add(self.native_source_rows)
            .saturating_add(self.sql_output_bytes.div_ceil(1024))
            .saturating_add(
                self.sql_executions
                    .saturating_add(
                        self.native_source_queries
                            .max(self.source_engine.statements),
                    )
                    .saturating_add(self.write_engine.statements)
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
        serde_json::json!({"metric_version":Self::SCORE_VERSION,
            "coverage":if self.engine.profiled_statements==self.sql_executions && self.native_source_queries.max(self.source_engine.statements)==self.source_engine.profiled_statements && self.write_engine.statements==self.write_engine.profiled_statements {"engine_and_boundary_work"} else {"partial_engine_and_boundary_work"},
            "work_units":self.work_units(),"orchid_work_units":self.orchid_work_units(),"engine_work_units":self.engine_work_units(),
            "unprofiled_sql_executions":self.sql_executions.saturating_sub(self.engine.profiled_statements),
            "unprofiled_source_queries":self.native_source_queries.max(self.source_engine.statements).saturating_sub(self.source_engine.profiled_statements),
            "unprofiled_write_queries":self.write_engine.statements.saturating_sub(self.write_engine.profiled_statements),
            "work_per_result_row":self.work_per_result_row(),"measurements":self})
    }
    pub(crate) fn add_work(&mut self, other: &Self) {
        self.engine.merge(&other.engine);
        self.source_engine.merge(&other.source_engine);
        self.write_engine.merge(&other.write_engine);
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

/// Actual engine operator work. CPU includes SQL UDFs and is not added again to
/// Orchid's elapsed time. Operator output cardinalities count visits, not final
/// result rows. Peak memory is a maximum, never a sum of overlapping peaks.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EngineCost {
    pub statements: u64,
    pub profiled_statements: u64,
    pub rows_scanned: u64,
    pub operator_rows: u64,
    pub cpu_nanos: u64,
    pub latency_nanos: u64,
    pub peak_memory_bytes: u64,
    pub peak_spill_bytes: u64,
    pub sql_bytes: u64,
    pub operators: u64,
    pub max_plan_depth: u64,
}
impl EngineCost {
    pub fn work_units(&self) -> u64 {
        self.rows_scanned.saturating_add(self.operator_rows)
    }
    pub fn merge(&mut self, other: &Self) {
        self.statements = self.statements.saturating_add(other.statements);
        self.profiled_statements = self
            .profiled_statements
            .saturating_add(other.profiled_statements);
        self.rows_scanned = self.rows_scanned.saturating_add(other.rows_scanned);
        self.operator_rows = self.operator_rows.saturating_add(other.operator_rows);
        self.cpu_nanos = self.cpu_nanos.saturating_add(other.cpu_nanos);
        self.latency_nanos = self.latency_nanos.saturating_add(other.latency_nanos);
        self.sql_bytes = self.sql_bytes.saturating_add(other.sql_bytes);
        self.operators = self.operators.saturating_add(other.operators);
        self.peak_memory_bytes = self.peak_memory_bytes.max(other.peak_memory_bytes);
        self.peak_spill_bytes = self.peak_spill_bytes.max(other.peak_spill_bytes);
        self.max_plan_depth = self.max_plan_depth.max(other.max_plan_depth);
    }
}
