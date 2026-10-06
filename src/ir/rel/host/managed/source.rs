//! Selective SQL access to managed records; decoding reuses the catalog codec.
use super::*;
use crate::ir::catalog::source::{Endpoints, GraphSource, NativeState, Neighbor};
use std::{
    collections::{BTreeSet, HashMap},
    sync::Mutex,
};
type Address = (bool, String, ElementId);
#[derive(Debug, Default)]
struct Cache {
    records: HashMap<Address, Option<PropertyGraph>>,
    error: Option<String>,
    rows: usize,
    queries: Vec<String>,
    handles: HashMap<(String, ElementId, String), i64>,
    neighbors: HashMap<(bool, String, ElementId, Vec<String>), Vec<Neighbor>>,
}
#[derive(Debug)]
pub(super) struct Source {
    store: ManagedStore,
    host: SharedHost,
    cache: Mutex<Cache>,
}
impl Source {
    pub fn new(store: ManagedStore, host: SharedHost) -> Self {
        Self {
            store,
            host,
            cache: Default::default(),
        }
    }
    fn attempt<T: Default>(&self, f: impl FnOnce() -> Result<T, String>) -> T {
        match f() {
            Ok(v) => v,
            Err(e) => {
                self.cache.lock().unwrap().error.get_or_insert(e);
                T::default()
            }
        }
    }
    fn query(&self, request: HostRequest) -> Result<RecordBatch, String> {
        let sql = request.sql.clone();
        let batch = self.host.query(request)?;
        let mut cache = self.cache.lock().unwrap();
        cache.rows += batch.num_rows();
        cache.queries.push(sql);
        Ok(batch)
    }
    fn fetch(&self, edge: bool, name: &str, ids: &[ElementId]) -> Result<(), String> {
        let missing = {
            let cache = self.cache.lock().unwrap();
            ids.iter()
                .filter(|id| {
                    !cache
                        .records
                        .contains_key(&(edge, name.into(), (*id).clone()))
                })
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        for chunk in missing.into_iter().collect::<Vec<_>>().chunks(1024) {
            let array = ScalarValue::iter_to_array(chunk.iter().map(|id| id.scalar().clone()))
                .map_err(|e| e.to_string())?;
            let input = RecordBatch::try_new(
                Arc::new(Schema::new(vec![Field::new(
                    "id",
                    array.data_type().clone(),
                    false,
                )])),
                vec![array],
            )
            .map_err(|e| e.to_string())?;
            let request=HostRequest::new(format!("SELECT kind,name,id,payload FROM {} WHERE kind=$1 AND name=$2 AND live AND id IN (SELECT id FROM __orchiddb_managed_keys)",self.store.sql_table())).parameters(vec![ScalarValue::Int32(Some(if edge{2}else{1})),ScalarValue::Utf8(Some(name.into()))]).relation("__orchiddb_managed_keys",input);
            let batch = self.query(request)?;
            let records = records(&batch)?;
            let mut cache = self.cache.lock().unwrap();
            for id in chunk {
                cache.records.insert((edge, name.into(), id.clone()), None);
            }
            for record in records {
                let id = record.id.clone();
                let mut graph = PropertyGraph::new();
                graph.apply_incremental_records(&[record])?;
                cache.records.insert((edge, name.into(), id), Some(graph));
            }
        }
        Ok(())
    }
    fn record(&self, edge: bool, name: &str, id: &ElementId) -> Option<PropertyGraph> {
        self.attempt(|| {
            self.fetch(edge, name, std::slice::from_ref(id))?;
            Ok(self
                .cache
                .lock()
                .unwrap()
                .records
                .get(&(edge, name.into(), id.clone()))
                .cloned()
                .flatten())
        })
    }
}
impl GraphSource for Source {
    #[cfg(feature = "duckdb")]
    fn executor(&self) -> Option<Arc<Mutex<crate::ir::rel::sql::DuckDbExecutor>>> {
        self.host.legacy_executor()
    }
    fn supports_dynamic_schema(&self) -> bool {
        true
    }
    fn public_addresses(&self, value: &Value, edge: bool) -> Vec<(bool, String, ElementId)> {
        self.attempt(|| {
            let batch = self.query(
                HostRequest::new(format!(
                    "SELECT name,id FROM {} WHERE kind=$1 AND live AND public_key=$2",
                    self.store.sql_table()
                ))
                .parameters(vec![
                    ScalarValue::Int32(Some(if edge { 2 } else { 1 })),
                    ScalarValue::Utf8(Some(crate::ir::catalog::properties::public_id_key(value))),
                ]),
            )?;
            (0..batch.num_rows())
                .map(|row| {
                    Ok((
                        edge,
                        text(&batch, 0, row)?,
                        ElementId::new(
                            ScalarValue::try_from_array(batch.column(1), row)
                                .map_err(|e| e.to_string())?,
                        )?,
                    ))
                })
                .collect()
        })
    }
    fn native_state(&self, edge: bool, name: &str, id: &ElementId) -> Option<NativeState> {
        self.record(edge, name, id)
            .map(|g| g.native_state(edge, name, id))
    }
    fn invalidate(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.records.clear();
        cache.neighbors.clear();
    }
    fn function(&self, name: &str, args: &[Value]) -> Option<Result<Value, String>> {
        let catalog=crate::ir::functions::selected_operator_table().ok()?;
        if catalog.overloads(name).is_empty() { return None; }
        Some(crate::ir::functions::host_execution::value(self.host.as_ref(), &catalog.target_name(name), args))
    }
    fn property_handle(&self, name: &str, id: &ElementId, key: &str) -> i64 {
        let mut cache = self.cache.lock().unwrap();
        let next = -(cache.handles.len() as i64) - 1;
        *cache
            .handles
            .entry((name.into(), id.clone(), key.into()))
            .or_insert(next)
    }
    fn property(&self, edge: bool, name: &str, id: &ElementId, key: &str) -> Value {
        self.record(edge, name, id)
            .map(|g| {
                if edge {
                    g.edge_property(name, id.clone(), key)
                } else {
                    g.node_property(name, id.clone(), key)
                }
            })
            .unwrap_or(Value::Null)
    }
    fn ids(&self, edge: bool, name: &str) -> Vec<ElementId> {
        self.attempt(|| {
            let batch = self.query(
                HostRequest::new(format!(
                    "SELECT id FROM {} WHERE kind=$1 AND name=$2 AND live ORDER BY id",
                    self.store.sql_table()
                ))
                .parameters(vec![
                    ScalarValue::Int32(Some(if edge { 2 } else { 1 })),
                    ScalarValue::Utf8(Some(name.into())),
                ]),
            )?;
            crate::ir::rel::host::mapped_storage::keys(&batch, 0)
        })
    }
    fn endpoints(&self, name: &str, id: &ElementId) -> Option<Endpoints> {
        self.record(true, name, id)
            .and_then(|g| g.edge_endpoints(name, id.clone()))
    }
    fn exists(&self, edge: bool, name: &str, id: &ElementId) -> bool {
        self.record(edge, name, id).is_some()
    }
    fn prefetch_neighbors(&self, incoming: bool, nodes: &[(String, ElementId)], types: &[String]) {
        self.attempt(||{
            let mut types=types.to_vec();types.sort();types.dedup();
            let missing={let cache=self.cache.lock().unwrap();nodes.iter().filter(|(name,id)|!cache.neighbors.contains_key(&(incoming,name.clone(),id.clone(),types.clone()))).cloned().collect::<BTreeSet<_>>()};
            for chunk in missing.into_iter().collect::<Vec<_>>().chunks(1024){
                let arrays=vec![ScalarValue::iter_to_array(chunk.iter().map(|(name,_)|ScalarValue::Utf8(Some(name.clone())))).map_err(|e|e.to_string())?,ScalarValue::iter_to_array(chunk.iter().map(|(_,id)|id.scalar().clone())).map_err(|e|e.to_string())?];
                let input=RecordBatch::try_new(Arc::new(Schema::new(vec![Field::new("name",DataType::Utf8,false),Field::new("id",DataType::Int64,false)])),arrays).map_err(|e|e.to_string())?;
                let (owner,other)=if incoming{("dst","src")}else{("src","dst")};
                let parameters=types.iter().map(|name|ScalarValue::Utf8(Some(name.clone()))).collect::<Vec<_>>();
                let filter=if types.is_empty(){String::new()}else{format!(" AND edge.name IN ({})",(1..=types.len()).map(|i|format!("${i}")).collect::<Vec<_>>().join(","))};
                let request=HostRequest::new(format!("SELECT keys.name,keys.id,edge.name,edge.id,edge.{other}_name,edge.{other}_id FROM __orchiddb_managed_frontier AS keys JOIN {} AS edge ON edge.{owner}_name=keys.name AND edge.{owner}_id=keys.id WHERE edge.kind=2 AND edge.live{filter} ORDER BY edge.name,edge.id",self.store.sql_table())).parameters(parameters).relation("__orchiddb_managed_frontier",input);
                let batch=self.query(request)?;let mut cache=self.cache.lock().unwrap();
                for (name,id) in chunk{cache.neighbors.insert((incoming,name.clone(),id.clone(),types.clone()),vec![]);}
                for row in 0..batch.num_rows(){
                    let id=|column|ElementId::new(ScalarValue::try_from_array(batch.column(column),row).map_err(|e|e.to_string())?);
                    cache.neighbors.entry((incoming,text(&batch,0,row)?,id(1)?,types.clone())).or_default().push((text(&batch,2,row)?,id(3)?,text(&batch,4,row)?,id(5)?));
                }
            }
            Ok(())
        });
    }
    fn neighbors(
        &self,
        incoming: bool,
        name: &str,
        id: &ElementId,
        types: &[String],
    ) -> Vec<Neighbor> {
        self.prefetch_neighbors(incoming, &[(name.into(), id.clone())], types);
        let mut types = types.to_vec();
        types.sort();
        types.dedup();
        self.cache
            .lock()
            .unwrap()
            .neighbors
            .get(&(incoming, name.into(), id.clone(), types))
            .cloned()
            .unwrap_or_default()
    }
    fn prefetch(&self, values: &mut dyn Iterator<Item = &Value>) {
        let mut groups = BTreeMap::new();
        for value in values {
            crate::ir::catalog::source::collect_element_addresses(value, &mut groups);
        }
        for ((edge, name), ids) in groups {
            self.attempt(|| self.fetch(edge, &name, &ids.into_iter().collect::<Vec<_>>()));
        }
    }
    fn stats(&self) -> (usize, Vec<String>) {
        let cache = self.cache.lock().unwrap();
        (cache.rows, cache.queries.clone())
    }
    fn check(&self) -> Result<(), String> {
        self.cache.lock().unwrap().error.clone().map_or(Ok(()), Err)
    }
}
