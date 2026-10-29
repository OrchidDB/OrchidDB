//! Execute authorized event batches through the common SQL-required engine.
//! Cypher parsing and normalization are shared across batches.
use super::*;
pub(super) struct EventProgram {
    normalized: GraphPlan,
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
                    let expr = crate::ir::IrExpr::Call { name: "cypher_id".into(), args: vec![crate::ir::IrExpr::Binding(binding.clone())] };
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
        Ok(Self { normalized, hidden })
    }
    pub async fn execute(
        &self,
        plan: &GraphPlan,
        graph: PropertyGraph,
        timeout: Duration,
    ) -> Result<RecordBatch, String> {
        self.execute_indexed(plan, graph, timeout).await
            .map(|(batch, _)| batch)
    }
    pub async fn execute_indexed(
        &self,
        plan: &GraphPlan,
        graph: PropertyGraph,
        timeout: Duration,
    ) -> Result<(RecordBatch, std::collections::HashMap<String, usize>), String> {
        // Event batches use the same SQL-required execution path as snapshots.
        // Keep the parsed/normalized Cypher plan, then bind this authorized
        // batch as Arrow input. No alternate native relational evaluator.
        let (rows, _) = tokio::time::timeout(timeout,
            crate::ir::rel::runtime::execute_rows_with_jvm(
                &self.normalized, &graph, Default::default()))
            .await.map_err(|_| "event query timed out".to_string())??;
        let Node::GraphReturn {
            fields,
            result_form,
            ..
        } = plan.root.as_ref()
        else {
            return Err("event query requires RETURN".into());
        };
        let mut index = std::collections::HashMap::with_capacity(rows.len());
        for (i, row) in rows.iter().enumerate() {
            let key = match row.bindings.get(&self.hidden) {
                Some(crate::ir::Value::String(key)) => key,
                Some(crate::ir::Value::Scalar(datafusion::common::ScalarValue::Utf8(Some(key)))) => key,
                other => return Err(format!("event identity missing: {other:?}")),
            };
            if index.insert(key.clone(), i).is_some() {
                return Err("event query produced multiple rows per identity".into());
            }
        }
        let batch = crate::ir::runtime::output::finalize_return(
            fields,
            *result_form,
            rows,
            &graph,
            &plan.policy,
        )
        .map(|r| r.batch)
        .map_err(|e| e.to_string())?;
        Ok((batch, index))
    }
}
