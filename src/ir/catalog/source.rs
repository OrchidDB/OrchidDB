//! Query-scoped storage access for residual graph kernels. SQL islands read the
//! same transaction directly; this interface only fetches their residual needs.
use super::*;
pub(crate) type Endpoints = (String, ElementId, String, ElementId);
pub(crate) type Neighbor = (String, ElementId, String, ElementId);
#[derive(Debug, Clone)]
pub(crate) struct NativeState {
    pub node: Option<Value>,
    pub public_id: Option<Value>,
    pub labels: Option<Vec<String>>,
    pub cypher_id: Option<i64>,
    pub null_keys: std::collections::BTreeSet<String>,
    pub allow_null: bool,
}
pub(crate) trait GraphSource: std::fmt::Debug + Send + Sync {
    #[cfg(feature = "duckdb")]
    fn executor(&self) -> Option<Arc<std::sync::Mutex<crate::ir::rel::sql::DuckDbExecutor>>>;
    fn invalidate(&self) {}
    fn supports_dynamic_schema(&self) -> bool {
        false
    }
    fn public_addresses(&self, _value: &Value, _edge: bool) -> Vec<(bool, String, ElementId)> {
        vec![]
    }
    fn native_state(&self, _edge: bool, _name: &str, _id: &ElementId) -> Option<NativeState> {
        None
    }
    fn function(&self, name: &str, args: &[Value]) -> Option<Result<Value, String>>;
    fn property_handle(&self, name: &str, id: &ElementId, key: &str) -> i64;
    fn property(&self, edge: bool, name: &str, id: &ElementId, key: &str) -> Value;
    fn ids(&self, edge: bool, name: &str) -> Vec<ElementId>;
    fn endpoints(&self, name: &str, id: &ElementId) -> Option<Endpoints>;
    fn exists(&self, edge: bool, name: &str, id: &ElementId) -> bool;
    fn prefetch_neighbors(&self, incoming: bool, nodes: &[(String, ElementId)], types: &[String]);
    fn neighbors(
        &self,
        incoming: bool,
        name: &str,
        id: &ElementId,
        types: &[String],
    ) -> Vec<Neighbor>;
    fn prefetch(&self, values: &mut dyn Iterator<Item = &Value>);
    fn access_decisions(&self) -> Vec<crate::ir::rel::statistics::OptimizerDecision> {
        Vec::new()
    }
    fn stats(&self) -> (usize, Vec<String>);
    fn check(&self) -> Result<(), String>;
}
impl PropertyGraph {
    /// Read mappings describe scans; dynamic managed sources own their writes.
    pub(crate) fn property_allocator(&self) -> i64 {
        self.overlay.borrow().next_property_id
    }
    pub(crate) fn set_property_allocator(&self, next: i64) {
        self.overlay.borrow_mut().next_property_id = next;
    }
    pub(crate) fn write_mapping(
        &self,
    ) -> Option<&std::sync::Arc<crate::ir::rel::mapping::GraphMapping>> {
        if self
            .source
            .as_ref()
            .is_some_and(|s| s.supports_dynamic_schema())
        {
            None
        } else {
            self.mapping.as_ref()
        }
    }
    pub(crate) fn hydrate_native_state(&self, edge: bool, name: &str, id: &ElementId) {
        let address = (edge, name.to_owned(), id.clone());
        if self.native_hydrated.borrow().contains(&address) {
            return;
        }
        let Some(state) = self
            .source
            .as_ref()
            .and_then(|s| s.native_state(edge, name, id))
        else {
            return;
        };
        let key = (name.to_owned(), id.clone());
        let mut overlay = self.overlay.borrow_mut();
        if let Some(native) = state.node {
            if overlay
                .restore_native_node_state(key.clone(), &native)
                .is_err()
            {
                return;
            }
        }
        if let Some(public_id) = state.public_id {
            overlay
                .public_ids
                .entry(address.clone())
                .or_insert(public_id);
        }
        if let Some(labels) = state.labels {
            overlay
                .node_label_sets
                .entry(key.clone())
                .or_insert(labels.into_iter().collect());
        }
        if let Some(value) = state.cypher_id {
            overlay.cypher_ids.entry(address.clone()).or_insert(value);
        }
        if !state.null_keys.is_empty() {
            overlay
                .edge_null_properties
                .entry(key)
                .or_insert(state.null_keys);
        }
        overlay.allow_null_property_values |= state.allow_null;
        self.native_hydrated.borrow_mut().insert(address);
    }
    pub(crate) fn native_state(&self, edge: bool, name: &str, id: &ElementId) -> NativeState {
        self.hydrate_native_state(edge, name, id);
        let overlay = self.overlay.borrow();
        let key = (name.to_owned(), id.clone());
        let address = (edge, name.to_owned(), id.clone());
        NativeState {
            node: (!edge).then(|| overlay.native_node_state(&key)),
            public_id: overlay.public_ids.get(&address).cloned(),
            labels: (!edge).then(|| self.node_labels(name, id.clone())),
            cypher_id: overlay.cypher_ids.get(&address).copied(),
            null_keys: overlay
                .edge_null_properties
                .get(&key)
                .cloned()
                .unwrap_or_default(),
            allow_null: overlay.allow_null_property_values,
        }
    }
    /// Fence cached base reads after storage effects while retaining property handles.
    pub fn invalidate_source_cache(&self) {
        if let Some(source) = &self.source {
            source.invalidate();
        }
    }
    pub(crate) fn normalize_source_rows(
        &self,
        rows: &mut [crate::ir::runtime::Row],
    ) -> Result<(), String> {
        if self.mapping.is_none() {
            return Ok(());
        }
        fn key(
            graph: &PropertyGraph,
            edge: bool,
            name: &str,
            id: &mut ElementId,
        ) -> Result<(), String> {
            if let Some(kind) = graph.key_types.get(&(edge, name.into())) {
                *id = id.cast_to(kind)?;
            }
            Ok(())
        }
        fn value(graph: &PropertyGraph, v: &mut Value) -> Result<(), String> {
            match v {
                Value::Node { label, id } => key(graph, false, label, id)?,
                Value::Edge {
                    rel_type,
                    id,
                    src_label,
                    src_id,
                    dst_label,
                    dst_id,
                    ..
                } => {
                    key(graph, true, rel_type, id)?;
                    key(graph, false, src_label, src_id)?;
                    key(graph, false, dst_label, dst_id)?;
                }
                Value::List(v) | Value::Path(v) => {
                    for v in v {
                        value(graph, v)?;
                    }
                }
                Value::Map(m) => {
                    for v in m.values_mut() {
                        value(graph, v)?;
                    }
                }
                _ => {}
            }
            Ok(())
        }
        for row in rows {
            for v in row.bindings.values_mut() {
                value(self, v)?;
            }
        }
        Ok(())
    }
    pub(crate) fn check_source(&self) -> Result<(), String> {
        self.source.as_ref().map_or(Ok(()), |s| s.check())
    }
    pub(crate) fn prefetch_source(&self, rows: &[crate::ir::runtime::Row]) {
        if let Some(source) = &self.source {
            source.prefetch(&mut rows.iter().flat_map(|r| r.bindings.values()));
        }
    }
    pub(crate) fn prefetch_adjacency(
        &self,
        nodes: &[(String, ElementId)],
        dir: crate::ir::plan::Direction,
        types: &[String],
    ) {
        if let Some(source) = &self.source {
            if dir != crate::ir::plan::Direction::In {
                source.prefetch_neighbors(false, nodes, types);
            }
            if dir != crate::ir::plan::Direction::Out {
                source.prefetch_neighbors(true, nodes, types);
            }
        }
    }
    pub(crate) fn base_exists(&self, edge: bool, name: &str, id: &ElementId) -> bool {
        self.source.as_ref().map_or_else(
            || self.base_cell(edge, name, id).is_some(),
            |s| s.exists(edge, name, id),
        )
    }
}

pub(crate) fn collect_element_addresses(
    v: &Value,
    groups: &mut BTreeMap<(bool, String), BTreeSet<ElementId>>,
) {
    match v {
        Value::Node { label, id } => {
            groups
                .entry((false, label.clone()))
                .or_default()
                .insert(id.clone());
        }
        Value::Edge { rel_type, id, .. } => {
            groups
                .entry((true, rel_type.clone()))
                .or_default()
                .insert(id.clone());
        }
        Value::Property { owner, .. } | Value::VertexProperty { owner, .. } => {
            collect_element_addresses(owner, groups)
        }
        Value::List(v) | Value::Path(v) => {
            for v in v {
                collect_element_addresses(v, groups);
            }
        }
        Value::Map(m) => {
            for v in m.values() {
                collect_element_addresses(v, groups);
            }
        }
        _ => {}
    }
}
