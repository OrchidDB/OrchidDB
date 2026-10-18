//! Aggregate intent is retained until its consumers prove which parts are
//! observable. This rule removes collection materialization, never evaluation
//! of the aggregate argument, and retains distinct equality and group keys.
use super::*;
use crate::ir::{
    expr::{AggKind, IrExpr, Lit},
    plan::{ApplyKind, Node, ProjectMode},
};
use std::collections::BTreeSet;

pub const COLLECT_SIZE: RuleDescriptor = RuleDescriptor {
    id: "graph.aggregate.collect_size",
    version: 1,
    stage: Stage::GraphNormalize,
    kind: RuleKind::EquivalentRewrite,
    traversal: Traversal::BottomUp,
    required: false,
    required_facts: &[
        "null-eliding collection",
        "cardinality-only consumers",
        "closed projection scope",
    ],
    invalidated_facts: &["aggregate types", "island safety", "ordering demand"],
};
/// A node scan emits each graph identity once. A leading OPTIONAL adds only
/// a null row on empty input, which a global count of that binding ignores.
/// Restrict this to the scan itself: joins, expansions and correlated inputs
/// can repeat identities, and grouped counts observe the synthetic null group.
pub const NODE_SCAN_COUNT: RuleDescriptor = RuleDescriptor {
    id: "graph.aggregate.node_scan_count",
    version: 1,
    stage: Stage::GraphNormalize,
    kind: RuleKind::EquivalentRewrite,
    traversal: Traversal::BottomUp,
    required: false,
    required_facts: &["unique nonnull node scan identities", "global binding counts", "singleton uncorrelated optional input"],
    invalidated_facts: &["aggregate input", "island safety", "ordering demand"],
};

pub fn node_scan_count(node: &mut Node) -> bool {
    let Node::GraphAggregate { group, aggs, input, .. } = node else { return false };
    if !group.is_empty() || aggs.is_empty() { return false; }
    let scan = match input.as_ref() {
        Node::GraphApply { kind: ApplyKind::Optional | ApplyKind::Inner,
            correlation, outputs, optional_missing: crate::ir::policy::OptionalMissing::Null,
            left, right } if correlation.is_empty()
                && matches!(left.as_ref(), Node::GraphOneRow) => {
            // The frontend represents the OPTIONAL body as an inner apply
            // from its empty correlation scope. That scope has exactly one
            // row per outer invocation; the outer input above is singleton.
            let right = match right.as_ref() {
                Node::GraphApply { kind: ApplyKind::Inner, correlation,
                    outputs: inner_outputs, left, right, .. }
                    if correlation.is_empty() && inner_outputs == outputs
                        && matches!(left.as_ref(), Node::GraphCorrelate { bindings } if bindings.is_empty()) => right.as_ref(),
                right => right,
            };
            if !matches!(right, Node::GraphNodeScan { binding, .. }
                if outputs.as_slice() == [binding.clone()]) { return false; }
            right
        }
        scan @ Node::GraphNodeScan { .. } => scan,
        _ => return false,
    };
    let Node::GraphNodeScan { binding, .. } = scan else { return false };
    if !aggs.iter().all(|agg| matches!(agg.kind, AggKind::CountRows | AggKind::CountDistinct)
        && matches!(&agg.arg, Some(IrExpr::Binding(name)) if name == binding)) {
        return false;
    }
    let scan = scan.clone();
    for agg in aggs {
        agg.kind = AggKind::CountRows;
        agg.arg = None;
        agg.distinct = false;
    }
    *input = Box::new(scan);
    true
}

pub const INDEPENDENT_COLLECTIONS: RuleDescriptor = RuleDescriptor {
    id: "graph.aggregate.independent_optional_collections", version: 1,
    stage: Stage::GraphNormalize, kind: RuleKind::EquivalentRewrite,
    traversal: Traversal::BottomUp, required: false,
    required_facts: &["independent optional node scans", "global distinct null-eliding collections"],
    invalidated_facts: &["aggregate input", "island safety"],
};

/// A Cartesian optional product only repeats each scan's values. DISTINCT
/// collections ignore those repetitions and synthetic nulls, so each scan can
/// be aggregated once. Never cross predicates, explicit ordering or effects.
pub fn independent_collections(node: &mut Node) -> bool {
    fn scans<'a>(node: &'a Node, out: &mut Vec<&'a Node>) -> bool {
        if matches!(node, Node::GraphOneRow) { return true; }
        let Node::GraphApply { kind: ApplyKind::Optional, left, right, outputs, .. } = node else { return false };
        let right = match right.as_ref() {
            Node::GraphApply { kind: ApplyKind::Inner, left, right, correlation, outputs: inner, .. }
                if correlation.is_empty() && inner == outputs && matches!(left.as_ref(), Node::GraphCorrelate { .. }) => right.as_ref(),
            _ => return false,
        };
        let Node::GraphNodeScan { binding, .. } = right else { return false };
        if outputs.as_slice() != [binding.clone()] || !scans(left, out) { return false; }
        out.push(right); true
    }
    let Node::GraphAggregate { group, aggs, input, .. } = node else { return false };
    if !group.is_empty() || aggs.len() < 2 || !aggs.iter().all(|a| a.kind == AggKind::CollectNonNull && a.distinct) { return false; }
    let mut sources = Vec::new();
    if !scans(input, &mut sources) { return false; }
    let mut branches = Vec::new();
    for agg in aggs.iter() {
        let binding = match &agg.arg {
            Some(IrExpr::Property { binding, .. }) => binding,
            Some(IrExpr::Call { name, args }) if name == "cypher_subscript" => {
                let [IrExpr::Binding(binding), IrExpr::Lit(Lit::String(_))] = args.as_slice() else { return false };
                binding
            }
            _ => return false,
        };
        let matches = sources.iter().filter(|n| matches!(n, Node::GraphNodeScan { binding: b, .. } if b == binding)).collect::<Vec<_>>();
        if matches.len() != 1 { return false; }
        branches.push(Node::GraphAggregate { group: vec![], aggs: vec![agg.clone()],
            fields: vec![agg.alias.clone()], input: Box::new((**matches[0]).clone()) });
    }
    *node = branches.into_iter().reduce(|left, right| Node::GraphJoin {
        kind: crate::ir::plan::JoinKind::Cross, condition: None, left: Box::new(left), right: Box::new(right),
    }).unwrap();
    true
}

fn size_binding(expr: &IrExpr) -> Option<&str> {
    if let IrExpr::Call { name, args } = expr
        && name.eq_ignore_ascii_case("size")
        && let [IrExpr::Binding(name)] = args.as_slice()
    {
        return Some(name);
    }
    None
}
/// Candidate references are neutralized in a scratch expression before free
/// reference analysis. Lexical binder shadowing is respected by the walker.
pub fn collect_size(node: &mut Node) -> bool {
    let Node::GraphProject {
        mode: ProjectMode::ReplaceScope,
        items,
        input,
        ..
    } = node
    else {
        return false;
    };
    let Node::GraphAggregate { aggs, .. } = input.as_mut() else {
        return false;
    };
    let mut candidates = BTreeSet::new();
    let mut observed = BTreeSet::new();
    for item in items.iter() {
        let mut expr = item.expr.clone();
        graph::walk_expr(&mut expr, &graph::Scope::default(), &mut |expr, scope| {
            if let Some(binding) = size_binding(expr).filter(|name| !scope.is_bound(name)) {
                candidates.insert(binding.to_owned());
                *expr = IrExpr::Lit(Lit::Int(0));
            }
        });
        graph::walk_expr(&mut expr, &graph::Scope::default(), &mut |expr, scope| {
            let binding = match expr {
                IrExpr::Binding(name)
                | IrExpr::Id(name)
                | IrExpr::Label(name)
                | IrExpr::IsBound(name)
                | IrExpr::SimplePath(name) => Some(name),
                IrExpr::Property { binding, .. } | IrExpr::HasLabel { binding, .. } => {
                    Some(binding)
                }
                _ => None,
            };
            if let Some(binding) = binding.filter(|name| !scope.is_bound(name)) {
                observed.insert(binding.clone());
            }
        });
    }
    candidates.retain(|name| !observed.contains(name));
    candidates.retain(|name| {
        aggs.iter()
            .any(|agg| &agg.alias == name && agg.kind == AggKind::CollectNonNull)
    });
    if candidates.is_empty() {
        return false;
    }
    for agg in aggs
        .iter_mut()
        .filter(|agg| candidates.contains(&agg.alias))
    {
        agg.kind = if agg.distinct {
            AggKind::CountDistinct
        } else {
            AggKind::CountRows
        };
    }
    for item in items {
        graph::walk_expr(
            &mut item.expr,
            &graph::Scope::default(),
            &mut |expr, scope| {
                if let Some(binding) = size_binding(expr)
                    .filter(|name| !scope.is_bound(name) && candidates.contains(*name))
                {
                    *expr = IrExpr::Binding(binding.to_owned());
                }
            },
        );
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{
        expr::AggCall,
        plan::{ProjectErrorPolicy, ProjectionItem},
    };
    fn size(name: &str) -> IrExpr {
        IrExpr::Call {
            name: "size".into(),
            args: vec![IrExpr::binding(name)],
        }
    }
    fn project(items: Vec<IrExpr>, distinct: bool) -> Node {
        Node::GraphProject {
            mode: ProjectMode::ReplaceScope,
            error_policy: ProjectErrorPolicy::PropagateError,
            items: items
                .into_iter()
                .enumerate()
                .map(|(i, expr)| ProjectionItem {
                    alias: format!("r{i}"),
                    expr,
                })
                .collect(),
            input: Node::GraphAggregate {
                group: vec![],
                fields: vec!["xs".into()],
                aggs: vec![AggCall {
                    kind: AggKind::CollectNonNull,
                    alias: "xs".into(),
                    arg: Some(IrExpr::Call {
                        name: "throwing_argument".into(),
                        args: vec![],
                    }),
                    distinct,
                }],
                input: Node::GraphOneRow.boxed(),
            }
            .boxed(),
        }
    }
    #[test]
    fn cardinality_specialization_retains_distinct_and_argument() {
        for distinct in [false, true] {
            let mut node = project(vec![size("xs")], distinct);
            assert!(collect_size(&mut node));
            let Node::GraphProject { items, input, .. } = node else {
                unreachable!()
            };
            assert_eq!(items[0].expr, IrExpr::binding("xs"));
            let Node::GraphAggregate { aggs, .. } = *input else {
                unreachable!()
            };
            assert_eq!(
                aggs[0].kind,
                if distinct {
                    AggKind::CountDistinct
                } else {
                    AggKind::CountRows
                }
            );
            assert_eq!(aggs[0].distinct, distinct);
            assert!(matches!(aggs[0].arg, Some(IrExpr::Call { .. })));
        }
    }
    #[test]
    fn visible_list_or_shadowed_binding_prevents_wrong_rewrite() {
        let mut observed = project(vec![size("xs"), IrExpr::binding("xs")], false);
        assert!(!collect_size(&mut observed));
        let local = IrExpr::ListTransform {
            list: Box::new(IrExpr::List(vec![])),
            item: "xs".into(),
            map: Box::new(size("xs")),
        };
        let mut shadowed = project(vec![local], false);
        assert!(!collect_size(&mut shadowed));
        let mut open = project(vec![size("xs")], false);
        let Node::GraphProject { mode, .. } = &mut open else {
            unreachable!()
        };
        *mode = ProjectMode::PreserveVisible;
        assert!(!collect_size(&mut open));
    }
}

/// Requirements carried into collection implementation selection. Ordering is
/// an observation requirement, not permission to add/remove arbitrary windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionOrder {
    PreserveInput,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionNulls {
    Exclude,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionEmpty {
    TypedEmptyList,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectionContract {
    pub nulls: CollectionNulls,
    pub empty: CollectionEmpty,
    pub order: CollectionOrder,
    pub distinct: bool,
}
impl CollectionContract {
    pub fn for_aggregate(agg: &crate::ir::expr::AggCall) -> Option<Self> {
        (agg.kind == AggKind::CollectNonNull).then_some(Self {
            nulls: CollectionNulls::Exclude,
            empty: CollectionEmpty::TypedEmptyList,
            order: CollectionOrder::PreserveInput,
            distinct: agg.distinct,
        })
    }
}

#[cfg(test)]
mod independent_collection_tests {
    use super::*;
    #[test]
    fn only_independent_global_distinct_collections_are_factored() {
        let queries = [
            ("OPTIONAL MATCH (f:F) OPTIONAL MATCH (n:N) RETURN collect(DISTINCT n.num) AS a, collect(DISTINCT f.num) AS b", true),
            ("OPTIONAL MATCH (f:F) OPTIONAL MATCH (n:N) RETURN collect(n.num) AS a, collect(DISTINCT f.num) AS b", false),
            ("OPTIONAL MATCH (f:F) OPTIONAL MATCH (n:N) WHERE n.num = f.num RETURN collect(DISTINCT n.num) AS a, collect(DISTINCT f.num) AS b", false),
            ("OPTIONAL MATCH (f:F) OPTIONAL MATCH (n:N) RETURN f.num, collect(DISTINCT n.num) AS a, collect(DISTINCT f.num) AS b", false),
        ];
        for (query, expected) in queries {
            let mut plan = crate::language::cypher::planner::CypherPlanner::new().plan(
                &crate::language::cypher::parser::parse_query(query).unwrap()).unwrap();
            let mut changed = false;
            super::super::graph::walk(&mut plan.root, &mut |_, _| {}, &mut |node, _| {
                changed |= independent_collections(node);
            });
            assert_eq!(changed, expected, "{query}");
        }
    }
}
