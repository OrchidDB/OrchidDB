//! Diagnostic execution contract for an already validated graph plan.
use orchiddb::ir::{plan::{GraphPlan, Node, ProcedureMode}, procedures::ProcedureCatalog};

/// Only immutable registered table procedures can be certified as no-ops.
/// Require the exact standalone, argument-free, void shape: no upstream rows,
/// predicates, side effects, or argument evaluation may be silently discarded.
pub fn is_noop(plan: &GraphPlan, catalog: &ProcedureCatalog) -> bool {
    let Node::GraphReturn { fields, input, .. } = plan.root.as_ref() else { return false };
    if !fields.is_empty() { return false; }
    let Node::GraphProcedureCall {
        name, args, yields, mode: ProcedureMode::Read, input: None,
    } = input.as_ref() else { return false };
    args.is_empty() && yields.is_empty() && catalog.get(name).is_some_and(|procedure| {
        procedure.signature.inputs.is_empty() && procedure.signature.outputs.is_empty()
    })
}

pub fn output() -> serde_json::Value {
    serde_json::json!({
        "execution_kind": "no_op", "sql": null,
        "requires_sql_execution": false, "result_columns": [], "result_rows": [],
        "regions": [], "kernels": []
    })
}
