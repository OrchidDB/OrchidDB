//! Execute a prepared event-local program through the existing host kernels.
//! No per-message DataFusion physical planning or alternate Cypher evaluator.
use super::*;
use crate::ir::{
    Row,
    rel::runtime::{
        CompileOptions, KernelState, SubplanRunner, compile_for_host,
        program::{CompiledProgram, StageOperation},
    },
    runtime::{IrResult, RuntimeError},
};
#[derive(Debug)]
struct NoSubplans;
impl SubplanRunner for NoSubplans {
    fn run(
        &self,
        _: &datafusion::logical_expr::LogicalPlan,
        _: &mut KernelState,
    ) -> IrResult<Vec<Row>> {
        Err(RuntimeError::Unsupported(
            "event program contains a correlated subplan".into(),
        ))
    }
}
pub(super) struct EventProgram {
    program: CompiledProgram,
    hidden: String,
}
impl EventProgram {
    pub fn new(plan: &GraphPlan) -> Result<Self, String> {
        let mut normalized = plan.clone();
        // MATCH is lowered with an identity input. Eliminate only the proven
        // one-row, uncorrelated inner apply, not joins or optional matches.
        fn normalize(node: &mut Node) {
            if let Node::GraphApply {
                kind: crate::ir::ApplyKind::Inner,
                correlation,
                left,
                right,
                ..
            } = node
            {
                if correlation.is_empty() && matches!(left.as_ref(), Node::GraphOneRow) {
                    *node = *std::mem::replace(right, Box::new(Node::GraphEmpty));
                }
            }
            for child in crate::ir::analysis::children_mut(node) {
                normalize(child);
            }
        }
        normalize(&mut normalized.root);
        let debug = format!("{normalized:?}");
        let mut hidden = "__orchid_event_identity".to_string();
        while debug.contains(&hidden) {
            hidden.push('_');
        }
        fn identity(node: &mut Node, hidden: &str) {
            match node {
                Node::GraphNodeScan { binding, .. } => {
                    let expr = crate::ir::IrExpr::Id(binding.clone());
                    let input = Box::new(std::mem::replace(node, Node::GraphEmpty));
                    *node = Node::GraphBind {
                        bind: hidden.into(),
                        kind: crate::ir::BindKind::Scalar,
                        expr: Some(expr),
                        input,
                    };
                    return;
                }
                Node::GraphProject {
                    mode: crate::ir::ProjectMode::ReplaceScope,
                    items,
                    ..
                } => items.push(crate::ir::ProjectionItem {
                    alias: hidden.into(),
                    expr: crate::ir::IrExpr::Binding(hidden.into()),
                }),
                _ => (),
            }
            for child in crate::ir::analysis::children_mut(node) {
                identity(child, hidden);
            }
        }
        identity(&mut normalized.root, &hidden);
        let Node::GraphReturn { fields, .. } = normalized.root.as_mut() else {
            return Err("event query requires RETURN".into());
        };
        fields.push(hidden.clone());
        let logical = compile_for_host(
            &normalized,
            &PropertyGraph::new(),
            CompileOptions {
                rule_context: Default::default(),
                sql_islands: false,
                language_functions: false,
                native_values: false,
                prune_return: true,
            },
        )
        .map_err(|e| e.to_string())?;
        let program = CompiledProgram::new(&logical, &Default::default()).map_err(|e| e.to_string())?;
        for stage in &program.stages {
            if !matches!(stage.operation, StageOperation::Kernel(_)) {
                return Err("event program requires a native kernel".into());
            }
        }
        Ok(Self { program, hidden })
    }
    pub fn execute(
        &self,
        plan: &GraphPlan,
        graph: PropertyGraph,
        timeout: Duration,
    ) -> Result<RecordBatch, String> {
        self.execute_indexed(plan, graph, timeout)
            .map(|(batch, _)| batch)
    }
    pub fn execute_indexed(
        &self,
        plan: &GraphPlan,
        graph: PropertyGraph,
        timeout: Duration,
    ) -> Result<(RecordBatch, std::collections::HashMap<String, usize>), String> {
        let mut state = KernelState::for_host(graph, Arc::new(NoSubplans), &Default::default());
        let now = chrono::Utc::now().timestamp_micros();
        state.start_clocks(now, now).map_err(|e| e.to_string())?;
        state.set_deadline(Instant::now().checked_add(timeout));
        let mut outputs: Vec<Vec<Row>> =
            (0..self.program.stages.len()).map(|_| Vec::new()).collect();
        for stage in &self.program.stages {
            state.check().map_err(|e| e.to_string())?;
            let StageOperation::Kernel(kernel) = &stage.operation else {
                unreachable!()
            };
            let rows = if let Some(mut cursor) = kernel.source_cursor() {
                let mut rows = Vec::new();
                while let Some(batch) = cursor.next(1024, &mut state).map_err(|e| e.to_string())? {
                    rows.extend(batch);
                }
                rows
            } else {
                let inputs = stage
                    .inputs
                    .iter()
                    .map(|i| std::mem::take(&mut outputs[*i]))
                    .collect();
                kernel
                    .invoke(inputs, &mut state)
                    .map_err(|e| e.to_string())?
            };
            outputs[stage.id] = rows;
        }
        let Node::GraphReturn {
            fields,
            result_form,
            ..
        } = plan.root.as_ref()
        else {
            return Err("event query requires RETURN".into());
        };
        let rows = std::mem::take(&mut outputs[self.program.root]);
        let mut index = std::collections::HashMap::with_capacity(rows.len());
        for (i, row) in rows.iter().enumerate() {
            let Some(crate::ir::Value::String(key)) = row.bindings.get(&self.hidden) else {
                return Err("event identity missing".into());
            };
            if index.insert(key.clone(), i).is_some() {
                return Err("event query produced multiple rows per identity".into());
            }
        }
        let batch = crate::ir::runtime::output::finalize_return(
            fields,
            *result_form,
            rows,
            &state.graph,
            &plan.policy,
        )
        .map(|r| r.batch)
        .map_err(|e| e.to_string())?;
        Ok((batch, index))
    }
}
use std::time::Instant;
