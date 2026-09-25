//! Writes to the session-local property graph overlay.

use super::*;

impl PropertyGraph {
    /// Logical Cypher labels do not participate in the physical element address.
    pub fn node_labels(&self, storage: &str, id: i64) -> Vec<String> {
        self.overlay.borrow().node_label_sets.get(&(storage.to_string(), id))
            .map(|labels| labels.iter().cloned().collect())
            .unwrap_or_else(|| vec![storage.to_string()])
    }

    pub fn set_node_labels(&self, node: &Value, labels: impl IntoIterator<Item = String>) -> CatalogResult<()> {
        let Value::Node { label, id } = node else {
            return Err(CatalogError::Schema("Labels require a node".into()));
        };
        if !self.node_is_live(label, *id) {
            return Err(CatalogError::Schema("Cannot label a deleted node".into()));
        }
        self.overlay.borrow_mut().node_label_sets.insert((label.clone(), *id), labels.into_iter().collect());
        self.pending.borrow_mut().nodes.insert((label.clone(), *id));
        Ok(())
    }

    pub fn node_matches_labels(&self, storage: &str, id: i64, expr: &crate::ir::plan::LabelExpr) -> bool {
        use crate::ir::plan::LabelExpr;
        match expr {
            LabelExpr::Any => true,
            // Provider label matching remains single-valued (Gremlin).
            LabelExpr::AnyOf(names) => names.iter().any(|name| name == storage),
            LabelExpr::AllOf(names) => {
                let labels = self.node_labels(storage, id);
                names.iter().all(|name| labels.contains(name))
            }
            LabelExpr::Not(inner) => !self.node_matches_labels(storage, id, inner),
        }
    }

    /// Append an edge between two node values. Returns the new edge value.
    pub fn insert_edge(
        &self,
        rel_type: impl Into<String>,
        src: &Value,
        dst: &Value,
        properties: BTreeMap<String, Value>,
    ) -> CatalogResult<Value> {
        if properties.values().any(Value::contains_cardinality_value) {
            return Err(CatalogError::Schema("Cardinality values cannot be stored as graph properties".into()));
        }
        let rel_type = rel_type.into();
        let (src_label, src_id) = node_ref(src, &rel_type, "source")?;
        let (dst_label, dst_id) = node_ref(dst, &rel_type, "destination")?;
        if !self.node_is_live(&src_label, src_id) {
            return Err(CatalogError::Schema(format!(
                "relationship `{rel_type}` source node `{src_label}#{src_id}` does not exist"
            )));
        }
        if !self.node_is_live(&dst_label, dst_id) {
            return Err(CatalogError::Schema(format!(
                "relationship `{rel_type}` destination node `{dst_label}#{dst_id}` does not exist"
            )));
        }
        let declared = self.rel_endpoint_labels(&rel_type);
        if !declared.is_empty()
            && !declared
                .iter()
                .any(|(s, d)| s == &src_label && d == &dst_label)
        {
            return Err(CatalogError::Schema(format!(
                "relationship `{rel_type}` endpoints `{src_label}`→`{dst_label}` \
                 do not match the declared endpoint labels"
            )));
        }
        let base = self
            .edge_row_counts
            .get(&rel_type)
            .copied()
            .unwrap_or_else(|| {
                self.edges
                    .get(&rel_type)
                    .map(|table| table.batch.num_rows() as i64)
                    .unwrap_or(0)
            });
        let mut overlay = self.overlay.borrow_mut();
        let counter = overlay
            .inserted_edge_counts
            .entry(rel_type.clone())
            .or_insert(0);
        let id = base + *counter;
        *counter += 1;
        overlay.unassigned_public_ids.insert((true,rel_type.clone(),id));
        overlay
            .inserted_out_adj
            .entry((src_label.clone(), src_id))
            .or_default()
            .push((rel_type.clone(), id));
        overlay
            .inserted_in_adj
            .entry((dst_label.clone(), dst_id))
            .or_default()
            .push((rel_type.clone(), id));
        note_keys(&mut overlay.inserted_edge_keys, &rel_type, &properties);
        overlay.inserted_edges.insert(
            (rel_type.clone(), id),
            InsertedEdge {
                src_label: src_label.clone(),
                src_id,
                dst_label: dst_label.clone(),
                dst_id,
                properties,
            },
        );
        self.pending
            .borrow_mut()
            .edges
            .insert((rel_type.clone(), id));
        Ok(Value::Edge {
            rel_type,
            id,
            src_label,
            src_id,
            dst_label,
            dst_id,
            projected_properties: None,
        })
    }

    pub fn insert_node(
        &self,
        label: impl Into<String>,
        properties: BTreeMap<String, Value>,
    ) -> Value {
        let label = label.into();
        let base_rows = self
            .nodes
            .get(&label)
            .map(|table| table.batch.num_rows() as i64)
            .unwrap_or(0);
        let mut overlay = self.overlay.borrow_mut();
        let counter = overlay
            .inserted_node_counts
            .entry(label.clone())
            .or_insert(0);
        let id = base_rows + *counter;
        *counter += 1;
        overlay.unassigned_public_ids.insert((false,label.clone(),id));
        note_keys(&mut overlay.inserted_node_keys, &label, &properties);
        overlay
            .inserted_nodes
            .insert((label.clone(), id), properties);
        self.pending.borrow_mut().nodes.insert((label.clone(), id));
        Value::Node { label, id }
    }

    pub fn set_property(&self, target: &Value, key: impl Into<String>, value: Value) -> CatalogResult<()> {
        if value.contains_cardinality_value() {
            return Err(CatalogError::Schema("Cardinality values cannot be stored as graph properties".into()));
        }
        let key = key.into();
        if matches!(target, Value::VertexProperty { .. }) { return self.set_meta_property(target, &key, value); }
        if let Value::Node {label,id} = target {
            // Scalar language writes replace any existing Gremlin multi-property.
            let address=(label.clone(),*id);
            let mut overlay=self.overlay.borrow_mut();
            if let Some(records)=overlay.vertex_properties.get_mut(&address) {records.remove(&key);if records.is_empty(){overlay.vertex_properties.remove(&address);}}
        }
        self.set_property_scalar(target,key,value)
    }

    pub(super) fn set_property_scalar(
        &self,
        target: &Value,
        key: impl Into<String>,
        value: Value,
    ) -> CatalogResult<()> {
        if value.contains_cardinality_value() {
            return Err(CatalogError::Schema("Cardinality values cannot be stored as graph properties".into()));
        }
        let key = key.into();
        match target {
            Value::Node { label, id } => {
                let node_key = (label.clone(), *id);
                let mut overlay = self.overlay.borrow_mut();
                if overlay.deleted_nodes.contains(&node_key) {
                    return Ok(());
                }
                self.pending.borrow_mut().nodes.insert(node_key.clone());
                if let Some(props) = overlay.inserted_nodes.get_mut(&node_key) {
                    if matches!(value, Value::Null) {
                        props.remove(&key);
                    } else {
                        props.insert(key.clone(), value);
                        note_key(&mut overlay.inserted_node_keys, label, &key);
                    }
                } else if matches!(value, Value::Null) {
                    // A null assignment removes the property; store a null
                    // override so any base-table column stays shadowed.
                    overlay
                        .node_property_overrides
                        .entry(node_key)
                        .or_default()
                        .insert(key.clone(), Value::Null);
                } else {
                    overlay
                        .node_property_overrides
                        .entry(node_key)
                        .or_default()
                        .insert(key.clone(), value);
                    note_key(&mut overlay.override_node_keys, label, &key);
                }
                Ok(())
            }
            Value::Edge { rel_type, id, .. } => {
                let edge_key = (rel_type.clone(), *id);
                let mut overlay = self.overlay.borrow_mut();
                if overlay.deleted_edges.contains(&edge_key) {
                    return Ok(());
                }
                if let Some(keys) = overlay.edge_null_properties.get_mut(&edge_key) {
                    keys.remove(&key);
                    if keys.is_empty() { overlay.edge_null_properties.remove(&edge_key); }
                }
                self.pending.borrow_mut().edges.insert(edge_key.clone());
                if let Some(edge) = overlay.inserted_edges.get_mut(&edge_key) {
                    if matches!(value, Value::Null) {
                        edge.properties.remove(&key);
                    } else {
                        edge.properties.insert(key.clone(), value);
                        note_key(&mut overlay.inserted_edge_keys, rel_type, &key);
                    }
                } else if matches!(value, Value::Null) {
                    overlay
                        .edge_property_overrides
                        .entry(edge_key)
                        .or_default()
                        .insert(key.clone(), Value::Null);
                } else {
                    overlay
                        .edge_property_overrides
                        .entry(edge_key)
                        .or_default()
                        .insert(key.clone(), value);
                    note_key(&mut overlay.override_edge_keys, rel_type, &key);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Apply a whole property map to an element. `replace` discards every
    /// property not named in `properties` (Cypher `n = {…}`); otherwise the
    /// map is merged over the existing bag (`n += {…}`).
    pub fn set_properties(
        &self,
        target: &Value,
        properties: BTreeMap<String, Value>,
        replace: bool,
    ) -> CatalogResult<()> {
        if properties.values().any(Value::contains_cardinality_value) {
            return Err(CatalogError::Schema("Cardinality values cannot be stored as graph properties".into()));
        }
        if let Value::Node{label,id}=target {
            let mut overlay=self.overlay.borrow_mut();
            if replace {overlay.vertex_properties.remove(&(label.clone(),*id));}
            else if let Some(records)=overlay.vertex_properties.get_mut(&(label.clone(),*id)) {for key in properties.keys(){records.remove(key);}}
        }
        match target {
            Value::Node { label, id } => {
                let node_key = (label.clone(), *id);
                let mut overlay = self.overlay.borrow_mut();
                if overlay.deleted_nodes.contains(&node_key) {
                    return Ok(());
                }
                self.pending.borrow_mut().nodes.insert(node_key.clone());
                if let Some(props) = overlay.inserted_nodes.get_mut(&node_key) {
                    if replace {
                        *props = properties.clone();
                    } else {
                        props.extend(properties.clone());
                    }
                    note_keys(&mut overlay.inserted_node_keys, label, &properties);
                    return Ok(());
                }
                note_keys(&mut overlay.override_node_keys, label, &properties);
                if replace {
                    overlay.replaced_node_properties.insert(node_key.clone());
                    overlay.node_property_overrides.insert(node_key, properties);
                } else {
                    overlay
                        .node_property_overrides
                        .entry(node_key)
                        .or_default()
                        .extend(properties);
                }
                Ok(())
            }
            Value::Edge { rel_type, id, .. } => {
                let edge_key = (rel_type.clone(), *id);
                let mut overlay = self.overlay.borrow_mut();
                if overlay.deleted_edges.contains(&edge_key) {
                    return Ok(());
                }
                if replace {
                    overlay.edge_null_properties.remove(&edge_key);
                } else if let Some(keys) = overlay.edge_null_properties.get_mut(&edge_key) {
                    for key in properties.keys() { keys.remove(key); }
                    if keys.is_empty() { overlay.edge_null_properties.remove(&edge_key); }
                }
                self.pending.borrow_mut().edges.insert(edge_key.clone());
                if let Some(edge) = overlay.inserted_edges.get_mut(&edge_key) {
                    if replace {
                        edge.properties = properties.clone();
                    } else {
                        edge.properties.extend(properties.clone());
                    }
                    note_keys(&mut overlay.inserted_edge_keys, rel_type, &properties);
                    return Ok(());
                }
                note_keys(&mut overlay.override_edge_keys, rel_type, &properties);
                if replace {
                    overlay.replaced_edge_properties.insert(edge_key.clone());
                    overlay.edge_property_overrides.insert(edge_key, properties);
                } else {
                    overlay
                        .edge_property_overrides
                        .entry(edge_key)
                        .or_default()
                        .extend(properties);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub fn delete_value(&self, target: &Value, detach: bool) -> CatalogResult<()> {
        match target {
            Value::VertexProperty { .. } | Value::Property { .. } => self.remove_property(target),
            Value::Node { label, id } => {
                let outgoing = self.out_edges(label, *id, &[]);
                let incoming = self.in_edges(label, *id, &[]);
                if detach {
                    let mut overlay = self.overlay.borrow_mut();
                    for (rel_type, edge_row, _, _) in outgoing.iter().chain(incoming.iter()) {
                        overlay.deleted_edges.insert((rel_type.clone(), *edge_row));
                        self.pending
                            .borrow_mut()
                            .edges
                            .insert((rel_type.clone(), *edge_row));
                    }
                } else if !outgoing.is_empty() || !incoming.is_empty() {
                    return Err(CatalogError::DeleteIntegrity(format!(
                        "cannot delete node `{label}` (id {id}): it still has relationships; \
                         use DETACH DELETE to remove them first"
                    )));
                }
                self.overlay
                    .borrow_mut()
                    .deleted_nodes
                    .insert((label.clone(), *id));
                self.pending.borrow_mut().nodes.insert((label.clone(), *id));
                Ok(())
            }
            Value::Edge { rel_type, id, .. } => {
                self.overlay
                    .borrow_mut()
                    .deleted_edges
                    .insert((rel_type.clone(), *id));
                self.pending
                    .borrow_mut()
                    .edges
                    .insert((rel_type.clone(), *id));
                Ok(())
            }
            _ => Ok(()),
        }
    }

}
