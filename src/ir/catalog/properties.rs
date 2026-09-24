//! Native Gremlin property records and public identities. All state lives in the
//! graph overlay, so graph clones are transaction checkpoints.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cardinality {
    Single,
    List,
    Set,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct VertexPropertyRecord {
    pub id: i64,
    pub value: Value,
    pub public_id: Option<Value>,
    pub meta: BTreeMap<String, Value>,
}

impl PropertyGraph {
    /// Select Gremlin's null-valued-property feature. Disabled by default for
    /// compatibility with providers that interpret null assignments as removal.
    /// Stored records keep their presence independently of this write policy.
    pub fn enable_null_property_values(&self, enabled: bool) {
        self.overlay.borrow_mut().allow_null_property_values = enabled;
        // Include the provider setting in the next delta for existing entities.
        let mut pending = self.pending.borrow_mut();
        for label in self.labels() {
            for id in self.node_ids(&label).unwrap_or_default() { pending.nodes.insert((label.clone(), id)); }
        }
        for label in self.rel_types() {
            for id in self.edge_ids(&label) { pending.edges.insert((label.clone(), id)); }
        }
    }

    pub fn supports_null_property_values(&self) -> bool {
        self.overlay.borrow().allow_null_property_values
    }

    /// Materialize a legacy scalar property exactly once. A list-valued column
    /// remains one property; cardinality is represented only by record count.
    fn ensure_vertex_property(&self, owner: &Value, key: &str) {
        let Value::Node { label, id } = owner else {
            return;
        };
        let address = (label.clone(), *id);
        if self
            .overlay
            .borrow()
            .vertex_properties
            .get(&address)
            .is_some_and(|m| m.contains_key(key))
        {
            return;
        }
        let value = self.node_property(label, *id, key);
        let mut ov = self.overlay.borrow_mut();
        let records = if value == Value::Null {
            vec![]
        } else {
            let property_id = ov.next_property_id;
            ov.next_property_id += 1;
            vec![VertexPropertyRecord {
                id: property_id,
                value,
                public_id: None,
                meta: BTreeMap::new(),
            }]
        };
        ov.vertex_properties
            .entry(address.clone())
            .or_default()
            .insert(key.into(), records);
        self.pending.borrow_mut().nodes.insert(address);
    }

    pub fn properties(&self, owner: &Value, keys: &[String]) -> Vec<Value> {
        self.properties_inner(owner, keys, false)
    }

    /// The JVM provider accepts double-underscore user keys. Only explicit
    /// overlay properties may use them: physical Arrow columns are not exposed.
    pub(crate) fn jvm_properties(&self, owner: &Value, keys: &[String]) -> Vec<Value> {
        self.properties_inner(owner, keys, true)
    }

    fn properties_inner(&self, owner: &Value, keys: &[String], jvm_user_keys: bool) -> Vec<Value> {
        match owner {
            Value::Node { label, id } => {
                if !self.node_is_live(label, *id) {
                    return vec![];
                }
                let keys = if keys.is_empty() {
                    self.node_property_keys(label)
                } else {
                    keys.to_vec()
                };
                let mut out = vec![];
                for key in keys.into_iter().filter(|k| {
                    (!k.starts_with("__") || (jvm_user_keys
                        && self.overlay.borrow().vertex_properties
                            .get(&(label.clone(), *id)).is_some_and(|m| m.contains_key(k))))
                        && (k != "id"
                            || self
                                .overlay
                                .borrow()
                                .vertex_properties
                                .get(&(label.clone(), *id))
                                .is_some_and(|m| m.contains_key(k)))
                }) {
                    self.ensure_vertex_property(owner, &key);
                    let ov = self.overlay.borrow();
                    if let Some(records) = ov
                        .vertex_properties
                        .get(&(label.clone(), *id))
                        .and_then(|m| m.get(&key))
                    {
                        out.extend(records.iter().map(|r| Value::VertexProperty {
                            id: r.id,
                            owner: Box::new(owner.clone()),
                            key: key.clone(),
                            value: Box::new(r.value.clone()),
                        }));
                    }
                }
                out
            }
            Value::Edge { rel_type, id, .. } => {
                if !self.overlay.borrow().edge_is_live(rel_type, *id) {
                    return vec![];
                }
                let keys = if keys.is_empty() {
                    self.edge_property_keys(rel_type)
                } else {
                    keys.to_vec()
                };
                keys.into_iter()
                    .filter(|k| {
                        if k == "id" { return false; }
                        if !k.starts_with("__") { return true; }
                        if !jvm_user_keys { return false; }
                        let ov = self.overlay.borrow();
                        let address = (rel_type.clone(), *id);
                        ov.inserted_edges.get(&address).is_some_and(|e| e.properties.contains_key(k))
                            || ov.edge_property_overrides.get(&address).is_some_and(|p| p.contains_key(k))
                            || ov.edge_null_properties.get(&address).is_some_and(|keys| keys.contains(k))
                    })
                    .filter_map(|key| {
                        let value = self.edge_property(rel_type, *id, &key);
                        let present_null = self.overlay.borrow().edge_null_properties
                            .get(&(rel_type.clone(), *id)).is_some_and(|keys| keys.contains(&key));
                        (value != Value::Null || present_null).then(|| Value::Property {
                            owner: Box::new(owner.clone()),
                            key,
                            value: Box::new(value),
                        })
                    })
                    .collect()
            }
            Value::VertexProperty {
                id,
                owner: vertex,
                key,
                ..
            } => {
                let Value::Node {
                    label,
                    id: vertex_id,
                } = vertex.as_ref()
                else {
                    return vec![];
                };
                let ov = self.overlay.borrow();
                ov.vertex_properties
                    .get(&(label.clone(), *vertex_id))
                    .and_then(|m| m.get(key))
                    .into_iter()
                    .flatten()
                    .filter(|r| r.id == *id)
                    .flat_map(|r| r.meta.iter())
                    .filter(|(key, _)| keys.is_empty() || keys.contains(key))
                    .map(|(key, value)| Value::Property {
                        owner: Box::new(owner.clone()),
                        key: key.clone(),
                        value: Box::new(value.clone()),
                    })
                    .collect()
            }
            _ => vec![],
        }
    }

    /// Gremlin writes honor the configured null-valued-property feature.
    /// Scalar-language writes use `set_property`, where null removes a property.
    pub fn set_gremlin_property(&self, target: &Value, key: &str, value: Value) -> CatalogResult<()> {
        match target {
            Value::Node { .. } => {
                self.set_vertex_property(target, key, value, Cardinality::Single, BTreeMap::new())?;
            }
            Value::VertexProperty { .. } => self.set_meta_property(target, key, value)?,
            Value::Edge { rel_type, id, .. } => {
                let is_null = value == Value::Null && self.supports_null_property_values();
                self.set_property_scalar(target, key, value)?;
                let mut overlay = self.overlay.borrow_mut();
                if is_null && overlay.edge_is_live(rel_type, *id) {
                    overlay.edge_null_properties.entry((rel_type.clone(), *id))
                        .or_default().insert(key.into());
                    note_key(&mut overlay.override_edge_keys, rel_type, key);
                }
            }
            _ => return Err(CatalogError::Schema("Property requires an element".into())),
        }
        Ok(())
    }

    pub fn set_vertex_property(
        &self,
        owner: &Value,
        key: &str,
        value: Value,
        cardinality: Cardinality,
        meta: BTreeMap<String, Value>,
    ) -> CatalogResult<Value> {
        self.set_vertex_property_inner(owner, key, value, cardinality, meta, false)
    }

    pub(crate) fn set_jvm_vertex_property(
        &self,
        owner: &Value,
        key: &str,
        value: Value,
        cardinality: Cardinality,
        meta: BTreeMap<String, Value>,
    ) -> CatalogResult<Value> {
        self.set_vertex_property_inner(owner, key, value, cardinality, meta, true)
    }

    fn set_vertex_property_inner(
        &self,
        owner: &Value,
        key: &str,
        value: Value,
        cardinality: Cardinality,
        meta: BTreeMap<String, Value>,
        jvm_user_keys: bool,
    ) -> CatalogResult<Value> {
        let Value::Node { label, id } = owner else {
            return Err(CatalogError::Schema(
                "Vertex property requires a vertex".into(),
            ));
        };
        if key == "id" {
            self.set_element_public_id(owner, self.element_public_id(owner))?;
        }
        // An internal Arrow column must not become the first list/set member
        // when the JVM explicitly creates a user property with the same name.
        if jvm_user_keys && key.starts_with("__") {
            self.overlay.borrow_mut().vertex_properties
                .entry((label.clone(), *id)).or_default()
                .entry(key.into()).or_default();
        }
        self.ensure_vertex_property(owner, key);
        let address = (label.clone(), *id);
        let allow_null = self.supports_null_property_values();
        let mut ov = self.overlay.borrow_mut();
        if cardinality == Cardinality::Set {
            let member_key = crate::ir::value::set_member_key(&value);
            if let Some(record) = ov
                .vertex_properties
                .get_mut(&address)
                .and_then(|m| m.get_mut(key))
                .and_then(|rs| rs.iter_mut().find(|r| crate::ir::value::set_member_key(&r.value) == member_key))
            {
                for (key, value) in meta {
                    if value == Value::Null && !allow_null { record.meta.remove(&key); }
                    else { record.meta.insert(key, value); }
                }
                let result = Value::VertexProperty {
                    id: record.id,
                    owner: Box::new(owner.clone()),
                    key: key.into(),
                    value: Box::new(record.value.clone()),
                };
                self.pending.borrow_mut().nodes.insert(address);
                return Ok(result);
            }
        }
        let property_id = ov.next_property_id;
        ov.next_property_id += 1;
        let records = ov
            .vertex_properties
            .entry(address.clone())
            .or_default()
            .entry(key.into())
            .or_default();
        if cardinality == Cardinality::Single {
            records.clear();
        }
        if value != Value::Null || allow_null {
            records.push(VertexPropertyRecord {
                id: property_id,
                value: value.clone(),
                public_id: None,
                meta: meta.into_iter().filter(|(_, value)| allow_null || value != &Value::Null).collect(),
            });
        }
        let scalar = records
            .first()
            .map(|r| r.value.clone())
            .unwrap_or(Value::Null);
        drop(ov);
        // Keep the scalar catalog view for Cypher and ordinary property filters.
        self.set_property_scalar(owner, key, scalar)?;
        // A null record still contributes a key to property enumeration.
        note_key(&mut self.overlay.borrow_mut().override_node_keys, label, key);
        self.pending.borrow_mut().nodes.insert(address);
        Ok(Value::VertexProperty {
            id: property_id,
            owner: Box::new(owner.clone()),
            key: key.into(),
            value: Box::new(value),
        })
    }

    pub fn set_meta_property(&self, target: &Value, key: &str, value: Value) -> CatalogResult<()> {
        let Value::VertexProperty {
            id,
            owner,
            key: property_key,
            ..
        } = target
        else {
            return Err(CatalogError::Schema(
                "Meta-property requires a vertex property".into(),
            ));
        };
        let Value::Node {
            label,
            id: vertex_id,
        } = owner.as_ref()
        else {
            return Err(CatalogError::Schema("Invalid vertex property owner".into()));
        };
        let address = (label.clone(), *vertex_id);
        let allow_null = self.supports_null_property_values();
        let mut ov = self.overlay.borrow_mut();
        let record = ov
            .vertex_properties
            .get_mut(&address)
            .and_then(|m| m.get_mut(property_key))
            .and_then(|rs| rs.iter_mut().find(|r| r.id == *id))
            .ok_or_else(|| CatalogError::Schema("Vertex property no longer exists".into()))?;
        if value == Value::Null && !allow_null {
            record.meta.remove(key);
        } else {
            record.meta.insert(key.into(), value);
        }
        self.pending.borrow_mut().nodes.insert(address);
        Ok(())
    }

    pub fn remove_property(&self, target: &Value) -> CatalogResult<()> {
        match target {
            Value::Property { owner, key, .. } => {
                if let Value::VertexProperty { id, owner: vertex, key: property_key, .. } = owner.as_ref() {
                    if let Value::Node { label, id: vertex_id } = vertex.as_ref() {
                        let address = (label.clone(), *vertex_id);
                        if let Some(record) = self.overlay.borrow_mut().vertex_properties
                            .get_mut(&address).and_then(|m| m.get_mut(property_key))
                            .and_then(|records| records.iter_mut().find(|r| r.id == *id)) {
                            record.meta.remove(key);
                        }
                        self.pending.borrow_mut().nodes.insert(address);
                    }
                    Ok(())
                } else {
                    self.set_property(owner, key, Value::Null)
                }
            },
            Value::VertexProperty { id, owner, key, .. } => {
                let Value::Node {
                    label,
                    id: vertex_id,
                } = owner.as_ref()
                else {
                    return Ok(());
                };
                let address = (label.clone(), *vertex_id);
                let mut ov = self.overlay.borrow_mut();
                let scalar = if let Some(records) = ov
                    .vertex_properties
                    .get_mut(&address)
                    .and_then(|m| m.get_mut(key))
                {
                    records.retain(|r| r.id != *id);
                    records
                        .first()
                        .map(|r| r.value.clone())
                        .unwrap_or(Value::Null)
                } else {
                    return Ok(());
                };
                drop(ov);
                self.set_property_scalar(owner, key, scalar)
            }
            _ => Ok(()),
        }
    }

    pub fn element_public_id(&self, element: &Value) -> Value {
        let address = match element {
            Value::Node { label, id } => (false, label.clone(), *id),
            Value::Edge { rel_type, id, .. } => (true, rel_type.clone(), *id),
            Value::VertexProperty { id, owner, key, .. } => {
                if let Value::Node {
                    label,
                    id: vertex_id,
                } = owner.as_ref()
                {
                    if let Some(public_id) = self
                        .overlay
                        .borrow()
                        .vertex_properties
                        .get(&(label.clone(), *vertex_id))
                        .and_then(|m| m.get(key))
                        .and_then(|rs| rs.iter().find(|r| r.id == *id))
                        .and_then(|r| r.public_id.clone())
                    {
                        return public_id;
                    }
                }
                return Value::Long(*id);
            }
            _ => return Value::Null,
        };
        if let Some(value) = self.overlay.borrow().public_ids.get(&address) {
            return value.clone();
        }
        let legacy = if address.0 {
            self.edge_property(&address.1, address.2, "id")
        } else {
            self.node_property(&address.1, address.2, "id")
        };
        if legacy != Value::Null {
            legacy
        } else {
            Value::String(format!("{}#{}", address.1, address.2))
        }
    }

    /// Allocate an identity independently of ordinary properties for Gremlin
    /// creation. Legacy catalog insertions retain their existing id-column API.
    pub fn assign_generated_public_id(&self, element: &Value) -> CatalogResult<()> {
        let (edge, name, id) = match element {
            Value::Node { label, id } => (false, label, *id),
            Value::Edge { rel_type, id, .. } => (true, rel_type, *id),
            _ => return Err(CatalogError::Schema("Expected element".into())),
        };
        let mut candidate = Value::String(format!("{name}#{id}"));
        let mut suffix = 0;
        while self
            .find_element_by_public_id(&candidate, edge)
            .is_some_and(|other| other != *element)
        {
            suffix += 1;
            candidate = Value::String(format!("{name}#{id}_{suffix}"));
        }
        self.set_element_public_id(element, candidate)?;
        if !edge && self.node_property(name, id, "id") != Value::Null {
            self.ensure_vertex_property(element, "id");
        }
        Ok(())
    }

    pub fn set_vertex_property_public_id(
        &self,
        property: &Value,
        public_id: Value,
    ) -> CatalogResult<()> {
        let Value::VertexProperty { id, owner, key, .. } = property else {
            return Err(CatalogError::Schema("Expected vertex property".into()));
        };
        let Value::Node {
            label,
            id: vertex_id,
        } = owner.as_ref()
        else {
            return Err(CatalogError::Schema("Expected vertex owner".into()));
        };
        let mut ov = self.overlay.borrow_mut();
        if ov
            .vertex_properties
            .values()
            .flat_map(|m| m.values())
            .flatten()
            .any(|r| {
                r.id != *id
                    && r.public_id
                        .as_ref()
                        .unwrap_or(&Value::Long(r.id))
                        .three_valued_eq(&public_id)
                        == Some(true)
            })
        {
            return Err(CatalogError::Schema(
                "Vertex property id already exists".into(),
            ));
        }
        let record = ov
            .vertex_properties
            .get_mut(&(label.clone(), *vertex_id))
            .and_then(|m| m.get_mut(key))
            .and_then(|rs| rs.iter_mut().find(|r| r.id == *id))
            .ok_or_else(|| CatalogError::Schema("Vertex property no longer exists".into()))?;
        let next_id = public_id.as_i64().and_then(|id| id.checked_add(1));
        record.public_id = Some(public_id);
        if let Some(next) = next_id {
            ov.next_property_id = ov.next_property_id.max(next);
        }

        self.pending
            .borrow_mut()
            .nodes
            .insert((label.clone(), *vertex_id));
        Ok(())
    }

    pub fn set_element_public_id(&self, element: &Value, public_id: Value) -> CatalogResult<()> {
        if matches!(public_id, Value::Null | Value::List(_) | Value::Map(_)) {
            return Err(CatalogError::Schema("Invalid element id".into()));
        }
        let address = match element {
            Value::Node { label, id } => (false, label.clone(), *id),
            Value::Edge { rel_type, id, .. } => (true, rel_type.clone(), *id),
            _ => {
                return Err(CatalogError::Schema(
                    "Element id requires a vertex or edge".into(),
                ));
            }
        };
        if self
            .find_element_by_public_id(&public_id, address.0)
            .is_some_and(|other| other != *element)
        {
            return Err(CatalogError::Schema(format!(
                "Element with id {public_id:?} already exists"
            )));
        }
        if address.0 {
            self.pending
                .borrow_mut()
                .edges
                .insert((address.1.clone(), address.2));
        } else {
            self.pending
                .borrow_mut()
                .nodes
                .insert((address.1.clone(), address.2));
        }
        let mut ov = self.overlay.borrow_mut();
        if let Some(old) = ov.public_ids.insert(address.clone(), public_id.clone()) {
            if let Some(entries) = ov.public_id_lookup.get_mut(&public_id_key(&old)) {
                entries.retain(|entry| entry != &address);
            }
        }
        ov.unassigned_public_ids.remove(&address);
        ov.public_id_lookup
            .entry(public_id_key(&public_id))
            .or_default()
            .push(address);
        Ok(())
    }

    pub fn find_element_by_public_id(&self, public_id: &Value, edge: bool) -> Option<Value> {
        let mut addresses = Vec::new();
        {
            let ov = self.overlay.borrow();
            addresses.extend(
                ov.public_id_lookup
                    .get(&public_id_key(public_id))
                    .into_iter()
                    .flatten()
                    .filter(|(kind, _, _)| *kind == edge)
                    .cloned(),
            );
            addresses.extend(
                ov.unassigned_public_ids
                    .iter()
                    .filter(|(kind, _, _)| *kind == edge)
                    .cloned(),
            );
        }
        // Legacy Arrow catalogs have implicit identities. Native inserts use
        // the explicit index above, so fixture/import creation is linear.
        if edge {
            for (label, count) in &self.edge_row_counts {
                for id in 0..*count {
                    addresses.push((true, label.clone(), id));
                }
            }
        } else {
            for (label, table) in &self.nodes {
                for id in 0..table.batch.num_rows() as i64 {
                    addresses.push((false, label.clone(), id));
                }
            }
        }
        for (edge, name, id) in addresses {
            let value = if edge {
                if !self.overlay.borrow().edge_is_live(&name, id) {
                    continue;
                }
                let Some((src_label, src_id, dst_label, dst_id)) = self.edge_endpoints(&name, id)
                else {
                    continue;
                };
                Value::Edge {
                    rel_type: name,
                    id,
                    src_label,
                    src_id,
                    dst_label,
                    dst_id,
                    projected_properties: None,
                }
            } else {
                if !self.node_is_live(&name, id) {
                    continue;
                }
                Value::Node { label: name, id }
            };
            if self.element_public_id(&value).three_valued_eq(public_id) == Some(true) {
                return Some(value);
            }
        }
        None
    }
}

impl GraphOverlay {
    pub(super) fn rebuild_public_id_lookup(&mut self) {
        self.public_id_lookup.clear();
        for (address, id) in &self.public_ids {
            self.public_id_lookup
                .entry(public_id_key(id))
                .or_default()
                .push(address.clone());
        }
        self.unassigned_public_ids = self
            .inserted_nodes
            .keys()
            .map(|(label, id)| (false, label.clone(), *id))
            .chain(
                self.inserted_edges
                    .keys()
                    .map(|(label, id)| (true, label.clone(), *id)),
            )
            .filter(|address| !self.public_ids.contains_key(address))
            .collect();
    }
    pub(super) fn native_node_state(&self, key: &(String, i64)) -> Value {
        let records = self
            .vertex_properties
            .get(key)
            .map(|m| {
                m.iter()
                    .map(|(key, rs)| {
                        (
                            key.clone(),
                            Value::List(
                                rs.iter()
                                    .map(|r| {
                                        Value::List(vec![
                                            Value::Long(r.id),
                                            r.value.clone(),
                                            Value::Map(r.meta.clone()),
                                            r.public_id.clone().unwrap_or(Value::Null),
                                        ])
                                    })
                                    .collect(),
                            ),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        Value::List(vec![
            Value::Long(self.next_property_id),
            self.public_ids
                .get(&(false, key.0.clone(), key.1))
                .cloned()
                .unwrap_or(Value::Null),
            Value::Map(records),
        ])
    }
    pub(super) fn restore_native_node_state(
        &mut self,
        key: (String, i64),
        state: &Value,
    ) -> Result<(), String> {
        let Value::List(fields) = state else {
            return Err("Invalid native node state".into());
        };
        let [Value::Long(next), public_id, Value::Map(props)] = fields.as_slice() else {
            return Err("Invalid native node fields".into());
        };
        self.next_property_id = self.next_property_id.max(*next);
        self.public_ids.remove(&(false, key.0.clone(), key.1));
        if public_id != &Value::Null {
            self.public_ids
                .insert((false, key.0.clone(), key.1), public_id.clone());
        }
        let mut records = BTreeMap::new();
        for (name, value) in props {
            let Value::List(items) = value else {
                return Err("Invalid property records".into());
            };
            let mut rs = vec![];
            for item in items {
                let Value::List(fields) = item else {
                    return Err("Invalid property record".into());
                };
                let [Value::Long(id), value, Value::Map(meta), public_id] = fields.as_slice()
                else {
                    return Err("Invalid property fields".into());
                };
                rs.push(VertexPropertyRecord {
                    id: *id,
                    value: value.clone(),
                    public_id: if public_id == &Value::Null {
                        None
                    } else {
                        Some(public_id.clone())
                    },
                    meta: meta.clone(),
                });
            }
            records.insert(name.clone(), rs);
        }
        if records.is_empty() {
            self.vertex_properties.remove(&key);
        } else {
            self.vertex_properties.insert(key, records);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn null_presence_incremental_replay_and_removal() {
        let graph = PropertyGraph::new();
        let vertex = graph.insert_node("node", BTreeMap::new());
        let edge = graph.insert_edge("edge", &vertex, &vertex, BTreeMap::new()).unwrap();
        let checkpoint = graph.snapshot_encode().unwrap();
        graph.enable_null_property_values(true);
        let property = graph.set_vertex_property(&vertex, "null", Value::Null,
            Cardinality::Single, [("meta".into(), Value::Null)].into()).unwrap();
        graph.set_gremlin_property(&edge, "null", Value::Null).unwrap();
        let pending = graph.pending.borrow().clone();
        let records = graph.incremental_records(&pending.nodes, &pending.edges).unwrap();
        let mut restored = PropertyGraph::snapshot_decode(&checkpoint).unwrap();
        restored.apply_incremental_records(&records).unwrap();
        assert!(restored.supports_null_property_values());
        assert_eq!(restored.properties(&vertex, &[]), vec![property.clone()]);
        assert_eq!(restored.properties(&property, &[]).len(), 1);
        assert_eq!(restored.properties(&edge, &[]).len(), 1);
        let edge_property = restored.properties(&edge, &[])[0].clone();
        let meta = restored.properties(&property, &[])[0].clone();
        restored.remove_property(&edge_property).unwrap();
        restored.remove_property(&meta).unwrap();
        restored.remove_property(&property).unwrap();
        let pending = restored.pending.borrow().clone();
        let records = restored.incremental_records(&pending.nodes, &pending.edges).unwrap();
        let mut again = graph.clone();
        again.apply_incremental_records(&records).unwrap();
        assert!(again.properties(&vertex, &[]).is_empty());
        assert!(again.properties(&edge, &[]).is_empty());
        assert!(again.properties(&property, &[]).is_empty());
        assert_eq!(graph.properties(&vertex, &[]), vec![property]);
        assert_eq!(graph.properties(&edge, &[]), vec![edge_property]);
    }

    #[test]
    fn native_metadata_incremental_replay_and_checkpoint_rollback() {
        let graph = PropertyGraph::new();
        let v = graph.insert_node("Account", BTreeMap::new());
        let e = graph.insert_edge("loop", &v, &v, BTreeMap::new()).unwrap();
        let checkpoint = graph.snapshot_encode().unwrap();
        let p = graph
            .set_vertex_property(
                &v,
                "roles",
                Value::List(vec![Value::String("admin".into())]),
                Cardinality::List,
                [("since".into(), Value::Int(2020))].into(),
            )
            .unwrap();
        graph
            .set_vertex_property_public_id(&p, Value::String("property-1".into()))
            .unwrap();
        graph
            .set_element_public_id(&v, Value::String("account-1".into()))
            .unwrap();
        graph.set_element_public_id(&e, Value::Long(900)).unwrap();
        let pending = graph.pending.borrow().clone();
        let records = graph
            .incremental_records(&pending.nodes, &pending.edges)
            .unwrap();
        let mut restored = PropertyGraph::snapshot_decode(&checkpoint).unwrap();
        restored.apply_incremental_records(&records).unwrap();
        assert_eq!(restored.properties(&v, &[]), vec![p.clone()]);
        assert_eq!(
            restored.element_public_id(&v),
            Value::String("account-1".into())
        );
        assert_eq!(restored.element_public_id(&e), Value::Long(900));
        assert_eq!(
            restored.element_public_id(&p),
            Value::String("property-1".into())
        );
        assert_eq!(restored.properties(&p, &[]), graph.properties(&p, &[]));
        let before = restored.clone();
        restored.remove_property(&p).unwrap();
        assert!(restored.properties(&v, &[]).is_empty());
        restored = before;
        assert_eq!(restored.properties(&v, &[]), vec![p.clone()]);
        restored
            .remove_property(&restored.properties(&p, &[])[0])
            .unwrap();
        let pending = restored.pending.borrow().clone();
        let records = restored
            .incremental_records(&pending.nodes, &pending.edges)
            .unwrap();
        let mut again = graph.clone();
        again.apply_incremental_records(&records).unwrap();
        assert!(again.properties(&p, &[]).is_empty());
        assert_eq!(again.properties(&v, &[]), vec![p]);
    }
}

fn public_id_key(value: &Value) -> String {
    use bigdecimal::{BigDecimal, FromPrimitive};
    let number = match value {
        Value::Byte(v) => Some(BigDecimal::from(*v)),
        Value::UInt8(v) => Some(BigDecimal::from(*v)),
        Value::Short(v) => Some(BigDecimal::from(*v)),
        Value::UInt16(v) => Some(BigDecimal::from(*v)),
        Value::Int(v) | Value::Long(v) => Some(BigDecimal::from(*v)),
        Value::UInt32(v) => Some(BigDecimal::from(*v)),
        Value::UInt64(v) => Some(BigDecimal::from(*v)),
        Value::BigInt(v) | Value::UInt128(v) => Some(BigDecimal::from(v.clone())),
        Value::Float32(v) => BigDecimal::from_f32(*v),
        Value::Float(v) => BigDecimal::from_f64(*v),
        Value::BigDecimal(v) => Some(v.clone()),
        _ => None,
    };
    if let Some(number) = number {
        format!("number:{}", number.normalized())
    } else {
        format!("{value:?}")
    }
}
