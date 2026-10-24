//! Scoped traversal of the canonical Graph IR. Children are shared with the
//! exhaustive read-effect walker; every expression-bearing variant is explicit.
use super::*;
use crate::ir::{
    expr::{IrExpr, Lit},
    plan::*,
};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Scope {
    /// Distinguishes correlated/control regions even for identical expressions.
    pub node_path: Vec<usize>,
    /// Stack rather than a set: nested binders may shadow the same name.
    pub binders: Vec<String>,
}
impl Scope {
    pub fn is_bound(&self, name: &str) -> bool {
        self.binders.iter().rev().any(|binding| binding == name)
    }
    fn with_bindings(&self, names: &[&str]) -> Self {
        let mut scope = self.clone();
        scope
            .binders
            .extend(names.iter().map(|name| (*name).to_owned()));
        scope
    }
}
/// Post-order expression visitor. The collection is visited outside the local
/// binder; only its body sees the iterator/accumulator. Substitution clients
/// must respect `scope.is_bound` and avoid introducing captured free names.
pub fn walk_expr(expr: &mut IrExpr, scope: &Scope, visit: &mut impl FnMut(&mut IrExpr, &Scope)) {
    use IrExpr::*;
    match expr {
        Binary { lhs, rhs, .. } => {
            walk_expr(lhs, scope, visit);
            walk_expr(rhs, scope, visit);
        }
        Not(value) | IsNull(value) | IsNotNull(value) => walk_expr(value, scope, visit),
        StringPredicate {
            target, pattern, ..
        } => {
            walk_expr(target, scope, visit);
            walk_expr(pattern, scope, visit);
        }
        List(values) | Call { args: values, .. } => {
            for value in values {
                walk_expr(value, scope, visit);
            }
        }
        Quantifier {
            list,
            item,
            predicate,
            ..
        }
        | ListFilter {
            list,
            item,
            predicate,
        } => {
            walk_expr(list, scope, visit);
            walk_expr(predicate, &scope.with_bindings(&[item]), visit);
        }
        ListTransform { list, item, map } => {
            walk_expr(list, scope, visit);
            walk_expr(map, &scope.with_bindings(&[item]), visit);
        }
        ListReduce {
            collection,
            accumulator,
            item,
            map,
        } => {
            walk_expr(collection, scope, visit);
            walk_expr(map, &scope.with_bindings(&[accumulator, item]), visit);
        }
        Case { arms, otherwise } => {
            for (condition, value) in arms {
                walk_expr(condition, scope, visit);
                walk_expr(value, scope, visit);
            }
            if let Some(value) = otherwise {
                walk_expr(value, scope, visit);
            }
        }
        Lit(_)
        | Binding(_)
        | Property { .. }
        | Id(_)
        | Label(_)
        | HasLabel { .. }
        | IsBound(_)
        | SimplePath(_) => {}
    }
    visit(expr, scope);
}
/// Visit all embedded expressions including mutation arguments and repeat
/// modulators. This is not DataFusion `expressions()`, which is empty for the
/// structural bridge's extension nodes.
fn node_exprs(node: &mut Node, scope: &Scope, visit: &mut impl FnMut(&mut IrExpr, &Scope)) {
    use Node::*;
    let mut expr = |value: &mut IrExpr| walk_expr(value, scope, visit);
    match node {
        GraphJvm { operation, .. } => {
            for arg in &mut operation.arguments {
                expr(&mut arg.expr);
            }
        }
        GraphBind {
            expr: Some(value), ..
        } => expr(value),
        GraphRepeat {
            emit,
            until,
            prefix_predicate,
            ..
        } => {
            if let EmitMode::AfterEachIfPredicate(value) = emit {
                expr(value);
            }
            if let Some(value) = until {
                expr(value);
            }
            if let Some(value) = prefix_predicate {
                expr(value);
            }
        }
        GraphFilter { condition, .. } | GraphPathFilter { condition, .. } => expr(condition),
        GraphCurrentProject { expr: value, .. } => expr(value),
        GraphProject { items, .. } => {
            for item in items {
                expr(&mut item.expr);
            }
        }
        GraphCreate { nodes, edges, .. } => {
            for node in nodes {
                if let Some(value) = &mut node.properties {
                    expr(value);
                }
            }
            for edge in edges {
                if let Some(value) = &mut edge.properties {
                    expr(value);
                }
            }
        }
        GraphSetProperty { items, .. } => {
            for item in items {
                expr(&mut item.target);
                expr(&mut item.value);
            }
        }
        GraphDelete { targets, .. } => {
            for target in targets {
                expr(target);
            }
        }
        GraphAggregate { group, aggs, .. } => {
            for item in group {
                expr(&mut item.expr);
            }
            for agg in aggs {
                if let Some(arg) = &mut agg.arg {
                    expr(arg);
                }
            }
        }
        GraphGroupMap { key, value, .. } | GraphGroupSideEffect { key, value, .. } => {
            expr(key);
            if let GroupValue::Aggregate(agg) = value {
                if let Some(arg) = &mut agg.arg {
                    expr(arg);
                }
            }
        }
        GraphGroupCountSideEffect { key, .. } => expr(key),
        GraphSideEffect { value, .. } => expr(value),
        GraphSort { keys, .. } | GraphBarrier { order: keys, .. } => {
            for key in keys {
                expr(&mut key.expr);
            }
        }
        GraphSample { weight, .. } => {
            if let Some(value) = weight {
                expr(value);
            }
        }
        GraphSliceExpr { offset, fetch, .. } => {
            if let Some(value) = offset {
                expr(value);
            }
            if let Some(value) = fetch {
                expr(value);
            }
        }
        GraphJoin { condition, .. } => {
            if let Some(value) = condition {
                expr(value);
            }
        }
        GraphUnwind { input_expr, .. } => expr(input_expr),
        GraphQuantifier {
            input_expr,
            predicate,
            item_binding,
            ..
        } => {
            expr(input_expr);
            walk_expr(predicate, &scope.with_bindings(&[item_binding]), visit);
        }
        GraphCollect { value, order, .. } => {
            expr(value);
            for key in order {
                expr(&mut key.expr);
            }
        }
        GraphListComprehension {
            input_expr,
            item,
            filter,
            map_expr,
            ..
        } => {
            expr(input_expr);
            let local = scope.with_bindings(&[item]);
            if let Some(value) = filter {
                walk_expr(value, &local, visit);
            }
            if let Some(value) = map_expr {
                walk_expr(value, &local, visit);
            }
        }
        GraphChoose { selector, .. } => match selector {
            ChooseSelector::Boolean(value) | ChooseSelector::Value(value) => expr(value),
            ChooseSelector::Predicates(values) => {
                for value in values {
                    expr(value);
                }
            }
        },
        GraphProcedureCall { args, .. } => {
            for arg in args {
                expr(&mut arg.value);
            }
        }
        GraphReturn { .. }
        | GraphConstructTriples { .. }
        | GraphDescribe { .. }
        | GraphAsk { .. }
        | GraphNodeScan { .. }
        | GraphRelScan { .. }
        | GraphValues { .. }
        | GraphOneRow
        | GraphEmpty
        | GraphCorrelate { .. }
        | GraphSparqlGraphNames { .. }
        | GraphSparqlTriplePattern { .. }
        | GraphBind { expr: None, .. }
        | GraphExpand { .. }
        | GraphPathPattern { .. }
        | GraphRdfPropertyPath { .. }
        | GraphMerge { .. }
        | GraphReadSideEffect { .. }
        | GraphCap { .. }
        | GraphShortestPath { .. }
        | GraphDistinct { .. }
        | GraphSlice { .. }
        | GraphApply { .. }
        | GraphUnion { .. }
        | GraphCoalesce { .. }
        | GraphSelect { .. }
        | GraphSparqlMinus { .. }
        | GraphExtension { .. } => {}
    }
}
pub(crate) fn node_exprs_ref(node: &Node, scope: &Scope, visit: &mut impl FnMut(&IrExpr, &Scope)) {
    use Node::*;
    let mut expr = |value: &IrExpr| visit_expr(value, scope, visit);
    match node {
        GraphJvm { operation, .. } => {
            for arg in &operation.arguments {
                expr(&arg.expr);
            }
        }
        GraphBind {
            expr: Some(value), ..
        } => expr(value),
        GraphRepeat {
            emit,
            until,
            prefix_predicate,
            ..
        } => {
            if let EmitMode::AfterEachIfPredicate(value) = emit {
                expr(value);
            }
            if let Some(value) = until {
                expr(value);
            }
            if let Some(value) = prefix_predicate {
                expr(value);
            }
        }
        GraphFilter { condition, .. } | GraphPathFilter { condition, .. } => expr(condition),
        GraphCurrentProject { expr: value, .. } => expr(value),
        GraphProject { items, .. } => {
            for item in items {
                expr(&item.expr);
            }
        }
        GraphCreate { nodes, edges, .. } => {
            for node in nodes {
                if let Some(value) = &node.properties {
                    expr(value);
                }
            }
            for edge in edges {
                if let Some(value) = &edge.properties {
                    expr(value);
                }
            }
        }
        GraphSetProperty { items, .. } => {
            for item in items {
                expr(&item.target);
                expr(&item.value);
            }
        }
        GraphDelete { targets, .. } => {
            for target in targets {
                expr(target);
            }
        }
        GraphAggregate { group, aggs, .. } => {
            for item in group {
                expr(&item.expr);
            }
            for agg in aggs {
                if let Some(arg) = &agg.arg {
                    expr(arg);
                }
            }
        }
        GraphGroupMap { key, value, .. } | GraphGroupSideEffect { key, value, .. } => {
            expr(key);
            if let GroupValue::Aggregate(agg) = value {
                if let Some(arg) = &agg.arg {
                    expr(arg);
                }
            }
        }
        GraphGroupCountSideEffect { key, .. } => expr(key),
        GraphSideEffect { value, .. } => expr(value),
        GraphSort { keys, .. } | GraphBarrier { order: keys, .. } => {
            for key in keys {
                expr(&key.expr);
            }
        }
        GraphSample { weight, .. } => {
            if let Some(value) = weight {
                expr(value);
            }
        }
        GraphSliceExpr { offset, fetch, .. } => {
            if let Some(value) = offset {
                expr(value);
            }
            if let Some(value) = fetch {
                expr(value);
            }
        }
        GraphJoin { condition, .. } => {
            if let Some(value) = condition {
                expr(value);
            }
        }
        GraphUnwind { input_expr, .. } => expr(input_expr),
        GraphQuantifier {
            input_expr,
            predicate,
            item_binding,
            ..
        } => {
            expr(input_expr);
            visit_expr(predicate, &scope.with_bindings(&[item_binding]), visit);
        }
        GraphCollect { value, order, .. } => {
            expr(value);
            for key in order {
                expr(&key.expr);
            }
        }
        GraphListComprehension {
            input_expr,
            item,
            filter,
            map_expr,
            ..
        } => {
            expr(input_expr);
            let local = scope.with_bindings(&[item]);
            if let Some(value) = filter {
                visit_expr(value, &local, visit);
            }
            if let Some(value) = map_expr {
                visit_expr(value, &local, visit);
            }
        }
        GraphChoose { selector, .. } => match selector {
            ChooseSelector::Boolean(value) | ChooseSelector::Value(value) => expr(value),
            ChooseSelector::Predicates(values) => {
                for value in values {
                    expr(value);
                }
            }
        },
        GraphProcedureCall { args, .. } => {
            for arg in args {
                expr(&arg.value);
            }
        }
        GraphReturn { .. }
        | GraphConstructTriples { .. }
        | GraphDescribe { .. }
        | GraphAsk { .. }
        | GraphNodeScan { .. }
        | GraphRelScan { .. }
        | GraphValues { .. }
        | GraphOneRow
        | GraphEmpty
        | GraphCorrelate { .. }
        | GraphSparqlGraphNames { .. }
        | GraphSparqlTriplePattern { .. }
        | GraphBind { expr: None, .. }
        | GraphExpand { .. }
        | GraphPathPattern { .. }
        | GraphRdfPropertyPath { .. }
        | GraphMerge { .. }
        | GraphReadSideEffect { .. }
        | GraphCap { .. }
        | GraphShortestPath { .. }
        | GraphDistinct { .. }
        | GraphSlice { .. }
        | GraphApply { .. }
        | GraphUnion { .. }
        | GraphCoalesce { .. }
        | GraphSelect { .. }
        | GraphSparqlMinus { .. }
        | GraphExtension { .. } => {}
    }
}
/// Bottom-up traversal keeps metadata in place; it never reconstructs a node
/// from a partial list of fields. Each nested region has a distinct scope path.
pub fn walk(
    root: &mut Node,
    expressions: &mut impl FnMut(&mut IrExpr, &Scope),
    nodes: &mut impl FnMut(&mut Node, &Scope),
) {
    fn visit(
        node: &mut Node,
        scope: &Scope,
        expressions: &mut impl FnMut(&mut IrExpr, &Scope),
        nodes: &mut impl FnMut(&mut Node, &Scope),
    ) {
        for (index, child) in crate::ir::analysis::children_mut(node)
            .into_iter()
            .enumerate()
        {
            let mut child_scope = scope.clone();
            child_scope.node_path.push(index);
            visit(child, &child_scope, expressions, nodes);
        }
        node_exprs(node, scope, expressions);
        nodes(node, scope);
    }
    visit(root, &Scope::default(), expressions, nodes);
}

pub const REMOVE_TRUE_FILTER: RuleDescriptor = RuleDescriptor {
    id: "graph.filter.literal_true",
    version: 1,
    stage: Stage::GraphNormalize,
    kind: RuleKind::EquivalentRewrite,
    traversal: Traversal::BottomUp,
    required: false,
    required_facts: &["literal boolean true"],
    invalidated_facts: &["cardinality", "island safety"],
};
/// One pass suffices: each successful rewrite strictly removes one operator.
/// No predicate computation (including volatile/throwing calls) is discarded.
pub fn normalize(plan: &GraphPlan, session: &mut RuleSession) -> Result<GraphPlan, RuleError> {
    session.context.validate()?;
    let mut plan = plan.clone();
    let mut error = None;
    walk(&mut plan.root, &mut |_, _| {}, &mut |node, _| {
        if error.is_some() {
            return;
        }
        match session.visit(REMOVE_TRUE_FILTER) {
            Err(value) => {
                error = Some(value);
                return;
            }
            Ok(false) => return,
            Ok(true) => {}
        }
        if matches!(
            node,
            Node::GraphFilter {
                condition: IrExpr::Lit(Lit::Bool(true)),
                ..
            }
        ) {
            let Node::GraphFilter { input, .. } = std::mem::replace(node, Node::GraphEmpty) else {
                unreachable!()
            };
            *node = *input;
            session.changed(REMOVE_TRUE_FILTER, || {
                "literal true preserves rows, effects, order and hidden fields".into()
            });
        }
    });
    if error.is_none() && plan.policy.language == crate::ir::policy::Language::Cypher {
        walk(&mut plan.root, &mut |_, _| {}, &mut |node, _| {
            if error.is_some() {
                return;
            }
            match session.visit(super::aggregate::INDEPENDENT_COLLECTIONS) {
                Err(value) => { error = Some(value); return; }
                Ok(true) if super::aggregate::independent_collections(node) => session.changed(super::aggregate::INDEPENDENT_COLLECTIONS, || "independent DISTINCT collections eliminate optional Cartesian multiplication".into()),
                _ => {}
            }
            match session.visit(super::aggregate::NODE_SCAN_COUNT) {
                Err(value) => { error = Some(value); return; }
                Ok(true) if super::aggregate::node_scan_count(node) => session.changed(super::aggregate::NODE_SCAN_COUNT, || "global binding counts ignore optional null extension; node scan identities are unique".into()),
                _ => {}
            }
            match session.visit(super::aggregate::COLLECT_SIZE) {
                Err(value) => error = Some(value),
                Ok(true) if super::aggregate::collect_size(node) => session.changed(super::aggregate::COLLECT_SIZE, || "cardinality-only collection retains argument evaluation, null exclusion and distinct equality".into()),
                _ => {}
            }
        });
    }
    error.map_or(Ok(plan), Err)
}

/// Read-only visitor uses the same exhaustive scoped traversal as rewrites.
pub fn visit_expressions(plan: &GraphPlan, mut visit: impl FnMut(&IrExpr, &Scope)) {
    fn walk(node: &Node, scope: &Scope, visit: &mut impl FnMut(&IrExpr, &Scope)) {
        for (index, child) in crate::ir::analysis::children(node).into_iter().enumerate() {
            let mut child_scope = scope.clone();
            child_scope.node_path.push(index);
            walk(child, &child_scope, visit);
        }
        node_exprs_ref(node, scope, visit);
    }
    walk(&plan.root, &Scope::default(), &mut visit);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::policy::{GraphPlanPolicy, ResultForm};
    #[test]
    fn normalization_preserves_nested_metadata_and_bridge_roundtrip() {
        let values = Node::GraphValues {
            bindings: vec!["x".into()],
            rows: vec![vec![crate::ir::Value::Int(1)]],
            bulk: Some(vec![3]),
        };
        let input = Node::GraphRepeat {
            loop_name: Some("loop".into()),
            times: Some(2),
            emit: EmitMode::AfterEachIfTraversal(
                Node::GraphOneRow.filter(IrExpr::lit_bool(true)).boxed(),
            ),
            until_first: false,
            until: Some(IrExpr::lit_bool(false)),
            until_traversal: None,
            path: Some("hidden_path".into()),
            path_objects: PathObjects::VerticesOnly,
            prefix_predicate: None,
            prefix_traversal: Some(Node::GraphOneRow.filter(IrExpr::lit_bool(true)).boxed()),
            seed: values.filter(IrExpr::lit_bool(true)).boxed(),
            body: Node::GraphOneRow.filter(IrExpr::lit_bool(true)).boxed(),
        }
        .return_(vec!["x".into()], ResultForm::TraverserStream);
        let plan = GraphPlan::new(GraphPlanPolicy::gremlin(), input);
        let mut context = RuleContext::default();
        context.trace_limit = 20;
        let mut session = RuleSession::new(context.clone());
        let normalized = normalize(&plan, &mut session).unwrap();
        assert_eq!(normalized.policy, plan.policy);
        assert_eq!(context.revision(), 4);
        let bridged = crate::ir::df::to_logical_plan(&normalized).unwrap();
        assert_eq!(
            crate::ir::df::from_logical_plan(&bridged).unwrap(),
            normalized
        );
        assert_eq!(normalize(&normalized, &mut session).unwrap(), normalized);
        assert_eq!(context.revision(), 4);
    }
    #[test]
    fn list_scopes_do_not_capture_outer_or_shadowed_names() {
        let mut expression = IrExpr::ListTransform {
            list: Box::new(IrExpr::binding("x")),
            item: "x".into(),
            map: Box::new(IrExpr::ListReduce {
                collection: Box::new(IrExpr::binding("x")),
                accumulator: "acc".into(),
                item: "x".into(),
                map: Box::new(IrExpr::binding("x")),
            }),
        };
        let mut scopes = vec![];
        walk_expr(&mut expression, &Scope::default(), &mut |expr, scope| {
            if let IrExpr::Binding(name) = expr {
                scopes.push(scope.binders.clone());
                if !scope.is_bound(name) {
                    *name = "outer".into();
                }
            }
        });
        assert_eq!(
            scopes,
            vec![
                vec![],
                vec!["x".to_string()],
                vec!["x".to_string(), "acc".to_string(), "x".to_string()]
            ]
        );
        let IrExpr::ListTransform { list, .. } = expression else {
            unreachable!()
        };
        assert_eq!(*list, IrExpr::binding("outer"));
    }
    #[test]
    fn nonliteral_filters_and_effectful_inputs_are_retained() {
        let call = IrExpr::Call {
            name: "rand".into(),
            args: vec![],
        };
        let input = Node::GraphDelete {
            targets: vec![IrExpr::binding("n")],
            detach: true,
            input: Node::GraphOneRow.boxed(),
        };
        let plan = GraphPlan::new(
            GraphPlanPolicy::cypher(),
            input
                .clone()
                .filter(IrExpr::lit_bool(true))
                .filter(call.clone()),
        );
        let normalized = normalize(&plan, &mut RuleSession::new(RuleContext::default())).unwrap();
        assert_eq!(*normalized.root, input.filter(call));
    }
}

/// Borrowed expression visitor for analyses in scalar hot paths. It shares the
/// same binder rules as mutation traversal without cloning expression trees.
pub fn visit_expr(expr: &IrExpr, scope: &Scope, visit: &mut impl FnMut(&IrExpr, &Scope)) {
    use IrExpr::*;
    match expr {
        Binary { lhs, rhs, .. } => {
            visit_expr(lhs, scope, visit);
            visit_expr(rhs, scope, visit);
        }
        Not(value) | IsNull(value) | IsNotNull(value) => visit_expr(value, scope, visit),
        StringPredicate {
            target, pattern, ..
        } => {
            visit_expr(target, scope, visit);
            visit_expr(pattern, scope, visit);
        }
        List(values) | Call { args: values, .. } => {
            for value in values {
                visit_expr(value, scope, visit);
            }
        }
        Quantifier {
            list,
            item,
            predicate,
            ..
        }
        | ListFilter {
            list,
            item,
            predicate,
        } => {
            visit_expr(list, scope, visit);
            visit_expr(predicate, &scope.with_bindings(&[item]), visit);
        }
        ListTransform { list, item, map } => {
            visit_expr(list, scope, visit);
            visit_expr(map, &scope.with_bindings(&[item]), visit);
        }
        ListReduce {
            collection,
            accumulator,
            item,
            map,
        } => {
            visit_expr(collection, scope, visit);
            visit_expr(map, &scope.with_bindings(&[accumulator, item]), visit);
        }
        Case { arms, otherwise } => {
            for (condition, value) in arms {
                visit_expr(condition, scope, visit);
                visit_expr(value, scope, visit);
            }
            if let Some(value) = otherwise {
                visit_expr(value, scope, visit);
            }
        }
        Lit(_)
        | Binding(_)
        | Property { .. }
        | Id(_)
        | Label(_)
        | HasLabel { .. }
        | IsBound(_)
        | SimplePath(_) => {}
    }
    visit(expr, scope);
}
