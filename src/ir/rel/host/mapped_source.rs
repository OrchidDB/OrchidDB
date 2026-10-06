//! Transaction-local, demand-driven access for native kernels. No eager data
//! scans: normal relational work stays in DuckDB islands.
use super::mapped_storage::{self, *};
use super::{HostRelational, HostRequest};
use crate::ir::catalog::{
    PropertyGraph,
    source::{Endpoints, GraphSource, Neighbor},
};
#[cfg(all(test, feature = "duckdb"))]
use crate::ir::rel::sql::DuckDbExecutor;
use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

fn lookup_filters(columns: &KeyColumns, ids: &[ElementId]) -> Vec<datafusion::logical_expr::Expr> {
    if columns.len() == 1 {
        return vec![
            datafusion::logical_expr::Expr::Column(datafusion::common::Column::from_name(
                &columns.columns()[0],
            ))
            .in_list(
                ids.iter()
                    .map(|id| datafusion::logical_expr::Expr::Literal(id.scalar().clone(), None))
                    .collect(),
                false,
            ),
        ];
    }
    let mut tuples = Vec::new();
    for id in ids {
        let components = id.components();
        if components.len() != columns.len() {
            continue;
        }
        if let Some(e) = columns
            .columns()
            .iter()
            .zip(components)
            .map(|(name, value)| {
                datafusion::logical_expr::Expr::Column(datafusion::common::Column::from_name(name))
                    .eq(datafusion::logical_expr::Expr::Literal(value, None))
            })
            .reduce(|a, b| a.and(b))
        {
            tuples.push(e);
        }
    }
    // Balance tuple predicates so large frontiers do not create deep expression trees.
    while tuples.len() > 1 {
        tuples = tuples
            .chunks(2)
            .map(|pair| {
                if pair.len() == 2 {
                    pair[0].clone().or(pair[1].clone())
                } else {
                    pair[0].clone()
                }
            })
            .collect();
    }
    tuples
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
    complete_sources: BTreeSet<(bool, String)>,
    attempted_scans: BTreeSet<(bool, String)>,
    source_order: HashMap<(bool, String), Vec<ElementId>>,
    access_decisions: Vec<crate::ir::rel::statistics::OptimizerDecision>,
}
pub(super) struct Source {
    host: Arc<dyn HostRelational + Send + Sync>,
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
    fn cached_keys(
        &self,
        edge: bool,
        name: &str,
        column: Option<&KeyColumns>,
        ids: &[ElementId],
    ) -> Option<Vec<ElementId>> {
        let cache = self.cache.lock().unwrap();
        if !cache.complete_sources.contains(&(edge, name.to_string())) {
            return None;
        }
        let wanted = ids.iter().collect::<BTreeSet<_>>();
        Some(
            cache
                .source_order
                .get(&(edge, name.to_string()))?
                .iter()
                .filter_map(|id| {
                    let record = cache
                        .records
                        .get(&(edge, name.to_string(), id.clone()))?
                        .as_ref()?;
                    let owner = if let Some(column) = column {
                        let mapping = self.mapping.edge(name)?;
                        let (_, src, _, dst) = record.endpoints.as_ref()?;
                        if column.columns() == mapping.src_column.columns() {
                            src
                        } else {
                            dst
                        }
                    } else {
                        id
                    };
                    wanted.contains(owner).then(|| id.clone())
                })
                .collect(),
        )
    }
    fn scan_cost(
        &self,
        edge: bool,
        name: &str,
        column: Option<&KeyColumns>,
        count: usize,
    ) -> Option<(f64, f64)> {
        let snapshot = self.mapping.statistics()?;
        let (source, key) = if edge {
            let m = self.mapping.edge(name)?;
            (
                &m.source,
                column.unwrap_or(m.id_column.as_ref().unwrap_or(&m.src_column)),
            )
        } else {
            let m = self.mapping.node(name)?;
            (&m.source, &m.id_column)
        };
        let direct = if let MappedSource::Table(table) = source {
            snapshot.sources.get(table).and_then(|stats| {
                let ndv = if key.len() == 1 {
                    stats
                        .columns
                        .get(&key.columns()[0])
                        .map(|c| c.estimated_distinct.unwrap_or(c.sample_distinct as f64))
                } else {
                    stats
                        .groups
                        .iter()
                        .find(|g| g.columns == key.columns())
                        .map(|g| g.sample_distinct as f64)
                }?;
                let rows = stats.estimated_rows?;
                let bytes = stats.estimated_bytes?;
                Some((rows, bytes, ndv, bytes + 8.0 * rows))
            })
        } else {
            None
        };
        let (rows, bytes, ndv, work) =
            direct.or_else(|| self.mapping.source_access_cost(source, key))?;
        if rows > 65536.0 || bytes > 16.0 * 1024.0 * 1024.0 || rows <= 0.0 {
            return None;
        }
        let fraction = (count as f64 / ndv.max(1.0)).min(1.0);
        let lookup = (count.div_ceil(1024) as f64) * work + fraction * bytes;
        let scan = bytes + work;
        (scan <= lookup && (count > 1024 || fraction >= 0.75)).then_some((lookup, scan))
    }
    fn fetch(
        &self,
        edge: bool,
        name: &str,
        column: Option<&KeyColumns>,
        ids: &[ElementId],
    ) -> Result<Vec<ElementId>, String> {
        if let Some(keys) = self.cached_keys(edge, name, column, ids) {
            return Ok(keys);
        }
        self.fetch_read(edge, name, column, ids, false)
    }
    fn maybe_scan(
        &self,
        edge: bool,
        name: &str,
        column: Option<&KeyColumns>,
        ids: &[ElementId],
    ) -> Result<bool, String> {
        if self.cached_keys(edge, name, column, ids).is_some() {
            return Ok(true);
        }
        if self
            .cache
            .lock()
            .unwrap()
            .attempted_scans
            .contains(&(edge, name.into()))
        {
            return Ok(false);
        }
        let Some((lookup, scan)) = self.scan_cost(edge, name, column, ids.len()) else {
            return Ok(false);
        };
        self.cache
            .lock()
            .unwrap()
            .attempted_scans
            .insert((edge, name.into()));
        self.fetch_read(edge, name, column, ids, true)?;
        let complete = self
            .cache
            .lock()
            .unwrap()
            .complete_sources
            .contains(&(edge, name.into()));
        self.cache.lock().unwrap().access_decisions.push(crate::ir::rel::statistics::OptimizerDecision{
            optimization:"native_frontier_access".into(),before:vec!["batched_lookup".into()],after:vec![if complete{"bounded_scan_cache"}else{"batched_lookup_after_scan_cap"}.into()],estimated_work_before:lookup,estimated_work_after:scan,
            reason:format!("source {name}; {} distinct frontier tuples; full-source scan model without assuming an index; actual scan capped at 65536 rows/16 MiB; statement-local cache",ids.len())});
        Ok(complete)
    }
    fn fetch_read(
        &self,
        edge: bool,
        name: &str,
        column: Option<&KeyColumns>,
        ids: &[ElementId],
        scan: bool,
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
        let edge_filter = endpoints
            .and_then(|m| m.foreign_key_columns())
            .map(|(_, _, _, fk)| format!(" AND {}", fk.present_sql()))
            .unwrap_or_default();
        let (batch, sql) = if matches!(src, MappedSource::Computed(_)) {
            use crate::ir::rel::mapping::id_expr;
            use datafusion::logical_expr::{Expr, LogicalPlanBuilder};
            let source = self.mapping.source_plan(src).map_err(|e| e.to_string())?;
            let mut selected = vec![
                id_expr(&source, key, name)
                    .map_err(|e| e.to_string())?
                    .alias("__key"),
            ];
            selected.extend(
                props
                    .iter()
                    .map(|(p, c)| Expr::Column(datafusion::common::Column::from_name(c)).alias(p)),
            );
            if let Some(edge) = endpoints {
                selected.push(
                    id_expr(&source, &edge.src_column, name)
                        .map_err(|e| e.to_string())?
                        .alias("__src"),
                );
                selected.push(
                    id_expr(&source, &edge.dst_column, name)
                        .map_err(|e| e.to_string())?
                        .alias("__dst"),
                );
            }
            let mut plan = LogicalPlanBuilder::from(source);
            if !scan {
                for filter in lookup_filters(column.unwrap_or(key), ids) {
                    plan = plan.filter(filter).map_err(|e| e.to_string())?;
                }
            }
            plan = plan.project(selected).map_err(|e| e.to_string())?;
            if scan {
                plan = plan.limit(0, Some(65537)).map_err(|e| e.to_string())?;
            }
            let (batch, queries) = self
                .host
                .execute_plan(&self.mapping, plan.build().map_err(|e| e.to_string())?)?;
            if scan
                && (batch.num_rows() > 65536 || batch.get_array_memory_size() > 16 * 1024 * 1024)
            {
                return Ok(vec![]);
            }
            (batch, queries.join(";\n"))
        } else {
            let sql = if scan {
                format!(
                    "SELECT {} FROM {} WHERE true{edge_filter} LIMIT 65537",
                    projection.join(","),
                    resolved_source(&self.mapping, src, &[])?
                )
            } else {
                format!(
                    "SELECT {} FROM {} WHERE {} IN (SELECT key FROM __orchiddb_write_values){edge_filter}",
                    projection.join(","),
                    resolved_source(
                        &self.mapping,
                        src,
                        &lookup_filters(column.unwrap_or(key), ids)
                    )?,
                    column.unwrap_or(key).sql(None)
                )
            };
            let request = if scan {
                HostRequest::new(&sql)
            } else {
                HostRequest::new(&sql).relation("__orchiddb_write_values", input)
            };
            let batch = self.host.query(request)?;
            if scan
                && (batch.num_rows() > 65536 || batch.get_array_memory_size() > 16 * 1024 * 1024)
            {
                let mut cache = self.cache.lock().unwrap();
                cache.rows += batch.num_rows();
                cache.queries.push(sql.clone());
                return Ok(vec![]);
            }
            (batch, sql)
        };
        let keys = keys(&batch, 0)?;
        if keys.iter().collect::<BTreeSet<_>>().len() != keys.len() {
            return Err(format!("duplicate primary key in mapping `{name}`"));
        }
        let mut cache = self.cache.lock().unwrap();
        cache.rows += batch.num_rows();
        cache.queries.push(sql);
        if scan {
            cache.complete_sources.insert((edge, name.into()));
            cache.source_order.insert((edge, name.into()), keys.clone());
        }
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
                    ElementId::new(
                        ScalarValue::try_from_array(batch.column(index), row)
                            .map_err(|e| e.to_string())?,
                    )?
                    .cast_to(&self.key_types[&(false, m.src_label.clone())])?,
                    m.dst_label.clone(),
                    ElementId::new(
                        ScalarValue::try_from_array(batch.column(index + 1), row)
                            .map_err(|e| e.to_string())?,
                    )?
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
        let missing = missing.into_iter().collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }
        if self.maybe_scan(edge, name, None, &missing)? {
            return Ok(());
        }
        // Bound parameter batches; typed keys never become generated SQL text.
        for chunk in missing.chunks(1024) {
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
        *cache
            .property_handles
            .entry((name.into(), id.clone(), key.into()))
            .or_insert(next)
    }
    #[cfg(feature = "duckdb")]
    fn executor(&self) -> Option<Arc<Mutex<crate::ir::rel::sql::DuckDbExecutor>>> {
        self.host.legacy_executor()
    }
    fn invalidate(&self) {
        let mut cache = self.cache.lock().unwrap();
        cache.records.clear();
        cache.neighbors.clear();
        cache.complete_sources.clear();
        cache.attempted_scans.clear();
        cache.source_order.clear();
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
            let lowered =
                crate::ir::functions::with_operator_table(self.operators.clone(), || {
                    crate::ir::rel::RelBackend::new()
                        .lower(&plan, &PropertyGraph::new())
                        .map_err(|e| e.to_string())
                })?;
            let (batch, queries) = self.host.execute_plan(&self.mapping, lowered.plan)?;
            {
                let mut cache = self.cache.lock().unwrap();
                cache.queries.extend(queries);
                cache.rows += batch.num_rows();
            }
            Ok(crate::ir::catalog::array_value(
                batch
                    .column_by_name("value")
                    .ok_or("missing SQL scalar result")?
                    .as_ref(),
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
            if matches!(src, MappedSource::Computed(_)) {
                let plan = self.mapping.source_plan(src).map_err(|e| e.to_string())?;
                let projection = crate::ir::rel::mapping::id_expr(&plan, key, name)
                    .map_err(|e| e.to_string())?
                    .alias("__key");
                let plan = datafusion::logical_expr::LogicalPlanBuilder::from(plan)
                    .project(vec![projection])
                    .and_then(|b| b.build())
                    .map_err(|e| e.to_string())?;
                let (batch, queries) = self.host.execute_plan(&self.mapping, plan)?;
                let ids = keys(&batch, 0)?;
                {
                    let mut cache = self.cache.lock().unwrap();
                    cache.rows += batch.num_rows();
                    cache.queries.extend(queries);
                }
                if edge {
                    self.records(true, name, &ids)?;
                }
                return Ok(ids);
            }
            let edge_filter = edge
                .then(|| self.mapping.edge(name))
                .flatten()
                .and_then(|m| m.foreign_key_columns())
                .map(|(_, _, _, fk)| format!(" WHERE {}", fk.present_sql()))
                .unwrap_or_default();
            let sql = format!(
                "SELECT {} FROM {}{edge_filter}",
                key.sql(None),
                resolved_source(&self.mapping, src, &[])?
            );
            let ids = keys(&query(self.host.as_ref(), &sql)?, 0)?;
            {
                let mut cache = self.cache.lock().unwrap();
                cache.rows += ids.len();
                cache.queries.push(sql);
            }
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
                let missing = missing.into_iter().collect::<Vec<_>>();
                if !missing.is_empty() {
                    self.maybe_scan(
                        true,
                        &rel,
                        Some(if incoming {
                            &m.dst_column
                        } else {
                            &m.src_column
                        }),
                        &missing,
                    )?;
                }
                for chunk in missing.chunks(1024) {
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
    fn prefetch(&self, values: &mut dyn Iterator<Item = &Value>) {
        let mut groups = BTreeMap::new();
        for v in values {
            crate::ir::catalog::source::collect_element_addresses(v, &mut groups);
        }
        for ((edge, name), ids) in groups {
            self.attempt(|| self.records(edge, &name, &ids.into_iter().collect::<Vec<_>>()));
        }
    }
    fn access_decisions(&self) -> Vec<crate::ir::rel::statistics::OptimizerDecision> {
        self.cache.lock().unwrap().access_decisions.clone()
    }
    fn stats(&self) -> (usize, Vec<String>) {
        let cache = self.cache.lock().unwrap();
        (cache.rows, cache.queries.clone())
    }
    fn check(&self) -> Result<(), String> {
        self.cache.lock().unwrap().error.clone().map_or(Ok(()), Err)
    }
}
pub fn attach(
    host: Arc<dyn HostRelational + Send + Sync>,
    mapping: Arc<GraphMapping>,
) -> Result<PropertyGraph, String> {
    let mut graph;
    let mut resolved = (*mapping).clone();
    // Runtime statements may write or observe a new snapshot. Only persistent
    // enforcement contracts survive here; snapshot proofs belong to caller-owned
    // immutable compile/execution scopes.
    resolved.set_constraint_scope(None);
    {
        let connection = host.as_ref();
        // Bind schemas without reading source rows. Query mappings use their
        // explicitly registered dependencies.
        for name in mapping.labels() {
            if let MappedSource::Table(t) = &mapping.node(&name).unwrap().source {
                if mapping.logical_source(t).is_some()
                    || mapping.collection_source(t).is_some()
                    || mapping.representation_source(t).is_some()
                {
                    continue;
                }
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
                if mapping.logical_source(t).is_some()
                    || mapping.collection_source(t).is_some()
                    || mapping.representation_source(t).is_some()
                {
                    continue;
                }
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
        if let Some(snapshot) = mapping.statistics() {
            resolved
                .set_statistics(snapshot.clone())
                .map_err(|e| e.to_string())?;
        }
        graph = mapped_storage::metadata(connection, Arc::new(resolved))?;
    }
    graph.source = Some(Arc::new(Source {
        host,
        operators: crate::ir::functions::selected_operator_table().map_err(|e| e.to_string())?,
        mapping: graph.mapping.clone().unwrap(),
        cache: Default::default(),
        key_types: graph.key_types.clone(),
    }));
    Ok(graph)
}

#[cfg(all(test, feature = "duckdb"))]
mod tests {
    use super::*;
    use crate::engine::mapped_source::attach;

    #[test]
    fn borrowed_prefetch_finds_nested_elements_and_reuses_cached_records() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people(id BIGINT, name VARCHAR); INSERT INTO people VALUES (1, 'one'), (2, 'two'); CREATE TABLE links(id BIGINT, src BIGINT, dst BIGINT); INSERT INTO links VALUES (7, 1, 2)").unwrap();
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema(
            "people",
            Arc::new(Schema::new(vec![
                Field::new("id", arrow::datatypes::DataType::Int64, true),
                Field::new("name", arrow::datatypes::DataType::Utf8, true),
            ])),
        );
        mapping.register_table_schema(
            "links",
            Arc::new(Schema::new(
                ["id", "src", "dst"]
                    .map(|name| Field::new(name, arrow::datatypes::DataType::Int64, true))
                    .to_vec(),
            )),
        );
        mapping.map_node(
            crate::ir::rel::mapping::NodeMapping::table("Person", "people", "id")
                .property("name", "name"),
        );
        let mut edge = crate::ir::rel::mapping::EdgeMapping::table(
            "LINK", "links", "src", "dst", "Person", "Person",
        );
        edge.id_column = Some("id".into());
        mapping.map_edge(edge);
        let graph = attach(
            Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),
            Arc::new(mapping),
        )
        .unwrap();
        let node = |id| Value::Node {
            label: "Person".into(),
            id: ElementId::from(id),
        };
        let edge = Value::Edge {
            rel_type: "LINK".into(),
            id: 7i64.into(),
            src_label: "Person".into(),
            src_id: 1i64.into(),
            dst_label: "Person".into(),
            dst_id: 2i64.into(),
            projected_properties: None,
        };
        let rows = vec![crate::ir::runtime::Row::new().with(
            "nested",
            Value::Map(BTreeMap::from([
                (
                    "elements".into(),
                    Value::List(vec![
                        node(1i64),
                        Value::Path(vec![node(2i64), edge]),
                        node(1i64),
                    ]),
                ),
                ("payload".into(), Value::String("x".repeat(262_144))),
            ])),
        )];
        let source = graph.source.as_ref().unwrap();
        let before = source.stats().1.len();
        graph.prefetch_source(&rows);
        source.check().unwrap();
        let after = source.stats().1.len();
        assert_eq!(after - before, 2, "one batched fetch per element type");
        assert_eq!(
            source.property(false, "Person", &1i64.into(), "name"),
            Value::String("one".into())
        );
        assert_eq!(
            source.property(false, "Person", &2i64.into(), "name"),
            Value::String("two".into())
        );
        assert_eq!(
            source.endpoints("LINK", &7i64.into()),
            Some(("Person".into(), 1i64.into(), "Person".into(), 2i64.into()))
        );
        graph.prefetch_source(&rows);
        graph.prefetch_source(&[]);
        source.check().unwrap();
        assert_eq!(source.stats().1.len(), after);
    }

    #[test]
    fn statistics_choose_dense_scan_and_sparse_lookup() {
        for (statistics, count, expected_queries, derived) in [
            (false, 3000, 3, false),
            (true, 3000, 1, false),
            (true, 2, 1, false),
            (true, 3000, 1, true),
        ] {
            let db = duckdb::Connection::open_in_memory().unwrap();
            db.execute_batch("CREATE TABLE people AS SELECT i::BIGINT id, 'name-'||i AS name FROM range(5000) t(i)").unwrap();
            let mut mapping = GraphMapping::new();
            mapping.register_table_schema(
                "people",
                Arc::new(Schema::new(vec![
                    Field::new("id", arrow::datatypes::DataType::Int64, true),
                    Field::new("name", arrow::datatypes::DataType::Utf8, true),
                ])),
            );
            mapping.map_node(if derived {
                crate::ir::rel::mapping::NodeMapping::query(
                    "Person",
                    "SELECT id,name FROM people",
                    "id",
                )
                .property("name", "name")
            } else {
                crate::ir::rel::mapping::NodeMapping::table("Person", "people", "id")
                    .property("name", "name")
            });
            if statistics {
                let snapshot = crate::ir::rel::statistics::generate_duckdb(
                    &db,
                    mapping.statistics_request().unwrap(),
                )
                .unwrap();
                mapping.set_statistics(Arc::new(snapshot)).unwrap();
            }
            let graph = attach(
                Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),
                Arc::new(mapping),
            )
            .unwrap();
            let source = graph.source.unwrap();
            let values = (0..count)
                .map(|i| Value::Node {
                    label: "Person".into(),
                    id: ElementId::new(ScalarValue::Int64(Some(i))).unwrap(),
                })
                .collect::<Vec<_>>();
            source.prefetch(&mut values.iter());
            source.check().unwrap();
            assert_eq!(
                source.stats().1.len(),
                expected_queries,
                "{:?}",
                source.stats()
            );
            assert_eq!(
                source.access_decisions().len(),
                usize::from(statistics && count > 1024)
            );
            for i in [0, count - 1] {
                assert_eq!(
                    source.property(
                        false,
                        "Person",
                        &ElementId::new(ScalarValue::Int64(Some(i))).unwrap(),
                        "name"
                    ),
                    Value::String(format!("name-{i}"))
                );
            }
            source.prefetch(&mut values.iter());
            assert_eq!(source.stats().1.len(), expected_queries);
        }
    }

    #[test]
    fn stale_statistics_scan_cap_falls_back_once() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people AS SELECT i::BIGINT id FROM range(5000) t(i)")
            .unwrap();
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema(
            "people",
            Arc::new(Schema::new(vec![Field::new(
                "id",
                arrow::datatypes::DataType::Int64,
                true,
            )])),
        );
        mapping.map_node(crate::ir::rel::mapping::NodeMapping::table(
            "Person", "people", "id",
        ));
        let snapshot =
            crate::ir::rel::statistics::generate_duckdb(&db, mapping.statistics_request().unwrap())
                .unwrap();
        mapping.set_statistics(Arc::new(snapshot)).unwrap();
        db.execute_batch("INSERT INTO people SELECT i FROM range(5000,70000) t(i)")
            .unwrap();
        let graph = attach(
            Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),
            Arc::new(mapping),
        )
        .unwrap();
        let source = graph.source.unwrap();
        let values = |start| {
            (start..start + 3000)
                .map(|i| Value::Node {
                    label: "Person".into(),
                    id: ElementId::new(ScalarValue::Int64(Some(i))).unwrap(),
                })
                .collect::<Vec<_>>()
        };
        source.prefetch(&mut values(0).iter());
        source.check().unwrap();
        assert_eq!(source.stats().1.len(), 4);
        assert_eq!(
            source.access_decisions()[0].after,
            vec!["batched_lookup_after_scan_cap"]
        );
        source.prefetch(&mut values(5000).iter());
        source.check().unwrap();
        assert_eq!(
            source.stats().1.len(),
            7,
            "a capped scan must not be retried"
        );
        assert!(source.exists(
            false,
            "Person",
            &ElementId::new(ScalarValue::Int64(Some(7999))).unwrap()
        ));
    }

    #[test]
    fn dense_adjacency_scan_preserves_parallel_edges_and_direction() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people AS SELECT i::BIGINT id FROM range(5000) t(i); CREATE TABLE links AS SELECT i::BIGINT id,(i/2)::BIGINT src,((i/2+1)%5000)::BIGINT dst FROM range(10000) t(i)").unwrap();
        let mut mapping = GraphMapping::new();
        for (table, columns) in [("people", vec!["id"]), ("links", vec!["id", "src", "dst"])] {
            mapping.register_table_schema(
                table,
                Arc::new(Schema::new(
                    columns
                        .into_iter()
                        .map(|c| Field::new(c, arrow::datatypes::DataType::Int64, true))
                        .collect::<Vec<_>>(),
                )),
            );
        }
        mapping.map_node(crate::ir::rel::mapping::NodeMapping::table(
            "Person", "people", "id",
        ));
        let mut edge = crate::ir::rel::mapping::EdgeMapping::table(
            "LINK", "links", "src", "dst", "Person", "Person",
        );
        edge.id_column = Some("id".into());
        mapping.map_edge(edge);
        let snapshot =
            crate::ir::rel::statistics::generate_duckdb(&db, mapping.statistics_request().unwrap())
                .unwrap();
        mapping.set_statistics(Arc::new(snapshot)).unwrap();
        let graph = attach(
            Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),
            Arc::new(mapping),
        )
        .unwrap();
        let source = graph.source.unwrap();
        let id = |i| ElementId::new(ScalarValue::Int64(Some(i))).unwrap();
        let nodes = (0..3000)
            .map(|i| ("Person".into(), id(i)))
            .collect::<Vec<_>>();
        source.prefetch_neighbors(false, &nodes, &["LINK".into()]);
        source.check().unwrap();
        assert_eq!(source.stats().1.len(), 1);
        let outgoing = source.neighbors(false, "Person", &id(10), &["LINK".into()]);
        assert!(
            outgoing.len() >= 2,
            "parallel edges were lost: {outgoing:?}"
        );
        source.prefetch_neighbors(true, &nodes, &["LINK".into()]);
        source.check().unwrap();
        assert_eq!(source.stats().1.len(), 1);
        assert!(
            !source
                .neighbors(true, "Person", &id(11), &["LINK".into()])
                .is_empty()
        );
        assert_eq!(
            outgoing,
            source.neighbors(false, "Person", &id(10), &["LINK".into()])
        );
    }

    #[test]
    fn jvm_views_allocate_property_handles_lazily_and_consistently() {
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE people(id VARCHAR PRIMARY KEY, name VARCHAR); INSERT INTO people VALUES ('a','Alice'),('b','Bob'); CREATE VIEW unrelated AS SELECT error('unexpected scan')::VARCHAR AS id FROM range(1)").unwrap();
        let mut mapping = GraphMapping::new();
        mapping.map_node(
            crate::ir::rel::mapping::NodeMapping::table("Person", "people", "id")
                .property("name", "name"),
        );
        mapping.map_node(crate::ir::rel::mapping::NodeMapping::table(
            "Unrelated",
            "unrelated",
            "id",
        ));
        let graph = attach(
            Arc::new(Mutex::new(DuckDbExecutor::from_connection(db))),
            Arc::new(mapping),
        )
        .unwrap();
        let source = graph.source.clone().unwrap();
        let mut store = crate::jvm_bridge::Store::from_execution_graph(graph);
        assert_eq!(
            source.stats().0,
            0,
            "JVM construction must not fetch graph rows"
        );
        let a = Value::Node {
            label: "Person".into(),
            id: ElementId::new(ScalarValue::Utf8(Some("a".into()))).unwrap(),
        };
        let b = Value::Node {
            label: "Person".into(),
            id: ElementId::new(ScalarValue::Utf8(Some("b".into()))).unwrap(),
        };
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
        assert!(
            source
                .stats()
                .1
                .iter()
                .all(|sql| !sql.contains("unrelated"))
        );
    }
}
