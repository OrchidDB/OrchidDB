//! Catalog reference reads are source access. The consuming projection stays SQL.
use super::*;
use crate::ir::expr::IrExpr;

fn closed_key(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Lit(_) => true,
        IrExpr::List(values) => values.iter().all(closed_key),
        IrExpr::Call { name, args } if matches!(name.as_str(),
            "cast_byte" | "cast_short" | "cast_long" | "cast_bigint" | "cast_float"
            | "cast_bigdecimal" | "datetime_literal" | "map_literal") => args.iter().all(closed_key),
        _ => false,
    }
}

fn source_argument(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Binding(_) => true,
        IrExpr::List(values) => values.iter().all(source_argument),
        IrExpr::Call { name, args } if name == "map_literal" => args.iter().all(source_argument),
        _ => closed_key(expr),
    }
}

fn reference_call(name: &str, args: &[IrExpr]) -> bool {
    matches!(name, "gremlin_vertex_ref" | "gremlin_edge_ref" | "gremlin_vertex_property_ref")
        && args.iter().all(|arg| closed_key(arg)
            || matches!(arg, IrExpr::Call { name, args } if name == "gremlin_vertex_ref" && args.iter().all(closed_key)))
}
fn service_call(name: &str, args: &[IrExpr]) -> bool {
    matches!(name, "tinker_search" | "tinker_degree_centrality")
        && args.iter().all(source_argument)
}
fn eager_container(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Lit(_) | IrExpr::Binding(_) => true,
        IrExpr::List(values) => values.iter().all(eager_container),
        IrExpr::Call { name, args } if reference_call(name, args) => true,
        IrExpr::Call { name, args } if matches!(name.as_str(), "map_literal" | "gremlin_token_literal" | "gremlin_direction_literal") => args.iter().all(eager_container),
        _ => false,
    }
}
fn extract(expr: &mut IrExpr, fields: &mut Vec<String>, outputs: &[String], reads: &mut Vec<(String, IrExpr)>) {
    if matches!(expr, IrExpr::Call { name, args } if reference_call(name,args) || service_call(name,args)) {
        let mut suffix = reads.len();
        let name = loop {
            let name = format!("__orchid_element_reference_{suffix}");
            if !fields.contains(&name) && !outputs.contains(&name) { break name; }
            suffix += 1;
        };
        fields.push(name.clone());
        reads.push((name.clone(), std::mem::replace(expr, IrExpr::Binding(name))));
    } else {
        match expr {
            IrExpr::List(values) | IrExpr::Call { args: values, .. } => {
                for value in values { extract(value, fields, outputs, reads); }
            }
            _ => {}
        }
    }
}

/// Read catalog references and graph services as sources. Eager literal maps
/// and lists may contain references, but conditional/arbitrary expressions
/// never have reads hoisted from them. SQL still constructs their containers.
pub(super) fn lift(
    node: &mut Node,
    input: LogicalPlan,
    fields: &mut Vec<String>,
) -> (LogicalPlan, Vec<String>) {
    let eligible = |expr: &IrExpr| eager_container(expr)
        || matches!(expr, IrExpr::Call { name, args } if service_call(name,args));
    let (expressions, outputs): (Vec<&mut IrExpr>, Vec<String>) = match node {
        Node::GraphProject { items, error_policy: crate::ir::plan::ProjectErrorPolicy::PropagateError, .. }
            if items.iter().all(|item| eligible(&item.expr)) => {
                let outputs = items.iter().map(|item| item.alias.clone()).collect();
                (items.iter_mut().map(|item| &mut item.expr).collect(), outputs)
            }
        Node::GraphCurrentProject { expr, fields, .. } if eligible(expr) => (vec![expr], fields.clone()),
        Node::GraphUnwind { input_expr, bind, .. } if eager_container(input_expr) => (vec![input_expr], vec![bind.clone()]),
        _ => return (input, Vec::new()),
    };
    let mut reads = Vec::new();
    for expr in expressions { extract(expr, fields, &outputs, &mut reads); }
    if reads.is_empty() { return (input, Vec::new()); }
    let hidden = reads.iter().map(|(name, _)| name.clone()).collect();
    let LogicalPlan::Extension(extension) = kernel("ElementReferenceSource", vec![input], move |mut inputs, state| {
        let mut rows = inputs.remove(0);
        // No reads on an empty stream. Each row retains its bulk and traversal
        // state, while reference errors remain errors instead of missing rows.
        for row in &mut rows {
            for (name, reference) in &reads {
                let value = eval(reference, row, &state.graph)?;
                row.bindings.insert(name.clone(), value);
            }
        }
        Ok(rows)
    }) else { unreachable!() };
    let mut source = extension.node.as_any().downcast_ref::<RowKernel>().unwrap().clone();
    source.boundary = Some(policy::BoundaryKind::GraphSource);
    source.streaming = true;
    (LogicalPlan::Extension(Extension { node: Arc::new(source) }), hidden)
}
