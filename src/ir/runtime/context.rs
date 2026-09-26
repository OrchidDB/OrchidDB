//! Execution state and native kernels used by DataFusion operators.

use std::collections::{BTreeMap, BTreeSet};
use crate::ir::catalog::PropertyGraph;
use crate::ir::plan::{Direction, LabelExpr};
use crate::ir::value::Value;
use super::expr::eval;
use super::scalar::shortest_paths;
use super::{RuntimeError, IrResult, Row};

#[derive(Debug)]
pub(crate) struct ExecutionContext {
    pub(crate) relational_groups: BTreeMap<String,crate::ir::rel::runtime::control::groups::GroupAccumulator>,
    pub(crate) sql_timeout: Option<std::time::Duration>,
    pub(crate) jvm: crate::ir::jvm::JvmExecution,
    pub(crate) random_steps: BTreeMap<String, super::ops::sample::JavaRandom>,
    pub(crate) side_effect_reducers: BTreeMap<String, String>,
    pub(crate) side_effects: BTreeMap<String, Value>,
    pub(crate) group_counts: BTreeMap<String, Vec<(Value, u64)>>,
    pub(crate) step_state: Vec<StepStateFrame>,
    step_limit: Option<u64>,
    steps: u64,
}

#[derive(Debug, Default)]
pub(crate) struct StepStateFrame {
    pub(crate) active: bool,
    pub(crate) cursor: usize,
    pub(crate) distinct_seen: Vec<BTreeSet<Vec<u8>>>,
}

impl ExecutionContext {
    pub(crate) fn side_effect_value(&mut self, label: &str, _graph: &PropertyGraph) -> IrResult<Value> {
        if let Some(value) = crate::ir::rel::runtime::control::groups::group_side_effect_value(self, label, _graph)? {return Ok(value);}
        if let Some(value) = self.side_effects.get(label) {
            if self.side_effect_reducers.get(label).is_some_and(|r| r == "tree") {
                let paths = match value { Value::BulkSet(items) => Value::List(items.clone()), other => other.clone() };
                return super::scalar::eval_call("tree_value", vec![paths], _graph);
            }
            return Ok(value.clone());
        }
        Ok(group_count_map_value(self, label))
    }

    pub(crate) fn read_side_effect(&mut self, label: &str, rows: Vec<Row>, graph: &PropertyGraph) -> IrResult<Vec<Row>> {
        let value = self.side_effect_value(label, graph)?;
        Ok(rows.into_iter().map(|mut row| {
            // Scope lookup resolves current map keys before traversal side effects.
            let selected = match row.bindings.get("current") {
                Some(Value::Map(map)) => map.get(label),
                Some(Value::TypedMap(entries)) => entries.iter().find_map(|(key, value)|
                    matches!(key, Value::String(key) if key == label).then_some(value)),
                _ => None,
            }.cloned().unwrap_or_else(|| value.clone());
            row.bindings.insert("current".into(), selected);
            row
        }).collect())
    }

    pub(crate) fn cap_side_effects(&mut self, labels: &[String], graph: &PropertyGraph) -> IrResult<Vec<Row>> {
        let value = if labels.len() == 1 { self.finalized_side_effect_value(&labels[0], graph)? } else {
            let mut entries = BTreeMap::new();
            for label in labels { entries.insert(label.clone(), self.finalized_side_effect_value(label, graph)?); }
            Value::Map(entries)
        };
        Ok(vec![Row::new().with("current", value)])
    }

    fn finalized_side_effect_value(&mut self, label: &str, graph: &PropertyGraph) -> IrResult<Value> {
        if let Some(value) = crate::ir::rel::runtime::control::groups::group_side_effect_finalize(self, label, graph)? {return Ok(value);}
        self.side_effect_value(label, graph)
    }

    const STEP_LIMIT_ENV: &'static str = "ORCHIDDB_EXECUTION_MAX_STEPS";

    pub(crate) fn charge(&mut self, units: u64) -> IrResult<()> {
        self.steps = self.steps.saturating_add(units);
        if let Some(limit) = self.step_limit {
            if self.steps > limit {
                return Err(RuntimeError::ExecutionLimit(format!(
                    "{} exceeded after {} execution steps",
                    Self::STEP_LIMIT_ENV,
                    self.steps
                )));
            }
        }
        Ok(())
    }

    pub(crate) fn push_step_state_frame(&mut self) {
        self.step_state.push(Default::default());
    }

    pub(crate) fn pop_step_state_frame(&mut self) {
        self.step_state.pop();
    }

    pub(crate) fn activate_step_state_frame(&mut self) {
        if let Some(frame) = self.step_state.last_mut() {
            frame.cursor = 0;
            frame.active = true;
        }
    }

    pub(crate) fn deactivate_step_state_frame(&mut self) {
        if let Some(frame) = self.step_state.last_mut() {
            frame.active = false;
        }
    }

    pub(crate) fn next_distinct_seen(&mut self) -> Option<&mut BTreeSet<Vec<u8>>> {
        let frame = self.step_state.last_mut()?;
        if !frame.active {
            return None;
        }
        let cursor = frame.cursor;
        frame.cursor += 1;
        if frame.distinct_seen.len() <= cursor {
            frame.distinct_seen.resize_with(cursor + 1, BTreeSet::new);
        }
        Some(&mut frame.distinct_seen[cursor])
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self {
            relational_groups: BTreeMap::new(),
            sql_timeout:None,
            jvm: Default::default(),
            random_steps: BTreeMap::new(),
            side_effect_reducers: BTreeMap::new(),
            side_effects: BTreeMap::new(),
            group_counts: BTreeMap::new(),
            step_state: Vec::new(),
            step_limit: std::env::var(Self::STEP_LIMIT_ENV)
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|limit| *limit > 0),
            steps: 0,
        }
    }
}

pub(crate) fn procedure_call_op(
    name: &str,
    args: &[crate::ir::plan::ProcedureArg],
    yields: &[String],
    upstream: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    if let Some(procedure) = graph.procedures.get(name) {
        let mut output=Vec::new();
        for row in upstream {
            let values=args.iter().map(|arg|eval(&arg.value,&row,graph)).collect::<IrResult<Vec<_>>>()?;
            if values.len()!=procedure.signature.inputs.len() || values.iter().zip(&procedure.signature.inputs).any(|(value,field)|!field.accepts(value)) {
                return Err(RuntimeError::Diagnosed {code: crate::ir::diagnostics::RuntimeDiagnosis::InvalidType,
                    message:format!("Invalid arguments to procedure {name}")});
            }
            if procedure.signature.outputs.is_empty() {output.push(row);continue;}
            for candidate in &procedure.rows {
                if !values.iter().zip(candidate).all(|(a,b)|
                    (matches!(a,Value::Null) && matches!(b,Value::Null)) || a.three_valued_eq(b)==Some(true)) {continue;}
                let mut result=row.clone();
                for field in yields {
                    let index=procedure.signature.outputs.iter().position(|output|output.name==*field)
                        .ok_or_else(||RuntimeError::Type(format!("Unknown procedure output {field}")))?;
                    result.bindings.insert(field.clone(),candidate[values.len()+index].clone());
                }
                output.push(result);
            }
        }
        return Ok(output);
    }
    if matches!(name, "gremlin.io.read" | "gremlin.io.write") {
        for row in upstream {
            let values = args.iter().map(|arg| eval(&arg.value, &row, graph)).collect::<IrResult<Vec<_>>>()?;
            let [Value::String(path), Value::String(reader)] = values.as_slice() else {
                return Err(RuntimeError::Runtime("io requires path and codec strings".into()));
            };
            if name == "gremlin.io.read" {
                super::scalar::import::read(graph, path, reader)?;
            } else {
                super::scalar::export::write(graph, path, reader)?;
            }
        }
        return Ok(vec![]);
    }
    if name.starts_with("gremlin.mutation.") {
        let mut result=Vec::with_capacity(upstream.len());
        for mut row in upstream {
            let values=args.iter().map(|arg|eval(&arg.value,&row,graph)).collect::<IrResult<Vec<_>>>()?;
            let value=super::scalar::mutations::call(name,&values,graph)?;
            if let Some(binding)=yields.first(){row.bindings.insert(binding.clone(),value);}
            result.push(row);
        }
        return Ok(result);
    }
    let _ = args;
    let normalized = name.to_ascii_lowercase();
    if yields.is_empty() {
        return Ok(upstream);
    }
    let yield_first = yields.first().map(|s| s.as_str()).unwrap_or("value");

    let values_per_call: Vec<Value> = match normalized.as_str() {
        "db.labels" => graph.labels().into_iter().map(Value::String).collect(),
        "db.relationshiptypes" => graph.rel_types().into_iter().map(Value::String).collect(),
        "db.propertykeys" => {
            let mut keys = std::collections::BTreeSet::new();
            for label in graph.labels() {
                for key in graph.node_property_keys(&label) {
                    keys.insert(key);
                }
            }
            for rel_type in graph.rel_types() {
                for key in graph.edge_property_keys(&rel_type) {
                    keys.insert(key);
                }
            }
            keys.into_iter().map(Value::String).collect()
        }
        _ => Vec::new(),
    };

    let mut out = Vec::new();
    for row in upstream {
        if values_per_call.is_empty() {
            // Unknown / unhandled procedure: pass the row through with the
            // declared yield bindings set to `Null` so downstream filters
            // stay well-typed.
            let mut new_row = row;
            for binding in yields {
                new_row.bindings.insert(binding.clone(), Value::Null);
            }
            out.push(new_row);
        } else {
            for value in &values_per_call {
                let mut new_row = row.clone();
                new_row
                    .bindings
                    .insert(yield_first.to_string(), value.clone());
                for extra in yields.iter().skip(1) {
                    new_row.bindings.insert(extra.clone(), Value::Null);
                }
                out.push(new_row);
            }
        }
    }
    Ok(out)
}

fn group_count_map_value(ctx: &ExecutionContext, label: &str) -> Value {
    let map = ctx
        .group_counts
        .get(label)
        .map(|counts| {
            counts
                .iter()
                .map(|(key, count)| (key.clone(), Value::Long(*count as i64)))
                .collect()
        })
        .unwrap_or_default();
    Value::map_from_entries(map)
}

pub(crate) fn shortest_path_op(
    source: &str,
    target: Option<&str>,
    direction: Direction,
    rel_types: &LabelExpr,
    max_distance: Option<f64>,
    include_edges: bool,
    output: &str,
    all_paths: bool,
    rows: Vec<Row>,
    graph: &PropertyGraph,
) -> IrResult<Vec<Row>> {
    let rel_filter = match rel_types {
        LabelExpr::Any => Vec::new(),
        LabelExpr::AnyOf(names) | LabelExpr::AllOf(names) => names.clone(),
        LabelExpr::Not(_) => Vec::new(),
    };
    let mut out = Vec::new();
    for row in rows {
        let start = row.bindings.get(source).unwrap_or(&Value::Null);
        let paths = match target.and_then(|binding| row.bindings.get(binding)) {
            Some(target) => shortest_paths(
                graph,
                start,
                Some(target),
                direction,
                &rel_filter,
                max_distance,
                include_edges,
            ),
            None => shortest_paths(
                graph,
                start,
                None,
                direction,
                &rel_filter,
                max_distance,
                include_edges,
            ),
        };
        match paths {
            Value::List(items) if all_paths || target.is_none() => {
                for path in items {
                    let mut next = row.clone();
                    next.bindings.insert(output.to_string(), path);
                    out.push(next);
                }
            }
            Value::List(mut items) => {
                if let Some(path) = items.pop() {
                    let mut next = row.clone();
                    next.bindings.insert(output.to_string(), path);
                    out.push(next);
                }
            }
            path => {
                let mut next = row.clone();
                next.bindings.insert(output.to_string(), path);
                out.push(next);
            }
        }
    }
    Ok(out)
}
