//! Writes to the session-local property graph overlay.

use super::*;

impl PropertyGraph {
    /// Cypher numeric identity is independent of labels and Gremlin public IDs.
    pub fn cypher_id(&self, value: &Value) -> Option<i64> {
        let (edge, name, id) = match value {
            Value::Node { label, id } => (false, label, id.clone()),
            Value::Edge { rel_type, id, .. } => (true, rel_type, id.clone()),
            _ => return None,
        };
        if let Some(id) = self.overlay.borrow().cypher_ids.get(&(edge, name.clone(), id.clone())) { return Some(id.clone()); }
        // Older checkpoints predate numeric Cypher IDs. Fill their insertion
        // slots deterministically, retaining any IDs already persisted by newer writes.
        {
            let mut overlay = self.overlay.borrow_mut();
            let counts = if edge { &overlay.inserted_edge_counts } else { &overlay.inserted_node_counts };
            let mut sources = counts.iter().map(|(name, count)| (name.clone(), *count)).collect::<Vec<_>>();
            sources.sort();
            let base_count = if edge { self.edge_row_counts.values().sum::<i64>() }
                else { self.nodes.values().map(|table| table.batch.num_rows() as i64).sum::<i64>() };
            let used = overlay.cypher_ids.iter().filter(|((is_edge, _, _), _)| *is_edge == edge)
                .map(|(_, id)| id.clone()).collect::<std::collections::BTreeSet<_>>();
            let mut next = base_count;
            for (source, count) in sources {
                let base = if edge { self.edge_row_counts.get(&source).copied().unwrap_or(0) }
                    else { self.nodes.get(&source).map(|table| table.batch.num_rows() as i64).unwrap_or(0) };
                for row in base..base + count {
                    let key = (edge, source.clone(), row.into());
                    if !overlay.cypher_ids.contains_key(&key) {
                        while used.contains(&next) { next += 1; }
                        overlay.cypher_ids.insert(key, next);
                        next += 1;
                    }
                }
            }
            if let Some(id) = overlay.cypher_ids.get(&(edge, name.clone(), id.clone())) { return Some(id.clone()); }
        }
        let mut offset = 0;
        if edge {
            for candidate in &self.edge_order {
                if candidate == name { return id.as_i64().map(|id| offset + id); }
                offset += self.edge_row_counts.get(candidate).copied().unwrap_or(0);
            }
        } else {
            for candidate in &self.node_order {
                if candidate == name { return id.as_i64().map(|id| offset + id); }
                offset += self.nodes[candidate].batch.num_rows() as i64;
            }
        }
        None
    }

    /// Logical Cypher labels do not participate in the physical element address.
    pub fn node_labels(&self, storage: &str, id: ElementId) -> Vec<String> {
        self.overlay.borrow().node_label_sets.get(&(storage.to_string(), id.clone()))
            .map(|labels| labels.iter().cloned().collect())
            .unwrap_or_else(|| vec![storage.to_string()])
    }

    pub fn set_node_labels(&self, node: &Value, labels: impl IntoIterator<Item = String>) -> CatalogResult<()> {
        let Value::Node { label, id } = node else {
            return Err(CatalogError::Schema("Labels require a node".into()));
        };
        if !self.node_is_live(label, id.clone()) {
            return Err(CatalogError::Schema("Cannot label a deleted node".into()));
        }
        self.overlay.borrow_mut().node_label_sets.insert((label.clone(), id.clone()), labels.into_iter().collect());
        self.pending.borrow_mut().nodes.insert((label.clone(), id.clone()));
        Ok(())
    }

    pub fn node_matches_labels(&self, storage: &str, id: ElementId, expr: &crate::ir::plan::LabelExpr) -> bool {
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
    pub fn insert_edge(&self, rel_type: impl Into<String>, src:&Value,dst:&Value,properties:BTreeMap<String,Value>)->CatalogResult<Value> {
        self.insert_edge_with_key(rel_type,src,dst,properties,None)
    }

    pub fn insert_edge_with_key(
        &self,
        rel_type: impl Into<String>,
        src: &Value,
        dst: &Value,
        properties: BTreeMap<String, Value>,
        supplied_key: Option<&Value>,
    ) -> CatalogResult<Value> {
        if properties.values().any(Value::contains_cardinality_value) {
            return Err(CatalogError::Schema("Cardinality values cannot be stored as graph properties".into()));
        }
        let rel_type = rel_type.into();
        let (src_label, src_id) = node_ref(src, &rel_type, "source")?;
        let (dst_label, dst_id) = node_ref(dst, &rel_type, "destination")?;
        if !self.node_is_live(&src_label, src_id.clone()) {
            return Err(CatalogError::Schema(format!(
                "relationship `{rel_type}` source node `{src_label}#{src_id}` does not exist"
            )));
        }
        if !self.node_is_live(&dst_label, dst_id.clone()) {
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
        let mut properties=self.mapped_insert_properties(true,&rel_type,properties)?;
        // An FK edge is identified by its child row. Graph callers supply the
        // endpoints, not a second primary key for the relationship.
        let child_id = self
            .mapping
            .as_ref()
            .and_then(|m| m.edge(&rel_type))
            .and_then(|m| m.foreign_key)
            .map(|child| match child {
                crate::ir::rel::mapping::ForeignKeyEndpoint::Source => &src_id,
                crate::ir::rel::mapping::ForeignKeyEndpoint::Destination => &dst_id,
            });
        let derived_key = child_id.map(|id| Value::Scalar(id.scalar().clone()));
        if let (Some(supplied), Some(child)) = (supplied_key, child_id) {
            let supplied = ElementId::try_from(supplied).map_err(CatalogError::Schema)?;
            if supplied.cast_to(&child.scalar().data_type()).map_err(CatalogError::Schema)? != *child {
                return Err(CatalogError::Schema(
                    "foreign-key relationship identity must equal its child key".into(),
                ));
            }
        }
        let mapped_id = self.mapped_insert_key(
            true,
            &rel_type,
            &mut properties,
            derived_key.as_ref().or(supplied_key),
        )?;
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
        let cypher_id = self.edge_row_counts.values().sum::<i64>()
            + overlay.inserted_edge_counts.values().sum::<i64>();
        let counter = overlay
            .inserted_edge_counts
            .entry(rel_type.clone())
            .or_insert(0);
        let id = mapped_id.unwrap_or_else(|| ElementId::from(base + *counter));
        *counter += 1;
        if child_id.is_some() && overlay.deleted_edges.remove(&(rel_type.clone(), id.clone())) {
            // DELETE + CREATE replaces one FK slot. Only remove adjacency for
            // that slot; ordinary inserts must not scan the accumulated batch.
            let previous = overlay.inserted_edges.get(&(rel_type.clone(), id.clone()))
                .map(|edge| ((edge.src_label.clone(), edge.src_id.clone()),
                             (edge.dst_label.clone(), edge.dst_id.clone())));
            if let Some((src, dst)) = previous {
                if let Some(refs) = overlay.inserted_out_adj.get_mut(&src) {
                    refs.retain(|(rel, key)| rel != &rel_type || key != &id);
                }
                if let Some(refs) = overlay.inserted_in_adj.get_mut(&dst) {
                    refs.retain(|(rel, key)| rel != &rel_type || key != &id);
                }
            }
        }
        overlay.cypher_ids.insert((true, rel_type.clone(), id.clone().into()), cypher_id);
        overlay.unassigned_public_ids.insert((true,rel_type.clone(),id.clone().into()));
        overlay
            .inserted_out_adj
            .entry((src_label.clone(), src_id.clone()))
            .or_default()
            .push((rel_type.clone(), id.clone().into()));
        overlay
            .inserted_in_adj
            .entry((dst_label.clone(), dst_id.clone()))
            .or_default()
            .push((rel_type.clone(), id.clone().into()));
        note_keys(&mut overlay.inserted_edge_keys, &rel_type, &properties);
        overlay.inserted_edges.insert(
            (rel_type.clone(), id.clone().into()),
            InsertedEdge {
                src_label: src_label.clone(),
                src_id: src_id.clone(),
                dst_label: dst_label.clone(),
                dst_id: dst_id.clone(),
                properties,
            },
        );
        self.pending
            .borrow_mut()
            .edges
            .insert((rel_type.clone(), id.clone().into()));
        Ok(Value::Edge {
            rel_type,
            id: id.clone().into(),
            src_label,
            src_id,
            dst_label,
            dst_id,
            projected_properties: None,
        })
    }

    pub fn insert_node(&self, label: impl Into<String>, properties: BTreeMap<String, Value>) -> Value {
        self.try_insert_node(label, properties).expect("fixture node insertion")
    }

    pub fn try_insert_node(&self, label:impl Into<String>, properties:BTreeMap<String,Value>)->CatalogResult<Value> {
        self.try_insert_node_with_key(label,properties,None)
    }

    pub fn try_insert_node_with_key(
        &self,
        label: impl Into<String>,
        properties: BTreeMap<String, Value>,
        supplied_key: Option<&Value>,
    ) -> CatalogResult<Value> {
        let label = label.into();
        let mut properties=self.mapped_insert_properties(false,&label,properties)?;
        let mapped_id = self.mapped_insert_key(false, &label, &mut properties, supplied_key)?;
        let base_rows = self
            .nodes
            .get(&label)
            .map(|table| table.batch.num_rows() as i64)
            .unwrap_or(0);
        let mut overlay = self.overlay.borrow_mut();
        let cypher_id = self.nodes.values().map(|table| table.batch.num_rows() as i64).sum::<i64>()
            + overlay.inserted_node_counts.values().sum::<i64>();
        let counter = overlay
            .inserted_node_counts
            .entry(label.clone())
            .or_insert(0);
        let id = mapped_id.unwrap_or_else(|| ElementId::from(base_rows + *counter));
        *counter += 1;
        overlay.cypher_ids.insert((false, label.clone(), id.clone().into()), cypher_id);
        overlay.unassigned_public_ids.insert((false,label.clone(),id.clone().into()));
        note_keys(&mut overlay.inserted_node_keys, &label, &properties);
        overlay
            .inserted_nodes
            .insert((label.clone(), id.clone().into()), properties);
        self.pending.borrow_mut().nodes.insert((label.clone(), id.clone().into()));
        Ok(Value::Node { label, id: id.clone().into() })
    }

    pub fn set_property(&self, target: &Value, key: impl Into<String>, value: Value) -> CatalogResult<()> {
        if value.contains_cardinality_value() {
            return Err(CatalogError::Schema("Cardinality values cannot be stored as graph properties".into()));
        }
        let key = key.into();
        if matches!(target, Value::VertexProperty { .. }) { return self.set_meta_property(target, &key, value); }
        if let Value::Node {label,id} = target {
            // Scalar language writes replace any existing Gremlin multi-property.
            let address=(label.clone(),id.clone());
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
                let node_key = (label.clone(), id.clone());
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
                let edge_key = (rel_type.clone(), id.clone());
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
            if replace {overlay.vertex_properties.remove(&(label.clone(),id.clone()));}
            else if let Some(records)=overlay.vertex_properties.get_mut(&(label.clone(),id.clone())) {for key in properties.keys(){records.remove(key);}}
        }
        match target {
            Value::Node { label, id } => {
                let node_key = (label.clone(), id.clone());
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
                let edge_key = (rel_type.clone(), id.clone());
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
                let outgoing = self.out_edges(label, id.clone(), &[]);
                let incoming = self.in_edges(label, id.clone(), &[]);
                if detach {
                    let mut overlay = self.overlay.borrow_mut();
                    for (rel_type, edge_row, _, _) in outgoing.iter().chain(incoming.iter()) {
                        overlay.deleted_edges.insert((rel_type.clone(), edge_row.clone()));
                        self.pending
                            .borrow_mut()
                            .edges
                            .insert((rel_type.clone(), edge_row.clone()));
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
                    .insert((label.clone(), id.clone()));
                self.pending.borrow_mut().nodes.insert((label.clone(), id.clone()));
                Ok(())
            }
            Value::Edge { rel_type, id, .. } => {
                self.overlay
                    .borrow_mut()
                    .deleted_edges
                    .insert((rel_type.clone(), id.clone()));
                self.pending
                    .borrow_mut()
                    .edges
                    .insert((rel_type.clone(), id.clone()));
                Ok(())
            }
            _ => Ok(()),
        }
    }

}
