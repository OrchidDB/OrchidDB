use crate::{
    LogicalGraphPlan, Result,
    extension::{GraphNode, graph_node},
    transform::traversal::map_graph_inputs,
};

pub fn remove_empty_projects(plan: LogicalGraphPlan) -> Result<LogicalGraphPlan> {
    let Some(node) = graph_node(&plan) else {
        return Ok(plan);
    };
    match &node.node {
        GraphNode::Project { exprs } if exprs.is_empty() => {
            remove_empty_projects(node.inputs[0].clone())
        }
        _ => map_graph_inputs(plan, remove_empty_projects),
    }
}
