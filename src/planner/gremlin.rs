//! Gremlin → Graph IR planner.
//!
//! Wraps `crate::ir::bridge::gremlin::lower_traversal`.

use crate::ir::bridge::gremlin::{self as bridge, GremlinTraversal};
use crate::ir::plan::GraphPlan;

#[derive(Debug, Clone, Copy, Default)]
pub struct GremlinPlanner;

impl GremlinPlanner {
    pub fn new() -> Self {
        Self
    }

    pub fn plan(&self, traversal: &GremlinTraversal) -> GraphPlan {
        bridge::lower_traversal(traversal)
    }
}
