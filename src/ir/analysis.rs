//! Effect analysis for Graph IR read execution.
//!
//! Read backends must inspect the entire plan before running any part of it.
//! A write can be hidden in a branch, an apply right-hand side, or a repeat
//! modulator even if that subtree would produce no rows at runtime.

use crate::ir::plan::{EmitMode, GraphPlan, Node, ProcedureMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Pure,
    QueryLocalState,
    ReadProcedure,
    SourceMutation,
    OpaqueExtension,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadCapabilities {
    pub query_local_state: bool,
    pub read_procedures: bool,
}

impl ReadCapabilities {
    /// Local DuckDB reads may use temporary query state, but cannot invoke
    /// procedures without a separately planned adapter.
    pub const LOCAL_DUCKDB: Self = Self {
        query_local_state: true,
        read_procedures: false,
    };

    /// The widest read profile. Mutations and opaque extensions are still
    /// refused because their effects cannot be proven safe.
    pub const ALL_READS: Self = Self {
        query_local_state: true,
        read_procedures: true,
    };
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReadValidationError {
    #[error("source mutation {operator} at Graph IR child path {path:?}")]
    SourceMutation {
        operator: &'static str,
        path: Vec<usize>,
    },
    #[error("opaque extension {name:?} at Graph IR child path {path:?}; read effects are unknown")]
    OpaqueExtension { name: String, path: Vec<usize> },
    #[error("{effect:?} requires a read capability at {operator}, Graph IR child path {path:?}")]
    MissingCapability {
        effect: Effect,
        operator: &'static str,
        path: Vec<usize>,
    },
}

/// Validate the complete plan against a backend's read capabilities.
/// Child paths are zero-based indexes in the order returned by `children`.
pub fn validate_read_capabilities(
    plan: &GraphPlan,
    capabilities: ReadCapabilities,
) -> Result<(), ReadValidationError> {
    let mut stack = vec![(plan.root.as_ref(), Vec::new())];
    let mut first_capability_error = None;
    while let Some((node, path)) = stack.pop() {
        let effect = node_effect(node);
        match effect {
            Effect::SourceMutation => {
                return Err(ReadValidationError::SourceMutation {
                    operator: operator_name(node),
                    path,
                });
            }
            Effect::OpaqueExtension => {
                let Node::GraphExtension { name, .. } = node else {
                    unreachable!()
                };
                first_capability_error.get_or_insert(ReadValidationError::OpaqueExtension {
                    name: name.clone(),
                    path: path.clone(),
                });
            }
            Effect::QueryLocalState if !capabilities.query_local_state => {
                first_capability_error
                    .get_or_insert_with(|| missing_capability(effect, node, path.clone()));
            }
            Effect::ReadProcedure if !capabilities.read_procedures => {
                first_capability_error
                    .get_or_insert_with(|| missing_capability(effect, node, path.clone()));
            }
            _ => {}
        }
        for (index, child) in children(node).into_iter().enumerate().rev() {
            let mut child_path = path.clone();
            child_path.push(index);
            stack.push((child, child_path));
        }
    }
    first_capability_error.map_or(Ok(()), Err)
}

pub fn validate_read_only(plan: &GraphPlan) -> Result<(), ReadValidationError> {
    validate_read_capabilities(plan, ReadCapabilities::ALL_READS)
}

/// A lightweight predicate for callers that route writes to a different
/// executor. Unlike validation, this does not reject opaque extensions.
pub fn contains_source_mutation(root: &Node) -> bool {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node_effect(node) == Effect::SourceMutation {
            return true;
        }
        stack.extend(children(node));
    }
    false
}

pub fn node_effect(node: &Node) -> Effect {
    match node {
        Node::GraphJvm { .. }
        | Node::GraphCreate { .. }
        | Node::GraphMerge { .. }
        | Node::GraphSetProperty { .. }
        | Node::GraphDelete { .. }
        | Node::GraphProcedureCall {
            mode: ProcedureMode::Write,
            ..
        } => Effect::SourceMutation,
        Node::GraphProcedureCall {
            mode: ProcedureMode::Read,
            ..
        } => Effect::ReadProcedure,
        Node::GraphGroupSideEffect { .. } | Node::GraphGroupCountSideEffect { .. } | Node::GraphSideEffect { .. } | Node::GraphReadSideEffect { .. } | Node::GraphCap { .. } | Node::GraphSample { .. } => Effect::QueryLocalState,
        Node::GraphExtension { .. } => Effect::OpaqueExtension,
        _ => Effect::Pure,
    }
}

fn missing_capability(effect: Effect, node: &Node, path: Vec<usize>) -> ReadValidationError {
    ReadValidationError::MissingCapability {
        effect,
        operator: operator_name(node),
        path,
    }
}

fn operator_name(node: &Node) -> &'static str {
    match node {
        Node::GraphCreate { .. } => "GraphCreate",
        Node::GraphMerge { .. } => "GraphMerge",
        Node::GraphSetProperty { .. } => "GraphSetProperty",
        Node::GraphDelete { .. } => "GraphDelete",
        Node::GraphProcedureCall { .. } => "GraphProcedureCall",
        Node::GraphGroupSideEffect { .. } => "GraphGroupSideEffect",
        Node::GraphGroupCountSideEffect { .. } => "GraphGroupCountSideEffect",
        Node::GraphSideEffect { .. } => "GraphSideEffect",
        Node::GraphReadSideEffect { .. } => "GraphReadSideEffect",
        Node::GraphCap { .. } => "GraphCap",
        Node::GraphSample { .. } => "GraphSample",
        Node::GraphExtension { .. } => "GraphExtension",
        _ => "Graph IR node",
    }
}

/// Exhaustive child traversal. New node variants must be classified here,
/// so a newly added branch cannot silently bypass the read contract.
pub(crate) fn children(node: &Node) -> Vec<&Node> {
    use Node::*;
    match node {
        GraphSideEffect { value_input, input, .. } => vec![input, value_input],
        GraphGroupSideEffect { input, key_input, value, .. } => {
            let mut nodes = vec![input.as_ref(), key_input.as_ref()];
            if let crate::ir::plan::GroupValue::Traversal { traversal, .. } = value { nodes.push(traversal); }
            nodes
        }
        GraphGroupMap { input, value, .. } => {
            let mut nodes = vec![input.as_ref()];
            if let crate::ir::plan::GroupValue::Traversal { traversal, .. } = value { nodes.push(traversal); }
            nodes
        }
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
        | GraphJvm { input, .. }
        | GraphAggregate { input, .. }
        | GraphGroupCountSideEffect { input, .. }
        | GraphReadSideEffect { input, .. }
        | GraphCap { input, .. }
        | GraphShortestPath { input, .. }
        | GraphDistinct { input, .. }
        | GraphSort { input, .. }
        | GraphSample { input, .. }
        | GraphSlice { input, .. }
        | GraphSliceExpr { input, .. }
        | GraphBarrier { input, .. }
        | GraphUnwind { input, .. }
        | GraphQuantifier { input, .. }
        | GraphCollect { input, .. }
        | GraphListComprehension { input, .. }
        | GraphSelect { input, .. }
        | GraphExpand { input, .. }
        | GraphProject { input, .. } => vec![input],
        GraphJoin { left, right, .. }
        | GraphApply { left, right, .. }
        | GraphUnion { left, right, .. }
        | GraphSparqlMinus { left, right, .. } => vec![left, right],
        GraphRepeat {
            emit,
            seed,
            body,
            until_traversal,
            prefix_traversal,
            ..
        } => {
            let mut out = vec![seed.as_ref(), body.as_ref()];
            out.extend(until_traversal.iter().map(|node| node.as_ref()));
            out.extend(prefix_traversal.iter().map(|node| node.as_ref()));
            if let EmitMode::AfterEachIfTraversal(traversal) = emit {
                out.push(traversal);
            }
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
        GraphNodeScan { .. }
        | GraphRelScan { .. }
        | GraphValues { .. }
        | GraphOneRow
        | GraphEmpty
        | GraphCorrelate { .. }
        | GraphSparqlTriplePattern { .. }
        | GraphSparqlGraphNames { .. }
        | GraphRdfPropertyPath { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::catalog::PropertyGraph;
    use crate::ir::expr::IrExpr;
    use crate::ir::plan::{
        ApplyKind, ChooseArm, ChooseSelector, ChooseUnmatched, CoalesceSuccess, EmitMode,
        PathObjects,
    };
    use crate::ir::policy::{GraphPlanPolicy, OptionalMissing, ResultForm};
    use crate::ir::rel::{RelBackend, RelError};

    fn plan(root: Node) -> GraphPlan {
        GraphPlan::new(GraphPlanPolicy::gremlin(), root)
    }

    fn write() -> Node {
        Node::GraphSetProperty {
            items: vec![],
            input: Node::GraphOneRow.boxed(),
        }
    }

    #[test]
    fn finds_writes_in_correlated_and_branch_subplans() {
        let apply = Node::GraphApply {
            kind: ApplyKind::Semi,
            correlation: vec![],
            outputs: vec![],
            optional_missing: OptionalMissing::Null,
            left: Node::GraphOneRow.boxed(),
            right: Node::GraphCoalesce {
                success: CoalesceSuccess::FirstNonEmpty,
                output: "current".into(),
                correlation: vec![],
                arm_outputs: vec![],
                input: Node::GraphOneRow.boxed(),
                arms: vec![Node::GraphEmpty, write()],
            }
            .boxed(),
        };
        let query = plan(apply);
        assert_eq!(
            validate_read_only(&query),
            Err(ReadValidationError::SourceMutation {
                operator: "GraphSetProperty",
                path: vec![1, 2],
            })
        );
        assert!(contains_source_mutation(&query.root));

        let choose = plan(Node::GraphChoose {
            selector: ChooseSelector::Boolean(IrExpr::lit_bool(true)),
            output: "current".into(),
            correlation: vec![],
            arms: vec![ChooseArm {
                key: None,
                body: Node::GraphEmpty,
            }],
            default: Some(
                Node::GraphProcedureCall {
                    name: "write_proc".into(),
                    args: vec![],
                    yields: vec![],
                    mode: ProcedureMode::Write,
                    input: None,
                }
                .boxed(),
            ),
            unmatched: ChooseUnmatched::Drop,
            input: Node::GraphOneRow.boxed(),
        });
        assert!(matches!(
            validate_read_only(&choose),
            Err(ReadValidationError::SourceMutation {
                operator: "GraphProcedureCall",
                path,
            }) if path == vec![2]
        ));
    }

    fn repeat_with_write_in(slot: &str) -> GraphPlan {
        let mut repeat = Node::GraphRepeat {
            loop_name: None,
            times: Some(1),
            emit: EmitMode::AfterLoop,
            until_first: false,
            until: None,
            until_traversal: None,
            path: None,
            path_objects: PathObjects::VerticesOnly,
            prefix_predicate: None,
            prefix_traversal: None,
            seed: Node::GraphOneRow.boxed(),
            body: Node::GraphOneRow.boxed(),
        };
        if let Node::GraphRepeat {
            body,
            until_traversal,
            prefix_traversal,
            emit,
            ..
        } = &mut repeat
        {
            match slot {
                "body" => *body = write().boxed(),
                "until" => *until_traversal = Some(write().boxed()),
                "prefix" => *prefix_traversal = Some(write().boxed()),
                "emit" => *emit = EmitMode::AfterEachIfTraversal(write().boxed()),
                _ => unreachable!(),
            }
        }
        plan(repeat)
    }

    #[test]
    fn finds_writes_in_every_repeat_traversal() {
        for (slot, expected_index) in [("body", 1), ("until", 2), ("prefix", 2), ("emit", 2)] {
            assert_eq!(
                validate_read_only(&repeat_with_write_in(slot)),
                Err(ReadValidationError::SourceMutation {
                    operator: "GraphSetProperty",
                    path: vec![expected_index],
                }),
                "slot {slot}"
            );
        }
    }

    #[test]
    fn mutation_inside_opaque_extension_takes_precedence() {
        let query = plan(Node::GraphExtension {
            name: "custom".into(),
            metadata: vec![],
            inputs: vec![write()],
        });
        assert_eq!(
            validate_read_only(&query),
            Err(ReadValidationError::SourceMutation {
                operator: "GraphSetProperty",
                path: vec![0],
            })
        );
        let opaque = plan(Node::GraphExtension {
            name: "custom".into(),
            metadata: vec![],
            inputs: vec![Node::GraphOneRow],
        });
        assert!(matches!(
            validate_read_only(&opaque),
            Err(ReadValidationError::OpaqueExtension { .. })
        ));
    }

    #[test]
    fn local_capabilities_allow_query_state_but_not_procedures() {
        let local_state = plan(Node::GraphCap {
            labels: vec!["counts".into()],
            input: Node::GraphGroupCountSideEffect {
                label: "counts".into(),
                key: IrExpr::lit_int(1),
                input: Node::GraphOneRow.boxed(),
            }
            .boxed(),
        });
        assert_eq!(
            validate_read_capabilities(&local_state, ReadCapabilities::LOCAL_DUCKDB),
            Ok(())
        );
        let read_procedure = plan(Node::GraphProcedureCall {
            name: "db.labels".into(),
            args: vec![],
            yields: vec!["label".into()],
            mode: ProcedureMode::Read,
            input: None,
        });
        assert!(matches!(
            validate_read_capabilities(&read_procedure, ReadCapabilities::LOCAL_DUCKDB),
            Err(ReadValidationError::MissingCapability {
                effect: Effect::ReadProcedure,
                ..
            })
        ));
        assert_eq!(validate_read_only(&read_procedure), Ok(()));
    }

    #[test]
    fn relational_entry_rejects_nested_write_before_lowering() {
        let query = plan(Node::GraphUnion {
            all: true,
            align: crate::ir::plan::UnionAlign::ByPosition,
            left: Node::GraphOneRow.boxed(),
            right: write().boxed(),
        });
        assert!(matches!(
            RelBackend::new().lower(&query, &PropertyGraph::new()),
            Err(RelError::ReadValidation(
                ReadValidationError::SourceMutation {
                    operator: "GraphSetProperty",
                    ..
                }
            ))
        ));
    }

    #[test]
    fn relational_context_retains_policy_result_form() {
        let mut query = plan(Node::GraphOneRow);
        query.policy.result_form = ResultForm::Boolean;
        let lowered = RelBackend::new()
            .lower(&query, &PropertyGraph::new())
            .expect("one-row read should lower");
        assert_eq!(lowered.result_form, ResultForm::Boolean);
    }
}
