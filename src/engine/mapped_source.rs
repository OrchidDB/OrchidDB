//! Transaction-local, demand-driven access for native kernels. No eager data
//! scans: normal relational work stays in DuckDB islands.
use super::mapped_storage::{self, *};
use crate::ir::catalog::{
    PropertyGraph,
    source::{Endpoints, GraphSource, Neighbor},
};
use crate::ir::rel::sql::DuckDbExecutor;
use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

fn lookup_filters(columns: &KeyColumns, ids: &[ElementId]) -> Vec<datafusion::logical_expr::Expr> {
    if columns.len() != 1 { return vec![]; }
    vec![datafusion::logical_expr::Expr::Column(datafusion::common::Column::from_name(&columns.columns()[0]))
        .in_list(ids.iter().map(|id| datafusion::logical_expr::Expr::Literal(id.scalar().clone(), None)).collect(), false)]
}

type Address = (bool, String, ElementId);
#[derive(Clone, Debug)]
struct Record {
    properties: BTreeMap<String, Value>,
    endpoints: Option<Endpoints>,
}
#[derive(Default, Debug)]
struct Cache {
    records: HashMap<Address, Option<Record>>,
    property_handles: HashMap<(String, ElementId, String), i64>,
    neighbors: HashMap<(bool, String, ElementId, String), Vec<Neighbor>>,
    error: Option<String>,
    rows: usize,
    queries: Vec<String>,
}
pub(super) struct Source {
    executor: Arc<Mutex<DuckDbExecutor>>,
    mapping: Arc<GraphMapping>,
    operators: Arc<dyn crate::ir::functions::OperatorTable>,
    cache: Mutex<Cache>,
    key_types: HashMap<(bool, String), arrow::datatypes::DataType>,
}
impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MappedSqlSource")
    }
}
impl Source {
    fn attempt<T: Default>(&self, op: impl FnOnce() -> Result<T, String>) -> T {
        match op() {
            Ok(v) => v,
            Err(e) => {
                self.cache.lock().unwrap().error.get_or_insert(e);
                T::default()
            }
        }
    }
    fn fetch(
        &self,
        edge: bool,
        name: &str,
        column: Option<&KeyColumns>,
        ids: &[ElementId],
    ) -> Result<Vec<ElementId>, String> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let (src, key, props, endpoints) = if edge {
            let Some(m) = self.mapping.edge(name) else {
                return Ok(vec![]);
            };
            (
                &m.source,
                m.id_column.as_ref().unwrap_or(&m.src_column),
                &m.properties,
                Some(m),
            )
        } else {
            let Some(m) = self.mapping.node(name) else {
                return Ok(vec![]);
            };
            (&m.source, &m.id_column, &m.properties, None)
        };
        let mut projection = vec![format!("{} AS __key", key.sql(None))];
        projection.extend(property_projection(props));
        if let Some(m) = endpoints {
            projection.extend([
                format!("{} AS __src", m.src_column.sql(None)),
                format!("{} AS __dst", m.dst_column.sql(None)),
            ]);
        }
        let array = ScalarValue::iter_to_array(ids.iter().map(|id| id.scalar().clone()))
            .map_err(|e| e.to_string())?;
        let input = RecordBatch::try_new(
            Arc::new(Schema::new(vec![Field::new(
                "key",
                array.data_type().clone(),
                false,
            )])),
            vec![array],
        )
        .map_err(|e| e.to_string())?;
        let edge_filter = endpoints.and_then(|m| m.foreign_key_columns())
            .map(|(_, _, _, fk)| format!(" AND {}", fk.present_sql()))
            .unwrap_or_default();
        let sql = format!(
            "SELECT {} FROM {} WHERE {} IN (SELECT key FROM __orchiddb_write_values(?, ?)){edge_filter}",
            projection.join(","),
            resolved_source(&self.mapping, src, &lookup_filters(column.unwrap_or(key), ids))?,
            column.unwrap_or(key).sql(None)
        );
        let batch = {
            let mut executor = self.executor.lock().map_err(|e| e.to_string())?;
            let connection = executor.connection().map_err(|e| e.to_string())?;
            let mut stmt = connection.prepare(&sql).map_err(|e| e.to_string())?;
            let reader = stmt
                .query_arrow(arrow_recordbatch_to_query_params(input))
                .map_err(|e| e.to_string())?;
            let schema = reader.get_schema();
            arrow::compute::concat_batches(&schema, &reader.collect::<Vec<_>>())
                .map_err(|e| e.to_string())?
        };
        let keys = keys(&batch, 0)?;
        if keys.iter().collect::<BTreeSet<_>>().len() != keys.len() {
            return Err(format!("duplicate primary key in mapping `{name}`"));
        }
        let mut cache = self.cache.lock().unwrap();
        cache.rows += batch.num_rows();
        cache.queries.push(sql);
        if column.is_none() {
            for id in ids {
                cache.records.insert((edge, name.into(), id.clone()), None);
            }
        }
        for (row, id) in keys.iter().enumerate() {
            let properties = props
                .keys()
                .enumerate()
                .map(|(i, p)| {
                    (
                        p.clone(),
                        crate::ir::catalog::array_value(batch.column(i + 1).as_ref(), row, None),
                    )
                })
                .collect();
            let endpoints = if let Some(m) = endpoints {
                let index = props.len() + 1;
                Some((
                    m.src_label.clone(),
                    ElementId::new(ScalarValue::try_from_array(batch.column(index), row).map_err(|e| e.to_string())?)?
                        .cast_to(&self.key_types[&(false, m.src_label.clone())])?,
                    m.dst_label.clone(),
                    ElementId::new(ScalarValue::try_from_array(batch.column(index + 1), row).map_err(|e| e.to_string())?)?
                        .cast_to(&self.key_types[&(false, m.dst_label.clone())])?,
                ))
            } else {
                None
            };
            cache.records.insert(
                (edge, name.into(), id.clone()),
                Some(Record {
                    properties,
                    endpoints,
                }),
            );
        }
        Ok(keys)
    }
    fn records(&self, edge: bool, name: &str, ids: &[ElementId]) -> Result<(), String> {
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
        // Bound parameter batches; typed keys never become generated SQL text.
        for chunk in missing.into_iter().collect::<Vec<_>>().chunks(1024) {
            self.fetch(edge, name, None, chunk)?;
        }
        Ok(())
    }
    fn record(&self, edge: bool, name: &str, id: &ElementId) -> Option<Record> {
        self.attempt(|| self.records(edge, name, std::slice::from_ref(id)));
        self.cache
            .lock()
            .unwrap()
            .records
            .get(&(edge, name.into(), id.clone()))
            .cloned()
            .flatten()
    }
}
impl GraphSource for Source {
    fn property_handle(&self, name: &str, id: &ElementId, key: &str) -> i64 {
        // These are invocation-local property handles, not element keys. Share
        // lazy allocation across transaction views and keep it disjoint from
        // overlay-created property handles (which are nonnegative).
        let mut cache = self.cache.lock().unwrap();
        let next = -(cache.property_handles.len() as i64) - 1;
        *cache.property_handles.entry((name.into(), id.clone(), key.into())).or_insert(next)
    }
    fn executor(&self) -> Arc<Mutex<DuckDbExecutor>> {
        self.executor.clone()
    }
    fn function(&self, name: &str, args: &[Value]) -> Option<Result<Value, String>> {
        if self.operators.overloads(name).is_empty() {
            return None;
        }
        Some((|| {
            use crate::ir::{
                expr::{IrExpr, Lit},
                plan::{GraphPlan, Node, ProjectErrorPolicy, ProjectMode, ProjectionItem},
                policy::GraphPlanPolicy,
            };
            let args = args
                .iter()
                .map(|v| value_scalar(v).map(|s| IrExpr::Lit(Lit::Scalar(s))))
                .collect::<Result<Vec<_>, _>>()?;
            let plan = GraphPlan::new(
                GraphPlanPolicy::cypher(),
                Node::GraphProject {
                    input: Box::new(Node::GraphOneRow),
                    mode: ProjectMode::PreserveVisible,
                    error_policy: ProjectErrorPolicy::PropagateError,
                    items: vec![ProjectionItem {
                        alias: "value".into(),
                        expr: IrExpr::Call {
                            name: name.into(),
                            args,
                        },
                    }],
                },
            );
            let prepared =
                crate::ir::functions::with_operator_table(self.operators.clone(), || {
                    let lowered = crate::ir::rel::RelBackend::new()
                        .lower(&plan, &PropertyGraph::new())
                        .map_err(|e| e.to_string())?;
                    tokio::runtime::Handle::current()
                        .block_on(crate::ir::rel::sql::prepare(
                            &lowered,
                            crate::ir::rel::sql::SqlDialect::DuckDb,
                        ))
                        .map_err(|e| e.to_string())
                })?;
            let batch = self
                .executor
                .lock()
                .map_err(|e| e.to_string())?
                .execute_prepared_arrow(&prepared)
                .map_err(|e| e.to_string())?;
            {
                let mut cache = self.cache.lock().unwrap();
                cache.queries.push(prepared.query);
                cache.rows += batch.num_rows();
            }
            Ok(crate::ir::catalog::array_value(
                batch.column_by_name("value").ok_or("missing SQL scalar result")?.as_ref(),
                0,
                None,
            ))
        })())
    }
    fn property(&self, edge: bool, name: &str, id: &ElementId, key: &str) -> Value {
        self.record(edge, name, id)
            .and_then(|r| r.properties.get(key).cloned())
            .unwrap_or(Value::Null)
    }
    fn exists(&self, edge: bool, name: &str, id: &ElementId) -> bool {
        self.record(edge, name, id).is_some()
    }
    fn endpoints(&self, name: &str, id: &ElementId) -> Option<Endpoints> {
        self.record(true, name, id).and_then(|r| r.endpoints)
    }
    fn ids(&self, edge: bool, name: &str) -> Vec<ElementId> {
        self.attempt(|| {
            let (src, key) = if edge {
                let Some(m) = self.mapping.edge(name) else {
                    return Ok(vec![]);
                };
                (&m.source, m.id_column.as_ref().unwrap_or(&m.src_column))
            } else {
                let Some(m) = self.mapping.node(name) else {
                    return Ok(vec![]);
                };
                (&m.source, &m.id_column)
            };
            let mut executor = self.executor.lock().map_err(|e| e.to_string())?;
            let edge_filter = edge.then(|| self.mapping.edge(name)).flatten()
                .and_then(|m| m.foreign_key_columns())
                .map(|(_, _, _, fk)| format!(" WHERE {}", fk.present_sql()))
                .unwrap_or_default();
            let sql = format!("SELECT {} FROM {}{edge_filter}", key.sql(None), resolved_source(&self.mapping, src, &[])?);
            let ids = keys(
                &query(executor.connection().map_err(|e| e.to_string())?, &sql)?,
                0,
            )?;
            {
                let mut cache = self.cache.lock().unwrap();
                cache.rows += ids.len();
                cache.queries.push(sql);
            }
            drop(executor);
            if edge {
                self.records(true, name, &ids)?;
            }
            Ok(ids)
        })
    }
    fn prefetch_neighbors(&self, incoming: bool, nodes: &[(String, ElementId)], types: &[String]) {
        self.attempt(|| {
            for rel in self.mapping.rel_types() {
                if !types.is_empty() && !types.contains(&rel) {
                    continue;
                }
                let m = self.mapping.edge(&rel).unwrap();
                let label = if incoming { &m.dst_label } else { &m.src_label };
                let missing = {
                    let cache = self.cache.lock().unwrap();
                    nodes
                        .iter()
                        .filter(|(name, id)| {
                            name == label
                                && !cache.neighbors.contains_key(&(
                                    incoming,
                                    name.clone(),
                                    id.clone(),
                                    rel.clone(),
                                ))
                        })
                        .map(|(_, id)| id.clone())
                        .collect::<BTreeSet<_>>()
                };
                for chunk in missing.into_iter().collect::<Vec<_>>().chunks(1024) {
                    let keys = self.fetch(
                        true,
                        &rel,
                        Some(if incoming {
                            &m.dst_column
                        } else {
                            &m.src_column
                        }),
                        chunk,
                    )?;
                    let mut grouped = chunk
                        .iter()
                        .map(|id| (id.clone(), vec![]))
                        .collect::<BTreeMap<_, Vec<Neighbor>>>();
                    let mut cache = self.cache.lock().unwrap();
                    for key in keys {
                        let Some(r) = cache
                            .records
                            .get(&(true, rel.clone(), key.clone()))
                            .and_then(Option::as_ref)
                        else {
                            continue;
                        };
                        let Some((sl, si, dl, di)) = r.endpoints.clone() else {
                            continue;
                        };
                        let (owner, other_label, other_id) =
                            if incoming { (di, sl, si) } else { (si, dl, di) };
                        grouped.entry(owner).or_default().push((
                            rel.clone(),
                            key,
                            other_label,
                            other_id,
                        ));
                    }
                    for (id, neighbors) in grouped {
                        cache
                            .neighbors
                            .insert((incoming, label.clone(), id, rel.clone()), neighbors);
                    }
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
        let cache = self.cache.lock().unwrap();
        self.mapping
            .rel_types()
            .iter()
            .filter(|rel| types.is_empty() || types.contains(rel))
            .flat_map(|rel| {
                cache
                    .neighbors
                    .get(&(incoming, name.into(), id.clone(), rel.clone()))
                    .into_iter()
                    .flatten()
                    .cloned()
            })
            .collect()
    }
    fn prefetch(&self, values: &[Value]) {
        fn collect(v: &Value, groups: &mut BTreeMap<(bool, String), BTreeSet<ElementId>>) {
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
                Value::List(v) | Value::Path(v) => {
                    for v in v {
                        collect(v, groups);
                    }
                }
                Value::Map(m) => {
                    for v in m.values() {
                        collect(v, groups);
                    }
                }
                _ => {}
            }
        }
        let mut groups = BTreeMap::new();
        for v in values {
            collect(v, &mut groups);
        }
        for ((edge, name), ids) in groups {
            self.attempt(|| self.records(edge, &name, &ids.into_iter().collect::<Vec<_>>()));
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
pub(super) fn attach(
    executor: Arc<Mutex<DuckDbExecutor>>,
    mapping: Arc<GraphMapping>,
) -> Result<PropertyGraph, String> {
    let mut graph;
    let mut resolved = (*mapping).clone();
    // Runtime statements may write or observe a new snapshot. Only persistent
    // enforcement contracts survive here; snapshot proofs belong to caller-owned
    // immutable compile/execution scopes.
    resolved.set_constraint_scope(None);
    {
        let mut guard = executor.lock().map_err(|e| e.to_string())?;
        let connection = guard.connection().map_err(|e| e.to_string())?;
        mapped_storage::register(connection)?;
        // Bind schemas without reading source rows. Query mappings use their
        // explicitly registered dependencies.
        for name in mapping.labels() {
            if let MappedSource::Table(t) = &mapping.node(&name).unwrap().source {
                if mapping.logical_source(t).is_some() || mapping.collection_source(t).is_some() || mapping.representation_source(t).is_some() { continue; }
                resolved.register_table_schema(
                    t,
                    query(
                        connection,
                        &format!("SELECT * FROM {} WHERE false", table(t)),
                    )?
                    .schema(),
                );
            }
        }
        for name in mapping.rel_types() {
            if let MappedSource::Table(t) = &mapping.edge(&name).unwrap().source {
                if mapping.logical_source(t).is_some() || mapping.collection_source(t).is_some() || mapping.representation_source(t).is_some() { continue; }
                resolved.register_table_schema(
                    t,
                    query(
                        connection,
                        &format!("SELECT * FROM {} WHERE false", table(t)),
                    )?
                    .schema(),
                );
            }
        }
        graph = mapped_storage::metadata(connection, Arc::new(resolved))?;
    }
    graph.source = Some(Arc::new(Source {
        executor,
        operators: crate::ir::functions::selected_operator_table().map_err(|e| e.to_string())?,
        mapping: graph.mapping.clone().unwrap(),
        cache: Default::default(),
        key_types: graph.key_types.clone(),
    }));
    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jvm_views_allocate_property_handles_lazily_and_consistently() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people(id VARCHAR PRIMARY KEY, name VARCHAR); INSERT INTO people VALUES ('a','Alice'),('b','Bob'); CREATE VIEW unrelated AS SELECT error('unexpected scan')::VARCHAR AS id FROM range(1)").unwrap();
        let mut mapping = GraphMapping::new();
        mapping.map_node(crate::ir::rel::mapping::NodeMapping::table("Person", "people", "id").property("name", "name"));
        mapping.map_node(crate::ir::rel::mapping::NodeMapping::table("Unrelated", "unrelated", "id"));
        let graph = attach(Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))), Arc::new(mapping)).unwrap();
        let source = graph.source.clone().unwrap();
        let mut store = crate::jvm_bridge::Store::from_execution_graph(graph);
        assert_eq!(source.stats().0, 0, "JVM construction must not fetch graph rows");
        let a = Value::Node {label:"Person".into(),id:ElementId::new(ScalarValue::Utf8(Some("a".into()))).unwrap()};
        let b = Value::Node {label:"Person".into(),id:ElementId::new(ScalarValue::Utf8(Some("b".into()))).unwrap()};
        let checkpoint = store.graph().clone();
        let writer_b = store.graph().properties(&b, &["name".into()]);
        let reader_a = checkpoint.properties(&a, &["name".into()]);
        let writer_a = store.graph().properties(&a, &["name".into()]);
        assert_eq!(reader_a, writer_a);
        assert_eq!(writer_b, checkpoint.properties(&b, &["name".into()]));
        // Committing a writer that allocated in a different order must retain
        // the same identities seen by readers of the prior checkpoint.
        let response = store.request(&serde_json::json!({"op":"commit"}));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(writer_a, store.graph().properties(&a, &["name".into()]));
        source.check().unwrap();
        assert_eq!(source.stats().0, 2);
        assert!(source.stats().1.iter().all(|sql| !sql.contains("unrelated")));
    }
}
