//! Pure table-procedure specialization. This is deliberately narrower than a
//! general read-procedure capability: only successful, primitive constant
//! arguments may select rows from a registered immutable table snapshot.
use super::*;
use crate::ir::plan::ProcedureMode;

/// Specialize eligible calls before capability validation. Calls left intact
/// must still pass the normal capability check (or execute as native kernels).
pub(super) fn specialize_plan(plan: &GraphPlan, graph: &PropertyGraph) -> GraphPlan {
    let mut result = plan.clone();
    specialize_node(&mut result.root, graph);
    result
}

fn specialize_node(node: &mut Node, graph: &PropertyGraph) {
    for child in crate::ir::analysis::children_mut(node) {
        specialize_node(child, graph);
    }
    if let Some(replacement) = specialize_call(node, graph) {
        *node = replacement;
    }
}

fn primitive(value: &Value) -> bool {
    matches!(value, Value::Null | Value::Bool(_) | Value::Int(_) | Value::Long(_)
        | Value::Float(_) | Value::String(_))
}

// Total, deterministic source algebra only: an empty joined procedure table
// is allowed to suppress its evaluation without hiding an error or effect.
fn total_source(node: &Node) -> bool {
    fn scalar(expr: &IrExpr) -> bool {
        matches!(expr, IrExpr::Binding(_) | IrExpr::Lit(_))
    }
    match node {
        Node::GraphOneRow | Node::GraphEmpty => true,
        Node::GraphValues { rows, bulk: None, .. } => rows.iter().flatten().all(primitive),
        Node::GraphProject { items, input, .. } => items.iter().all(|item| scalar(&item.expr)) && total_source(input),
        Node::GraphSort { keys, input } => keys.iter().all(|key| scalar(&key.expr)) && total_source(input),
        Node::GraphFilter { condition: IrExpr::Lit(Lit::Bool(_) | Lit::Null), input } => total_source(input),
        Node::GraphUnwind { input_expr: IrExpr::List(items), input, .. } =>
            items.iter().all(|item| matches!(item, IrExpr::Lit(_))) && total_source(input),
        Node::GraphAggregate { group, aggs, input, .. } =>
            group.iter().all(|item| scalar(&item.expr)) && aggs.iter().all(|agg| agg.kind == AggKind::CountRows && !agg.distinct
                && agg.arg.as_ref().is_none_or(|arg| matches!(arg, IrExpr::Binding(_))))
                && total_source(input),
        Node::GraphJoin { kind: crate::ir::plan::JoinKind::Cross, condition: None, left, right } =>
            total_source(left) && total_source(right),
        _ => false,
    }
}

// Prove cardinality structurally; a LIMIT 1 is only at-most-one and is not
// sufficient. An ungrouped aggregate emits one row even on an empty input.
fn singleton_fields(node: &Node) -> Option<Vec<String>> {
    match node {
        Node::GraphOneRow => Some(Vec::new()),
        Node::GraphAggregate { group, fields, .. } if group.is_empty() && total_source(node) => Some(fields.clone()),
        Node::GraphProject { mode, items, input, .. } => {
            // A join may suppress an empty table's outer evaluation. Keep
            // volatile or throwing projected expressions at their native call
            // boundary instead of moving them across that evaluation phase.
            if !items.iter().all(|item| matches!(item.expr, IrExpr::Binding(_) | IrExpr::Lit(_))) { return None; }
            let upstream = singleton_fields(input)?;
            match mode {
                crate::ir::plan::ProjectMode::ReplaceScope => Some(items.iter().map(|item| item.alias.clone()).collect()),
                crate::ir::plan::ProjectMode::PreserveVisible => {
                    let mut fields = upstream;
                    for item in items { if !fields.contains(&item.alias) { fields.push(item.alias.clone()); } }
                    Some(fields)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) fn specialize_call(node: &Node, graph: &PropertyGraph) -> Option<Node> {
    let Node::GraphProcedureCall { name, args, yields, mode: ProcedureMode::Read, input } = node else {
        return None;
    };
    let procedure = graph.procedures.get(name)?;
    if args.len() != procedure.signature.inputs.len() {
        return None;
    }
    // Do not evaluate clocks, callbacks, throwing expressions, or arguments
    // depending on an upstream row. Failed validation stays at runtime so an
    // empty upstream does not acquire a spurious compile-time exception.
    let values = args.iter().map(|arg| {
        match &arg.value {
            IrExpr::Lit(value) => Some(lit_to_value(value)),
            _ => None,
        }
    }).collect::<Option<Vec<_>>>()?;
    if values.iter().zip(&procedure.signature.inputs)
        .any(|(value, field)| !primitive(value) || !field.accepts(value)) {
        return None;
    }
    if procedure.signature.outputs.is_empty() {
        return Some(input.as_deref().cloned().unwrap_or(Node::GraphOneRow));
    }
    // With exactly one outer row there is no outer-order dimension. Sort
    // after the join, carrying the table's registration ordinal explicitly.
    let upstream_fields = input.as_deref().map(singleton_fields).unwrap_or(Some(Vec::new()))?;
    if yields.iter().any(|name| upstream_fields.contains(name)) { return None; }
    let selected = yields.iter().map(|name| {
        procedure.signature.outputs.iter().position(|field| &field.name == name)
    }).collect::<Option<Vec<_>>>()?;
    if selected.is_empty() { return None; }
    // Homogeneous primitive output columns only. Never silently coerce NUMBER
    // mixtures to DOUBLE, including integers outside its exact range.
    for index in &selected {
        let mut kind = None;
        for row in &procedure.rows {
            let value = row.get(values.len() + index)?;
            if !primitive(value) { return None; }
            if matches!(value, Value::Null) { continue; }
            let next = match value {
                Value::Int(_) | Value::Long(_) => 0,
                Value::Float(_) => 1,
                Value::Bool(_) => 2,
                Value::String(_) => 3,
                _ => unreachable!(),
            };
            if kind.is_some_and(|previous| previous != next) { return None; }
            kind = Some(next);
        }
    }
    let rows: Vec<Vec<Value>> = procedure.rows.iter().filter(|row| {
        values.iter().zip(*row).all(|(a,b)|
            (matches!(a,Value::Null) && matches!(b,Value::Null))
            || a.three_valued_eq(b) == Some(true))
    }).map(|row| selected.iter().map(|index| row[values.len()+index].clone()).collect())
      .collect();
    let mut output_fields = upstream_fields;
    output_fields.extend(yields.iter().cloned());
    let mut ordinal = String::from("__procedure_registration_order");
    while output_fields.contains(&ordinal) { ordinal.push('_'); }
    let mut bindings = yields.clone();
    bindings.push(ordinal.clone());
    let rows = rows.into_iter().enumerate().map(|(i, mut row)| {
        row.push(Value::Int(i as i64)); row
    }).collect();
    let mut table = Node::GraphValues { bindings, rows, bulk: None };
    if let Some(input) = input.as_deref().filter(|input| !matches!(input, Node::GraphOneRow)) {
        table = Node::GraphJoin {
            kind: crate::ir::plan::JoinKind::Cross,
            left: input.clone().boxed(), right: table.boxed(), condition: None,
        };
    }
    Some(Node::GraphProject {
        mode: crate::ir::plan::ProjectMode::ReplaceScope,
        items: output_fields.into_iter().map(|name| crate::ir::plan::ProjectionItem {
            alias: name.clone(), expr: IrExpr::Binding(name),
        }).collect(),
        error_policy: crate::ir::plan::ProjectErrorPolicy::PropagateError,
        input: Node::GraphSort {
            keys: vec![crate::ir::plan::SortKey {
                expr: IrExpr::Binding(ordinal),
                dir: crate::ir::plan::SortDir::Asc,
                nulls: crate::ir::plan::NullsOrder::Last,
            }],
            input: table.boxed(),
        }.boxed(),
    })
}
