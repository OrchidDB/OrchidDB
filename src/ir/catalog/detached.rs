//! Hydrate element cells already selected by a host relational executor.
//! This catalog is query-local scalar context; it never scans or writes storage.
use super::*;

impl PropertyGraph {
    pub(crate) fn detached_node_state(&self, label: &str, id: &ElementId) -> Value {
        self.hydrate_native_state(false, label, id);
        self.overlay.borrow().native_node_state(&(label.to_owned(), id.clone()))
    }

    pub(crate) fn restore_detached_node_state(&self, label: &str, id: &ElementId, state: &Value) -> Result<(), String> {
        self.overlay.borrow_mut().restore_native_node_state((label.to_owned(), id.clone()), state)
    }

    pub(crate) fn attach_element(
        &self,
        element: &Value,
        public_id: Value,
        properties: BTreeMap<String, Value>,
    ) -> CatalogResult<()> {
        {
            let mut overlay = self.overlay.borrow_mut();
            match element {
                Value::Node { label, id } => {
                    overlay
                        .public_ids
                        .insert((false, label.clone(), id.clone()), public_id.clone());
                    overlay
                        .inserted_nodes
                        .entry((label.clone(), id.clone()))
                        .or_default()
                        .extend(properties.clone());
                    overlay
                        .inserted_node_keys
                        .entry(label.clone())
                        .or_default()
                        .extend(properties.keys().cloned());
                }
                Value::Edge {
                    rel_type,
                    id,
                    src_label,
                    src_id,
                    dst_label,
                    dst_id,
                    ..
                } => {
                    overlay
                        .public_ids
                        .insert((true, rel_type.clone(), id.clone()), public_id.clone());
                    overlay.edge_null_properties.entry((rel_type.clone(), id.clone())).or_default()
                        .extend(properties.iter().filter_map(|(key,value)| (*value == Value::Null).then_some(key.clone())));
                    overlay.inserted_edges.insert(
                        (rel_type.clone(), id.clone()),
                        InsertedEdge {
                            src_label: src_label.clone(),
                            src_id: src_id.clone(),
                            dst_label: dst_label.clone(),
                            dst_id: dst_id.clone(),
                            properties: properties.clone(),
                        },
                    );
                    overlay
                        .inserted_edge_keys
                        .entry(rel_type.clone())
                        .or_default()
                        .extend(properties.keys().cloned());
                }
                _ => {
                    return Err(CatalogError::Schema(
                        "Expected detached graph element".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}
