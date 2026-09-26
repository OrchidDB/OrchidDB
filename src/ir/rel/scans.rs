//! Graph scans, property schemas, and Arrow materialization.

use super::*;

impl<'a> LoweringContext<'a> {
    pub(super) fn lower_node_scan(
        &mut self,
        binding: &str,
        labels: &LabelExpr,
    ) -> RelResult<LoweredNode> {
        if let Some(user_mapping) = self.options.mapping.clone() {
            return mapping::lower_mapped_node_scan(self, &user_mapping, binding, labels);
        }
        let predicate = labels;
        let labels = self.node_labels(labels)?;
        let prop_defs = self.node_property_defs(&labels)?;
        let schema = node_schema(binding, &prop_defs);
        let mut batches = Vec::new();
        for label in labels {
            if !self.graph.has_mutations() {
                let table = match self.graph.node_table(&label) {
                    Ok(table) => table,
                    Err(CatalogError::UnknownLabel(_)) => continue,
                    Err(err) => return Err(err.into()),
                };
                batches.push(normalize_node_table(
                    binding,
                    table,
                    &prop_defs,
                    schema.clone(),
                    self.language,
                )?);
                continue;
            }
            let ids = match self.graph.node_ids(&label) {
                Ok(ids) => ids,
                Err(CatalogError::UnknownLabel(_)) => continue,
                Err(err) => return Err(err.into()),
            };
            let ids = ids.into_iter().filter(|id| self.graph.node_matches_labels(&label, *id, predicate)).collect::<Vec<_>>();
            if ids.is_empty() {
                continue;
            }
            batches.push(materialize_node_scan_batch(
                binding,
                &label,
                &ids,
                self.graph,
                &prop_defs,
                schema.clone(),
                self.language,
            )?);
        }
        if batches.is_empty() {
            batches.push(RecordBatch::new_empty(schema));
        }
        let batches = collections::with_list_shadows(
            self.graph,
            self.language,
            binding,
            &list_shadow_candidates(&prop_defs),
            false,
            batches,
        )?;
        self.scan_batches_keyed("nodes", batches, &[id_col(binding), label_col(binding)])
    }

    pub(super) fn lower_rel_scan(
        &mut self,
        binding: &str,
        types: &LabelExpr,
    ) -> RelResult<LoweredNode> {
        if let Some(user_mapping) = self.options.mapping.clone() {
            return mapping::lower_mapped_rel_scan(self, &user_mapping, binding, types);
        }
        let rel_types = self.rel_types(types)?;
        let prop_defs = self.edge_property_defs(&rel_types)?;
        let schema = edge_schema(binding, &prop_defs);
        let mut batches = Vec::new();
        for rel_type in rel_types {
            if !self.graph.has_mutations() {
                let tables = match self.graph.edge_tables(&rel_type) {
                    Ok(tables) => tables,
                    Err(CatalogError::UnknownRelType(_)) => continue,
                    Err(err) => return Err(err.into()),
                };
                let mut base_id = 0;
                for table in tables {
                    batches.push(normalize_edge_table(
                        binding,
                        table,
                        base_id,
                        &prop_defs,
                        schema.clone(),
                        self.language,
                    )?);
                    base_id += table.batch.num_rows() as i64;
                }
                continue;
            }
            let ids = self.graph.edge_ids(&rel_type);
            if ids.is_empty() {
                continue;
            }
            batches.push(materialize_edge_scan_batch(
                binding,
                &rel_type,
                &ids,
                self.graph,
                &prop_defs,
                schema.clone(),
                self.language,
            )?);
        }
        if batches.is_empty() {
            batches.push(RecordBatch::new_empty(schema));
        }
        let batches = collections::with_list_shadows(
            self.graph,
            self.language,
            binding,
            &list_shadow_candidates(&prop_defs),
            true,
            batches,
        )?;
        self.scan_batches_keyed("edges", batches, &[id_col(binding), label_col(binding)])
    }

    pub(super) fn lower_values(
        &mut self,
        bindings: &[String],
        rows: &[Vec<Value>],
    ) -> RelResult<LoweredNode> {
        if matches!(self.language, Language::Gremlin | Language::Cypher) {
            // Arrow columns have a single physical type. Mixed graph-language values,
            // nested collections and arbitrary-precision numbers must remain native;
            // rendering them as text loses identity, ordering and numeric equality.
            for index in 0..bindings.len() {
                let values = rows
                    .iter()
                    .filter_map(|row| row.get(index))
                    .collect::<Vec<_>>();
                if homogeneous_scalar_type(values.iter().copied()).is_none()
                    && !(self.options.mapping.is_some() && values.iter().all(|v|matches!(v,Value::Null))) {
                    return Err(RelError::Unsupported(
                        "Heterogeneous graph-language values require native runtime types".into(),
                    ));
                }
            }
        }
        fn typed_key_value(value: &Value) -> bool {
            match value {
                Value::TypedMap(_) | Value::MapEntry(_) | Value::CardinalityValue {..} | Value::Token(_) | Value::Direction(_) => {
                    true
                }
                Value::List(items) | Value::Path(items) => items.iter().any(typed_key_value),
                Value::Map(items) => items.values().any(typed_key_value),
                _ => false,
            }
        }
        if rows.iter().flatten().any(typed_key_value) {
            return Err(RelError::Unsupported(
                "Typed map keys require native runtime values".into(),
            ));
        }
        let batch = values_batch(self.language, bindings, rows)?;
        self.scan_batches("values", vec![batch])
    }

    pub(super) fn scan_batches(
        &mut self,
        prefix: &str,
        batches: Vec<RecordBatch>,
    ) -> RelResult<LoweredNode> {
        self.scan_batches_keyed(prefix, batches, &[])
    }

    fn scan_batches_keyed(
        &mut self,
        prefix: &str,
        batches: Vec<RecordBatch>,
        primary_key: &[String],
    ) -> RelResult<LoweredNode> {
        let schema = batches
            .first()
            .map(RecordBatch::schema)
            .unwrap_or_else(|| Arc::new(Schema::empty()));
        // Catalog scans generate identity as (row id, label/type). Mapped
        // sources use their own lowering and do not receive an assumed key.
        let constraints = if primary_key.is_empty() {
            datafusion::common::Constraints::default()
        } else {
            let indices = primary_key.iter().map(|name| schema.index_of(name))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            datafusion::common::Constraints::new_unverified(vec![
                datafusion::common::Constraint::PrimaryKey(indices),
            ])
        };
        let provider = Arc::new(
            MemTable::try_new(schema, vec![batches])?.with_constraints(constraints)
        );
        let table_name = format!("__graph_rel_{}_{}", prefix, self.scan_counter);
        self.scan_counter += 1;
        let plan =
            LogicalPlanBuilder::scan(table_name, provider_as_source(provider), None)?.build()?;
        Ok(LoweredNode::new(plan))
    }

    pub(super) fn node_labels(&self, labels: &LabelExpr) -> RelResult<Vec<String>> {
        let mut out = match labels {
            LabelExpr::Any => self.graph.labels(),
            LabelExpr::AnyOf(labels) => labels.clone(),
            LabelExpr::AllOf(labels) => {
                if self.graph.has_mutations() { self.graph.labels() }
                else if labels.len() == 1 { labels.clone() }
                else { Vec::new() }
            }
            LabelExpr::Not(_) => return Err(RelError::Unsupported("negated label scan".into())),
        };
        out.sort();
        out.dedup();
        Ok(out)
    }

    pub(super) fn rel_types(&self, types: &LabelExpr) -> RelResult<Vec<String>> {
        let mut out = match types {
            LabelExpr::Any => self.graph.rel_types(),
            LabelExpr::AnyOf(types) => types.clone(),
            LabelExpr::AllOf(types) if types.len() == 1 => types.clone(),
            LabelExpr::AllOf(types) => {
                return Err(RelError::Unsupported(format!(
                    "multi-type relationship scan {types:?}"
                )));
            }
            LabelExpr::Not(_) => {
                return Err(RelError::Unsupported(
                    "negated relationship type scan".into(),
                ));
            }
        };
        out.sort();
        out.dedup();
        Ok(out)
    }

    pub(super) fn node_property_defs(&self, labels: &[String]) -> RelResult<Vec<PropertyDef>> {
        let mut defs = BTreeMap::<String, PropertyDef>::new();
        for label in labels {
            let table: Option<&NodeTable> = match self.graph.node_table(label) {
                Ok(table) => Some(table),
                Err(CatalogError::UnknownLabel(_)) => None,
                Err(err) => return Err(err.into()),
            };
            if let Some(table) = table {
                // Cypher fixtures use `id` as an ordinary primary-key property
                // and expect `RETURN n.*` and node printing to show it; Gremlin
                // treats element ids as separate from properties. This mirrors
                // `node_property_keys` vs `node_property_keys_with_id` — excluding
                // it unconditionally left `n.id` unresolvable, which the
                // NullOnMissing policy then turned into a silent `NULL`.
                let excluded: &[&str] = match self.language {
                    Language::Gremlin => &["id"],
                    _ => &[],
                };
                merge_property_defs(&mut defs, table.batch.schema().as_ref(), excluded)?;
                merge_struct_field_defs(&mut defs, &table.batch)?;
            }
            // Overlay-written property keys with no base column (brand-new
            // labels, or keys introduced by `SET` on existing elements) must
            // still surface in the relational scan. Their Arrow type has to be
            // inferred from the live overlay values; base keys keep the exact
            // column type recorded above.
            let keys = match self.language {
                Language::Gremlin => self.graph.node_property_keys(label),
                _ => self.graph.node_property_keys_with_id(label),
            };
            for key in keys {
                if defs.contains_key(&key) {
                    continue;
                }
                let data_type = infer_element_property_type(self.graph, false, label, &key);
                defs.insert(
                    key.clone(),
                    PropertyDef {
                        name: key,
                        data_type,
                        carries_union_tag: false,
                        struct_fields: Vec::new(),
                    },
                );
            }
        }
        Ok(defs.into_values().collect())
    }

    /// Ordered property keys for an element binding: catalog schema order
    /// (matching the catalog's `node_property_keys` /
    /// `edge_property_keys` iteration), filtered to the property columns
    /// actually present in the plan.
    pub(super) fn element_property_keys(
        &self,
        plan: &LogicalPlan,
        binding: &str,
        shape: BindingShape,
    ) -> Vec<String> {
        let mut keys = Vec::new();
        let mut push_keys = |label_keys: Vec<String>| {
            for key in label_keys {
                if !keys.contains(&key) && has_exact_col(plan, &prop_col(binding, &key)) {
                    keys.push(key);
                }
            }
        };
        match shape {
            BindingShape::Node => {
                for label in self.graph.node_label_order() {
                    push_keys(self.graph.node_property_keys_with_id(label));
                }
            }
            BindingShape::Edge => {
                for rel_type in self.graph.edge_rel_order() {
                    push_keys(self.graph.edge_property_keys(rel_type));
                }
            }
        }
        keys
    }

    /// Render a Cypher graph element the way the runtime's
    /// `expand_element` does: nodes as `{_ID: t:o, _LABEL: l, key: value,
    /// ...}` (null properties omitted), edges as
    /// `(st:so)-{_LABEL: r, _ID: t:o, ...}->(dt:do)`.
    pub(super) fn cypher_element_display_expr(
        &self,
        plan: &LogicalPlan,
        binding: &str,
        shape: BindingShape,
    ) -> RelResult<Expr> {
        let node_index =
            |label_expr: Expr| label_index_case(label_expr, self.graph.node_label_order(), 0, 1);
        let property_segments = |parts: &mut Vec<Expr>| {
            for key in self.element_property_keys(plan, binding, shape) {
                let name = prop_col(binding, &key);
                let Some(data_type) = plan_column_type(plan, &name) else {
                    continue;
                };
                let column = col_exact(&name);
                let rendered = concat_exprs(vec![
                    lit(format!(", {key}: ")),
                    render_property_text_expr(column.clone(), &data_type),
                ]);
                parts.push(Expr::Case(Case::new(
                    None,
                    vec![(Box::new(column.is_null()), Box::new(lit("")))],
                    Some(Box::new(rendered)),
                )));
            }
        };
        match shape {
            BindingShape::Node => {
                let mut parts = vec![
                    lit("{_ID: "),
                    node_index(col_exact(label_col(binding))),
                    lit(":"),
                    cast_utf8(col_exact(id_col(binding))),
                    lit(", _LABEL: "),
                    col_exact(label_col(binding)),
                ];
                property_segments(&mut parts);
                parts.push(lit("}"));
                Ok(concat_exprs(parts))
            }
            BindingShape::Edge => {
                let mut parts = vec![
                    lit("("),
                    node_index(col_exact(src_label_col(binding))),
                    lit(":"),
                    cast_utf8(col_exact(src_id_col(binding))),
                    lit(")-{_LABEL: "),
                    col_exact(label_col(binding)),
                    lit(", _ID: "),
                    rel_index_case(col_exact(label_col(binding)), self.graph),
                    lit(":"),
                    cast_utf8(col_exact(id_col(binding))),
                ];
                property_segments(&mut parts);
                parts.push(lit("}->("));
                parts.push(node_index(col_exact(dst_label_col(binding))));
                parts.push(lit(":"));
                parts.push(cast_utf8(col_exact(dst_id_col(binding))));
                parts.push(lit(")"));
                Ok(concat_exprs(parts))
            }
        }
    }

    pub(super) fn edge_property_defs(&self, rel_types: &[String]) -> RelResult<Vec<PropertyDef>> {
        let mut defs = BTreeMap::<String, PropertyDef>::new();
        for rel_type in rel_types {
            let tables: Option<&[EdgeTable]> = match self.graph.edge_tables(rel_type) {
                Ok(tables) => Some(tables),
                Err(CatalogError::UnknownRelType(_)) => None,
                Err(err) => return Err(err.into()),
            };
            if let Some(tables) = tables {
                for table in tables {
                    merge_property_defs(
                        &mut defs,
                        table.batch.schema().as_ref(),
                        &["src", "dst", "id", "__src_id", "__dst_id"],
                    )?;
                    merge_struct_field_defs(&mut defs, &table.batch)?;
                }
            }
            for key in self.graph.edge_property_keys(rel_type) {
                if defs.contains_key(&key) {
                    continue;
                }
                let data_type = infer_element_property_type(self.graph, true, rel_type, &key);
                defs.insert(
                    key.clone(),
                    PropertyDef {
                        name: key,
                        data_type,
                        carries_union_tag: false,
                        struct_fields: Vec::new(),
                    },
                );
            }
        }
        Ok(defs.into_values().collect())
    }
}

#[derive(Debug, Clone)]
pub(super) struct PropertyDef {
    pub(super) name: String,
    pub(super) data_type: DataType,
    pub(super) carries_union_tag: bool,
    pub(super) struct_fields: Vec<String>,
}

/// Encoded structured properties that may carry a native list shadow.
pub(super) fn list_shadow_candidates(defs: &[PropertyDef]) -> Vec<String> {
    defs.iter()
        .filter(|def| def.struct_fields.is_empty() && (def.carries_union_tag
            || matches!(def.data_type, DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View)))
        .map(|def| def.name.clone())
        .collect()
}

pub(super) fn merge_property_defs(
    defs: &mut BTreeMap<String, PropertyDef>,
    schema: &Schema,
    excluded: &[&str],
) -> RelResult<()> {
    for field in schema.fields() {
        if excluded.contains(&field.name().as_str()) {
            continue;
        }
        let carries_union_tag = crate::ir::value::field_value_type(field)
            .is_some_and(|kind| kind == "value");
        match defs.get_mut(field.name()) {
            Some(existing) if existing.data_type != *field.data_type() => {
                return Err(RelError::Unsupported(format!(
                    "property `{}` has mixed types `{:?}` and `{:?}`",
                    field.name(),
                    existing.data_type,
                    field.data_type()
                )));
            }
            Some(existing) => existing.carries_union_tag |= carries_union_tag,
            None => {
                defs.insert(
                    field.name().clone(),
                    PropertyDef {
                        name: field.name().clone(),
                        data_type: field.data_type().clone(),
                        carries_union_tag,
                        struct_fields: Vec::new(),
                    },
                );
            }
        }
    }
    Ok(())
}

pub(super) fn merge_struct_field_defs(
    defs: &mut BTreeMap<String, PropertyDef>,
    batch: &RecordBatch,
) -> RelResult<()> {
    use arrow::array::Array as _;
    for (name, def) in defs.iter_mut() {
        let Some(index) = schema_index(batch.schema().as_ref(), name) else {
            continue;
        };
        let field = batch.schema().field(index).clone();
        if !crate::ir::value::field_value_type(&field)
            .is_some_and(|kind| kind == "value")
        {
            continue;
        }
        let source = batch
            .column(index)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                RelError::Unsupported(format!("structured property `{name}` is not text"))
            })?;
        for row in 0..source.len() {
            if source.is_null(row) {
                continue;
            }
            let Some(Value::Map(map)) = crate::ir::catalog::parse_debug_value(source.value(row))
            else {
                continue;
            };
            if map.contains_key("__tag") || map.keys().any(|key| key.starts_with('\0')) {
                continue;
            }
            let keys = match map.get(STRUCT_ORDER_KEY) {
                Some(Value::List(order)) => order
                    .iter()
                    .filter_map(|value| match value {
                        Value::String(key) if map.contains_key(key) => Some(key.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
                _ => map
                    .keys()
                    .filter(|key| !key.starts_with("__"))
                    .cloned()
                    .collect(),
            };
            for key in keys {
                if !def.struct_fields.contains(&key) {
                    def.struct_fields.push(key);
                }
            }
            break;
        }
    }
    Ok(())
}

pub(super) fn node_schema(binding: &str, props: &[PropertyDef]) -> SchemaRef {
    let mut fields = vec![
        Field::new(id_col(binding), DataType::Int64, false),
        Field::new(label_col(binding), DataType::Utf8, false),
    ];
    for prop in props {
        fields.push(Field::new(
            prop_col(binding, &prop.name),
            prop.data_type.clone(),
            true,
        ));
        if prop.carries_union_tag {
            fields.push(Field::new(
                union_tag_col(binding, &prop.name),
                DataType::Utf8,
                true,
            ));
        }
        for field in &prop.struct_fields {
            fields.push(Field::new(
                struct_field_col(binding, &prop.name, field),
                DataType::Utf8,
                true,
            ));
        }
    }
    Arc::new(Schema::new(fields))
}

pub(super) fn edge_schema(binding: &str, props: &[PropertyDef]) -> SchemaRef {
    let mut fields = vec![
        Field::new(id_col(binding), DataType::Int64, false),
        Field::new(label_col(binding), DataType::Utf8, false),
        Field::new(src_label_col(binding), DataType::Utf8, false),
        Field::new(src_id_col(binding), DataType::Int64, false),
        Field::new(dst_label_col(binding), DataType::Utf8, false),
        Field::new(dst_id_col(binding), DataType::Int64, false),
    ];
    for prop in props {
        fields.push(Field::new(
            prop_col(binding, &prop.name),
            prop.data_type.clone(),
            true,
        ));
        if prop.carries_union_tag {
            fields.push(Field::new(
                union_tag_col(binding, &prop.name),
                DataType::Utf8,
                true,
            ));
        }
        for field in &prop.struct_fields {
            fields.push(Field::new(
                struct_field_col(binding, &prop.name, field),
                DataType::Utf8,
                true,
            ));
        }
    }
    Arc::new(Schema::new(fields))
}

pub(super) fn normalize_node_table(
    binding: &str,
    table: &NodeTable,
    props: &[PropertyDef],
    schema: SchemaRef,
    language: Language,
) -> RelResult<RecordBatch> {
    let rows = table.batch.num_rows();
    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from_iter_values(
            (0..rows).map(|row| row as i64),
        )),
        Arc::new(StringArray::from_iter_values(
            (0..rows).map(|_| table.label.as_str()),
        )),
    ];
    for prop in props {
        arrays.push(property_array(
            &table.batch,
            &prop.name,
            &prop.data_type,
            rows,
            language,
        )?);
        if prop.carries_union_tag {
            arrays.push(property_union_tag_array(&table.batch, &prop.name, rows)?);
        }
        for field in &prop.struct_fields {
            arrays.push(property_struct_field_array(
                &table.batch,
                &prop.name,
                field,
                rows,
                language,
            )?);
        }
    }
    debug_assert_eq!(schema.field(0).name(), &id_col(binding));
    Ok(RecordBatch::try_new(schema, arrays)?)
}

pub(super) fn normalize_edge_table(
    binding: &str,
    table: &EdgeTable,
    base_id: i64,
    props: &[PropertyDef],
    schema: SchemaRef,
    language: Language,
) -> RelResult<RecordBatch> {
    let rows = table.batch.num_rows();
    let src =
        table.batch.schema().index_of("__src_id").map_err(|_| {
            CatalogError::Schema(format!("edge `{}` missing __src_id", table.rel_type))
        })?;
    let dst =
        table.batch.schema().index_of("__dst_id").map_err(|_| {
            CatalogError::Schema(format!("edge `{}` missing __dst_id", table.rel_type))
        })?;
    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from_iter_values(
            (0..rows).map(|row| base_id + row as i64),
        )),
        Arc::new(StringArray::from_iter_values(
            (0..rows).map(|_| table.rel_type.as_str()),
        )),
        Arc::new(StringArray::from_iter_values(
            (0..rows).map(|_| table.src_label.as_str()),
        )),
        table.batch.column(src).clone(),
        Arc::new(StringArray::from_iter_values(
            (0..rows).map(|_| table.dst_label.as_str()),
        )),
        table.batch.column(dst).clone(),
    ];
    for prop in props {
        arrays.push(property_array(
            &table.batch,
            &prop.name,
            &prop.data_type,
            rows,
            language,
        )?);
        if prop.carries_union_tag {
            arrays.push(property_union_tag_array(&table.batch, &prop.name, rows)?);
        }
        for field in &prop.struct_fields {
            arrays.push(property_struct_field_array(
                &table.batch,
                &prop.name,
                field,
                rows,
                language,
            )?);
        }
    }
    debug_assert_eq!(schema.field(0).name(), &id_col(binding));
    Ok(RecordBatch::try_new(schema, arrays)?)
}

pub(super) fn property_array(
    batch: &RecordBatch,
    name: &str,
    expected: &DataType,
    rows: usize,
    language: Language,
) -> RelResult<ArrayRef> {
    match schema_index(batch.schema().as_ref(), name) {
        Some(idx) => {
            let schema = batch.schema();
            let field = schema.field(idx);
            if field.data_type() != expected {
                return Err(RelError::Unsupported(format!(
                    "property `{name}` has type {:?}, expected {expected:?}",
                    field.data_type()
                )));
            }
            // Structured (list / map) properties are stored as
            // debug-encoded strings; decode them to the display text the
            // runtime formatter would print so downstream projections and
            // comparisons see the same rendering.
            let is_encoded = crate::ir::value::field_value_type(field)
                .is_some_and(|kind| kind == "map" || kind == "value");
            if is_encoded {
                let source = batch
                    .column(idx)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or_else(|| {
                        RelError::Unsupported(format!(
                            "encoded property `{name}` is not a string column"
                        ))
                    })?;
                use arrow::array::Array as _;
                let mut builder = StringBuilder::new();
                for row in 0..source.len() {
                    if source.is_null(row) {
                        builder.append_null();
                        continue;
                    }
                    let raw = source.value(row);
                    match crate::ir::catalog::parse_debug_value(raw) {
                        Some(value) => builder.append_value(rel_display_value(
                            &value,
                            language,
                            literal_collection_context(language),
                        )),
                        None => builder.append_value(raw),
                    }
                }
                return Ok(Arc::new(builder.finish()) as ArrayRef);
            }
            Ok(batch.column(idx).clone())
        }
        None => Ok(arrow::array::new_null_array(expected, rows)),
    }
}

pub(super) fn property_union_tag_array(
    batch: &RecordBatch,
    name: &str,
    rows: usize,
) -> RelResult<ArrayRef> {
    let Some(idx) = schema_index(batch.schema().as_ref(), name) else {
        return Ok(arrow::array::new_null_array(&DataType::Utf8, rows));
    };
    let source = batch
        .column(idx)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| {
            RelError::Unsupported(format!(
                "union-valued property `{name}` is not a string column"
            ))
        })?;
    use arrow::array::Array as _;
    let mut builder = StringBuilder::new();
    for row in 0..source.len() {
        if source.is_null(row) {
            builder.append_null();
            continue;
        }
        let tag = crate::ir::catalog::parse_debug_value(source.value(row)).and_then(|value| {
            let Value::Map(map) = value else {
                return None;
            };
            match map.get("__tag") {
                Some(Value::String(tag)) => Some(tag.clone()),
                _ => None,
            }
        });
        match tag {
            Some(tag) => builder.append_value(tag),
            None => builder.append_null(),
        }
    }
    Ok(Arc::new(builder.finish()) as ArrayRef)
}

pub(super) fn property_struct_field_array(
    batch: &RecordBatch,
    name: &str,
    struct_field: &str,
    rows: usize,
    language: Language,
) -> RelResult<ArrayRef> {
    let Some(idx) = schema_index(batch.schema().as_ref(), name) else {
        return Ok(arrow::array::new_null_array(&DataType::Utf8, rows));
    };
    let source = batch
        .column(idx)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| {
            RelError::Unsupported(format!("structured property `{name}` is not text"))
        })?;
    use arrow::array::Array as _;
    let mut builder = StringBuilder::new();
    for row in 0..source.len() {
        if source.is_null(row) {
            builder.append_null();
            continue;
        }
        let value = crate::ir::catalog::parse_debug_value(source.value(row)).and_then(|value| {
            let Value::Map(map) = value else {
                return None;
            };
            map.get(struct_field).cloned()
        });
        match value {
            Some(Value::Null) | None => builder.append_null(),
            Some(value) => builder.append_value(rel_display_value(
                &value,
                language,
                literal_collection_context(language),
            )),
        }
    }
    Ok(Arc::new(builder.finish()) as ArrayRef)
}

/// Materialize a node label as a relational scan batch, overlay-aware.
///
/// Row identity comes from `PropertyGraph::node_ids`, which keeps original
/// base row ids (including gaps left by deletions) and appends ids of
/// inserted nodes. Properties are read through `node_property` so
/// create/set/delete mutations are visible, while base columns keep their
/// exact Arrow types (see [`materialize_property_array`]).
pub(super) fn materialize_node_scan_batch(
    binding: &str,
    label: &str,
    ids: &[i64],
    graph: &PropertyGraph,
    props: &[PropertyDef],
    schema: SchemaRef,
    language: Language,
) -> RelResult<RecordBatch> {
    let rows = ids.len();
    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(ids.to_vec())),
        Arc::new(StringArray::from_iter_values((0..rows).map(|_| label))),
    ];
    for prop in props {
        arrays.push(materialize_property_array(
            graph,
            false,
            label,
            ids,
            &prop.name,
            &prop.data_type,
            language,
        )?);
        if prop.carries_union_tag {
            arrays.push(materialize_union_tag_array(
                graph, false, label, ids, &prop.name,
            )?);
        }
        for field in &prop.struct_fields {
            arrays.push(materialize_struct_field_array(
                graph, false, label, ids, &prop.name, field, language,
            )?);
        }
    }
    debug_assert_eq!(schema.field(0).name(), &id_col(binding));
    Ok(RecordBatch::try_new(schema, arrays)?)
}

/// Materialize a relationship type as a relational scan batch, overlay-aware.
///
/// Row identity and endpoints come from `PropertyGraph::edge_ids` /
/// `edge_endpoints`, which already resolve grouped edge tables, deletion
/// gaps, and inserted edges. Properties are read through `edge_property`.
pub(super) fn materialize_edge_scan_batch(
    binding: &str,
    rel_type: &str,
    ids: &[i64],
    graph: &PropertyGraph,
    props: &[PropertyDef],
    schema: SchemaRef,
    language: Language,
) -> RelResult<RecordBatch> {
    let rows = ids.len();
    let mut src_labels = Vec::with_capacity(rows);
    let mut src_ids = Vec::with_capacity(rows);
    let mut dst_labels = Vec::with_capacity(rows);
    let mut dst_ids = Vec::with_capacity(rows);
    for &id in ids {
        let (src_label, src_id, dst_label, dst_id) =
            graph.edge_endpoints(rel_type, id).ok_or_else(|| {
                RelError::Unsupported(format!("edge `{rel_type}` row {id} has no endpoints"))
            })?;
        src_labels.push(src_label);
        src_ids.push(src_id);
        dst_labels.push(dst_label);
        dst_ids.push(dst_id);
    }
    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(ids.to_vec())),
        Arc::new(StringArray::from_iter_values((0..rows).map(|_| rel_type))),
        Arc::new(StringArray::from(src_labels)),
        Arc::new(Int64Array::from(src_ids)),
        Arc::new(StringArray::from(dst_labels)),
        Arc::new(Int64Array::from(dst_ids)),
    ];
    for prop in props {
        arrays.push(materialize_property_array(
            graph,
            true,
            rel_type,
            ids,
            &prop.name,
            &prop.data_type,
            language,
        )?);
        if prop.carries_union_tag {
            arrays.push(materialize_union_tag_array(
                graph, true, rel_type, ids, &prop.name,
            )?);
        }
        for field in &prop.struct_fields {
            arrays.push(materialize_struct_field_array(
                graph, true, rel_type, ids, &prop.name, field, language,
            )?);
        }
    }
    debug_assert_eq!(schema.field(0).name(), &id_col(binding));
    Ok(RecordBatch::try_new(schema, arrays)?)
}

/// Read a property of an element through the catalog's overlay-aware API.
pub(super) fn element_property_value(
    graph: &PropertyGraph,
    is_edge: bool,
    element: &str,
    id: i64,
    name: &str,
) -> Value {
    if is_edge {
        graph.edge_property(element, id, name)
    } else {
        graph.node_property(element, id, name)
    }
}

/// Locate the base Arrow cell for an element id, if it is a base (non-inserted)
/// row. Inserted overlay rows and unknown elements return `None`.
pub(super) fn base_cell<'a>(
    graph: &'a PropertyGraph,
    is_edge: bool,
    element: &str,
    id: i64,
) -> Option<(&'a RecordBatch, usize)> {
    if is_edge {
        let tables = graph.edge_tables(element).ok()?;
        let mut offset = 0_i64;
        for table in tables {
            let rows = table.batch.num_rows() as i64;
            if id >= offset && id < offset + rows {
                return Some((&table.batch, (id - offset) as usize));
            }
            offset += rows;
        }
        None
    } else {
        let table = graph.node_table(element).ok()?;
        let local = id as usize;
        (local < table.batch.num_rows()).then_some((&table.batch, local))
    }
}

/// Materialize one property column for a scan, overlay-aware.
///
/// For the scalar types the runtime can read back losslessly (booleans,
/// i32/i64, f64, strings) the effective `node_property`/`edge_property` value
/// is converted into the column's exact Arrow type, so base cells keep their
/// values and overlay writes surface with the same type. For any other base
/// type (narrow ints, `f32`, temporals, lists, …) the raw base cell is kept
/// verbatim for unmutated rows and the scan declines if the overlay changed
/// it, so a mismatch can never produce a wrong answer.
pub(super) fn materialize_property_array(
    graph: &PropertyGraph,
    is_edge: bool,
    element: &str,
    ids: &[i64],
    name: &str,
    data_type: &DataType,
    language: Language,
) -> RelResult<ArrayRef> {
    let supported = matches!(
        data_type,
        DataType::Boolean | DataType::Int32 | DataType::Int64 | DataType::Float64 | DataType::Utf8
    );
    let mut scalars: Vec<ScalarValue> = Vec::with_capacity(ids.len());
    for &id in ids {
        let value = element_property_value(graph, is_edge, element, id, name);
        if supported {
            scalars.push(value_to_scalar(&value, data_type, language)?);
            continue;
        }
        let Some((batch, local_row)) = base_cell(graph, is_edge, element, id) else {
            return Err(RelError::Unsupported(format!(
                "property `{name}` has base type `{data_type:?}` and overlay data"
            )));
        };
        let Some(idx) = schema_index(batch.schema().as_ref(), name) else {
            return Err(RelError::Unsupported(format!(
                "property `{name}` is missing from its base schema"
            )));
        };
        let field = batch.schema().field(idx).clone();
        let base_value = crate::ir::catalog::array_value(
            batch.column(idx).as_ref(),
            local_row,
            Some(field.as_ref()),
        );
        if base_value != value {
            return Err(RelError::Unsupported(format!(
                "property `{name}` has base type `{data_type:?}` and overlay data"
            )));
        }
        scalars.push(ScalarValue::try_from_array(
            batch.column(idx).as_ref(),
            local_row,
        )?);
    }
    Ok(ScalarValue::iter_to_array(scalars)?)
}

pub(super) fn value_to_scalar(
    value: &Value,
    data_type: &DataType,
    language: Language,
) -> RelResult<ScalarValue> {
    if crate::ir::temporal::contains_temporal(value) {
        return Err(RelError::Unsupported("Typed temporal properties require a residual kernel".into()));
    }
    let mismatch = || {
        RelError::Unsupported(format!(
            "property value `{}` cannot be stored in column type `{data_type:?}`",
            value.type_name()
        ))
    };
    match data_type {
        DataType::Boolean => match value {
            Value::Null => Ok(ScalarValue::Boolean(None)),
            Value::Bool(value) => Ok(ScalarValue::Boolean(Some(*value))),
            _ => Err(mismatch()),
        },
        DataType::Int32 => match value {
            Value::Null => Ok(ScalarValue::Int32(None)),
            _ => value_to_int(value)
                .and_then(|value| i32::try_from(value).ok())
                .map(|value| ScalarValue::Int32(Some(value)))
                .ok_or_else(mismatch),
        },
        DataType::Int64 => match value {
            Value::Null => Ok(ScalarValue::Int64(None)),
            _ => value_to_int(value)
                .map(|value| ScalarValue::Int64(Some(value)))
                .ok_or_else(mismatch),
        },
        DataType::Float64 => match value {
            Value::Null => Ok(ScalarValue::Float64(None)),
            _ => materialized_value_to_f64(value)
                .map(|value| ScalarValue::Float64(Some(value)))
                .ok_or_else(mismatch),
        },
        DataType::Utf8 => match value {
            Value::Null => Ok(ScalarValue::Utf8(None)),
            Value::String(value) | Value::DateTime(value) => {
                Ok(ScalarValue::Utf8(Some(value.clone())))
            }
            other => Ok(ScalarValue::Utf8(Some(rel_display_value(
                other,
                language,
                literal_collection_context(language),
            )))),
        },
        _ => Err(RelError::Unsupported(format!(
            "property column type `{data_type:?}` is not relationally lowered"
        ))),
    }
}

/// Integer conversion that refuses lossy float truncation, so a float written
/// into an integer column declines instead of silently truncating.
pub(super) fn value_to_int(value: &Value) -> Option<i64> {
    match value {
        Value::Byte(value) => Some(*value as i64),
        Value::UInt8(value) => Some(*value as i64),
        Value::Short(value) => Some(*value as i64),
        Value::UInt16(value) => Some(*value as i64),
        Value::Int(value) | Value::Long(value) => Some(*value),
        Value::UInt32(value) => Some(*value as i64),
        Value::UInt64(value) => i64::try_from(*value).ok(),
        Value::BigInt(value) => value.to_i64(),
        Value::UInt128(value) => value.to_i64(),
        _ => None,
    }
}

pub(super) fn materialized_value_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Float(value) => Some(*value),
        Value::Float32(value) => Some(f64::from(*value)),
        Value::BigInt(value) => value.to_f64(),
        Value::UInt128(value) => value.to_f64(),
        Value::BigDecimal(value) => value.to_f64(),
        other => other.as_i64().map(|value| value as f64),
    }
}

pub(super) fn materialize_union_tag_array(
    graph: &PropertyGraph,
    is_edge: bool,
    element: &str,
    ids: &[i64],
    name: &str,
) -> RelResult<ArrayRef> {
    let mut builder = StringBuilder::new();
    for &id in ids {
        let value = element_property_value(graph, is_edge, element, id, name);
        match union_tag_of(&value) {
            Some(tag) => builder.append_value(tag),
            None => builder.append_null(),
        }
    }
    Ok(Arc::new(builder.finish()) as ArrayRef)
}

pub(super) fn union_tag_of(value: &Value) -> Option<String> {
    match value {
        Value::Map(map) => match map.get("__tag") {
            Some(Value::String(tag)) => Some(tag.clone()),
            _ => None,
        },
        _ => None,
    }
}

pub(super) fn materialize_struct_field_array(
    graph: &PropertyGraph,
    is_edge: bool,
    element: &str,
    ids: &[i64],
    name: &str,
    struct_field: &str,
    language: Language,
) -> RelResult<ArrayRef> {
    let mut builder = StringBuilder::new();
    for &id in ids {
        let value = element_property_value(graph, is_edge, element, id, name);
        match struct_field_of(&value, struct_field) {
            Some(Value::Null) | None => builder.append_null(),
            Some(field_value) => builder.append_value(rel_display_value(
                &field_value,
                language,
                literal_collection_context(language),
            )),
        }
    }
    Ok(Arc::new(builder.finish()) as ArrayRef)
}

pub(super) fn struct_field_of(value: &Value, field: &str) -> Option<Value> {
    match value {
        Value::Map(map) => map.get(field).cloned(),
        _ => None,
    }
}

/// Infer an Arrow type for a property key that only exists in the overlay
/// (a brand-new label, or a key introduced by `SET`). Mirrors the runtime's
/// `ColumnKind` promotion: booleans, integers, floats, and everything else as
/// text. Base-schema keys never reach this — they keep their exact type.
pub(super) fn infer_element_property_type(
    graph: &PropertyGraph,
    is_edge: bool,
    element: &str,
    name: &str,
) -> DataType {
    let ids: Vec<i64> = if is_edge {
        graph.edge_ids(element)
    } else {
        graph.node_ids(element).unwrap_or_default()
    };
    let mut values = Vec::with_capacity(ids.len());
    for id in ids {
        values.push(element_property_value(graph, is_edge, element, id, name));
    }
    infer_property_data_type(&values)
}

// Retain the actual scalar widths when every non-null value agrees.
// In particular Gremlin Integer and Long have distinct public types.
fn homogeneous_scalar_type<'a>(values: impl Iterator<Item = &'a Value>) -> Option<DataType> {
    let mut kind = None;
    for value in values {
        let next = match value {
            Value::Null => continue,
            Value::Bool(_) => DataType::Boolean,
            Value::Byte(_) => DataType::Int8,
            Value::Short(_) => DataType::Int16,
            Value::Int(value) if i32::try_from(*value).is_ok() => DataType::Int32,
            Value::Int(_) | Value::Long(_) => DataType::Int64,
            Value::UInt8(_) => DataType::UInt8,
            Value::UInt16(_) => DataType::UInt16,
            Value::UInt32(_) => DataType::UInt32,
            Value::UInt64(_) => DataType::UInt64,
            Value::Float32(_) => DataType::Float32,
            Value::Float(_) => DataType::Float64,
            Value::String(_) => DataType::Utf8,
            _ => return None,
        };
        if kind.as_ref().is_some_and(|existing| *existing != next) {
            return None;
        }
        kind = Some(next);
    }
    kind
}

pub(super) fn infer_property_data_type(values: &[Value]) -> DataType {
    if let Some(kind) = homogeneous_scalar_type(values.iter()) {
        return kind;
    }
    #[derive(Clone, Copy, PartialEq)]
    enum Kind {
        Bool,
        Int,
        Float,
        Utf8,
    }
    let mut kind: Option<Kind> = None;
    for value in values {
        let candidate = match value {
            Value::Null => continue,
            Value::Bool(_) => Kind::Bool,
            Value::Byte(_)
            | Value::UInt8(_)
            | Value::Short(_)
            | Value::UInt16(_)
            | Value::Int(_)
            | Value::UInt32(_)
            | Value::Long(_)
            | Value::UInt64(_) => Kind::Int,
            Value::Float32(_) | Value::Float(_) => Kind::Float,
            _ => Kind::Utf8,
        };
        kind = Some(match kind {
            None => candidate,
            Some(existing) if existing == candidate => existing,
            Some(Kind::Int) if candidate == Kind::Float => Kind::Float,
            Some(Kind::Float) if candidate == Kind::Int => Kind::Float,
            _ => Kind::Utf8,
        });
    }
    match kind.unwrap_or(Kind::Utf8) {
        Kind::Bool => DataType::Boolean,
        Kind::Int => DataType::Int64,
        Kind::Float => DataType::Float64,
        Kind::Utf8 => DataType::Utf8,
    }
}

pub(super) fn schema_index(schema: &Schema, name: &str) -> Option<usize> {
    schema.index_of(name).ok().or_else(|| {
        let mut matches = schema
            .fields()
            .iter()
            .enumerate()
            .filter_map(|(idx, field)| field.name().eq_ignore_ascii_case(name).then_some(idx));
        let first = matches.next()?;
        matches.next().is_none().then_some(first)
    })
}

pub(super) fn values_batch(
    language: Language,
    bindings: &[String],
    rows: &[Vec<Value>],
) -> RelResult<RecordBatch> {
    if rows.iter().any(|row| row.len() != bindings.len()) {
        return Err(RelError::Unsupported(
            "GraphValues row width does not match bindings".into(),
        ));
    }
    let types = (0..bindings.len())
        .map(|idx| {
            let values = rows.iter().map(|row| &row[idx]).collect::<Vec<_>>();
            infer_value_type(&values)
        })
        .collect::<RelResult<Vec<_>>>()?;
    let fields = bindings
        .iter()
        .zip(types.iter())
        .map(|(binding, data_type)| Field::new(binding, data_type.clone(), true))
        .collect::<Vec<_>>();
    let schema = Arc::new(Schema::new(fields));
    let arrays = types
        .iter()
        .enumerate()
        .map(|(idx, data_type)| values_array(language, rows.iter().map(|row| &row[idx]), data_type))
        .collect::<RelResult<Vec<_>>>()?;
    Ok(RecordBatch::try_new(schema, arrays)?)
}

pub(super) fn infer_value_type(values: &[&Value]) -> RelResult<DataType> {
    if values.iter().any(|v|crate::ir::temporal::contains_temporal(v)) {
        return Err(RelError::Unsupported("Typed temporal values require a residual kernel".into()));
    }
    if let Some(kind) = homogeneous_scalar_type(values.iter().copied()) {
        return Ok(kind);
    }
    if values.iter().any(|value| matches!(value, Value::List(_)))
        && values
            .iter()
            .all(|value| matches!(value, Value::Null | Value::List(_)))
    {
        let elements = values
            .iter()
            .flat_map(|value| match value {
                Value::List(items) => items.iter().collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect::<Vec<_>>();
        let element_type = if elements.iter().any(|value| {
            matches!(
                value,
                Value::String(_)
                    | Value::DateTime(_)
                    | Value::BigInt(_)
                    | Value::UInt128(_)
                    | Value::BigDecimal(_)
                    | Value::InternalId { .. }
                    | Value::Node { .. }
                    | Value::Edge { .. }
                    | Value::VertexProperty { .. }
                    | Value::Property { .. }
                    | Value::Map(_)
                    | Value::TypedMap(_)
                    | Value::MapEntry(_) | Value::CardinalityValue {..}
                    | Value::BulkSet(_)
            | Value::Set(_)
                    | Value::Token(_)
                    | Value::Direction(_)
                    | Value::Path(_)
                    | Value::List(_)
            )
        }) {
            DataType::Utf8
        } else {
            infer_value_type(&elements)?
        };
        return Ok(DataType::List(Arc::new(Field::new(
            "item",
            element_type,
            true,
        ))));
    }
    let mut data_type = DataType::Utf8;
    for value in values.iter().copied() {
        match value {
            Value::Temporal(_) => return Err(RelError::Unsupported("typed Cypher temporal materialization requires a residual kernel".into())),
            Value::Null => {}
            Value::Bool(_) => data_type = promote_type(data_type, DataType::Boolean)?,
            Value::Byte(_)
            | Value::UInt8(_)
            | Value::Short(_)
            | Value::UInt16(_)
            | Value::Int(_)
            | Value::UInt32(_)
            | Value::Long(_)
            | Value::UInt64(_) => data_type = promote_type(data_type, DataType::Int64)?,
            Value::Float32(_) | Value::Float(_) => {
                data_type = promote_type(data_type, DataType::Float64)?
            }
            Value::String(_) | Value::DateTime(_) => {
                data_type = promote_type(data_type, DataType::Utf8)?
            }
            Value::BigInt(_)
            | Value::UInt128(_)
            | Value::BigDecimal(_)
            | Value::InternalId { .. }
            | Value::Node { .. }
            | Value::Edge { .. }
            | Value::VertexProperty { .. }
            | Value::Property { .. }
            | Value::List(_)
            | Value::Map(_)
            | Value::TypedMap(_)
            | Value::MapEntry(_) | Value::CardinalityValue {..}
            | Value::BulkSet(_)
            | Value::Set(_)
            | Value::Token(_)
            | Value::Direction(_)
            | Value::Path(_) => {
                data_type = DataType::Utf8;
                break;
            }
        }
    }
    Ok(data_type)
}

pub(super) fn promote_type(current: DataType, next: DataType) -> RelResult<DataType> {
    match (&current, &next) {
        (DataType::Utf8, _) => Ok(next),
        (_, DataType::Utf8) => Ok(current),
        (DataType::Int64, DataType::Float64) => Ok(DataType::Float64),
        (DataType::Float64, DataType::Int64) => Ok(DataType::Float64),
        _ if current == next => Ok(current),
        _ => Err(RelError::Unsupported(format!(
            "mixed GraphValues types `{current:?}` and `{next:?}`"
        ))),
    }
}

pub(super) fn values_array<'a>(
    language: Language,
    values: impl Iterator<Item = &'a Value>,
    data_type: &DataType,
) -> RelResult<ArrayRef> {
    match data_type {
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64 => {
            let wide = values_array(language, values, &DataType::Int64)?;
            Ok(arrow::compute::cast(&wide, data_type)?)
        }
        DataType::Float32 => {
            let wide = values_array(language, values, &DataType::Float64)?;
            Ok(arrow::compute::cast(&wide, data_type)?)
        }
        DataType::Boolean => {
            let mut builder = BooleanBuilder::new();
            for value in values {
                match value {
                    Value::Null => builder.append_null(),
                    Value::Bool(value) => builder.append_value(*value),
                    other => {
                        return Err(RelError::Unsupported(format!(
                            "cannot put `{}` in Boolean GraphValues column",
                            other.type_name()
                        )));
                    }
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Int64 => {
            let mut builder = Int64Builder::new();
            for value in values {
                match value {
                    Value::Null => builder.append_null(),
                    _ => match value.as_i64() {
                        Some(value) => builder.append_value(value),
                        None => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in Int64 GraphValues column",
                                value.type_name()
                            )));
                        }
                    },
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Float64 => {
            let mut builder = Float64Builder::new();
            for value in values {
                match value {
                    Value::Null => builder.append_null(),
                    Value::Float(value) => builder.append_value(*value),
                    Value::Float32(value) => builder.append_value(*value as f64),
                    _ => match value.as_i64() {
                        Some(value) => builder.append_value(value as f64),
                        None => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in Float64 GraphValues column",
                                value.type_name()
                            )));
                        }
                    },
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::Utf8 => {
            let mut builder = StringBuilder::new();
            for value in values {
                match value {
                    Value::Null => builder.append_null(),
                    Value::String(value) | Value::DateTime(value) => builder.append_value(value),
                    other => builder.append_value(graph_values_display(other, language)),
                }
            }
            Ok(Arc::new(builder.finish()))
        }
        DataType::List(field) => match field.data_type() {
            DataType::Boolean => {
                let mut builder = ListBuilder::new(BooleanBuilder::new());
                for value in values {
                    match value {
                        Value::Null => builder.append(false),
                        Value::List(items) => {
                            for item in items {
                                match item {
                                    Value::Null => builder.values().append_null(),
                                    Value::Bool(value) => builder.values().append_value(*value),
                                    other => {
                                        return Err(RelError::Unsupported(format!(
                                            "cannot put `{}` in Boolean GraphValues list",
                                            other.type_name()
                                        )));
                                    }
                                }
                            }
                            builder.append(true);
                        }
                        other => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in GraphValues list column",
                                other.type_name()
                            )));
                        }
                    }
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Int64 => {
                let mut builder = ListBuilder::new(Int64Builder::new());
                for value in values {
                    match value {
                        Value::Null => builder.append(false),
                        Value::List(items) => {
                            for item in items {
                                match item {
                                    Value::Null => builder.values().append_null(),
                                    item => match item.as_i64() {
                                        Some(value) => builder.values().append_value(value),
                                        None => {
                                            return Err(RelError::Unsupported(format!(
                                                "cannot put `{}` in Int64 GraphValues list",
                                                item.type_name()
                                            )));
                                        }
                                    },
                                }
                            }
                            builder.append(true);
                        }
                        other => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in GraphValues list column",
                                other.type_name()
                            )));
                        }
                    }
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Float64 => {
                let mut builder = ListBuilder::new(Float64Builder::new());
                for value in values {
                    match value {
                        Value::Null => builder.append(false),
                        Value::List(items) => {
                            for item in items {
                                match item {
                                    Value::Null => builder.values().append_null(),
                                    Value::Float(value) => builder.values().append_value(*value),
                                    Value::Float32(value) => {
                                        builder.values().append_value(f64::from(*value))
                                    }
                                    item => match item.as_i64() {
                                        Some(value) => builder.values().append_value(value as f64),
                                        None => {
                                            return Err(RelError::Unsupported(format!(
                                                "cannot put `{}` in Float64 GraphValues list",
                                                item.type_name()
                                            )));
                                        }
                                    },
                                }
                            }
                            builder.append(true);
                        }
                        other => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in GraphValues list column",
                                other.type_name()
                            )));
                        }
                    }
                }
                Ok(Arc::new(builder.finish()))
            }
            DataType::Utf8 => {
                let mut builder = ListBuilder::new(StringBuilder::new());
                for value in values {
                    match value {
                        Value::Null => builder.append(false),
                        Value::List(items) => {
                            for item in items {
                                match item {
                                    Value::Null => builder.values().append_null(),
                                    Value::String(value) | Value::DateTime(value) => {
                                        builder.values().append_value(value)
                                    }
                                    other => builder
                                        .values()
                                        .append_value(graph_values_display(other, language)),
                                }
                            }
                            builder.append(true);
                        }
                        other => {
                            return Err(RelError::Unsupported(format!(
                                "cannot put `{}` in GraphValues list column",
                                other.type_name()
                            )));
                        }
                    }
                }
                Ok(Arc::new(builder.finish()))
            }
            other => Err(RelError::Unsupported(format!(
                "GraphValues list element type `{other:?}`"
            ))),
        },
        other => Err(RelError::Unsupported(format!(
            "GraphValues type `{other:?}`"
        ))),
    }
}
