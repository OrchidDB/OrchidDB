use crate::{LogicalGraphPlan, Result};

pub trait LogicalGraphPlanTransform {
    fn transform(&self, plan: LogicalGraphPlan) -> Result<LogicalGraphPlan>;
}

impl<F> LogicalGraphPlanTransform for F
where
    F: Fn(LogicalGraphPlan) -> Result<LogicalGraphPlan>,
{
    fn transform(&self, plan: LogicalGraphPlan) -> Result<LogicalGraphPlan> {
        self(plan)
    }
}

pub trait TransformPass {
    fn name(&self) -> &'static str;

    fn apply(&self, plan: LogicalGraphPlan) -> Result<LogicalGraphPlan>;
}

#[derive(Clone, Copy)]
pub struct NamedTransformPass {
    name: &'static str,
    transform: fn(LogicalGraphPlan) -> Result<LogicalGraphPlan>,
}

impl NamedTransformPass {
    pub fn new(
        name: &'static str,
        transform: fn(LogicalGraphPlan) -> Result<LogicalGraphPlan>,
    ) -> Self {
        Self { name, transform }
    }
}

impl TransformPass for NamedTransformPass {
    fn name(&self) -> &'static str {
        self.name
    }

    fn apply(&self, plan: LogicalGraphPlan) -> Result<LogicalGraphPlan> {
        (self.transform)(plan)
    }
}

impl LogicalGraphPlanTransform for NamedTransformPass {
    fn transform(&self, plan: LogicalGraphPlan) -> Result<LogicalGraphPlan> {
        self.apply(plan)
    }
}

pub fn named_transform_pass(
    name: &'static str,
    transform: fn(LogicalGraphPlan) -> Result<LogicalGraphPlan>,
) -> NamedTransformPass {
    NamedTransformPass::new(name, transform)
}
