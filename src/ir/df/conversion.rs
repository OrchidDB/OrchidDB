//! Bidirectional conversion between Graph IR and DataFusion extension plans.

use super::*;

// ============================================================
// IR Node → LogicalPlan
// ============================================================

/// Convert a `GraphPlan` into a DataFusion `LogicalPlan` of nested
/// `Extension` nodes. Each Graph IR operator becomes its own concrete
/// `UserDefinedLogicalNodeCore` so HEP-style rule sets can downcast and
/// rewrite by operator kind.
pub fn to_logical_plan(plan: &GraphPlan) -> DFResult<LogicalPlan> {
    node_to_plan_with_policy(&plan.root, Some(plan.policy.clone()))
}

fn node_to_plan(node: &Node) -> DFResult<LogicalPlan> {
    node_to_plan_with_policy(node, None)
}

fn node_to_plan_with_policy(
    node: &Node,
    plan_policy: Option<GraphPlanPolicy>,
) -> DFResult<LogicalPlan> {
    let schema = build_schema_for_node(node)?;
    let plan: LogicalPlan = match node {
        Node::GraphReturn {
            fields,
            result_form,
            input,
        } => extension(GraphReturn {
            fields: fields.clone(),
            result_form: *result_form,
            plan_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphNodeScan {
            graph,
            binding,
            labels,
        } => extension(GraphNodeScan {
            graph: graph.clone(),
            binding: binding.clone(),
            labels: labels.clone(),
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphRelScan {
            graph,
            binding,
            types,
            dir,
        } => extension(GraphRelScan {
            graph: graph.clone(),
            binding: binding.clone(),
            types: types.clone(),
            dir: *dir,
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphValues {
            bindings,
            rows,
            bulk,
        } => extension(GraphValues {
            bindings: bindings.clone(),
            rows: rows.clone(),
            bulk: bulk.clone(),
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphOneRow => extension(GraphOneRow {
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphEmpty => extension(GraphEmpty {
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphCorrelate { bindings } => extension(GraphCorrelate {
            bindings: bindings.clone(),
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphBind {
            bind,
            kind,
            expr,
            input,
        } => extension(GraphBind {
            bind: bind.clone(),
            kind: *kind,
            expr: expr.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphExpand {
            graph,
            source,
            target,
            target_mode,
            target_labels,
            rel_binding,
            rel_types,
            dir,
            length,
            history,
            path,
            path_mode,
            match_mode,
            path_materialization,
            path_update,
            input,
        } => extension(GraphExpand {
            graph: graph.clone(),
            source: source.clone(),
            target: target.clone(),
            target_mode: *target_mode,
            target_labels: target_labels.clone(),
            rel_binding: rel_binding.clone(),
            rel_types: rel_types.clone(),
            dir: *dir,
            length: length.clone(),
            history: history.clone(),
            path: path.clone(),
            path_mode: *path_mode,
            match_mode: *match_mode,
            path_materialization: *path_materialization,
            path_update: *path_update,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphPathPattern {
            graph,
            path,
            selector,
            path_mode,
            match_mode,
            endpoints,
            parts,
            path_materialization,
            input,
        } => extension(GraphPathPattern {
            graph: graph.clone(),
            path: path.clone(),
            selector: selector.clone(),
            path_mode: *path_mode,
            match_mode: *match_mode,
            endpoints: endpoints.clone(),
            parts: parts.clone(),
            path_materialization: *path_materialization,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphRepeat {
            loop_name,
            times,
            emit,
            until_first,
            until,
            until_traversal,
            path,
            path_objects,
            prefix_predicate,
            prefix_traversal,
            seed,
            body,
        } => extension(GraphRepeat {
            loop_name: loop_name.clone(),
            times: *times,
            emit: emit.clone(),
            until_first: *until_first,
            until: until.clone(),
            until_traversal: until_traversal.clone(),
            path: path.clone(),
            path_objects: *path_objects,
            prefix_predicate: prefix_predicate.clone(),
            prefix_traversal: prefix_traversal.clone(),
            schema,
            inputs: vec![node_to_plan(seed)?, node_to_plan(body)?],
        }),
        Node::GraphPathFilter {
            condition,
            scope,
            input,
        } => extension(GraphPathFilter {
            condition: condition.clone(),
            scope: *scope,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphCreate {
            graph,
            nodes,
            edges,
            input,
        } => extension(GraphCreate {
            graph: graph.clone(),
            nodes: nodes.clone(),
            edges: edges.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        // MERGE's match-or-create control flow has no DataFusion
        // equivalent; the relational backend rejects it explicitly rather
        // than emitting a plan that silently drops the create arm.
        Node::GraphMerge { .. } => {
            return Err(DataFusionError::NotImplemented(
                "GraphMerge has no relational lowering".to_string(),
            ));
        }
        Node::GraphSetProperty { items, input } => extension(GraphSetProperty {
            items: items.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphDelete {
            targets,
            detach,
            input,
        } => extension(GraphDelete {
            targets: targets.clone(),
            detach: *detach,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphFilter { condition, input } => extension(GraphFilter {
            condition: condition.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphProject {
            mode,
            items,
            error_policy,
            input,
        } => extension(GraphProject {
            mode: *mode,
            items: items.clone(),
            error_policy: *error_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphJvm { operation, input } => extension(GraphJvm {
            operation: operation.clone(), schema, inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphCurrentProject {
            expr,
            fields,
            input,
        } => extension(GraphCurrentProject {
            expr: expr.clone(),
            fields: fields.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphAggregate {
            group,
            aggs,
            fields,
            input,
        } => extension(GraphAggregate {
            group: group.clone(),
            aggs: aggs.clone(),
            fields: fields.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphGroupMap {
            key,
            value,
            output,
            input,
        } => extension(GraphGroupMap {
            key: key.clone(),
            value: value.clone(),
            output: output.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphGroupSideEffect { label, key, value, key_input, input } => extension(GraphGroupSideEffect {
            label: label.clone(), key: key.clone(), value: value.clone(), key_input: key_input.clone(), schema, inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphGroupCountSideEffect { label, key, input } => {
            extension(GraphGroupCountSideEffect {
                label: label.clone(),
                key: key.clone(),
                schema,
                inputs: vec![node_to_plan(input)?],
            })
        }
        Node::GraphSideEffect { label, value_input, value, seed, reducer, eager, input } => extension(GraphSideEffect {
            label: label.clone(), value_input: value_input.clone(), value: value.clone(), seed: seed.clone(), reducer: reducer.clone(), eager: *eager,
            schema, inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphReadSideEffect { label, input } => extension(GraphReadSideEffect {
            label: label.clone(), schema, inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphCap { labels, input } => extension(GraphCap {
            labels: labels.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphShortestPath {
            source,
            target,
            direction,
            rel_types,
            max_distance,
            include_edges,
            output,
            all_paths,
            input,
        } => extension(GraphShortestPath {
            source: source.clone(),
            target: target.clone(),
            direction: *direction,
            rel_types: rel_types.clone(),
            max_distance: *max_distance,
            include_edges: *include_edges,
            output: output.clone(),
            all_paths: *all_paths,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphDistinct {
            keys,
            mode,
            bulk,
            input,
        } => extension(GraphDistinct {
            keys: keys.clone(),
            mode: *mode,
            bulk: *bulk,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphSort { keys, input } => extension(GraphSort {
            keys: keys.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphSample { kind, seed, step_id, weight, input } => extension(GraphSample {
            kind: *kind, seed: *seed, step_id: step_id.clone(), weight: weight.clone(),
            schema, inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphSlice { slice, input } => extension(GraphSlice {
            slice: slice.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphSliceExpr {
            offset,
            fetch,
            input,
        } => extension(GraphSliceExpr {
            offset: offset.clone(),
            fetch: fetch.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphBarrier {
            partition,
            order,
            slice,
            materialize,
            bulk_policy,
            input,
        } => extension(GraphBarrier {
            partition: partition.clone(),
            order: order.clone(),
            slice: slice.clone(),
            materialize: *materialize,
            bulk_policy: *bulk_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphJoin {
            kind,
            left,
            right,
            condition,
        } => extension(GraphJoin {
            kind: *kind,
            condition: condition.clone(),
            schema,
            inputs: vec![node_to_plan(left)?, node_to_plan(right)?],
        }),
        Node::GraphApply {
            kind,
            correlation,
            outputs,
            optional_missing,
            left,
            right,
        } => extension(GraphApply {
            kind: *kind,
            correlation: correlation.clone(),
            outputs: outputs.clone(),
            optional_missing: *optional_missing,
            schema,
            inputs: vec![node_to_plan(left)?, node_to_plan(right)?],
        }),
        Node::GraphUnion {
            all,
            align,
            left,
            right,
        } => extension(GraphUnion {
            all: *all,
            align: *align,
            schema,
            inputs: vec![node_to_plan(left)?, node_to_plan(right)?],
        }),
        Node::GraphUnwind {
            input_expr,
            bind,
            outer,
            input,
        } => extension(GraphUnwind {
            input_expr: input_expr.clone(),
            bind: bind.clone(),
            outer: *outer,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphQuantifier {
            kind,
            item_binding,
            input_expr,
            predicate,
            output,
            input,
        } => extension(GraphQuantifier {
            kind: *kind,
            item_binding: item_binding.clone(),
            input_expr: input_expr.clone(),
            predicate: predicate.clone(),
            output: output.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphCollect {
            value,
            distinct,
            order,
            alias,
            input,
        } => extension(GraphCollect {
            value: value.clone(),
            distinct: *distinct,
            order: order.clone(),
            alias: alias.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphCoalesce {
            success,
            output,
            correlation,
            arm_outputs,
            input,
            arms,
        } => {
            let mut children = vec![node_to_plan(input)?];
            for arm in arms {
                children.push(node_to_plan(arm)?);
            }
            extension(GraphCoalesce {
                success: *success,
                output: output.clone(),
                correlation: correlation.clone(),
                arm_outputs: arm_outputs.clone(),
                schema,
                inputs: children,
            })
        }
        Node::GraphChoose {
            selector,
            output,
            correlation,
            arms,
            default,
            unmatched,
            input,
        } => {
            let mut children = vec![node_to_plan(input)?];
            for arm in arms {
                children.push(node_to_plan(&arm.body)?);
            }
            if let Some(default) = default {
                children.push(node_to_plan(default)?);
            }
            extension(GraphChoose {
                selector: selector.clone(),
                output: output.clone(),
                correlation: correlation.clone(),
                arm_keys: arms.iter().map(|a| a.key.clone()).collect(),
                has_default: default.is_some(),
                unmatched: *unmatched,
                schema,
                inputs: children,
            })
        }
        Node::GraphSelect {
            labels,
            outputs,
            input,
        } => extension(GraphSelect {
            labels: labels.clone(),
            outputs: outputs.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphProcedureCall {
            name,
            args,
            yields,
            mode,
            input,
        } => {
            let inputs = match input {
                Some(input) => vec![node_to_plan(input)?],
                None => Vec::new(),
            };
            extension(GraphProcedureCall {
                name: name.clone(),
                args: args.clone(),
                yields: yields.clone(),
                mode: *mode,
                has_input: input.is_some(),
                schema,
                inputs,
            })
        }
        Node::GraphExtension {
            name,
            metadata,
            inputs,
        } => {
            let plan_inputs = inputs
                .iter()
                .map(node_to_plan)
                .collect::<DFResult<Vec<_>>>()?;
            extension(GraphExtension {
                op_name: name.clone(),
                metadata: metadata.clone(),
                schema,
                inputs: plan_inputs,
            })
        }

        // -------- SPARQL / RDF --------
        Node::GraphSparqlGraphNames { dataset, graph_scope } => extension(GraphSparqlGraphNames {
            dataset: dataset.clone(), graph_scope: graph_scope.clone(),
            schema, inputs: vec![],
        }),
        Node::GraphSparqlTriplePattern {
            dataset,
            graph_scope,
            subject,
            predicate,
            object,
            outputs,
        } => extension(GraphSparqlTriplePattern {
            dataset: dataset.clone(),
            graph_scope: graph_scope.clone(),
            subject: subject.clone(),
            predicate: predicate.clone(),
            object: object.clone(),
            outputs: outputs.clone(),
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphRdfPropertyPath {
            dataset,
            graph_scope,
            subject,
            object,
            path,
            path_materialization,
            zero_length,
        } => extension(GraphRdfPropertyPath {
            dataset: dataset.clone(),
            graph_scope: graph_scope.clone(),
            subject: subject.clone(),
            object: object.clone(),
            path: path.clone(),
            path_materialization: *path_materialization,
            zero_length: *zero_length,
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphSparqlMinus {
            compatible,
            shared,
            left,
            right,
        } => extension(GraphSparqlMinus {
            compatible: *compatible,
            shared: shared.clone(),
            schema,
            inputs: vec![node_to_plan(left)?, node_to_plan(right)?],
        }),
        Node::GraphService {
            endpoint,
            query,
            silent,
            outputs,
            input,
        } => extension(GraphService {
            endpoint: endpoint.clone(),
            query: query.clone(),
            silent: *silent,
            outputs: outputs.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphConstructTriples { template, input } => extension(GraphConstructTriples {
            template: template.clone(),
            plan_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphDescribe { terms, input } => extension(GraphDescribe {
            terms: terms.clone(),
            plan_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphAsk { field, input } => extension(GraphAsk {
            field: field.clone(),
            plan_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphListComprehension {
            input_expr,
            item,
            filter,
            map_expr,
            alias,
            input,
        } => extension(GraphListComprehension {
            input_expr: input_expr.clone(),
            item: item.clone(),
            filter: filter.clone(),
            map_expr: map_expr.clone(),
            alias: alias.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
    };
    Ok(plan)
}

fn extension<T: UserDefinedLogicalNodeCore + 'static>(node: T) -> LogicalPlan {
    LogicalPlan::Extension(Extension {
        node: Arc::new(node) as Arc<dyn UserDefinedLogicalNode>,
    })
}

// ============================================================
// LogicalPlan → IR Node (round-trip)
// ============================================================

/// Reconstruct a `GraphPlan` from a DataFusion `LogicalPlan` previously
/// produced by [`to_logical_plan`] and possibly rewritten by HEP rules.
/// The plan policy is carried by the root output-boundary extension node.
pub fn from_logical_plan(plan: &LogicalPlan) -> DFResult<GraphPlan> {
    let policy = root_plan_policy(plan).ok_or_else(|| {
        DataFusionError::Plan("expected root GraphIR output node to carry GraphPlanPolicy".into())
    })?;
    let node = plan_to_node(plan)?;
    Ok(GraphPlan::new(policy, node))
}

/// Compatibility helper for callers that need to reconstruct an older
/// extension tree without an embedded root policy.
pub fn from_logical_plan_with_policy(
    fallback_policy: GraphPlanPolicy,
    plan: &LogicalPlan,
) -> DFResult<GraphPlan> {
    let policy = root_plan_policy(plan).unwrap_or(fallback_policy);
    let node = plan_to_node(plan)?;
    Ok(GraphPlan::new(policy, node))
}

fn root_plan_policy(plan: &LogicalPlan) -> Option<GraphPlanPolicy> {
    let LogicalPlan::Extension(ext) = plan else {
        return None;
    };
    let any: &dyn Any = ext.node.as_ref().as_any();
    if let Some(op) = any.downcast_ref::<GraphReturn>() {
        return op.plan_policy.clone();
    }
    if let Some(op) = any.downcast_ref::<GraphConstructTriples>() {
        return op.plan_policy.clone();
    }
    if let Some(op) = any.downcast_ref::<GraphDescribe>() {
        return op.plan_policy.clone();
    }
    if let Some(op) = any.downcast_ref::<GraphAsk>() {
        return op.plan_policy.clone();
    }
    None
}

fn plan_to_node(plan: &LogicalPlan) -> DFResult<Node> {
    let LogicalPlan::Extension(ext) = plan else {
        return Err(DataFusionError::Plan(format!(
            "expected GraphIR Extension node, got {plan:?}"
        )));
    };
    let any: &dyn Any = ext.node.as_ref().as_any();

    // Walk the children up front so each extension's `rebuild` can take
    // owned `Node` values in the right order.
    let mut children: Vec<Node> = Vec::with_capacity(ext.node.inputs().len());
    for child in ext.node.inputs() {
        children.push(plan_to_node(child)?);
    }

    macro_rules! try_op {
        ($($t:ty),* $(,)?) => {
            $(
                if let Some(op) = any.downcast_ref::<$t>() {
                    return Ok(op.rebuild(children));
                }
            )*
        };
    }

    try_op!(
        GraphReturn,
        GraphNodeScan,
        GraphRelScan,
        GraphValues,
        GraphOneRow,
        GraphEmpty,
        GraphCorrelate,
        GraphBind,
        GraphExpand,
        GraphPathPattern,
        GraphRepeat,
        GraphPathFilter,
        GraphCreate,
        GraphSetProperty,
        GraphDelete,
        GraphFilter,
        GraphProject,
        GraphCurrentProject,
        GraphJvm,
        GraphAggregate,
        GraphGroupMap,
        GraphGroupSideEffect,
        GraphGroupCountSideEffect,
        GraphSideEffect,
        GraphReadSideEffect,
        GraphCap,
        GraphShortestPath,
        GraphDistinct,
        GraphSort,
        GraphSample,
        GraphSlice,
        GraphSliceExpr,
        GraphBarrier,
        GraphJoin,
        GraphApply,
        GraphUnion,
        GraphUnwind,
        GraphQuantifier,
        GraphCollect,
        GraphCoalesce,
        GraphChoose,
        GraphSelect,
        GraphProcedureCall,
        GraphExtension,
        // SPARQL / RDF
        GraphSparqlTriplePattern,
        GraphSparqlGraphNames,
        GraphRdfPropertyPath,
        GraphSparqlMinus,
        GraphService,
        GraphConstructTriples,
        GraphDescribe,
        GraphAsk,
        GraphListComprehension,
    );

    Err(DataFusionError::Plan(format!(
        "unrecognized graph extension node `{}`",
        ext.node.name()
    )))
}
