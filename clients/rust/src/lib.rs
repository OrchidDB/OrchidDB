//! SQL compilation and Arrow execution using application-owned sessions.
//! No database driver is included in this crate's normal dependency graph.
pub use arrow;
pub use arrow::record_batch::{RecordBatch, RecordBatchReader};
pub use orchiddb::compiler;
pub use orchiddb::compiler::{
    Authorization, CompileRequest, CompiledSql, Node, PermissionRelation, PermissionScope, compile,
    compile_json,
};
pub use orchiddb::execution::{ExecutionError, SqlDialect, SqlSession, execute};

pub mod statistics;
pub use statistics::Statistics;

/// SQL island planning and execution across application-owned sessions.
pub use orchiddb::federation;
pub use orchiddb::operations;
#[cfg(any(feature = "quickwit", feature = "elasticsearch"))]
pub use orchiddb::remote;
