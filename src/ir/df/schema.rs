//! Binding schemas and expression type inference for Graph IR extensions.

use super::*;

// ============================================================
// Schema computation (per binding)
// ============================================================

pub(super) fn build_schema_for_node(node: &Node) -> DFResult<DFSchemaRef> {
    build_schema_from_fields(schema_fields_for_node(node))
}

fn build_schema_from_fields(fields: Vec<Field>) -> DFResult<DFSchemaRef> {
    let schema = Schema::new(fields);
    let df_schema = DFSchema::try_from(schema).map_err(DataFusionError::from)?;
    Ok(Arc::new(df_schema))
}

fn binding_field(name: &str) -> Field {
    semantic_field(name, DataType::Utf8, true, "value")
}

fn semantic_field(name: &str, data_type: DataType, nullable: bool, value_type: &str) -> Field {
    Field::new(name, data_type, nullable).with_metadata(HashMap::from([(
        "graph_ir.value_type".to_string(),
        value_type.to_string(),
    )]))
}

fn hidden_field(name: &str, data_type: DataType, value_type: &str) -> Field {
    Field::new(name, data_type, false).with_metadata(HashMap::from([
        ("graph_ir.value_type".to_string(), value_type.to_string()),
        ("graph_ir.hidden".to_string(), "true".to_string()),
    ]))
}

fn field_named(fields: &[Field], name: &str) -> Option<Field> {
    fields
        .iter()
        .find(|field| field.name().as_str() == name)
        .cloned()
}

fn upsert_field(fields: &mut Vec<Field>, field: Field) {
    if let Some(existing) = fields
        .iter_mut()
        .find(|existing| existing.name().as_str() == field.name().as_str())
    {
        *existing = field;
    } else {
        fields.push(field);
    }
}

fn append_missing_fields(fields: &mut Vec<Field>, incoming: Vec<Field>) {
    for field in incoming {
        if !fields
            .iter()
            .any(|existing| existing.name().as_str() == field.name().as_str())
        {
            fields.push(field);
        }
    }
}

fn schema_fields_for_node(node: &Node) -> Vec<Field> {
    match node {
        Node::GraphReturn { fields, input, .. } => {
            let input_fields = schema_fields_for_node(input);
            fields
                .iter()
                .map(|name| field_named(&input_fields, name).unwrap_or_else(|| binding_field(name)))
                .collect()
        }
        Node::GraphNodeScan { binding, .. } => {
            vec![semantic_field(binding, DataType::Utf8, true, "node")]
        }
        Node::GraphRelScan { binding, .. } => {
            vec![semantic_field(binding, DataType::Utf8, true, "edge")]
        }
        Node::GraphValues {
            bindings,
            rows,
            bulk,
        } => {
            let mut fields = bindings
                .iter()
                .enumerate()
                .map(|(idx, binding)| {
                    let values = rows.iter().filter_map(|row| row.get(idx));
                    semantic_field(binding, infer_values_type(values), true, "value")
                })
                .collect::<Vec<_>>();
            if bulk.is_some() {
                fields.push(hidden_field("_bulk", DataType::UInt64, "traverser_bulk"));
            }
            fields
        }
        Node::GraphOneRow | Node::GraphEmpty => Vec::new(),
        Node::GraphCorrelate { bindings } => {
            bindings.iter().map(|name| binding_field(name)).collect()
        }
        Node::GraphBind {
            bind,
            kind,
            expr,
            input,
            ..
        } => {
            let mut fields = schema_fields_for_node(input);
            let field = match expr {
                Some(expr) => semantic_field(bind, infer_expr_type(expr), true, "value"),
                None => {
                    let value_type = match kind {
                        BindKind::Node => "node",
                        BindKind::Edge => "edge",
                        BindKind::Scalar => "value",
                    };
                    semantic_field(bind, DataType::Utf8, true, value_type)
                }
            };
            upsert_field(&mut fields, field);
            fields
        }
        Node::GraphExpand {
            target,
            rel_binding,
            history,
            path,
            input,
            ..
        } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(
                &mut fields,
                semantic_field(target, DataType::Utf8, true, "node"),
            );
            if let Some(binding) = rel_binding {
                upsert_field(
                    &mut fields,
                    semantic_field(binding, DataType::Utf8, true, "edge"),
                );
            }
            if let Some(binding) = path {
                upsert_field(
                    &mut fields,
                    semantic_field(binding, DataType::Utf8, true, "path"),
                );
            }
            if let Some(binding) = history {
                upsert_field(
                    &mut fields,
                    semantic_field(binding, DataType::Utf8, true, "path"),
                );
            }
            fields
        }
        Node::GraphPathPattern {
            path,
            endpoints,
            parts,
            input,
            ..
        } => {
            let mut fields = schema_fields_for_node(input);
            for endpoint in endpoints {
                upsert_field(
                    &mut fields,
                    semantic_field(endpoint, DataType::Utf8, true, "node"),
                );
            }
            for part in parts {
                if let PathPart::Rel {
                    bind: Some(binding),
                    ..
                } = part
                {
                    upsert_field(
                        &mut fields,
                        semantic_field(binding, DataType::Utf8, true, "edge"),
                    );
                }
            }
            upsert_field(
                &mut fields,
                semantic_field(path, DataType::Utf8, true, "path"),
            );
            fields
        }
        Node::GraphFilter { input, .. }
        | Node::GraphSort { input, .. }
        | Node::GraphSlice { input, .. }
        | Node::GraphSliceExpr { input, .. }
        | Node::GraphBarrier { input, .. }
        | Node::GraphPathFilter { input, .. }
        | Node::GraphSetProperty { input, .. }
        | Node::GraphDelete { input, .. } => schema_fields_for_node(input),
        Node::GraphMerge {
            outputs, match_arm, ..
        } => {
            let mut fields = schema_fields_for_node(match_arm);
            for output in outputs {
                upsert_field(
                    &mut fields,
                    semantic_field(output, DataType::Utf8, true, "node"),
                );
            }
            fields
        }
        Node::GraphCreate {
            nodes,
            edges,
            input,
            ..
        } => {
            let mut fields = schema_fields_for_node(input);
            for node in nodes {
                if let Some(bind) = &node.bind {
                    upsert_field(
                        &mut fields,
                        semantic_field(bind, DataType::Utf8, true, "node"),
                    );
                }
            }
            for edge in edges {
                if let Some(bind) = &edge.bind {
                    upsert_field(
                        &mut fields,
                        semantic_field(bind, DataType::Utf8, true, "edge"),
                    );
                }
            }
            fields
        }
        Node::GraphProject {
            mode, items, input, ..
        } => match mode {
            ProjectMode::ReplaceScope => items
                .iter()
                .map(|item| semantic_field(&item.alias, infer_expr_type(&item.expr), true, "value"))
                .collect(),
            ProjectMode::ReplaceCurrent => {
                let mut fields = schema_fields_for_node(input);
                for item in items {
                    fields.retain(|field| field.name().as_str() != item.alias.as_str());
                    fields.push(semantic_field(
                        &item.alias,
                        infer_expr_type(&item.expr),
                        true,
                        "value",
                    ));
                }
                fields
            }
            ProjectMode::PreserveVisible => {
                let mut fields = schema_fields_for_node(input);
                for item in items {
                    upsert_field(
                        &mut fields,
                        semantic_field(&item.alias, infer_expr_type(&item.expr), true, "value"),
                    );
                }
                fields
            }
        },
        Node::GraphCurrentProject {
            expr,
            fields,
            input,
        } => {
            let mut out = schema_fields_for_node(input);
            for name in fields {
                out.retain(|field| field.name().as_str() != name.as_str());
                out.push(semantic_field(name, infer_expr_type(expr), true, "value"));
            }
            out
        }
        Node::GraphAggregate {
            group,
            aggs,
            fields,
            ..
        } => {
            let mut out: Vec<Field> = group
                .iter()
                .map(|item| semantic_field(&item.alias, infer_expr_type(&item.expr), true, "value"))
                .collect();
            for agg in aggs {
                upsert_field(
                    &mut out,
                    semantic_field(&agg.alias, infer_agg_type(&agg.kind), false, "aggregate"),
                );
            }
            if fields.is_empty() {
                out
            } else {
                fields
                    .iter()
                    .map(|name| field_named(&out, name).unwrap_or_else(|| binding_field(name)))
                    .collect()
            }
        }
        Node::GraphGroupMap { output, .. } => {
            vec![semantic_field(output, DataType::Utf8, true, "map")]
        }
        Node::GraphGroupCountSideEffect { input, .. } => schema_fields_for_node(input),
        Node::GraphCap { labels, .. } if labels.len() == 1 => {
            vec![semantic_field("current", DataType::Utf8, true, "map")]
        }
        Node::GraphCap { .. } => {
            vec![semantic_field("current", DataType::Utf8, true, "map")]
        }
        Node::GraphShortestPath { output, .. } => {
            vec![semantic_field(output, DataType::Utf8, true, "path")]
        }
        Node::GraphDistinct { input, .. } => schema_fields_for_node(input),
        Node::GraphJoin { left, right, .. } | Node::GraphUnion { left, right, .. } => {
            let mut fields = schema_fields_for_node(left);
            append_missing_fields(&mut fields, schema_fields_for_node(right));
            fields
        }
        Node::GraphApply {
            outputs,
            left,
            right,
            ..
        } => {
            let mut fields = schema_fields_for_node(left);
            let right_fields = schema_fields_for_node(right);
            for output in outputs {
                upsert_field(
                    &mut fields,
                    field_named(&right_fields, output).unwrap_or_else(|| binding_field(output)),
                );
            }
            fields
        }
        Node::GraphUnwind { bind, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(&mut fields, binding_field(bind));
            fields
        }
        Node::GraphQuantifier { output, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(
                &mut fields,
                semantic_field(output, DataType::Boolean, false, "bool"),
            );
            fields
        }
        Node::GraphCollect { alias, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(
                &mut fields,
                semantic_field(alias, DataType::Utf8, true, "list"),
            );
            fields
        }
        Node::GraphCoalesce { output, input, .. } | Node::GraphChoose { output, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(&mut fields, binding_field(output));
            fields
        }
        Node::GraphSelect {
            labels,
            outputs,
            input,
        } => {
            let input_fields = schema_fields_for_node(input);
            outputs
                .iter()
                .zip(labels.iter())
                .map(|(output, label)| {
                    field_named(&input_fields, label)
                        .map(|field| {
                            semantic_field(
                                output,
                                field.data_type().clone(),
                                field.is_nullable(),
                                "value",
                            )
                        })
                        .unwrap_or_else(|| binding_field(output))
                })
                .collect()
        }
        Node::GraphRepeat { seed, .. } => schema_fields_for_node(seed),
        Node::GraphProcedureCall { yields, input, .. } => {
            let mut fields = input
                .as_deref()
                .map(schema_fields_for_node)
                .unwrap_or_default();
            for yield_name in yields {
                upsert_field(&mut fields, binding_field(yield_name));
            }
            fields
        }
        Node::GraphExtension { inputs, .. } => {
            let mut fields = Vec::new();
            for input in inputs {
                append_missing_fields(&mut fields, schema_fields_for_node(input));
            }
            fields
        }
        Node::GraphSparqlTriplePattern { outputs, .. } => outputs
            .iter()
            .map(|name| semantic_field(name, DataType::Utf8, true, "rdf_term"))
            .collect(),
        Node::GraphRdfPropertyPath {
            subject, object, ..
        } => {
            let mut fields = Vec::new();
            if let RdfTerm::Variable(name) = subject {
                upsert_field(
                    &mut fields,
                    semantic_field(name, DataType::Utf8, true, "rdf_term"),
                );
            }
            if let RdfTerm::Variable(name) = object {
                upsert_field(
                    &mut fields,
                    semantic_field(name, DataType::Utf8, true, "rdf_term"),
                );
            }
            fields
        }
        Node::GraphSparqlMinus { left, .. } => schema_fields_for_node(left),
        Node::GraphService { outputs, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            for output in outputs {
                upsert_field(
                    &mut fields,
                    semantic_field(output, DataType::Utf8, true, "rdf_term"),
                );
            }
            fields
        }
        Node::GraphConstructTriples { .. } | Node::GraphDescribe { .. } => {
            vec![semantic_field(
                "_rdf_graph",
                DataType::Utf8,
                false,
                "rdf_graph",
            )]
        }
        Node::GraphAsk { field, .. } => {
            vec![semantic_field(field, DataType::Boolean, false, "bool")]
        }
        Node::GraphListComprehension { alias, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(
                &mut fields,
                semantic_field(alias, DataType::Utf8, true, "list"),
            );
            fields
        }
    }
}

fn infer_values_type<'a>(values: impl Iterator<Item = &'a Value>) -> DataType {
    let mut inferred: Option<DataType> = None;
    for value in values {
        if matches!(value, Value::Null) {
            continue;
        }
        let current = value_to_data_type(value);
        inferred = Some(match inferred {
            None => current,
            Some(existing) if existing == current => existing,
            Some(DataType::Int64) if current == DataType::Float64 => DataType::Float64,
            Some(DataType::Float64) if current == DataType::Int64 => DataType::Float64,
            Some(_) => DataType::Utf8,
        });
    }
    inferred.unwrap_or(DataType::Utf8)
}

fn value_to_data_type(value: &Value) -> DataType {
    match value {
        Value::Bool(_) => DataType::Boolean,
        Value::Int(_) => DataType::Int64,
        Value::Float(_) => DataType::Float64,
        _ => DataType::Utf8,
    }
}

fn infer_expr_type(expr: &IrExpr) -> DataType {
    match expr {
        IrExpr::Lit(lit) => match lit {
            crate::ir::expr::Lit::Null => DataType::Utf8,
            crate::ir::expr::Lit::Bool(_) => DataType::Boolean,
            crate::ir::expr::Lit::Int(_) => DataType::Int64,
            crate::ir::expr::Lit::Float(_) => DataType::Float64,
            crate::ir::expr::Lit::String(_) => DataType::Utf8,
        },
        IrExpr::Binary { op, .. } => match op {
            crate::ir::expr::BinaryOp::Eq
            | crate::ir::expr::BinaryOp::Neq
            | crate::ir::expr::BinaryOp::Lt
            | crate::ir::expr::BinaryOp::Lte
            | crate::ir::expr::BinaryOp::Gt
            | crate::ir::expr::BinaryOp::Gte
            | crate::ir::expr::BinaryOp::And
            | crate::ir::expr::BinaryOp::Or => DataType::Boolean,
            crate::ir::expr::BinaryOp::Add
            | crate::ir::expr::BinaryOp::Sub
            | crate::ir::expr::BinaryOp::Mul
            | crate::ir::expr::BinaryOp::Div => DataType::Float64,
        },
        IrExpr::Not(_)
        | IrExpr::StringPredicate { .. }
        | IrExpr::IsNull(_)
        | IrExpr::IsNotNull(_)
        | IrExpr::IsBound(_)
        | IrExpr::SimplePath(_)
        | IrExpr::HasLabel { .. } => DataType::Boolean,
        IrExpr::List(_)
        | IrExpr::ListReduce { .. }
        | IrExpr::ListTransform { .. }
        | IrExpr::ListFilter { .. }
        | IrExpr::Case { .. }
        | IrExpr::Call { .. } => DataType::Utf8,
        IrExpr::Binding(_) | IrExpr::Property { .. } | IrExpr::Id(_) | IrExpr::Label(_) => {
            DataType::Utf8
        }
    }
}

fn infer_agg_type(kind: &crate::ir::expr::AggKind) -> DataType {
    match kind {
        crate::ir::expr::AggKind::CountRows
        | crate::ir::expr::AggKind::CountBulk
        | crate::ir::expr::AggKind::CountDistinct
        | crate::ir::expr::AggKind::CountIf => DataType::Int64,
        crate::ir::expr::AggKind::Avg
        | crate::ir::expr::AggKind::AvgOrZero
        | crate::ir::expr::AggKind::AvgOrNull
        | crate::ir::expr::AggKind::StDev
        | crate::ir::expr::AggKind::StDevP
        | crate::ir::expr::AggKind::PercentileCont
        | crate::ir::expr::AggKind::PercentileDisc => DataType::Float64,
        crate::ir::expr::AggKind::Sum
        | crate::ir::expr::AggKind::SumOrZero
        | crate::ir::expr::AggKind::Min
        | crate::ir::expr::AggKind::MinOrNull
        | crate::ir::expr::AggKind::Max
        | crate::ir::expr::AggKind::MaxOrNull
        | crate::ir::expr::AggKind::CollectRows
        | crate::ir::expr::AggKind::CollectTraversers => DataType::Utf8,
    }
}
