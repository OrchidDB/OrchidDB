//! Graph mutation operators.

use std::collections::BTreeMap;

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{CreateEdge, CreateNode, Node, SetMode, SetPropertyItem};
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::run::ExecutionContext;
use super::repeat::run_with_outer;
use super::super::{InterpretError, IrResult, Row};

pub(crate) fn create_op(
    nodes: &[CreateNode],
    edges: &[CreateEdge],
    upstream: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::with_capacity(upstream.len());
    for row in upstream {
        let mut row = row;
        for node in nodes {
            let properties = match &node.properties {
                Some(expr) => map_value(eval(expr, &row, graph)?)?,
                None => BTreeMap::new(),
            };
            let value = graph.insert_node(node.label.clone(), properties);
            if let Some(bind) = &node.bind {
                row.bindings.insert(bind.clone(), value);
            }
        }
        // Endpoints resolve against the row *after* every node in this
        // clause exists, so `CREATE (a:A)-[:R]->(b:B)` binds both ends.
        for edge in edges {
            let properties = match &edge.properties {
                Some(expr) => map_value(eval(expr, &row, graph)?)?,
                None => BTreeMap::new(),
            };
            let src = endpoint(&row, &edge.src, &edge.rel_type)?;
            let dst = endpoint(&row, &edge.dst, &edge.rel_type)?;
            let value = graph
                .insert_edge(edge.rel_type.clone(), &src, &dst, properties)
                .map_err(|err| InterpretError::Type(err.to_string()))?;
            if let Some(bind) = &edge.bind {
                row.bindings.insert(bind.clone(), value);
            }
        }
        out.push(row);
    }
    Ok(out)
}

/// `GraphMerge`: match-or-create, per input row.
pub(crate) fn merge_op(
    outputs: &[String],
    upstream: Vec<Row>,
    match_arm: &Node,
    create_arm: &Node,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::new();
    for outer_row in upstream {
        ctx.charge(1)?;
        let mut rows = run_with_outer(match_arm, &outer_row, graph, ctx)?;
        if rows.is_empty() {
            rows = run_with_outer(create_arm, &outer_row, graph, ctx)?;
        }
        ctx.charge(rows.len() as u64)?;
        for inner in rows {
            let mut row = outer_row.clone();
            for output in outputs {
                if let Some(value) = inner.bindings.get(output) {
                    row.bindings.insert(output.clone(), value.clone());
                }
            }
            out.push(row);
        }
    }
    Ok(out)
}

fn endpoint(row: &Row, binding: &str, rel_type: &str) -> IrResult<Value> {
    row.bindings.get(binding).cloned().ok_or_else(|| {
        InterpretError::Type(format!(
            "CREATE relationship `{rel_type}` endpoint `{binding}` is not bound"
        ))
    })
}

pub(crate) fn set_property_op(
    items: &[SetPropertyItem],
    upstream: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    for row in &upstream {
        for item in items {
            let target = eval(&item.target, row, graph)?;
            let value = eval(&item.value, row, graph)?;
            match item.mode {
                SetMode::Property => {
                    graph.set_property(&target, item.key.clone(), value)?;
                }
                SetMode::Replace | SetMode::Merge => {
                    let properties = map_value(value)?;
                    graph.set_properties(
                        &target,
                        properties,
                        matches!(item.mode, SetMode::Replace),
                    )?;
                }
            }
        }
    }
    Ok(upstream)
}

pub(crate) fn delete_op(
    targets: &[IrExpr],
    detach: bool,
    upstream: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    for row in &upstream {
        for target in targets {
            let value = eval(target, row, graph)?;
            graph.delete_value(&value, detach)?;
        }
    }
    Ok(upstream)
}

fn map_value(value: Value) -> IrResult<BTreeMap<String, Value>> {
    match value {
        Value::Map(map) => Ok(map),
        Value::Null => Ok(BTreeMap::new()),
        other => Err(InterpretError::Type(format!(
            "expected a map of properties, got {}",
            other.type_name()
        ))),
    }
}
