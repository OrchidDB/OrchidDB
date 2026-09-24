//! Query-local SQL lowering memo. A failed parent attempt can reuse successfully
//! lowered children without rebuilding their sources. Nothing survives a query.
use super::*;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Debug, Default)]
pub(super) struct IslandMemo {
    safe: BTreeMap<usize, bool>,
    stats: BTreeMap<usize, GraphPlanStats>,
    lowered: BTreeMap<(usize, BTreeMap<String, usize>), Option<LoweredNode>>,
    next_scan: usize,
    pub attempts: usize,
    pub hits: usize,
}
pub(super) type SharedMemo = Arc<Mutex<IslandMemo>>;
impl IslandMemo {
    pub fn new(root: &Node) -> SharedMemo {
        fn visit(node: &Node, memo: &mut IslandMemo) -> (bool, GraphPlanStats) {
            let mut safe = crate::ir::analysis::node_effect(node)
                == crate::ir::analysis::Effect::Pure
                && !matches!(
                    node,
                    Node::GraphCorrelate { .. } | Node::GraphValues { bulk: Some(_), .. }
                );
            let mut stats = GraphPlanStats {
                nodes: 1,
                depth: 1,
                ..Default::default()
            };
            if let Node::GraphExpand {
                dir: Direction::Both,
                ..
            } = node
            {
                stats.bidirectional_expands = 1;
            }
            if let Node::GraphProject { items, .. } = node {
                stats.select_history_projects = items
                    .iter()
                    .filter(|i| i.alias.starts_with("__gremlin_select_history_"))
                    .count();
            }
            for child in crate::ir::analysis::children(node) {
                let (child_safe, child_stats) = visit(child, memo);
                safe &= child_safe;
                stats.nodes += child_stats.nodes;
                stats.depth = stats.depth.max(child_stats.depth + 1);
                stats.bidirectional_expands += child_stats.bidirectional_expands;
                stats.select_history_projects += child_stats.select_history_projects;
            }
            let key = node as *const Node as usize;
            memo.safe.insert(key, safe);
            memo.stats.insert(key, stats);
            (safe, stats)
        }
        let mut memo = Self::default();
        visit(root, &mut memo);
        Arc::new(Mutex::new(memo))
    }
    pub fn safe(&self, node: &Node) -> bool {
        self.safe
            .get(&(node as *const Node as usize))
            .copied()
            .unwrap_or(false)
    }
}
impl RelBackend {
    pub(super) fn lower_island(
        &self,
        policy: &GraphPlanPolicy,
        root: &Node,
        graph: &PropertyGraph,
        memo: SharedMemo,
    ) -> RelResult<LoweredPlan> {
        if policy.language == Language::Sparql {
            return self.lower(&GraphPlan::new(policy.clone(), root.clone()), graph);
        }
        stacker::maybe_grow(8 * 1024 * 1024, 32 * 1024 * 1024, || {
            let stats = memo
                .lock()
                .unwrap()
                .stats
                .get(&(root as *const Node as usize))
                .copied()
                .unwrap_or_default();
            if policy.language == Language::Gremlin
                && stats.bidirectional_expands >= 2
                && stats.select_history_projects > 0
                && stats.depth > 28
            {
                return Err(RelError::Unsupported(
                    "Gremlin SQL island complexity fence".into(),
                ));
            }
            let scan_counter = memo.lock().unwrap().next_scan;
            let mut ctx = LoweringContext {
                graph,
                options: self.options.clone(),
                policy: policy.clone(),
                language: policy.language,
                scan_counter,
                correlate_plan: None,
                rdf_typed_terms_used: false,
                gremlin_label_binds: if policy.language == Language::Gremlin {
                    gremlin::label_bind_counts(root)
                } else {
                    BTreeMap::new()
                },
                island_memo: Some(memo.clone()),
            };
            let result = ctx.lower_node(root);
            memo.lock().unwrap().next_scan = ctx.scan_counter;
            let lowered = result?;
            Ok(LoweredPlan {
                fields: lowered
                    .fields
                    .clone()
                    .unwrap_or_else(|| output_fields(&lowered.plan)),
                result_form: lowered.result_form.unwrap_or(policy.result_form),
                plan: lowered.plan,
                islands: lowered.islands,
            })
        })
    }
}
impl LoweringContext<'_> {
    pub(super) fn memoized_lower_node(&mut self, node: &Node) -> RelResult<LoweredNode> {
        let memo = self.island_memo.clone();
        // Only original, immutable query nodes have stable pointer identities.
        // Temporary rewritten nodes and correlated scopes bypass this memo.
        let key = memo
            .as_ref()
            .filter(|m| self.correlate_plan.is_none() && m.lock().unwrap().safe(node))
            .map(|_| {
                (
                    node as *const Node as usize,
                    self.gremlin_label_binds.clone(),
                )
            });
        if let (Some(memo), Some(key)) = (&memo, &key) {
            let mut cache = memo.lock().unwrap();
            if let Some(found) = cache.lowered.get(key).cloned() {
                cache.hits += 1;
                return found
                    .ok_or_else(|| RelError::Unsupported("SQL lowering fence (memoized)".into()));
            }
            cache.attempts += 1;
        }
        let result = self.lower_node_inner(node);
        if let (Some(memo), Some(key)) = (memo, key) {
            memo.lock()
                .unwrap()
                .lowered
                .insert(key, result.as_ref().ok().cloned());
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn failed_parent_reuses_child_without_freezing_later_queries() {
        let root = Node::GraphProject {
            mode: crate::ir::plan::ProjectMode::PreserveVisible,
            error_policy: crate::ir::plan::ProjectErrorPolicy::PropagateError,
            items: vec![crate::ir::plan::ProjectionItem {
                alias: "list".into(),
                expr: IrExpr::List(vec![IrExpr::lit_int(1)]),
            }],
            input: Box::new(Node::GraphNodeScan {
                binding: "current".into(),
                labels: LabelExpr::Any,
                graph: "g".into(),
            }),
        };
        let graph = PropertyGraph::new();
        graph.insert_node("person", BTreeMap::new());
        let backend = RelBackend::new().preserving_traverser_state();
        let policy = GraphPlanPolicy::gremlin();
        let memo = IslandMemo::new(&root);
        assert!(
            backend
                .lower_island(&policy, &root, &graph, memo.clone())
                .is_err()
        );
        let Node::GraphProject { input, .. } = &root else {
            unreachable!()
        };
        let first = backend
            .lower_island(&policy, input, &graph, memo.clone())
            .unwrap();
        assert!(memo.lock().unwrap().hits > 0);
        graph.insert_node("person", BTreeMap::new());
        let fresh = backend
            .lower_island(&policy, input, &graph, IslandMemo::new(&root))
            .unwrap();
        let old = sql::plan_tables(&first.plan).await.unwrap();
        let new = sql::plan_tables(&fresh.plan).await.unwrap();
        assert_eq!(
            old.iter()
                .flat_map(|t| &t.batches)
                .map(|b| b.num_rows())
                .sum::<usize>(),
            1
        );
        assert_eq!(
            new.iter()
                .flat_map(|t| &t.batches)
                .map(|b| b.num_rows())
                .sum::<usize>(),
            2
        );
    }
}
