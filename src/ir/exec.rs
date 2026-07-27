//! Hybrid executable plans: SQL islands spliced into a Graph IR plan.
//!
//! The relational backend ([`crate::ir::rel`]) is all-or-nothing — it lowers
//! a whole [`GraphPlan`] to one relational plan, or fails. That makes a
//! single unsupported operator very expensive: `list_append` sitting in a
//! projection discards the scan, filter and joins beneath it, and the entire
//! query falls back to the tree interpreter.
//!
//! This module partitions instead. It walks the plan top-down looking for
//! *maximal* subtrees the relational backend accepts, executes each one as a
//! SQL island, and rewrites that subtree in place as [`Node::GraphValues`]
//! holding the island's result rows. Whatever could not be lowered runs
//! afterwards over those materialized rows.
//!
//! Splicing the results back in as `GraphValues` is what keeps this cheap:
//! the residual is still an ordinary Graph IR plan, so it needs no new
//! operator, no new executor, and no changes to any existing consumer of
//! `Node`.
//!
//! Two things are deliberately never islanded:
//!
//! * **Mutations.** `GraphCreate` has a relational lowering, but executing it
//!   through DataFusion would compute a result set without ever writing to
//!   the catalog. Writes must reach the graph store, so any subtree
//!   containing one stays with the interpreter.
//! * **Correlated subtrees.** A subtree under an `Apply`'s right side refers
//!   to bindings produced outside it. Lowering it standalone fails on the
//!   unresolved binding, so it declines to island on its own.

use crate::ir::catalog::{PropertyGraph, array_value};
use crate::ir::plan::{GraphPlan, Node};
use crate::ir::rel::{RelBackend, execute_lowered};
use crate::ir::value::Value;

/// What the partitioner did, for tests and for the harness to report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecStats {
    /// Subtrees executed relationally.
    pub islands: usize,
    /// Rows those islands returned in total.
    pub island_rows: usize,
    /// Operators left for the interpreter (islands excluded).
    pub residual_ops: usize,
}

/// Column-name suffixes the relational lowering uses to encode a graph
/// binding across several flat columns. Kept in sync with `ir::rel`.
const ID_SUFFIX: &str = "__id";
const LABEL_SUFFIX: &str = "__label";
const PROP_MARKER: &str = "__prop__";
const SRC_ID_SUFFIX: &str = "__src_id";
const SRC_LABEL_SUFFIX: &str = "__src_label";
const DST_ID_SUFFIX: &str = "__dst_id";
const DST_LABEL_SUFFIX: &str = "__dst_label";

/// Rewrite `plan` so every maximal relationally-lowerable subtree is replaced
/// by its already-computed rows.
///
/// The returned plan is an ordinary Graph IR plan and can be handed straight
/// to the interpreter. On any island failure the subtree is left untouched,
/// so the worst case is exactly today's behaviour.
pub async fn plan_with_islands(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    backend: &RelBackend,
) -> (GraphPlan, ExecStats) {
    let mut root = (*plan.root).clone();
    let mut stats = ExecStats::default();
    islandize(&mut root, &plan.policy, graph, backend, &mut stats).await;
    stats.residual_ops = count_ops(&root);
    (
        GraphPlan {
            policy: plan.policy.clone(),
            root: Box::new(root),
        },
        stats,
    )
}

/// Try to island `node`; if it declines, recurse into its children.
///
/// Boxed because the recursion is async — `islandize` awaits itself.
fn islandize<'a>(
    node: &'a mut Node,
    policy: &'a crate::ir::policy::GraphPlanPolicy,
    graph: &'a PropertyGraph,
    backend: &'a RelBackend,
    stats: &'a mut ExecStats,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
    Box::pin(async move {
        if let Some(values) = try_island(node, policy, graph, backend).await {
            if let Node::GraphValues { rows, .. } = &values {
                stats.islands += 1;
                stats.island_rows += rows.len();
            }
            *node = values;
            return;
        }
        for child in children_mut(node) {
            islandize(child, policy, graph, backend, stats).await;
        }
    })
}

/// Execute `node` relationally and materialize the result, or `None` if it is
/// not a legal or lowerable island.
async fn try_island(
    node: &Node,
    policy: &crate::ir::policy::GraphPlanPolicy,
    graph: &PropertyGraph,
    backend: &RelBackend,
) -> Option<Node> {
    // A bare source is already as cheap as it gets; islanding one buys
    // nothing and only costs a materialization.
    if children_count(node) == 0 {
        return None;
    }
    if contains_mutation(node) {
        return None;
    }
    let candidate = GraphPlan {
        policy: policy.clone(),
        root: Box::new(node.clone()),
    };
    let lowered = backend.lower(&candidate, graph).ok()?;
    let returned = execute_lowered(lowered).await.ok()?;
    batch_to_values(&returned)
}

/// Rebuild a relational result batch into `GraphValues` bindings.
///
/// The relational encoding spreads one graph binding over several columns
/// (`p__id`, `p__label`, `p__prop__age`). Node and edge bindings are
/// reassembled from their id/label columns; property columns are dropped
/// because the catalog remains the source of truth for property reads, so
/// carrying them would only risk disagreeing with it.
fn batch_to_values(returned: &crate::ir::interpreter::ReturnedBatches) -> Option<Node> {
    let batch = &returned.batch;
    let schema = batch.schema();
    let names: Vec<String> = schema
        .fields()
        .iter()
        .map(|field| field.name().to_string())
        .collect();

    enum Source {
        /// `(id column, label column)`
        NodeCols(usize, usize),
        /// `(src id, src label, dst id, dst label, edge id, edge label)`
        EdgeCols(usize, usize, usize, usize, Option<usize>, Option<usize>),
        Scalar(usize),
    }

    let find = |suffix: &str, binding: &str| -> Option<usize> {
        names.iter().position(|n| n == &format!("{binding}{suffix}"))
    };

    let mut bindings: Vec<String> = Vec::new();
    let mut sources: Vec<Source> = Vec::new();
    let mut seen: Vec<String> = Vec::new();

    for (index, name) in names.iter().enumerate() {
        if name.contains(PROP_MARKER) {
            continue;
        }
        // Derive the binding name from whichever structural suffix matched.
        let binding = [
            SRC_ID_SUFFIX,
            SRC_LABEL_SUFFIX,
            DST_ID_SUFFIX,
            DST_LABEL_SUFFIX,
            ID_SUFFIX,
            LABEL_SUFFIX,
        ]
        .iter()
        .find_map(|suffix| name.strip_suffix(*suffix))
        .map(str::to_string);

        match binding {
            Some(binding) => {
                if seen.contains(&binding) {
                    continue;
                }
                seen.push(binding.clone());
                let src_id = find(SRC_ID_SUFFIX, &binding);
                let dst_id = find(DST_ID_SUFFIX, &binding);
                if let (Some(src_id), Some(dst_id)) = (src_id, dst_id) {
                    let src_label = find(SRC_LABEL_SUFFIX, &binding)?;
                    let dst_label = find(DST_LABEL_SUFFIX, &binding)?;
                    bindings.push(binding.clone());
                    sources.push(Source::EdgeCols(
                        src_id,
                        src_label,
                        dst_id,
                        dst_label,
                        find(ID_SUFFIX, &binding),
                        find(LABEL_SUFFIX, &binding),
                    ));
                } else {
                    let id = find(ID_SUFFIX, &binding)?;
                    let label = find(LABEL_SUFFIX, &binding)?;
                    bindings.push(binding.clone());
                    sources.push(Source::NodeCols(id, label));
                }
            }
            None => {
                bindings.push(name.clone());
                sources.push(Source::Scalar(index));
            }
        }
    }

    let column_value = |index: usize, row: usize| -> Value {
        array_value(
            batch.column(index).as_ref(),
            row,
            Some(schema.field(index)),
        )
    };
    let as_i64 = |value: &Value| -> Option<i64> {
        match value {
            Value::Int(v) | Value::Long(v) => Some(*v),
            _ => None,
        }
    };
    let as_label = |value: &Value| -> Option<String> {
        match value {
            Value::String(v) => Some(v.clone()),
            _ => None,
        }
    };

    let mut rows = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        let mut out = Vec::with_capacity(sources.len());
        for source in &sources {
            let value = match source {
                Source::Scalar(index) => column_value(*index, row),
                Source::NodeCols(id, label) => {
                    let id_value = column_value(*id, row);
                    let label_value = column_value(*label, row);
                    match (as_i64(&id_value), as_label(&label_value)) {
                        (Some(id), Some(label)) => Value::Node { label, id },
                        // A null id is an outer-join miss, not a broken
                        // encoding; anything else means the island did not
                        // produce the shape we expect, so decline the island
                        // rather than fabricate a binding.
                        _ if matches!(id_value, Value::Null) => Value::Null,
                        _ => return None,
                    }
                }
                Source::EdgeCols(src_id, src_label, dst_id, dst_label, id, label) => {
                    let src_id_value = column_value(*src_id, row);
                    if matches!(src_id_value, Value::Null) {
                        Value::Null
                    } else {
                        Value::Edge {
                            rel_type: label
                                .and_then(|index| as_label(&column_value(index, row)))
                                .unwrap_or_default(),
                            id: id
                                .and_then(|index| as_i64(&column_value(index, row)))
                                .unwrap_or_default(),
                            src_label: as_label(&column_value(*src_label, row))?,
                            src_id: as_i64(&src_id_value)?,
                            dst_label: as_label(&column_value(*dst_label, row))?,
                            dst_id: as_i64(&column_value(*dst_id, row))?,
                            projected_properties: None,
                        }
                    }
                }
            };
            out.push(value);
        }
        rows.push(out);
    }

    Some(Node::GraphValues {
        bindings,
        rows,
        bulk: None,
    })
}

/// Does this subtree write to the graph?
fn contains_mutation(node: &Node) -> bool {
    if matches!(
        node,
        Node::GraphCreate { .. }
            | Node::GraphMerge { .. }
            | Node::GraphSetProperty { .. }
            | Node::GraphDelete { .. }
    ) {
        return true;
    }
    children(node).into_iter().any(contains_mutation)
}

fn count_ops(node: &Node) -> usize {
    1 + children(node).into_iter().map(count_ops).sum::<usize>()
}

fn children_count(node: &Node) -> usize {
    children(node).len()
}

fn children(node: &Node) -> Vec<&Node> {
    use Node::*;
    match node {
        GraphMerge {
            input,
            match_arm,
            create_arm,
            ..
        } => vec![input, match_arm, create_arm],
        GraphReturn { input, .. }
        | GraphConstructTriples { input, .. }
        | GraphDescribe { input, .. }
        | GraphAsk { input, .. }
        | GraphBind { input, .. }
        | GraphPathPattern { input, .. }
        | GraphPathFilter { input, .. }
        | GraphCreate { input, .. }
        | GraphSetProperty { input, .. }
        | GraphDelete { input, .. }
        | GraphFilter { input, .. }
        | GraphCurrentProject { input, .. }
        | GraphAggregate { input, .. }
        | GraphGroupMap { input, .. }
        | GraphGroupCountSideEffect { input, .. }
        | GraphCap { input, .. }
        | GraphShortestPath { input, .. }
        | GraphDistinct { input, .. }
        | GraphSort { input, .. }
        | GraphSlice { input, .. }
        | GraphSliceExpr { input, .. }
        | GraphBarrier { input, .. }
        | GraphUnwind { input, .. }
        | GraphQuantifier { input, .. }
        | GraphCollect { input, .. }
        | GraphListComprehension { input, .. }
        | GraphSelect { input, .. }
        | GraphExpand { input, .. }
        | GraphProject { input, .. }
        | GraphService { input, .. } => vec![input],
        GraphJoin { left, right, .. }
        | GraphApply { left, right, .. }
        | GraphUnion { left, right, .. }
        | GraphSparqlMinus { left, right, .. } => vec![left, right],
        GraphRepeat {
            seed,
            body,
            until_traversal,
            prefix_traversal,
            ..
        } => {
            let mut out = vec![seed.as_ref(), body.as_ref()];
            out.extend(until_traversal.iter().map(|node| node.as_ref()));
            out.extend(prefix_traversal.iter().map(|node| node.as_ref()));
            out
        }
        GraphCoalesce { input, arms, .. } => {
            let mut out = vec![input.as_ref()];
            out.extend(arms.iter());
            out
        }
        GraphChoose {
            input,
            arms,
            default,
            ..
        } => {
            let mut out = vec![input.as_ref()];
            out.extend(arms.iter().map(|arm| &arm.body));
            out.extend(default.iter().map(|node| node.as_ref()));
            out
        }
        GraphProcedureCall { input, .. } => input.iter().map(|node| node.as_ref()).collect(),
        GraphExtension { inputs, .. } => inputs.iter().collect(),
        _ => Vec::new(),
    }
}

fn children_mut(node: &mut Node) -> Vec<&mut Node> {
    use Node::*;
    match node {
        GraphMerge {
            input,
            match_arm,
            create_arm,
            ..
        } => vec![input, match_arm, create_arm],
        GraphReturn { input, .. }
        | GraphConstructTriples { input, .. }
        | GraphDescribe { input, .. }
        | GraphAsk { input, .. }
        | GraphBind { input, .. }
        | GraphPathPattern { input, .. }
        | GraphPathFilter { input, .. }
        | GraphCreate { input, .. }
        | GraphSetProperty { input, .. }
        | GraphDelete { input, .. }
        | GraphFilter { input, .. }
        | GraphCurrentProject { input, .. }
        | GraphAggregate { input, .. }
        | GraphGroupMap { input, .. }
        | GraphGroupCountSideEffect { input, .. }
        | GraphCap { input, .. }
        | GraphShortestPath { input, .. }
        | GraphDistinct { input, .. }
        | GraphSort { input, .. }
        | GraphSlice { input, .. }
        | GraphSliceExpr { input, .. }
        | GraphBarrier { input, .. }
        | GraphUnwind { input, .. }
        | GraphQuantifier { input, .. }
        | GraphCollect { input, .. }
        | GraphListComprehension { input, .. }
        | GraphSelect { input, .. }
        | GraphExpand { input, .. }
        | GraphProject { input, .. }
        | GraphService { input, .. } => vec![input],
        GraphJoin { left, right, .. }
        | GraphApply { left, right, .. }
        | GraphUnion { left, right, .. }
        | GraphSparqlMinus { left, right, .. } => vec![left, right],
        GraphRepeat {
            seed,
            body,
            until_traversal,
            prefix_traversal,
            ..
        } => {
            let mut out = vec![seed.as_mut(), body.as_mut()];
            out.extend(until_traversal.iter_mut().map(|node| node.as_mut()));
            out.extend(prefix_traversal.iter_mut().map(|node| node.as_mut()));
            out
        }
        GraphCoalesce { input, arms, .. } => {
            let mut out = vec![input.as_mut()];
            out.extend(arms.iter_mut());
            out
        }
        GraphChoose {
            input,
            arms,
            default,
            ..
        } => {
            let mut out = vec![input.as_mut()];
            out.extend(arms.iter_mut().map(|arm| &mut arm.body));
            out.extend(default.iter_mut().map(|node| node.as_mut()));
            out
        }
        GraphProcedureCall { input, .. } => input.iter_mut().map(|node| node.as_mut()).collect(),
        GraphExtension { inputs, .. } => inputs.iter_mut().collect(),
        _ => Vec::new(),
    }
}
