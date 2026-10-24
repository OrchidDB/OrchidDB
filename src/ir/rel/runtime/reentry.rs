//! Resume relational lowering after an executable boundary. The adapter only
//! encodes values; SQL continues to own projection, filtering and aggregation.
use super::*;

impl Compiler<'_> {
    // Each arm may end at a different executable boundary. Encode the same
    // binding schema for both, then let host SQL concatenate their streams.
    fn lower_union_after_boundary(&self, node: &Node) -> Result<Option<LogicalPlan>> {
        let children = crate::ir::analysis::children(node);
        let keys = children.iter().map(|child| *child as *const Node as usize).collect::<Vec<_>>();
        let inputs = keys.iter().map(|key| self.lowered.borrow().get(key).cloned()).collect::<Option<Vec<_>>>();
        let Some(inputs) = inputs else { return Ok(None); };
        let mut fields = vec!["current".to_string()];
        for child in children {
            for field in crate::ir::df::boundary_fields(child)? {
                if field != "__gremlin_group_members" && !fields.contains(&field) { fields.push(field); }
            }
        }
        let output = SqlOutput {
            implicit_fields: fields.clone(), fields, nodes: Vec::new(),
            policy: self.policy.clone(), preserve_bulk: true,
        };
        let mut encoded = Vec::new();
        for input in inputs {
            let LogicalPlan::Extension(extension) = kernel("EncodeSqlBindings", vec![input], |mut rows, _| Ok(rows.remove(0))) else { unreachable!() };
            let mut bridge = extension.node.as_any().downcast_ref::<RowKernel>().unwrap().clone();
            bridge.boundary = Some(policy::BoundaryKind::ResultTransport);
            bridge.streaming = true;
            bridge.schema = Arc::new(output.schema().as_ref().clone().try_into()?);
            bridge.relational_output = Some(output.clone());
            encoded.push(LogicalPlan::Extension(Extension { node: Arc::new(bridge) }));
        }
        let right = encoded.pop().unwrap();
        let union = datafusion::logical_expr::LogicalPlanBuilder::from(encoded.pop().unwrap()).union(right)?.build()?;
        let fields = super::super::output_fields(&union);
        let LogicalPlan::Extension(extension) = kernel("DecodeTraversers", vec![union], |mut rows, _| Ok(rows.remove(0))) else { unreachable!() };
        let mut decode = extension.node.as_any().downcast_ref::<RowKernel>().unwrap().clone();
        decode.relational_input = Some(fields);
        decode.relational_input_state = self.policy.language == crate::ir::policy::Language::Gremlin;
        for key in keys { self.lowered.borrow_mut().remove(&key); }
        Ok(Some(LogicalPlan::Extension(Extension { node: Arc::new(decode) })))
    }

    pub(super) fn lower_after_boundary(&self, node: &Node) -> Result<Option<LogicalPlan>> {
        if !self.sql || policy::boundary(node).is_some() {
            return Ok(None);
        }
        if matches!(node, Node::GraphUnion { all: true, .. })
            && self.policy.language == crate::ir::policy::Language::Gremlin
        {
            return self.lower_union_after_boundary(node);
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
        // Visible frontier schemas may also include labels that a particular
        // traverser has not bound. Preserve absence for those columns too;
        // match compatibility must not compare an absent label with NULL.
        let mut implicit_fields = if self.policy.language == crate::ir::policy::Language::Gremlin {
            fields.clone()
        } else { Vec::new() };
        if self.policy.language == crate::ir::policy::Language::Gremlin {
            // Graph IR's visible schema omits implicit traverser state. A SQL
            // island must transport it just as CorrelatedInput does, otherwise
            // repeat/path/sack consumers see a newly initialized traverser.
            for name in ["current", "__path", "__path_labels", "__edge_other", "__loops",
                "__sack", "__sack_merge", "__bulk_enabled", "__gremlin_bulk_safe",
                "__repeat_loop_stack"] {
                if !fields.iter().any(|field| field == name) {
                    fields.push(name.into());
                    implicit_fields.push(name.to_string());
                }
            }
        }
        if self.policy.language == crate::ir::policy::Language::Gremlin {
            // Group/branch frontiers deliberately have a dynamic binding
            // scope. Include the free inputs of this SQL operator rather than
            // treating the sentinel's visible schema as the complete row.
            crate::ir::rules::graph::node_exprs_ref(node, &Default::default(), &mut |expr, scope| {
                use crate::ir::expr::IrExpr;
                let name = match expr {
                    IrExpr::Binding(name) | IrExpr::IsBound(name) | IrExpr::Id(name)
                    | IrExpr::Label(name) | IrExpr::SimplePath(name)
                    | IrExpr::Property { binding: name, .. } | IrExpr::HasLabel { binding: name, .. }
                        if !scope.is_bound(name) => name,
                    _ => return,
                };
                if !fields.contains(name) {
                    fields.push(name.clone());
                    implicit_fields.push(name.clone());
                }
            });
            fields.retain(|field| field != "__gremlin_group_members");
        }
        if matches!(node, Node::GraphExpand { .. }) {
            // Expansion creates the orientation marker for its new edge.
            // The previous edge's marker is no longer in scope.
            fields.retain(|field| field != "__edge_other");
        }
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
        // A newly assigned NULL is present, unlike a missing input binding.
        implicit_fields.retain(|name| match node {
            Node::GraphProject { items, .. } => !items.iter().any(|item| &item.alias == name),
            Node::GraphBind { bind, .. } | Node::GraphUnwind { bind, .. } => bind != name,
            _ => true,
        });
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
            implicit_fields,
            preserve_bulk: self.policy.language == crate::ir::policy::Language::Gremlin
                && matches!(node, Node::GraphFilter { .. } | Node::GraphBind { .. }
                    | Node::GraphExpand { .. } | Node::GraphUnwind { .. }
                    | Node::GraphDistinct { bulk: crate::ir::plan::DistinctBulk::ResetToOne, .. }
                    | Node::GraphSort { .. }
                    | Node::GraphProject { mode: crate::ir::plan::ProjectMode::PreserveVisible
                        | crate::ir::plan::ProjectMode::ReplaceCurrent, .. }),
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
        let mut lowered = lowered;
        if matches!(node, Node::GraphDistinct { bulk: crate::ir::plan::DistinctBulk::ResetToOne, .. }) {
            let columns = lowered.plan.schema().columns().into_iter()
                .filter(|column| column.name != "__orchid_traverser_bulk")
                .map(datafusion::logical_expr::Expr::Column).collect::<Vec<_>>();
            lowered.plan = datafusion::logical_expr::LogicalPlanBuilder::from(lowered.plan)
                .project(columns)?.build()?;
            lowered.fields = None;
        }
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
        decode.relational_input_state = self.policy.language == crate::ir::policy::Language::Gremlin;
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
    preserve_bulk: bool,
    implicit_fields: Vec<String>,
}
impl SqlOutput {
    fn schema(&self) -> arrow::datatypes::SchemaRef {
        use arrow::datatypes::{DataType, Field, Schema};
        let mut names = self.fields.clone();
        if self.policy.language == crate::ir::policy::Language::Gremlin {
            names.push("__orchid_absent_state".into());
            names.push("__orchid_extra_bindings".into());
        }
        let mut fields = super::super::native_values::result_schema(&names).fields().to_vec();
        if fields.is_empty() {
            fields.push(Arc::new(Field::new(
                "__orchid_boundary_row",
                DataType::Boolean,
                false,
            )));
        }
        if self.preserve_bulk {
            fields.push(Arc::new(Field::new("__orchid_traverser_bulk", DataType::UInt64, false)));
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
        let mut names = self.fields.clone();
        let encoded_rows;
        let rows = if self.policy.language != crate::ir::policy::Language::Gremlin { rows } else {
            names.push("__orchid_absent_state".into());
            names.push("__orchid_extra_bindings".into());
            encoded_rows = rows.iter().map(|row| {
                let mut encoded = row.clone();
                let absent = self.implicit_fields.iter().filter(|name| !row.bindings.contains_key(*name))
                    .cloned().map(Value::String).collect();
                encoded.bindings.insert("__orchid_absent_state".into(), Value::List(absent));
                // Dynamic frontier labels remain opaque transport until an
                // operator explicitly reads them into its SQL input schema.
                let extra = row.bindings.iter().filter(|(name, _)| !self.fields.contains(name))
                    .map(|(name, value)| (name.clone(), value.clone())).collect();
                encoded.bindings.insert("__orchid_extra_bindings".into(), Value::Map(extra));
                encoded
            }).collect::<Vec<_>>();
            &encoded_rows
        };
        let base = super::super::native_values::result_batch(
            &names,
            if self.preserve_bulk { crate::ir::policy::ResultForm::RowSet }
                else { crate::ir::policy::ResultForm::TraverserStream },
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
        if self.preserve_bulk {
            columns.push(Arc::new(arrow::array::UInt64Array::from(
                rows.iter().map(|row| row.bulk).collect::<Vec<_>>())));
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
                let count = if !self.preserve_bulk && self.policy.language == crate::ir::policy::Language::Gremlin {
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
