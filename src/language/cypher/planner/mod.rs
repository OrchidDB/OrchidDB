pub mod error;
pub mod lowering;

pub use error::{CypherPlanError, CypherPlanResult};
pub use lowering::lower_query;

use crate::ir::plan::GraphPlan;
use crate::language::cypher::ast::{CypherProgram, Query};

#[derive(Debug, Clone, Default)]
pub struct CypherPlanner;

impl CypherPlanner {
    pub fn new() -> Self {
        Self
    }

    pub fn plan_program(&self, program: &CypherProgram) -> CypherPlanResult<GraphPlan> {
        let query = program.query.as_ref().ok_or_else(|| {
            CypherPlanError::Unsupported("syntax-only Cypher program".to_string())
        })?;
        self.plan(query)
    }

    pub fn plan(&self, query: &Query) -> CypherPlanResult<GraphPlan> {
        lower_query(query)
    }
}
