//! Logical graph plan rewrite infrastructure.
//!
//! Transform passes are intentionally small and named so graph-specific
//! normalization can grow by adding files instead of widening the planner.

mod pass;
mod pipeline;
pub mod rules;
mod traversal;

use crate::{LogicalGraphPlan, Result};

pub use pass::{
    LogicalGraphPlanTransform, NamedTransformPass, TransformPass, named_transform_pass,
};
pub use pipeline::{
    TransformOutcome, TransformPassReport, TransformPipeline, TransformReport,
    standard_normalization_pipeline,
};
pub use rules::{combine_adjacent_filters, desugar_bidirectional_expands, remove_empty_projects};

pub fn normalize_logical_graph_plan(plan: LogicalGraphPlan) -> Result<LogicalGraphPlan> {
    standard_normalization_pipeline().transform_plan(plan)
}
