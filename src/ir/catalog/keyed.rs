//! Index physical rows by their actual typed primary keys.
use super::*;
use datafusion::common::ScalarValue;

pub(super) fn array_key(array: &ArrayRef, row: usize) -> CatalogResult<ElementId> {
    ElementId::new(
        ScalarValue::try_from_array(array, row).map_err(|e| CatalogError::Schema(e.to_string()))?,
    )
    .map_err(CatalogError::Schema)
}
pub(super) fn validate_keys(keys: &[ElementId], rows: usize) -> CatalogResult<()> {
    if keys.len() != rows || keys.iter().collect::<HashSet<_>>().len() != rows {
        return Err(CatalogError::Schema(
            "primary keys must be unique and match the row count".into(),
        ));
    }
    Ok(())
}
impl PropertyGraph {
    pub fn add_keyed_nodes(&mut self, table: NodeTable, keys: Vec<ElementId>) -> CatalogResult<()> {
        validate_keys(&keys, table.batch.num_rows())?;
        self.source_keys |= keys
            .iter()
            .enumerate()
            .any(|(row, key)| key.as_i64() != Some(row as i64));
        if !self.node_order.contains(&table.label) {
            self.node_order.push(table.label.clone());
        }
        let locations = Arc::make_mut(&mut self.node_row_locations);
        locations.retain(|(label, _), _| label != &table.label);
        for (row, key) in keys.iter().enumerate() {
            locations.insert((table.label.clone(), key.clone()), row);
        }
        self.node_keys.insert(table.label.clone(), keys);
        self.nodes.insert(table.label.clone(), table);
        Ok(())
    }
    pub(crate) fn base_cell(
        &self,
        edge: bool,
        name: &str,
        id: &ElementId,
    ) -> Option<(&RecordBatch, usize)> {
        if edge {
            let location = self
                .edge_row_locations
                .get(&(name.to_owned(), id.clone()))?;
            Some((
                &self.edge_tables.get(name)?.get(location.table_index)?.batch,
                location.local_row as usize,
            ))
        } else {
            Some((
                &self.nodes.get(name)?.batch,
                *self
                    .node_row_locations
                    .get(&(name.to_owned(), id.clone()))?,
            ))
        }
    }
}

impl PropertyGraph {
    pub fn source_identity(&self, value: &Value) -> Option<Value> {
        if self.mapping.is_none() && !self.source_keys {
            return None;
        }
        match value {
            Value::Node { id, .. } | Value::Edge { id, .. } => {
                Some(Value::Scalar(id.scalar().clone()))
            }
            _ => None,
        }
    }
    pub(super) fn mapped_insert_key(
        &self,
        edge: bool,
        name: &str,
        properties: &mut BTreeMap<String, Value>,
        supplied: Option<&Value>,
    ) -> CatalogResult<Option<ElementId>> {
        let Some(mapping) = &self.mapping else {
            if self.source_keys {
                return Err(CatalogError::Schema(
                    "inserting into a keyed catalog requires a writable mapping".into(),
                ));
            }
            return Ok(None);
        };
        let (column, props) = if edge {
            let m = mapping.edge(name).ok_or_else(|| {
                CatalogError::Schema(format!("unmapped relationship type `{name}`"))
            })?;
            (m.id_column.as_ref().unwrap_or(&m.src_column), &m.properties)
        } else {
            let m = mapping
                .node(name)
                .ok_or_else(|| CatalogError::Schema(format!("unmapped label `{name}`")))?;
            (&m.id_column, &m.properties)
        };
        let value = if let Some(value) = supplied {
            value.clone()
        } else {
            let property = props
                .iter()
                .find_map(|(p, c)| (c == column).then_some(p))
                .ok_or_else(|| {
                    CatalogError::Schema(format!(
                        "creation requires a property mapped to primary key `{column}`"
                    ))
                })?;
            let value = properties.get(property).ok_or_else(|| {
                CatalogError::Schema(format!(
                    "creation requires primary key property `{property}`"
                ))
            })?;
            value.clone()
        };
        let key = match &value {
            Value::BigInt(v) | Value::UInt128(v) => {
                ElementId::new(ScalarValue::Utf8(Some(v.to_string())))
            }
            Value::BigDecimal(v) => ElementId::new(ScalarValue::Utf8(Some(v.to_string()))),
            _ => ElementId::try_from(&value),
        }
        .map_err(CatalogError::Schema)?;
        let keys = if edge {
            self.edge_keys.get(name)
        } else {
            self.node_keys.get(name)
        };
        let kind = self
            .key_types
            .get(&(edge, name.to_owned()))
            .cloned()
            .or_else(|| keys.and_then(|v| v.first()).map(|v| v.scalar().data_type()));
        let key = if let Some(kind) = kind {
            ElementId::new(
                key.scalar()
                    .cast_to(&kind)
                    .map_err(|e| CatalogError::Schema(e.to_string()))?,
            )
            .map_err(CatalogError::Schema)?
        } else {
            key
        };
        let exists = if edge {
            self.live_edge_endpoints(name, key.clone()).is_some()
        } else {
            self.node_is_live(name, key.clone())
        };
        if exists {
            return Err(CatalogError::Schema(format!(
                "duplicate primary key `{key}` for `{name}`"
            )));
        }
        for (property, column_name) in props {
            if column_name == column {
                properties
                    .entry(property.clone())
                    .or_insert_with(|| Value::Scalar(key.scalar().clone()));
            }
        }
        Ok(Some(key))
    }
}
impl PropertyGraph {
    #[cfg(feature = "duckdb")]
    pub(crate) fn validate_mapped_changes(&self) -> Result<(), String> {
        let Some(mapping) = &self.mapping else {
            return Ok(());
        };
        let overlay = self.overlay.borrow();
        for edge in [false, true] {
            let pending = self.pending.borrow();
            for (name, id) in if edge { &pending.edges } else { &pending.nodes } {
                let (props, key_column) = if edge {
                    let m = mapping
                        .edge(name)
                        .ok_or_else(|| format!("unmapped relationship `{name}`"))?;
                    (&m.properties, m.id_column.as_ref().unwrap_or(&m.src_column))
                } else {
                    let m = mapping
                        .node(name)
                        .ok_or_else(|| format!("unmapped label `{name}`"))?;
                    (&m.properties, &m.id_column)
                };
                let keys = if edge {
                    self.edge_property_keys(name)
                } else {
                    self.node_property_keys(name)
                };
                for key in keys {
                    if key == crate::ir::value::STRUCT_ORDER_KEY
                        || key == crate::ir::value::STRUCT_TYPES_KEY
                    {
                        continue;
                    }
                    if !props.contains_key(&key) {
                        return Err(format!("unmapped property `{name}.{key}`"));
                    }
                }
                if let Some((property, _)) = props.iter().find(|(_, column)| *column == key_column)
                {
                    let value = if edge {
                        self.edge_property(name, id.clone(), property)
                    } else {
                        self.node_property(name, id.clone(), property)
                    };
                    let live = if edge {
                        self.live_edge_endpoints(name, id.clone()).is_some()
                    } else {
                        self.node_is_live(name, id.clone())
                    };
                    if live && value == Value::Null {
                        return Err("primary key cannot be removed".into());
                    }
                    if value != Value::Null {
                        let actual = ElementId::try_from(&value)?
                            .scalar()
                            .cast_to(&id.scalar().data_type())
                            .map_err(|e| e.to_string())?;
                        if &actual != id.scalar() {
                            return Err("primary key changes are not supported".into());
                        }
                    }
                }
                if edge {
                    let m = mapping.edge(name).unwrap();
                    if let Some((_, src, _, dst)) = self.live_edge_endpoints(name, id.clone()) {
                        for (column, expected) in [(&m.src_column, src), (&m.dst_column, dst)] {
                            if let Some((property, _)) = props.iter().find(|(_, c)| *c == column) {
                                let value = self.edge_property(name, id.clone(), property);
                                let value = ElementId::try_from(&value)?
                                    .scalar()
                                    .cast_to(&expected.scalar().data_type())
                                    .map_err(|e| e.to_string())?;
                                if &value != expected.scalar() {
                                    return Err(
                                        "mapped edge endpoint changes are not supported".into()
                                    );
                                }
                            }
                        }
                    }
                }
                if !edge {
                    if let Some(labels) = overlay.node_label_sets.get(&(name.clone(), id.clone())) {
                        if labels.len() != 1 || !labels.contains(name) {
                            return Err("mapped storage cannot represent additional labels".into());
                        }
                    }
                    if let Some(properties) =
                        overlay.vertex_properties.get(&(name.clone(), id.clone()))
                    {
                        for records in properties.values() {
                            if records.len() > 1
                                || records.iter().any(|record| {
                                    !record.meta.is_empty() || record.public_id.is_some()
                                })
                            {
                                return Err("mapped storage cannot represent property cardinality or metadata".into());
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl PropertyGraph {
    pub(super) fn mapped_insert_properties(
        &self,
        edge: bool,
        name: &str,
        mut props: BTreeMap<String, Value>,
    ) -> CatalogResult<BTreeMap<String, Value>> {
        let address = (edge, name.to_owned());
        if let Some(keys) = self.unsupported_defaults.get(&address) {
            for key in keys {
                if !props.contains_key(key) {
                    return Err(CatalogError::Schema(format!(
                        "property `{name}.{key}` requires an explicit value; its source default is not a scalar constant"
                    )));
                }
            }
        }
        if let Some(defaults) = self.mapped_defaults.get(&address) {
            for (key, value) in defaults {
                props.entry(key.clone()).or_insert_with(|| value.clone());
            }
        }
        Ok(props)
    }
}
