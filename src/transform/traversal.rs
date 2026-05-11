use crate::{
    LogicalGraphPlan, Result,
    extension::{graph_node, logical_graph_extension_with_schema},
};

pub(crate) fn map_graph_inputs(
    plan: LogicalGraphPlan,
    f: fn(LogicalGraphPlan) -> Result<LogicalGraphPlan>,
) -> Result<LogicalGraphPlan> {
    let Some(node) = graph_node(&plan) else {
        return Ok(plan);
    };
    let inputs = node
        .inputs
        .iter()
        .cloned()
        .map(f)
        .collect::<Result<Vec<_>>>()?;
    Ok(logical_graph_extension_with_schema(
        node.node.clone(),
        inputs,
        node.schema.clone(),
    ))
}
