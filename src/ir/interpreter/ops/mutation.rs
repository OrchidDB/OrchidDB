//! Graph mutation operators.

use std::collections::BTreeMap;

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{CreateNode, SetPropertyItem};
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{InterpretError, IrResult, Row};

pub(crate) fn create_op(
    nodes: &[CreateNode],
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
        out.push(row);
    }
    Ok(out)
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
            graph.set_property(&target, item.key.clone(), value)?;
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
            "CREATE properties must be a map, got {}",
            other.type_name()
        ))),
    }
}
