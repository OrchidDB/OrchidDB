//! Cypher → Graph IR planner.
//!
//! Wraps `crate::ir::bridge::cypher::lower_query` with a small façade so
//! callers can hold a `CypherPlanner` value (matching the pattern used by
//! the legacy SQL-island planner under `src/language/cypher/planner/`).

use crate::ir::bridge::cypher::{self as bridge, CypherQuery};
use crate::ir::plan::GraphPlan;

/// Cypher planner that emits Graph IR.
#[derive(Debug, Clone, Copy, Default)]
pub struct CypherPlanner;

impl CypherPlanner {
    pub fn new() -> Self {
        Self
    }

    /// Lower a parsed Cypher query into a `GraphPlan`.
    pub fn plan(&self, query: &CypherQuery) -> GraphPlan {
        bridge::lower_query(query)
    }
}
