//! Gremlin → Graph IR planner.
//!
//! This planner replaces the previous DataFusion / DuckDB SQL-island
//! lowering. It walks a parsed `Traversal` and emits a `GraphPlan` whose
//! operators are the Graph IR nodes defined in
//! `docs/graph_ir_language_examples_v0_2_draft.md` (`GraphNodeScan`,
//! `GraphExpand`, `GraphFilter`, `GraphProject`, `GraphCurrentProject`,
//! `GraphAggregate`, `GraphSort`, `GraphSlice`, `GraphDistinct`,
//! `GraphApply`, `GraphUnion`, `GraphCoalesce`, `GraphChoose`,
//! `GraphSelect`, `GraphRepeat`, `GraphPathFilter`, …).
//!
//! No Apache DataFusion or DuckDB types appear here. Downstream code can
//! either run the resulting `GraphPlan` directly through
//! `crate::ir::interpreter::execute` or convert it to a DataFusion
//! `LogicalPlan::Extension` tree via `crate::ir::df::to_logical_plan`.

mod error;
mod lowering;

pub use error::{GremlinPlanError, GremlinPlanResult};
pub use lowering::lower_traversal;

use crate::ir::plan::GraphPlan;

use super::ast::{GremlinProgram, Traversal};

#[derive(Debug, Clone, Copy, Default)]
pub struct GremlinPlanner;

impl GremlinPlanner {
    pub fn new() -> Self {
        Self
    }

    /// Lower a parsed Gremlin program (carrying a lowered `Traversal`)
    /// into a `GraphPlan`.
    pub fn plan_program(&self, program: &GremlinProgram) -> GremlinPlanResult<GraphPlan> {
        let traversal = program.traversal.as_ref().ok_or_else(|| {
            GremlinPlanError::Plan(
                "Gremlin program has not been lowered to a traversal".to_string(),
            )
        })?;
        self.plan(traversal)
    }

    /// Lower a `Traversal` into a `GraphPlan`.
    pub fn plan(&self, traversal: &Traversal) -> GremlinPlanResult<GraphPlan> {
        lower_traversal(traversal)
    }
}
