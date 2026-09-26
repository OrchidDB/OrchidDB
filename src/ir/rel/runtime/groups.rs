use super::*;
use crate::ir::expr::{AggCall, AggKind};
use crate::ir::runtime::ops::{
    aggregate::{
        checked_bulk_total, compute_aggregate, flatten_group_lists, unwrap_single_group_value,
    },
    distinct::encode_value,
};
use std::collections::BTreeMap;
const GROUP_FLATTEN_VALUE_ALIAS: &str = "__group_flatten_value";
const GROUP_UNWRAP_VALUE_ALIAS: &str = "__group_unwrap_value";
#[derive(Debug, Clone)]
pub(super) enum GroupValue {
    CountBulk,
    Aggregate(AggCall),
    Traversal {
        traversal: Box<Subplan>,
        bulk_current: bool,
    },
}
pub(super) fn group_map_op(
    key: &IrExpr,
    value: &GroupValue,
    output: &str,
    rows: Vec<Row>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    let mut groups: Vec<(Value, Vec<Row>)> = Vec::new();
    for row in rows {
        let key_value = eval(key, &row, graph)?;
        if let Some((_, rows)) = groups.iter_mut().find(|(key, _)| key == &key_value) {
            rows.push(row);
        } else {
            groups.push((key_value, vec![row]));
        }
    }
    // Java HashMap bucket order determines which group consumes the shared RNG first.
    ops::java_hashmap::java_hashmap_order(&mut groups);
    let mut entries = Vec::new();
    for (key, mut group_rows) in groups {
        // A named group can have a key but no productive prefix members;
        // count/fold still receive the genuine empty stream.
        group_rows.retain(|row| !row.bindings.contains_key("__gremlin_group_empty"));
        let value = match value {
            GroupValue::CountBulk => {
                let total = checked_bulk_total(&group_rows)?;
                Value::Long(total as i64)
            }
            GroupValue::Traversal {
                traversal,
                bulk_current,
            } => {
                let group_rows = if *bulk_current && group_suffix_is_pure(traversal) {
                    compact_group_current(group_rows)?
                } else {
                    group_rows
                };
                let value = if traversal.group_barrier {
                    // The final reduction can have a post-processing suffix;
                    // group takes its first result, just like Traversal.next().
                    run_group_traversal(traversal, group_rows, graph, ctx)?
                        .into_iter()
                        .next()
                        .and_then(|mut row| row.bindings.remove("current"))
                } else {
                    // Without a barrier the map reducer assigns the first
                    // result of each member, retaining the last productive one.
                    let mut value = None;
                    for row in group_rows {
                        if let Some(result) = run_group_traversal(traversal, vec![row], graph, ctx)?
                            .into_iter()
                            .next()
                            .and_then(|mut row| row.bindings.remove("current"))
                        {
                            value = Some(result);
                        }
                    }
                    value
                };
                let Some(value) = value else {
                    continue;
                };
                value
            }
            GroupValue::Aggregate(agg) => {
                let value = compute_aggregate(agg, &group_rows, graph)?;
                if matches!(agg.kind, AggKind::CollectTraversers)
                    && agg.alias == GROUP_FLATTEN_VALUE_ALIAS
                {
                    flatten_group_lists(value)
                } else if matches!(agg.kind, AggKind::CollectTraversers)
                    && agg.alias == GROUP_UNWRAP_VALUE_ALIAS
                {
                    unwrap_single_group_value(value)
                } else {
                    value
                }
            }
        };
        entries.push((key, value));
    }
    let map = if entries
        .iter()
        .all(|(key, _)| matches!(key, Value::String(_)))
    {
        Value::Map(
            entries
                .into_iter()
                .map(|(key, value)| {
                    let Value::String(key) = key else {
                        unreachable!()
                    };
                    (key, value)
                })
                .collect(),
        )
    } else {
        Value::TypedMap(entries)
    };
    Ok(vec![Row::new().with(output, map)])
}

/// Named groups retain contributions at their first barrier. Prefix steps,
/// including graph and side-effect writers, execute exactly once at insertion.
/// The pure reducing suffix can then be reevaluated after new contributions.
#[derive(Debug)]
pub(crate) struct GroupAccumulator {
    rows: Vec<Row>,
    reducer: GroupValue,
    cached: Option<Value>,
    finalizer: Option<Box<Subplan>>,
    finalizer_reducer: Option<AggKind>,
    finalized_seed: Option<Value>,
    finalizing: bool,
}

const GROUP_MEMBERS: &str = "__gremlin_group_members";
const GROUP_KEY: &str = "__gremlin_group_key";
const GROUP_VALUE: &str = "__gremlin_group_value";

pub(super) fn group_side_effect_write(
    ctx: &mut ExecutionContext,
    label: &str,
    key: &IrExpr,
    value: &GroupValue,
    key_input: &Subplan,
    rows: &[Row],
    graph: &PropertyGraph,
) -> IrResult<()> {
    if ctx
        .relational_groups
        .get(label)
        .is_some_and(|state| state.finalizing)
    {
        return Err(RuntimeError::Unsupported(
            "a group finalizer cannot update its own side effect".into(),
        ));
    }
    let mut finalizer = None;
    let mut finalizer_reducer = None;
    let (prefix, reducer, reducing) = match value {
        GroupValue::Traversal { traversal, .. } => {
            let mut suffix = traversal.as_ref().clone();
            let prefix = split_group_prefix(&mut suffix);
            let reducing = prefix.is_some();
            let prefix =
                prefix.unwrap_or_else(|| std::mem::replace(&mut suffix, group_members_source()));
            if !group_suffix_is_pure(&suffix) {
                let (barrier, kind) = split_writer_finalizer(&mut suffix).ok_or_else(||
                    RuntimeError::Unsupported("group writers after a barrier currently require count() or fold() as the first barrier".into()))?;
                finalizer = Some(suffix.boxed());
                finalizer_reducer = Some(kind);
                suffix = barrier;
            }
            (
                Some(prefix),
                GroupValue::Traversal {
                    traversal: suffix.boxed(),
                    // Prefix evaluation may introduce bindings used by the
                    // reducer (for example a nested group's computed key).
                    bulk_current: false,
                },
                reducing,
            )
        }
        GroupValue::Aggregate(agg) => {
            let mut reducer = agg.clone();
            if reducer.arg.is_some() {
                reducer.arg = Some(IrExpr::Binding(GROUP_VALUE.into()));
            }
            (None, GroupValue::Aggregate(reducer), true)
        }
        other => (None, other.clone(), true),
    };
    // Register even on an empty stream: cap must return an empty map.
    ctx.relational_groups
        .entry(label.to_string())
        .or_insert_with(|| GroupAccumulator {
            rows: Vec::new(),
            reducer,
            cached: None,
            finalizer,
            finalizer_reducer,
            finalized_seed: None,
            finalizing: false,
        });
    let rows = run_group_traversal(key_input, rows.to_vec(), graph, ctx)?;
    let mut groups: Vec<(Value, Vec<Row>)> = Vec::new();
    for row in &rows {
        let key_value = eval(key, row, graph)?;
        if let Some((_, members)) = groups
            .iter_mut()
            .find(|(existing, _)| existing == &key_value)
        {
            members.push(row.clone());
        } else {
            groups.push((key_value, vec![row.clone()]));
        }
    }
    for (key, members) in groups {
        let members = if matches!(
            value,
            GroupValue::Traversal {
                bulk_current: true,
                ..
            }
        ) && prefix.as_ref().is_none_or(group_suffix_is_pure)
        {
            compact_group_current(members)?
        } else {
            members
        };
        let contributions = match &prefix {
            Some(prefix) if reducing => run_group_traversal(prefix, members, graph, ctx)?,
            Some(prefix) => {
                let mut results = Vec::new();
                for member in members {
                    if let Some(row) = run_group_traversal(prefix, vec![member], graph, ctx)?
                        .into_iter()
                        .next()
                    {
                        results.push(row);
                    }
                }
                results
            }
            None => {
                let mut members = members;
                if let GroupValue::Aggregate(agg) = value {
                    if let Some(arg) = &agg.arg {
                        for member in &mut members {
                            let captured = eval(arg, member, graph)?;
                            member.bindings.insert(GROUP_VALUE.into(), captured);
                        }
                    }
                }
                members
            }
        };
        let state = ctx
            .relational_groups
            .get_mut(label)
            .expect("group registered");
        state.cached = None;
        if matches!(state.reducer, GroupValue::CountBulk) {
            let bulk = checked_bulk_total(&contributions)?;
            if let Some(existing) = state
                .rows
                .iter_mut()
                .find(|row| row.bindings.get(GROUP_KEY) == Some(&key))
            {
                existing.bulk = existing.bulk.checked_add(bulk).ok_or_else(|| {
                    RuntimeError::ExecutionLimit("group traverser bulk overflow".into())
                })?;
            } else {
                let mut row = Row::new().with(GROUP_KEY, key);
                row.bulk = bulk;
                state.rows.push(row);
            }
            continue;
        }
        // Empty inputs to count/fold still define a productive group. Keep
        // its key separately from the contribution rows below.
        if contributions.is_empty() {
            state.rows.push(
                Row::new()
                    .with(GROUP_KEY, key)
                    .with("__gremlin_group_empty", Value::Bool(true)),
            );
        } else {
            state.rows.extend(contributions.into_iter().map(|mut row| {
                row.bindings.insert(GROUP_KEY.into(), key.clone());
                row
            }));
        }
    }
    Ok(())
}

pub(crate) fn group_side_effect_value(
    ctx: &mut ExecutionContext,
    label: &str,
    graph: &PropertyGraph,
) -> IrResult<Option<Value>> {
    let Some(mut state) = ctx.relational_groups.remove(label) else {
        return Ok(None);
    };
    let result = if let Some(value) = &state.cached {
        Ok(value.clone())
    } else {
        group_map_op(
            &IrExpr::Binding(GROUP_KEY.into()),
            &state.reducer,
            "current",
            state.rows.clone(),
            graph,
            ctx,
        )
        .and_then(|mut rows| {
            let value = rows
                .pop()
                .and_then(|mut row| row.bindings.remove("current"))
                .unwrap_or(Value::Map(BTreeMap::new()));
            if let Some(kind) = state.finalizer_reducer {
                merge_finalized_group(state.finalized_seed.as_ref(), value, kind)
            } else if matches!(state.reducer, GroupValue::CountBulk) {
                merge_group_count_seed(ctx.side_effects.get(label), value)
            } else {
                Ok(value)
            }
        })
    };
    if let Ok(value) = &result {
        state.cached = Some(value.clone());
    }
    ctx.relational_groups.insert(label.to_string(), state);
    result.map(Some)
}

/// Only cap executes a group's post-barrier writer. Select reads the pending
/// barrier value. An explicit second cap deliberately executes the finalizer
/// again, as TinkerPop's SideEffectCapStep does; ordinary cache reads never do.
pub(crate) fn group_side_effect_finalize(
    ctx: &mut ExecutionContext,
    label: &str,
    graph: &PropertyGraph,
) -> IrResult<Option<Value>> {
    let Some(value) = group_side_effect_value(ctx, label, graph)? else {
        return Ok(None);
    };
    let Some(finalizer) = ctx
        .relational_groups
        .get(label)
        .and_then(|state| state.finalizer.clone())
    else {
        return Ok(Some(value));
    };
    let state = ctx.relational_groups.get_mut(label).expect("group exists");
    if state.finalizing {
        return Err(RuntimeError::Unsupported(
            "recursive group finalization".into(),
        ));
    }
    state.finalizing = true;
    let result = (|| {
        let mut entries = group_map_entries(&value)?;
        for (_, value) in &mut entries {
            let rows = run_group_traversal(
                &finalizer,
                vec![Row::new().with("current", value.clone())],
                graph,
                ctx,
            )?;
            if let Some(result) = rows
                .into_iter()
                .next()
                .and_then(|mut row| row.bindings.remove("current"))
            {
                *value = result;
            }
        }
        Ok(Value::map_from_entries(entries))
    })();
    let state = ctx.relational_groups.get_mut(label).expect("group exists");
    state.finalizing = false;
    if let Ok(value) = &result {
        // Subsequent contributions reduce into the actual published map,
        // including any value transformation performed by the finalizer.
        state.rows.clear();
        state.finalized_seed = Some(value.clone());
        state.cached = Some(value.clone());
    }
    result.map(Some)
}

fn group_map_entries(value: &Value) -> IrResult<Vec<(Value, Value)>> {
    match value {
        Value::Map(entries) => Ok(entries
            .iter()
            .map(|(key, value)| (Value::String(key.clone()), value.clone()))
            .collect()),
        Value::TypedMap(entries) => Ok(entries.clone()),
        _ => Err(RuntimeError::Type(
            "group side effect must be a map".into(),
        )),
    }
}

fn merge_finalized_group(seed: Option<&Value>, value: Value, kind: AggKind) -> IrResult<Value> {
    let Some(seed) = seed else { return Ok(value) };
    let mut entries = group_map_entries(seed)?;
    for (key, incoming) in group_map_entries(&value)? {
        if let Some((_, existing)) = entries.iter_mut().find(|(candidate, _)| candidate == &key) {
            match (kind, existing, incoming) {
                (AggKind::CountBulk, Value::Long(previous), Value::Long(count)) => {
                    *previous = previous.wrapping_add(count)
                }
                (AggKind::CollectTraversers, Value::List(previous), Value::List(values)) => {
                    previous.extend(values)
                }
                _ => {
                    return Err(RuntimeError::Type(
                        "published group value is incompatible with its barrier reducer".into(),
                    ));
                }
            }
        } else {
            entries.push((key, incoming));
        }
    }
    Ok(Value::map_from_entries(entries))
}

/// Detach the first supported reducing barrier from its post-processing
/// traversal. Its prefix has already been captured at contribution insertion.
/// The registered map is the initial reducer value. Merge it with accumulated
/// contributions when publishing, never with a previously published result:
/// repeated reads and cache invalidation must not add the seed again.
fn merge_group_count_seed(seed: Option<&Value>, counts: Value) -> IrResult<Value> {
    let Some(seed) = seed else { return Ok(counts) };
    let mut entries = match seed {
        Value::Map(entries) => entries
            .iter()
            .map(|(key, value)| (Value::String(key.clone()), value.clone()))
            .collect::<Vec<_>>(),
        Value::TypedMap(entries) => entries.clone(),
        _ => {
            return Err(RuntimeError::Type(
                "groupCount side-effect seed must be a map".into(),
            ));
        }
    };
    let counts = match counts {
        Value::Map(entries) => entries
            .into_iter()
            .map(|(key, value)| (Value::String(key), value))
            .collect(),
        Value::TypedMap(entries) => entries,
        _ => unreachable!("groupCount produces a map"),
    };
    for (key, count) in counts {
        if let Some((_, value)) = entries.iter_mut().find(|(existing, _)| existing == &key) {
            let (Value::Long(initial), Value::Long(count)) = (&*value, count) else {
                return Err(RuntimeError::Type(
                    "groupCount side-effect counts must be longs".into(),
                ));
            };
            // Match the JVM Long reducer, including its signed overflow behavior.
            *value = Value::Long(initial.wrapping_add(count));
        } else {
            entries.push((key, count));
        }
    }
    Ok(Value::map_from_entries(entries))
}

/// Called only after the planner proves the value traversal cannot observe
/// member order or inherited labels/path/sack/loop state. Preserve multiplicity
/// in bulk; weighted reducers and graph expansion consume that bulk directly.
fn compact_group_current(rows: Vec<Row>) -> IrResult<Vec<Row>> {
    if rows.iter().any(|row| {
        row.bindings.contains_key("__sack")
            || row.bindings.get("__bulk_enabled") == Some(&Value::Bool(false))
    }) {
        return Ok(rows);
    }
    let mut compacted: Vec<Row> = Vec::new();
    let mut positions = BTreeMap::<Vec<u8>, usize>::new();
    for mut row in rows {
        let key = encode_value(row.bindings.get("current").unwrap_or(&Value::Null));
        if let Some(index) = positions.get(&key) {
            let previous = &mut compacted[*index];
            previous.bulk = previous.bulk.checked_add(row.bulk).ok_or_else(|| {
                RuntimeError::ExecutionLimit("group traverser bulk overflow".into())
            })?;
        } else {
            row.bindings.retain(|binding, _| {
                binding == "current"
                    || binding == "__bulk_enabled"
                    || binding == "__gremlin_bulk_safe"
            });
            positions.insert(key, compacted.len());
            compacted.push(row);
        }
    }
    Ok(compacted)
}

fn run_group_traversal(
    traversal: &Subplan,
    rows: Vec<Row>,
    graph: &PropertyGraph,
    ctx: &mut ExecutionContext,
) -> IrResult<Vec<Row>> {
    // A group's local barriers must not borrow persistent dedup state from
    // an enclosing repeat or from a different group.
    ctx.push_step_state_frame();
    let result = run_body_with_frontier(traversal, rows, graph, ctx);
    ctx.pop_step_state_frame();
    result
}

fn group_suffix_is_pure(plan: &Subplan) -> bool {
    !plan.observable
}
fn split_group_prefix(plan: &mut Subplan) -> Option<Subplan> {
    let (prefix, suffix) = plan.group_split.take()?;
    *plan = *suffix;
    Some(*prefix)
}
fn split_writer_finalizer(plan: &mut Subplan) -> Option<(Subplan, AggKind)> {
    let (barrier, suffix, kind) = plan.writer_split.take()?;
    *plan = *suffix;
    Some((*barrier, kind))
}
fn group_members_source() -> Subplan {
    Subplan {
        plan: kernel("GroupMembers", vec![], |_, s| Ok(s.frontier.clone())),
        prepared: Default::default(),
        observable: false,
        batchable: false,
        batch_names: Default::default(),
        barrier: false,
        group_barrier: false,
        group_split: None,
        writer_split: None,
    }
}
impl Subplan {
    fn boxed(self) -> Box<Self> {
        Box::new(self)
    }
}
impl Compiler<'_> {
    pub(super) fn group_value(&self, value: &crate::ir::plan::GroupValue) -> Result<GroupValue> {
        Ok(match value {
            crate::ir::plan::GroupValue::CountBulk => GroupValue::CountBulk,
            crate::ir::plan::GroupValue::Aggregate(a) => GroupValue::Aggregate(a.clone()),
            crate::ir::plan::GroupValue::Traversal {
                traversal,
                bulk_current,
            } => {
                let mut compiled = self.subplan(traversal)?;
                let mut suffix = traversal.as_ref().clone();
                if let Some(prefix) = ops::aggregate::split_group_prefix(&mut suffix) {
                    let mut compiled_suffix = self.subplan(&suffix)?;
                    if compiled_suffix.observable {
                        let mut finalizer = suffix.clone();
                        if let Some((barrier, kind)) =
                            ops::aggregate::split_writer_finalizer(&mut finalizer)
                        {
                            compiled_suffix.writer_split = Some((
                                Box::new(self.subplan(&barrier)?),
                                Box::new(self.subplan(&finalizer)?),
                                kind,
                            ));
                        }
                    }
                    compiled.group_split =
                        Some((Box::new(self.subplan(&prefix)?), Box::new(compiled_suffix)));
                }
                GroupValue::Traversal {
                    traversal: Box::new(compiled),
                    bulk_current: *bulk_current,
                }
            }
        })
    }
}
