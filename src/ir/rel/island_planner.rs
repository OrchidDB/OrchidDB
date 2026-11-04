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
    cypher_equality_fences: std::collections::BTreeSet<usize>,
    stats: BTreeMap<usize, GraphPlanStats>,
    lowered: BTreeMap<(usize, BTreeMap<String, usize>), Option<LoweredNode>>,
    next_scan: usize,
    scan_logical_labels: bool,
    terminal_distinct: Option<usize>,
    pub attempts: usize,
    pub hits: usize,
}
pub(super) type SharedMemo = Arc<Mutex<IslandMemo>>;
impl IslandMemo {
    pub fn new(root: &Node) -> SharedMemo {
        let mut memo = Self { scan_logical_labels: requires_logical_node_labels(root), terminal_distinct: super::operators::terminal_distinct(root), ..Self::default() };
        let mut results = BTreeMap::new();
        let mut pending = vec![(root, false)];
        while let Some((node, ready)) = pending.pop() {
            if !ready {
                pending.push((node, true));
                pending.extend(crate::ir::analysis::children(node).into_iter().map(|child| (child, false)));
                continue;
            }
            let mut safe = crate::ir::analysis::node_effect(node)
                == crate::ir::analysis::Effect::Pure
                && !matches!(node, Node::GraphValues { bulk: Some(_), .. });
            // Purity and closure are different: a correlated body is not a
            // standalone SQL island, but its enclosing apply/repeat can be.
            // Keep free correlation relative to each node, rather than the
            // traversal context, so memo entries never capture an outer row.
            let mut free_correlation = matches!(node, Node::GraphCorrelate { .. });
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
            for (index, child) in crate::ir::analysis::children(node).into_iter().enumerate() {
                let (child_safe, child_free, child_stats): (bool, bool, GraphPlanStats) = results.remove(&(child as *const Node as usize)).expect("child analyzed before parent");
                safe &= child_safe;
                let binds_child = index > 0
                    && matches!(
                        node,
                        Node::GraphApply { .. }
                            | Node::GraphRepeat { .. }
                            | Node::GraphCoalesce { .. }
                            | Node::GraphChoose { .. }
                    );
                free_correlation |= child_free && !binds_child;
                stats.nodes += child_stats.nodes;
                stats.depth = stats.depth.max(child_stats.depth + 1);
                stats.bidirectional_expands += child_stats.bidirectional_expands;
                stats.select_history_projects += child_stats.select_history_projects;
            }
            let key = node as *const Node as usize;
            memo.safe.insert(key, safe && !free_correlation);
            memo.stats.insert(key, stats);
            results.insert(key, (safe, free_correlation, stats));
        }
        // Keep operators whose key semantics differ from SQL grouping and
        // DISTINCT in the native runtime, along with every parent that would
        // otherwise absorb them into one SQL island.
        let mut pending = vec![(root, false)];
        while let Some((node, visited)) = pending.pop() {
            let key = node as *const Node as usize;
            let children = crate::ir::analysis::children(node);
            if visited {
                let local_fence = match node {
                    Node::GraphDistinct { .. } => true,
                    Node::GraphUnion { all, .. } => !all,
                    Node::GraphAggregate { group, aggs, .. } => {
                        !group.is_empty()
                            || aggs.iter().any(|agg| {
                                agg.distinct || agg.kind == crate::ir::expr::AggKind::CountDistinct
                            })
                    }
                    _ => false,
                };
                if local_fence
                    || children.iter().any(|child| {
                        memo.cypher_equality_fences
                            .contains(&(*child as *const Node as usize))
                    })
                {
                    memo.cypher_equality_fences.insert(key);
                }
            } else {
                pending.push((node, true));
                pending.extend(children.into_iter().map(|child| (child, false)));
            }
        }
        Arc::new(Mutex::new(memo))
    }
    pub fn safe(&self, node: &Node) -> bool {
        self.safe
            .get(&(node as *const Node as usize))
            .copied()
            .unwrap_or(false)
    }
    pub fn requires_cypher_equality(&self, node: &Node) -> bool {
        self.cypher_equality_fences
            .contains(&(node as *const Node as usize))
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
            // The host normalized the query before island discovery. Reuse its
            // context so separate SPARQL islands share budgets and diagnostics.
            return self.lower_normalized_in_query(&GraphPlan::new(policy.clone(), root.clone()), graph);
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
            let (scan_counter, scan_logical_labels, terminal_distinct) = {
                let memo = memo.lock().unwrap();
                (memo.next_scan, memo.scan_logical_labels, memo.terminal_distinct)
            };
            let mut ctx = LoweringContext {
                graph,
                options: self.options.clone(),
                policy: policy.clone(),
                language: policy.language,
                scan_counter,
                correlate_plan: None,
                rdf_typed_terms_used: false,
                scan_logical_labels,
                unordered_collects: BTreeSet::new(),
                unordered_distincts: terminal_distinct.into_iter().collect(),
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
            rule_context: self.options.rule_context.clone(),
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
            .filter(|m| self.correlate_plan.is_none() && self.unordered_collects.is_empty() && self.unordered_distincts.is_empty() && m.lock().unwrap().safe(node))
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
        // Semantic failures are fatal to the query. Caching them as a capability
        // miss could turn the next attempt into a different execution path.
        if matches!(&result, Err(RelError::RulePipeline(_) | RelError::Semantic(_))) { return result; }
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
#[path = "../../../tests/unit/extracted/ir__rel__island_planner__tests.rs"]
mod tests;
