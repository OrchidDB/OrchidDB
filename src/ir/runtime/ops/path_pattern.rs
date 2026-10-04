//! Iterative, shortest-first matching of property-graph path expressions.
//!
//! Search states carry a pattern position and an index into a flat path arena.
//! Extending a path neither recurses nor clones its prefix. Only selected results
//! materialize paths. Zero-hop pattern transitions stay at the same distance.
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{Direction, LabelExpr, PathPart, PathSelector, PathTies};
use crate::ir::policy::{MatchMode, PathMode};
use crate::ir::value::Value;

use super::super::context::ExecutionContext;
use super::super::{IrResult, Row, RuntimeError};
use super::expand::label_matches;
use super::source::matching_labels;

struct PathLink {
    parent: Option<usize>,
    node: Value,
    edge: Option<Value>,
}

struct SearchState {
    row: Arc<Row>,
    tail: usize,
    part: usize,
    hops: u64,
    // SIMPLE permits closing at the starting node, but no further expansion.
    closed: bool,
}

fn materialize(arena: &[PathLink], mut tail: usize) -> Vec<Value> {
    let mut path = Vec::new();
    loop {
        let link = &arena[tail];
        path.push(link.node.clone());
        if let Some(edge) = &link.edge {
            path.push(edge.clone());
        }
        match link.parent {
            Some(parent) => tail = parent,
            None => break,
        }
    }
    path.reverse();
    path
}

fn can_extend(
    arena: &[PathLink],
    mut tail: usize,
    edge: &Value,
    node: &Value,
    path_mode: PathMode,
    match_mode: MatchMode,
) -> Option<bool> {
    let unique_edges = match_mode == MatchMode::DifferentRelationships
        || matches!(
            path_mode,
            PathMode::Trail | PathMode::Simple | PathMode::Acyclic
        );
    let unique_nodes = matches!(path_mode, PathMode::Simple | PathMode::Acyclic);
    if !unique_edges && !unique_nodes {
        return Some(false);
    }
    let mut closed = false;
    loop {
        let link = &arena[tail];
        if unique_edges
            && link.edge.as_ref().is_some_and(|old| match (old, edge) {
                (
                    Value::Edge {
                        rel_type: a, id: x, ..
                    },
                    Value::Edge {
                        rel_type: b, id: y, ..
                    },
                ) => a == b && x == y,
                _ => false,
            })
        {
            return None;
        }
        if unique_nodes && &link.node == node {
            if path_mode == PathMode::Simple && link.parent.is_none() {
                closed = true;
            } else {
                return None;
            }
        }
        match link.parent {
            Some(parent) => tail = parent,
            None => break,
        }
    }
    Some(closed)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn path_pattern_op(
    path_binding: &str,
    selector: &PathSelector,
    parts: &[PathPart],
    path_mode: PathMode,
    match_mode: MatchMode,
    upstream: Vec<Row>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    if parts.is_empty() {
        return Ok(upstream);
    }
    // Validate once, including ranges on branches with no matching start nodes.
    for (index, part) in parts.iter().enumerate() {
        let valid = match part {
            PathPart::Node { .. } => index % 2 == 0,
            PathPart::Rel { length, .. } => {
                index % 2 == 1
                    && index + 1 < parts.len()
                    && length.max.is_none_or(|max| length.min <= max)
            }
        };
        if !valid {
            return Err(RuntimeError::Unsupported(
                "GraphPathPattern requires alternating Node/Rel/Node parts and valid length ranges"
                    .into(),
            ));
        }
    }
    if matches!(selector, PathSelector::Shortest { k: 0, .. }) {
        return Ok(vec![]);
    }
    let PathPart::Node { bind: head, labels } = &parts[0] else {
        unreachable!()
    };
    let mut out = Vec::new();
    for row in upstream {
        ctx.charge(1)?;
        let mut arena = Vec::new();
        let mut pending: BTreeMap<u64, VecDeque<SearchState>> = BTreeMap::new();
        for (label, id) in ground_first_node(&row, head, labels, graph, ctx)? {
            ctx.charge(1)?;
            let node = Value::Node { label, id };
            let mut seed = row.clone();
            seed.bindings.insert(head.clone(), node.clone());
            let tail = arena.len();
            arena.push(PathLink {
                parent: None,
                node,
                edge: None,
            });
            pending.entry(0).or_default().push_back(SearchState {
                row: Arc::new(seed),
                tail,
                part: 1,
                hops: 0,
                closed: false,
            });
        }
        let mut selected = 0_u32;
        let mut shortest = None;
        while let Some(mut entry) = pending.first_entry() {
            let distance = *entry.key();
            if shortest.is_some_and(|depth| distance > depth) {
                break;
            }
            let state = entry.get_mut().pop_front().unwrap();
            if entry.get().is_empty() {
                entry.remove();
            }
            ctx.charge(1)?;
            if state.part == parts.len() {
                ctx.charge(1)?;
                let mut result = (*state.row).clone();
                result.bindings.insert(
                    path_binding.into(),
                    Value::Path(materialize(&arena, state.tail)),
                );
                out.push(result);
                match selector {
                    PathSelector::All => (),
                    PathSelector::Any => break,
                    PathSelector::Shortest {
                        k,
                        ties: PathTies::Any,
                    } => {
                        selected += 1;
                        if selected >= *k {
                            break;
                        }
                    }
                    // Preserve the existing ALL SHORTEST contract: all ties at
                    // the minimum distance, independently of candidate order.
                    PathSelector::Shortest {
                        ties: PathTies::All,
                        ..
                    } => shortest = Some(distance),
                }
                continue;
            }
            let PathPart::Rel {
                bind: rel_bind,
                types,
                dir,
                length,
            } = &parts[state.part]
            else {
                unreachable!()
            };
            let PathPart::Node { bind, labels } = &parts[state.part + 1] else {
                unreachable!()
            };
            let Value::Node { label, id } = &arena[state.tail].node else {
                unreachable!()
            };
            let (label, id) = (label.clone(), id.clone());
            // Finish this segment without consuming an edge; in particular,
            // a zero-length segment binds the current node and a null edge.
            if state.hops >= u64::from(length.min)
                && graph.node_matches_labels(&label, id.clone(), labels)
                && state
                    .row
                    .bindings
                    .get(bind)
                    .is_none_or(|bound| bound == &arena[state.tail].node)
            {
                ctx.charge(1)?;
                let mut row = (*state.row).clone();
                row.bindings
                    .insert(bind.clone(), arena[state.tail].node.clone());
                if let Some(binding) = rel_bind {
                    row.bindings.insert(
                        binding.clone(),
                        if state.hops == 0 {
                            Value::Null
                        } else {
                            arena[state.tail].edge.clone().unwrap()
                        },
                    );
                }
                pending.entry(distance).or_default().push_back(SearchState {
                    row: Arc::new(row),
                    tail: state.tail,
                    part: state.part + 2,
                    hops: 0,
                    closed: state.closed,
                });
            }
            if state.closed
                || length.max.is_some_and(|max| state.hops >= u64::from(max))
                || shortest.is_some_and(|depth| distance >= depth)
            {
                continue;
            }
            let mut rel_filter = match types {
                LabelExpr::AnyOf(names) | LabelExpr::AllOf(names) => names.clone(),
                _ => vec![],
            };
            rel_filter.sort();
            rel_filter.dedup();
            graph.prefetch_adjacency(&[(label.clone(), id.clone())], *dir, &rel_filter);
            let mut edges = Vec::new();
            if *dir != Direction::In {
                edges.extend(
                    graph
                        .out_edges(&label, id.clone(), &rel_filter)
                        .into_iter()
                        .map(|edge| (Direction::Out, edge)),
                );
            }
            if *dir != Direction::Out {
                edges.extend(
                    graph
                        .in_edges(&label, id.clone(), &rel_filter)
                        .into_iter()
                        .map(|edge| (Direction::In, edge)),
                );
            }
            for (direction, (rel_type, edge_id, other_label, other_id)) in edges {
                ctx.charge(1)?;
                if !label_matches(&rel_type, types) {
                    continue;
                }
                if *dir == Direction::Both
                    && direction == Direction::In
                    && match_mode == MatchMode::DifferentRelationships
                    && label == other_label
                    && id == other_id
                {
                    continue;
                }
                let Some((sl, sid, dl, did)) = graph.edge_endpoints(&rel_type, edge_id.clone())
                else {
                    continue;
                };
                let edge = Value::Edge {
                    rel_type,
                    id: edge_id,
                    src_label: sl,
                    src_id: sid,
                    dst_label: dl,
                    dst_id: did,
                    projected_properties: None,
                };
                let node = Value::Node {
                    label: other_label,
                    id: other_id,
                };
                let Some(closed) =
                    can_extend(&arena, state.tail, &edge, &node, path_mode, match_mode)
                else {
                    continue;
                };
                let next_distance = distance
                    .checked_add(1)
                    .ok_or_else(|| RuntimeError::ExecutionLimit("path counter overflow".into()))?;
                let tail = arena.len();
                arena.push(PathLink {
                    parent: Some(state.tail),
                    node,
                    edge: Some(edge),
                });
                pending
                    .entry(next_distance)
                    .or_default()
                    .push_back(SearchState {
                        row: state.row.clone(),
                        tail,
                        part: state.part,
                        hops: state.hops + 1,
                        closed,
                    });
            }
        }
    }
    Ok(out)
}

fn ground_first_node(
    row: &Row,
    bind: &str,
    labels: &LabelExpr,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<(String, crate::ir::ElementId)>> {
    if let Some(value) = row.bindings.get(bind) {
        return Ok(match value {
            Value::Node { label, id } if graph.node_matches_labels(label, id.clone(), labels) => {
                vec![(label.clone(), id.clone())]
            }
            _ => vec![],
        });
    }
    let mut starts = Vec::new();
    for label in matching_labels(labels, graph) {
        let Ok(ids) = graph.node_ids(&label) else {
            continue;
        };
        for id in ids {
            ctx.charge(1)?;
            if graph.node_matches_labels(&label, id.clone(), labels) {
                starts.push((label.clone(), id));
            }
        }
    }
    Ok(starts)
}
