//! Demonstrate that the IR's DataFusion adapter is HEP-compatible:
//!
//! - Each Graph IR operator becomes its own concrete
//!   `UserDefinedLogicalNodeCore`, so HEP rules can downcast to a
//!   specific operator type and rewrite the plan in place.
//! - Plans round-trip via `to_logical_plan` ↔ `from_logical_plan`, so a
//!   HEP-rewritten plan can flow back into the IR interpreter unchanged.
//!
//! These tests stand in for a real HEP rule set: they manually
//! pattern-match and rewrite, then verify the rewritten plan still
//! executes correctly through the interpreter.

use std::sync::Arc;

use arrow::array::{Array, ArrayRef, Int64Array, StringArray};
use datafusion::error::Result as DFResult;
use datafusion::logical_expr::{Extension, LogicalPlan, UserDefinedLogicalNode};

use new_graph::ir::catalog::{PropertyGraph, edges_from_columns, nodes_from_columns};
use new_graph::ir::df::{
    GraphFilter, GraphNodeScan, GraphReturn, downcast_graph_ir, from_logical_plan, to_logical_plan,
};
use new_graph::ir::expr::{BinaryOp, IrExpr};
use new_graph::ir::interpreter::execute;
use new_graph::language::gremlin::ast::{Step, Traversal};
use new_graph::language::gremlin::planner::GremlinPlanner;
use new_graph::language::gremlin::semantics::{GValue, Predicate};

fn fixture() -> PropertyGraph {
    let names: ArrayRef = Arc::new(StringArray::from(vec!["alice", "bob", "carol", "dave"]));
    let ages: ArrayRef = Arc::new(Int64Array::from(vec![21, 35, 41, 19]));
    let person = nodes_from_columns("Person", vec![("name", names), ("age", ages)]);
    let mut graph = PropertyGraph::new();
    graph.add_nodes(person);
    let knows = edges_from_columns(
        "KNOWS",
        "Person",
        "Person",
        vec![0, 1],
        vec![1, 2],
        Vec::new(),
    );
    graph.add_edges(knows).unwrap();
    graph
}

fn plan(steps: Vec<Step>) -> new_graph::ir::plan::GraphPlan {
    GremlinPlanner::new().plan(&Traversal::new(steps)).unwrap()
}

#[test]
fn each_operator_is_its_own_user_defined_logical_node() {
    let p = plan(vec![
        Step::V { ids: Vec::new() },
        Step::HasLabel(vec!["Person".into()]),
    ]);
    let logical = to_logical_plan(&p).unwrap();

    // Walk the tree and check that the operator names match the IR
    // operator they came from.
    let mut names: Vec<String> = Vec::new();
    fn walk(plan: &LogicalPlan, names: &mut Vec<String>) {
        if let LogicalPlan::Extension(ext) = plan {
            names.push(ext.node.name().to_string());
            for child in ext.node.inputs() {
                walk(child, names);
            }
        }
    }
    walk(&logical, &mut names);
    assert_eq!(
        names,
        vec!["GraphReturn", "GraphFilter", "GraphBind", "GraphNodeScan"]
    );
}

#[test]
fn hep_rule_downcasts_and_rewrites_filter() {
    // Build a plan with a tautological filter `current = current` that a
    // simple HEP rule should drop.
    let p = plan(vec![
        Step::V { ids: Vec::new() },
        Step::HasLabel(vec!["Person".into()]),
        Step::Has {
            key: "name".into(),
            predicate: Predicate::eq(GValue::String("alice".into())),
        },
    ]);
    let logical = to_logical_plan(&p).unwrap();

    // Rule: drop a `GraphFilter` whose condition is a literal `true`.
    fn drop_true_filter(plan: &LogicalPlan) -> DFResult<LogicalPlan> {
        if let Some(filter) = downcast_graph_ir::<GraphFilter>(plan) {
            if matches!(
                &filter.condition,
                IrExpr::Lit(new_graph::ir::expr::Lit::Bool(true))
            ) {
                // Replace with the single child.
                let child = filter.inputs[0].clone();
                return drop_true_filter(&child);
            }
        }
        // Recurse into children.
        match plan {
            LogicalPlan::Extension(ext) => {
                let new_inputs: Vec<LogicalPlan> = ext
                    .node
                    .inputs()
                    .into_iter()
                    .map(drop_true_filter)
                    .collect::<DFResult<_>>()?;
                let new_node = ext
                    .node
                    .with_exprs_and_inputs(ext.node.expressions(), new_inputs)?;
                Ok(LogicalPlan::Extension(Extension { node: new_node }))
            }
            other => Ok(other.clone()),
        }
    }

    // Inject a true filter into the plan first.
    let LogicalPlan::Extension(top) = &logical else {
        panic!("not extension");
    };
    let return_node = downcast_graph_ir::<GraphReturn>(&logical).unwrap();
    let inner = return_node.inputs[0].clone();
    let inner_schema = match &inner {
        LogicalPlan::Extension(ext) => ext.node.schema().clone(),
        _ => panic!(),
    };
    let injected = LogicalPlan::Extension(Extension {
        node: Arc::new(GraphFilter {
            condition: IrExpr::lit_bool(true),
            schema: inner_schema,
            inputs: vec![inner],
        }) as Arc<dyn UserDefinedLogicalNode>,
    });
    let with_true = top
        .node
        .with_exprs_and_inputs(top.node.expressions(), vec![injected])
        .unwrap();
    let polluted = LogicalPlan::Extension(Extension { node: with_true });

    // Run the rule and round-trip back to IR.
    let cleaned = drop_true_filter(&polluted).unwrap();
    let rebuilt = from_logical_plan(&cleaned).expect("from logical plan");

    // The rule didn't touch the inner has(name='alice') filter, so the
    // result should still be 'alice' only (1 row).
    let result = execute(&rebuilt, &fixture()).unwrap();
    assert_eq!(result.batch.num_rows(), 1);
}

#[test]
fn round_trip_preserves_plan_semantics() {
    let p = plan(vec![
        Step::V { ids: Vec::new() },
        Step::HasLabel(vec!["Person".into()]),
        Step::Has {
            key: "age".into(),
            predicate: Predicate::Compare {
                op: new_graph::language::gremlin::semantics::CompareOp::Gte,
                value: GValue::Int(30),
            },
        },
        Step::Values(vec!["name".into()]),
    ]);
    let logical = to_logical_plan(&p).unwrap();
    let rebuilt = from_logical_plan(&logical).unwrap();

    let result = execute(&rebuilt, &fixture()).unwrap();
    let array = result
        .batch
        .column(0)
        .as_any()
        .downcast_ref::<StringArray>()
        .unwrap();
    let mut names: Vec<&str> = (0..array.len()).map(|i| array.value(i)).collect();
    names.sort();
    assert_eq!(names, vec!["bob", "carol"]);
}

#[test]
fn downcast_returns_concrete_node_metadata() {
    let p = plan(vec![Step::V { ids: Vec::new() }]);
    let logical = to_logical_plan(&p).unwrap();

    // The very bottom of the plan is a NodeScan; assert we can pull its
    // labels back out via downcast.
    fn find_scan(plan: &LogicalPlan) -> Option<&GraphNodeScan> {
        if let Some(scan) = downcast_graph_ir::<GraphNodeScan>(plan) {
            return Some(scan);
        }
        if let LogicalPlan::Extension(ext) = plan {
            for child in ext.node.inputs() {
                if let Some(s) = find_scan(child) {
                    return Some(s);
                }
            }
        }
        None
    }

    let scan = find_scan(&logical).expect("found GraphNodeScan");
    assert_eq!(scan.binding, "current");
    assert_eq!(scan.graph, "default");
}

#[test]
fn rebuild_via_with_exprs_and_inputs_preserves_metadata() {
    // After substituting children, the operator's IR fields (label
    // expressions, predicates, …) must stay intact. Otherwise HEP rules
    // would have to manually re-stitch every replacement.
    let p = plan(vec![
        Step::V { ids: Vec::new() },
        Step::HasLabel(vec!["Person".into()]),
    ]);
    let logical = to_logical_plan(&p).unwrap();

    fn rewrite(plan: &LogicalPlan) -> DFResult<LogicalPlan> {
        match plan {
            LogicalPlan::Extension(ext) => {
                let new_inputs: Vec<LogicalPlan> = ext
                    .node
                    .inputs()
                    .into_iter()
                    .map(rewrite)
                    .collect::<DFResult<_>>()?;
                let new_node = ext
                    .node
                    .with_exprs_and_inputs(ext.node.expressions(), new_inputs)?;
                Ok(LogicalPlan::Extension(Extension { node: new_node }))
            }
            other => Ok(other.clone()),
        }
    }

    let rewritten = rewrite(&logical).unwrap();
    // Despite the no-op rewrite, the filter's IR `condition` field must
    // remain — we should still be able to downcast and inspect it.
    fn find_filter(plan: &LogicalPlan) -> Option<&GraphFilter> {
        if let Some(f) = downcast_graph_ir::<GraphFilter>(plan) {
            return Some(f);
        }
        if let LogicalPlan::Extension(ext) = plan {
            for child in ext.node.inputs() {
                if let Some(f) = find_filter(child) {
                    return Some(f);
                }
            }
        }
        None
    }
    let filter = find_filter(&rewritten).expect("found GraphFilter");
    // The condition for a label filter is a HasLabel expression.
    match &filter.condition {
        IrExpr::HasLabel { label, .. } => assert_eq!(label, "Person"),
        IrExpr::Binary {
            op: BinaryOp::Or, ..
        } => {} // multiple labels would chain OR
        other => panic!("unexpected filter condition: {other:?}"),
    }
}
