//! GraphPathPattern — full property-graph path expression.
//!
//! Implements a linear chain of `Node`/`Rel` parts (the common shape
//! produced by Cypher `MATCH p = (a)-[]->(b)-[]->(c)` and Gremlin
//! `path()`). For each upstream row the matcher grounds the pattern
//! starting from `parts[0]` (a `Node` part), expanding one `Rel`+`Node`
//! pair at a time. `selector` collapses the matched paths per upstream
//! row when not `All`.

use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{Direction, LabelExpr, Length, PathPart, PathSelector, PathTies};
use crate::ir::value::Value;

use super::super::{InterpretError, IrResult, Row};
use super::expand::label_matches;
use super::source::matching_labels;

#[allow(clippy::too_many_arguments)]
pub(crate) fn path_pattern_op(
    path_binding: &str,
    selector: &PathSelector,
    parts: &[PathPart],
    upstream: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    if parts.is_empty() {
        return Ok(upstream);
    }
    let head = match &parts[0] {
        PathPart::Node { bind, labels } => (bind.clone(), labels.clone()),
        PathPart::Rel { .. } => {
            return Err(InterpretError::Unsupported(
                "GraphPathPattern: pattern must start with a Node part".into(),
            ));
        }
    };

    let mut out = Vec::new();
    for row in upstream {
        let starts = ground_first_node(&row, &head.0, &head.1, graph);
        let mut matches: Vec<Row> = Vec::new();
        for (start_label, start_id) in starts {
            let mut seed = row.clone();
            seed.bindings.insert(
                head.0.clone(),
                Value::Node {
                    label: start_label.clone(),
                    id: start_id,
                },
            );
            let initial_path = vec![Value::Node {
                label: start_label,
                id: start_id,
            }];
            walk(&parts[1..], seed, initial_path, &mut matches, graph);
        }
        // Bind the accumulated path on each match.
        for m in &mut matches {
            // path is recorded by `walk` under a temporary slot; pull it
            // out and store it on `path_binding`.
            if let Some(p) = m.bindings.remove("__path_pattern_acc") {
                m.bindings.insert(path_binding.to_string(), p);
            }
        }
        match selector {
            PathSelector::All => out.extend(matches),
            PathSelector::Any => {
                if let Some(first) = matches.into_iter().next() {
                    out.push(first);
                }
            }
            PathSelector::Shortest { k, ties } => {
                out.extend(pick_shortest(matches, *k as usize, *ties, path_binding));
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
) -> Vec<(String, i64)> {
    if let Some(Value::Node { label, id }) = row.bindings.get(bind) {
        if label_matches(label, labels) {
            return vec![(label.clone(), *id)];
        }
        return Vec::new();
    }
    let mut starts = Vec::new();
    for label in matching_labels(labels, graph) {
        let Ok(table) = graph.node_table(&label) else {
            continue;
        };
        for row_id in 0..table.batch.num_rows() {
            starts.push((label.clone(), row_id as i64));
        }
    }
    starts
}

fn walk(rest: &[PathPart], row: Row, path: Vec<Value>, out: &mut Vec<Row>, graph: &PropertyGraph) {
    if rest.is_empty() {
        let mut finished = row;
        finished
            .bindings
            .insert("__path_pattern_acc".into(), Value::Path(path));
        out.push(finished);
        return;
    }
    let (rel_bind, rel_types, dir, length) = match &rest[0] {
        PathPart::Rel {
            bind,
            types,
            dir,
            length,
        } => (bind.clone(), types, *dir, length),
        PathPart::Node { .. } => {
            // Two consecutive Node parts is malformed; bail out for this
            // branch rather than panicking.
            return;
        }
    };
    let next_node = match rest.get(1) {
        Some(PathPart::Node { bind, labels }) => (bind.clone(), labels.clone()),
        _ => return,
    };
    let Some(Value::Node { label, id }) = path.last().cloned() else {
        return;
    };
    let rel_filter = match rel_types {
        LabelExpr::Any => Vec::new(),
        LabelExpr::AnyOf(names) | LabelExpr::AllOf(names) => names.clone(),
        LabelExpr::Not(_) => Vec::new(),
    };
    expand_one_step(
        &label,
        id,
        dir,
        &rel_filter,
        length,
        rel_bind.as_deref(),
        &next_node,
        &row,
        &path,
        &rest[2..],
        out,
        graph,
    );
}

#[allow(clippy::too_many_arguments)]
fn expand_one_step(
    cur_label: &str,
    cur_id: i64,
    dir: Direction,
    rel_filter: &[String],
    length: &Length,
    rel_bind: Option<&str>,
    next_node: &(String, LabelExpr),
    row: &Row,
    path: &[Value],
    rest: &[PathPart],
    out: &mut Vec<Row>,
    graph: &PropertyGraph,
) {
    // Variable-length: try every hop count in [min, max] separately.
    let max = length.max.unwrap_or(length.min.max(1));
    let min = length.min.max(1);
    let mut frontier: Vec<(String, i64, Vec<Value>, Option<Value>)> =
        vec![(cur_label.to_string(), cur_id, path.to_vec(), None)];
    for hop in 1..=max {
        let mut next_frontier = Vec::new();
        for (cl, cid, path_so_far, _last_edge) in frontier.drain(..) {
            let edges = match dir {
                Direction::Out => graph.out_edges(&cl, cid, rel_filter),
                Direction::In => graph.in_edges(&cl, cid, rel_filter),
                Direction::Both => {
                    let mut e = graph.out_edges(&cl, cid, rel_filter);
                    e.extend(graph.in_edges(&cl, cid, rel_filter));
                    e
                }
            };
            for (rel_type, edge_row, other_label, other_id) in edges {
                let mut p = path_so_far.clone();
                let (sl, sid, dl, did) = match graph.edge_endpoints(&rel_type, edge_row) {
                    Some(t) => t,
                    None => continue,
                };
                let edge_value = Value::Edge {
                    rel_type: rel_type.clone(),
                    id: edge_row,
                    src_label: sl,
                    src_id: sid,
                    dst_label: dl,
                    dst_id: did,
                };
                p.push(edge_value.clone());
                p.push(Value::Node {
                    label: other_label.clone(),
                    id: other_id,
                });
                if hop >= min && label_matches(&other_label, &next_node.1) {
                    let mut new_row = row.clone();
                    new_row.bindings.insert(
                        next_node.0.clone(),
                        Value::Node {
                            label: other_label.clone(),
                            id: other_id,
                        },
                    );
                    if let Some(rb) = rel_bind {
                        new_row.bindings.insert(rb.to_string(), edge_value.clone());
                    }
                    walk(rest, new_row, p.clone(), out, graph);
                }
                next_frontier.push((other_label, other_id, p, Some(edge_value)));
            }
        }
        frontier = next_frontier;
        if frontier.is_empty() {
            break;
        }
    }
}

fn pick_shortest(mut matches: Vec<Row>, k: usize, ties: PathTies, path_binding: &str) -> Vec<Row> {
    if matches.is_empty() || k == 0 {
        return Vec::new();
    }
    matches.sort_by_key(|row| match row.bindings.get(path_binding) {
        Some(Value::Path(items)) => items.len(),
        _ => usize::MAX,
    });
    let shortest_len = match matches[0].bindings.get(path_binding) {
        Some(Value::Path(items)) => items.len(),
        _ => return matches.into_iter().take(k).collect(),
    };
    match ties {
        PathTies::Any => matches.into_iter().take(k).collect(),
        PathTies::All => matches
            .into_iter()
            .filter(|row| match row.bindings.get(path_binding) {
                Some(Value::Path(items)) => items.len() == shortest_len,
                _ => false,
            })
            .collect(),
    }
}
