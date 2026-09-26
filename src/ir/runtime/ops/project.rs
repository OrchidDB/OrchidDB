//! GraphProject and GraphCurrentProject.

use std::collections::BTreeMap;

use crate::ir::catalog::PropertyGraph;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{ProjectMode, ProjectionItem};
use crate::ir::policy::PropertyMissing;
use crate::ir::value::Value;

use super::super::expr::eval;
use super::super::{IrResult, Row};

pub(crate) fn project_op(
    mode: ProjectMode,
    items: &[ProjectionItem],
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let mut new_row = match mode {
            ProjectMode::PreserveVisible | ProjectMode::ReplaceCurrent => row.clone(),
            ProjectMode::ReplaceScope => Row {
                bindings: BTreeMap::new(),
                bulk: row.bulk,
            },
        };
        for item in items {
            let value = eval(&item.expr, &row, graph)?;
            new_row.bindings.insert(item.alias.clone(), value);
        }
        out.push(new_row);
    }
    Ok(out)
}

pub(crate) fn current_project_op(
    expr: &IrExpr,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let mut out = Vec::new();
    for mut row in rows {
        let direct_property = match expr {
            IrExpr::Property {
                binding,
                name,
                policy: PropertyMissing::DropUnproductive,
            } => row.bindings.get(binding).map(|owner| (owner, name)),
            _ => None,
        };
        // ValueTraversal uses Map.get(), whose missing key is a productive
        // null. Element.property(), by contrast, distinguishes absence from
        // a present null property. Keep Gremlin's case-sensitive map lookup
        // separate from the generic expression evaluator's struct lookup.
        let map_value = match direct_property {
            Some((Value::Map(map), name)) => Some(map.get(name).cloned().unwrap_or(Value::Null)),
            Some((Value::TypedMap(entries), name)) => Some(
                entries
                    .iter()
                    .find_map(|(key, value)| {
                        matches!(key, Value::String(key) if key == name).then(|| value.clone())
                    })
                    .unwrap_or(Value::Null),
            ),
            _ => None,
        };
        let value = match &map_value {
            Some(value) => value.clone(),
            None => eval(expr, &row, graph)?,
        };
        if matches!(value, Value::Null) {
            let productive_null = map_value.is_some()
                || direct_property.is_some_and(|(owner, name)| {
                    !graph
                        .properties(owner, std::slice::from_ref(name))
                        .is_empty()
                });
            if !productive_null {
                continue;
            }
        }
        row.bindings.insert("current".to_string(), value);
        out.push(row);
    }
    Ok(out)
}
