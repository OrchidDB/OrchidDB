//! Resume relational lowering after an executable boundary. The adapter only
//! encodes values; SQL continues to own projection, filtering and aggregation.
use super::*;

impl Compiler<'_> {
    pub(super) fn lower_after_boundary(&self, node: &Node) -> Result<Option<LogicalPlan>> {
        if !self.sql || policy::boundary(node).is_some() {
            return Ok(None);
        }
        let children = crate::ir::analysis::children(node);
        if children.len() != 1 {
            return Ok(None);
        }
        let child = children[0];
        let key = child as *const Node as usize;
        let Some(input) = self.lowered.borrow().get(&key).cloned() else {
            return Ok(None);
        };
        let mut fields = crate::ir::df::boundary_fields(child)?;
        if let Node::GraphAggregate { group, aggs, .. } = node {
            let expressions = group
                .iter()
                .map(|g| &g.expr)
                .chain(aggs.iter().filter_map(|a| a.arg.as_ref()));
            if let Some(reads) = expressions
                .map(optimize::references)
                .collect::<Option<Vec<_>>>()
            {
                let reads: std::collections::BTreeSet<_> = reads.into_iter().flatten().collect();
                fields.retain(|field| reads.contains(field));
            }
        }
        let mut rewritten = node.clone();
        *crate::ir::analysis::children_mut(&mut rewritten)[0] = Node::GraphCorrelate {
            bindings: fields.clone(),
        };
        if crate::ir::analysis::contains_source_mutation(child) {
            let barrier = self.eager_sql_boundary(&rewritten, input)?;
            self.lowered.borrow_mut().remove(&key);
            return Ok(Some(barrier));
        }
        let mut bridge = match kernel("EncodeSqlBindings", vec![input], |mut rows, _| {
            Ok(rows.remove(0))
        }) {
            LogicalPlan::Extension(e) => {
                e.node.as_any().downcast_ref::<RowKernel>().unwrap().clone()
            }
            _ => unreachable!(),
        };
        bridge.boundary = Some(policy::BoundaryKind::ResultTransport);
        bridge.streaming = true;
        let mut nodes = Vec::new();
        if let Node::GraphExpand {
            source,
            target,
            target_mode,
            ..
        } = node
        {
            nodes.push(source.clone());
            if *target_mode == crate::ir::plan::TargetMode::Existing {
                nodes.push(target.clone());
            }
        }
        nodes.sort();
        nodes.dedup();
        let output = SqlOutput {
            fields: fields.clone(),
            nodes,
            policy: self.policy.clone(),
        };
        bridge.schema = Arc::new(output.schema().as_ref().clone().try_into()?);
        bridge.relational_output = Some(output);
        let source = LogicalPlan::Extension(Extension {
            node: Arc::new(bridge),
        });
        let mut context = super::super::LoweringContext {
            graph: self.graph,
            options: super::super::RelBackendOptions {
                rule_context: self.rule_context.clone(),
                mapping: self.graph.mapping.clone(),
                language_functions: self.language_functions,
                native_values: true,
                tolerate_internal_path_state: true,
                ..Default::default()
            },
            policy: self.policy.clone(),
            language: self.policy.language,
            scan_counter: 0,
            correlate_plan: Some(source),
            rdf_typed_terms_used: false,
            scan_logical_labels: true,
            unordered_collects: Default::default(),
            unordered_distincts: Default::default(),
            gremlin_label_binds: Default::default(),
            island_memo: None,
        };
        let lowered = match context.lower_node(&rewritten) {
            Ok(plan) => plan,
            Err(error) => {
                if std::env::var_os("ORCHIDDB_EXPLAIN_DAG").is_some() {
                    eprintln!("SQL reentry: {error}");
                }
                return Ok(None);
            }
        };
        let fields = lowered
            .fields
            .unwrap_or_else(|| super::super::output_fields(&lowered.plan));
        let mut decode = match kernel("DecodeTraversers", vec![lowered.plan], |mut rows, _| {
            Ok(rows.remove(0))
        }) {
            LogicalPlan::Extension(e) => {
                e.node.as_any().downcast_ref::<RowKernel>().unwrap().clone()
            }
            _ => unreachable!(),
        };
        decode.relational_input = Some(fields);
        self.lowered.borrow_mut().remove(&key);
        Ok(Some(LogicalPlan::Extension(Extension {
            node: Arc::new(decode),
        })))
    }
}

#[derive(Clone)]
pub(super) struct SqlOutput {
    fields: Vec<String>,
    nodes: Vec<String>,
    policy: crate::ir::policy::GraphPlanPolicy,
}
impl SqlOutput {
    fn schema(&self) -> arrow::datatypes::SchemaRef {
        use arrow::datatypes::{DataType, Field, Schema};
        let mut fields = super::super::native_values::result_schema(&self.fields)
            .fields()
            .to_vec();
        if fields.is_empty() {
            fields.push(Arc::new(Field::new(
                "__orchid_boundary_row",
                DataType::Boolean,
                false,
            )));
        }
        for node in &self.nodes {
            fields.push(Arc::new(Field::new(
                super::super::id_col(node),
                DataType::Int64,
                true,
            )));
            fields.push(Arc::new(Field::new(
                super::super::label_col(node),
                DataType::Utf8,
                true,
            )));
        }
        Arc::new(Schema::new(fields))
    }
    pub(super) fn encode(
        &self,
        rows: &[Row],
        graph: &PropertyGraph,
    ) -> std::result::Result<RecordBatch, QueryExecutionError> {
        use datafusion::common::ScalarValue;
        let base = super::super::native_values::result_batch(
            &self.fields,
            crate::ir::policy::ResultForm::TraverserStream,
            rows,
            graph,
            &self.policy,
        )?;
        let mut columns = base.columns().to_vec();
        if columns.is_empty() {
            columns.push(Arc::new(arrow::array::BooleanArray::from(
                vec![true; base.num_rows()],
            )));
        }
        for node in &self.nodes {
            let mut ids = Vec::new();
            let mut labels = Vec::new();
            for row in rows {
                let (id, label) = match row.get(node) {
                    Value::Node { id, label } => {
                        let value =
                            match id.scalar() {
                                ScalarValue::Int64(Some(id)) => *id,
                                _ => return Err(
                                    "SQL boundary node identity requires a compatible typed key"
                                        .into(),
                                ),
                            };
                        (Some(value), Some(label))
                    }
                    Value::Null => (None, None),
                    _ => return Err("Expansion source is not a node".into()),
                };
                let count = if self.policy.language == crate::ir::policy::Language::Gremlin {
                    usize::try_from(row.bulk).map_err(|_| "Multiplicity overflow")?
                } else {
                    1
                };
                ids.extend(std::iter::repeat_n(id, count));
                labels.extend(std::iter::repeat_n(label, count));
            }
            columns.push(Arc::new(arrow::array::Int64Array::from(ids)));
            columns.push(Arc::new(arrow::array::StringArray::from(labels)));
        }
        RecordBatch::try_new_with_options(
            self.schema(),
            columns,
            &arrow::array::RecordBatchOptions::new().with_row_count(Some(base.num_rows())),
        )
        .map_err(|e| e.to_string().into())
    }
}
