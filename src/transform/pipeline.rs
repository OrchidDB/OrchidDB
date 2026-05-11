use crate::{LogicalGraphPlan, Result};

use super::{
    LogicalGraphPlanTransform, TransformPass, named_transform_pass,
    rules::{combine_adjacent_filters, desugar_bidirectional_expands, remove_empty_projects},
};

#[derive(Debug, Clone)]
pub struct TransformOutcome {
    pub plan: LogicalGraphPlan,
    pub report: TransformReport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransformReport {
    pub passes: Vec<TransformPassReport>,
}

impl TransformReport {
    pub fn changed_passes(&self) -> impl Iterator<Item = &TransformPassReport> {
        self.passes.iter().filter(|pass| pass.changed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransformPassReport {
    pub name: &'static str,
    pub changed: bool,
}

#[derive(Default)]
pub struct TransformPipeline {
    passes: Vec<Box<dyn TransformPass + Send + Sync>>,
}

impl TransformPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pass<P>(mut self, pass: P) -> Self
    where
        P: TransformPass + Send + Sync + 'static,
    {
        self.push_pass(pass);
        self
    }

    pub fn push_pass<P>(&mut self, pass: P)
    where
        P: TransformPass + Send + Sync + 'static,
    {
        self.passes.push(Box::new(pass));
    }

    pub fn run(&self, mut plan: LogicalGraphPlan) -> Result<TransformOutcome> {
        let mut report = TransformReport::default();
        for pass in &self.passes {
            let before = format!("{plan:?}");
            plan = pass.apply(plan)?;
            let after = format!("{plan:?}");
            report.passes.push(TransformPassReport {
                name: pass.name(),
                changed: before != after,
            });
        }
        Ok(TransformOutcome { plan, report })
    }

    pub fn transform_plan(&self, plan: LogicalGraphPlan) -> Result<LogicalGraphPlan> {
        Ok(self.run(plan)?.plan)
    }
}

impl LogicalGraphPlanTransform for TransformPipeline {
    fn transform(&self, plan: LogicalGraphPlan) -> Result<LogicalGraphPlan> {
        self.transform_plan(plan)
    }
}

pub fn standard_normalization_pipeline() -> TransformPipeline {
    TransformPipeline::new()
        .with_pass(named_transform_pass(
            "desugar_bidirectional_expands",
            desugar_bidirectional_expands,
        ))
        .with_pass(named_transform_pass(
            "combine_adjacent_filters",
            combine_adjacent_filters,
        ))
        .with_pass(named_transform_pass(
            "remove_empty_projects",
            remove_empty_projects,
        ))
}
