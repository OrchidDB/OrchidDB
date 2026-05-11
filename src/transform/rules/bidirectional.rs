use crate::{
    Direction, LogicalGraphPlan, LogicalGraphPlanBuilder, Result,
    extension::{GraphNode, graph_node, logical_graph_extension_with_schema},
    transform::traversal::map_graph_inputs,
};

pub fn desugar_bidirectional_expands(plan: LogicalGraphPlan) -> Result<LogicalGraphPlan> {
    let Some(node) = graph_node(&plan) else {
        return Ok(plan);
    };
    match &node.node {
        GraphNode::Expand {
            source_binding,
            edge_binding,
            target_binding,
            direction: Direction::Both,
            edge,
            target,
        } => {
            let input = desugar_bidirectional_expands(node.inputs[0].clone())?;
            let out = logical_graph_extension_with_schema(
                GraphNode::Expand {
                    source_binding: source_binding.clone(),
                    edge_binding: edge_binding.clone(),
                    target_binding: target_binding.clone(),
                    direction: Direction::Out,
                    edge: edge.clone(),
                    target: target.clone(),
                },
                vec![input.clone()],
                node.schema.clone(),
            );
            let in_ = logical_graph_extension_with_schema(
                GraphNode::Expand {
                    source_binding: source_binding.clone(),
                    edge_binding: edge_binding.clone(),
                    target_binding: target_binding.clone(),
                    direction: Direction::In,
                    edge: edge.clone(),
                    target: target.clone(),
                },
                vec![input],
                node.schema.clone(),
            );
            LogicalGraphPlanBuilder::union_all(vec![out, in_])
        }
        GraphNode::VarExpand {
            source_binding,
            edge_binding,
            target_binding,
            path_binding,
            direction: Direction::Both,
            edge,
            target,
            min_depth,
            max_depth,
            simple_path,
        } => {
            let input = desugar_bidirectional_expands(node.inputs[0].clone())?;
            let out = logical_graph_extension_with_schema(
                GraphNode::VarExpand {
                    source_binding: source_binding.clone(),
                    edge_binding: edge_binding.clone(),
                    target_binding: target_binding.clone(),
                    path_binding: path_binding.clone(),
                    direction: Direction::Out,
                    edge: edge.clone(),
                    target: target.clone(),
                    min_depth: *min_depth,
                    max_depth: *max_depth,
                    simple_path: *simple_path,
                },
                vec![input.clone()],
                node.schema.clone(),
            );
            let in_ = logical_graph_extension_with_schema(
                GraphNode::VarExpand {
                    source_binding: source_binding.clone(),
                    edge_binding: edge_binding.clone(),
                    target_binding: target_binding.clone(),
                    path_binding: path_binding.clone(),
                    direction: Direction::In,
                    edge: edge.clone(),
                    target: target.clone(),
                    min_depth: *min_depth,
                    max_depth: *max_depth,
                    simple_path: *simple_path,
                },
                vec![input],
                node.schema.clone(),
            );
            LogicalGraphPlanBuilder::union_all(vec![out, in_])
        }
        _ => map_graph_inputs(plan, desugar_bidirectional_expands),
    }
}
