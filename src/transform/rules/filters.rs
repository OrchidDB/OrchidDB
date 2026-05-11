use crate::{
    LogicalGraphPlan, Result,
    extension::{GraphNode, graph_node, logical_graph_extension_with_schema},
    transform::traversal::map_graph_inputs,
};

pub fn combine_adjacent_filters(plan: LogicalGraphPlan) -> Result<LogicalGraphPlan> {
    let Some(node) = graph_node(&plan) else {
        return Ok(plan);
    };
    match &node.node {
        GraphNode::Filter { predicate } => {
            let input = combine_adjacent_filters(node.inputs[0].clone())?;
            if let Some(inner) = graph_node(&input)
                && let GraphNode::Filter {
                    predicate: inner_predicate,
                } = &inner.node
            {
                return Ok(logical_graph_extension_with_schema(
                    GraphNode::Filter {
                        predicate: inner_predicate.clone().and(predicate.clone()),
                    },
                    vec![inner.inputs[0].clone()],
                    node.schema.clone(),
                ));
            }
            Ok(logical_graph_extension_with_schema(
                node.node.clone(),
                vec![input],
                node.schema.clone(),
            ))
        }
        _ => map_graph_inputs(plan, combine_adjacent_filters),
    }
}
