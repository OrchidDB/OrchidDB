//! Query-scoped storage access for residual graph kernels. SQL islands read the
//! same transaction directly; this interface only fetches their residual needs.
use super::*;
pub(crate) type Endpoints = (String, ElementId, String, ElementId);
pub(crate) type Neighbor = (String, ElementId, String, ElementId);
pub(crate) trait GraphSource: std::fmt::Debug + Send + Sync {
    #[cfg(feature = "duckdb")]
    fn executor(&self) -> Arc<std::sync::Mutex<crate::ir::rel::sql::DuckDbExecutor>>;
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
    fn prefetch(&self, values: &[Value]);
    fn stats(&self) -> (usize, Vec<String>);
    fn check(&self) -> Result<(), String>;
}
impl PropertyGraph {
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
                *id = ElementId::new(id.scalar().cast_to(kind).map_err(|e| e.to_string())?)?;
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
            let values = rows
                .iter()
                .flat_map(|r| r.bindings.values().cloned())
                .collect::<Vec<_>>();
            source.prefetch(&values);
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
