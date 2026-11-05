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
    labels: Option<Vec<String>>,
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
    key_defaults: HashMap<(String, String), Option<String>>,
    emptiness_checked: BTreeSet<(bool, String)>,
    nonempty_sources: BTreeSet<(bool, String)>,
    empty_lookups: BTreeSet<(bool, String)>,
    edge_defaults: HashMap<(String, String, String), (BTreeMap<String, Value>, BTreeSet<String>)>,
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
        // A failed lookup is evidence to consider an empty source, not proof.
        // Confirm once before caching emptiness, without probing healthy sources
        // ahead of every first fetch. Estimates alone never prove emptiness.
        let address = (edge, name.to_owned());
        let first = {
            let mut cache = self.cache.lock().unwrap();
            cache.empty_lookups.contains(&address)
                && !cache.nonempty_sources.contains(&address)
                && cache.emptiness_checked.insert(address.clone())
        };
        if first {
            let source = if edge { self.mapping.edge(name).map(|m| &m.source) }
                else { self.mapping.node(name).map(|m| &m.source) };
            if let Some(MappedSource::Table(source)) = source
                && self.mapping.representation_source(source).is_none()
                && self.mapping.collection_source(source).is_none()
                && self.mapping.logical_source(source).is_none() {
                let filter = if edge { self.mapping.edge(name).and_then(|m| m.foreign_key_columns())
                    .map(|(_, _, _, fk)| format!(" WHERE {}", fk.present_sql())).unwrap_or_default() }
                    else { String::new() };
                let sql = format!("SELECT 1 FROM {}{filter} LIMIT 1", table(source));
                let batch = self.host.query(HostRequest::new(&sql))?;
                let mut cache = self.cache.lock().unwrap();
                cache.queries.push(sql);
                if batch.num_rows() == 0 {
                    cache.complete_sources.insert(address.clone());
                    cache.source_order.insert(address, Vec::new());
                    return Ok(true);
                }
            }
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
        let labels_column = if edge { None } else { self.mapping.node(name).and_then(|node| node.labels_column.as_ref()) };
        if let Some(column) = labels_column {
            projection.push(format!("{} AS __logical_labels", quote(column)));
        }
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
        let (batch, sql) = if matches!(src, MappedSource::Computed(_) | MappedSource::Cypher(_)) {
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
            if let Some(column) = labels_column {
                selected.push(Expr::Column(datafusion::common::Column::from_name(column)).alias("__logical_labels"));
            }
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
            (batch, sql)
        };
        if scan
            && (batch.num_rows() > 65536 || batch.get_array_memory_size() > 16 * 1024 * 1024)
        {
            // A discarded scan still consumed source work. Account for it
            // uniformly before falling back to demand-driven lookups.
            let mut cache = self.cache.lock().unwrap();
            cache.rows += batch.num_rows();
            cache.queries.push(sql);
            return Ok(vec![]);
        }
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
        if keys.is_empty() {
            cache.empty_lookups.insert((edge, name.into()));
        } else {
            cache.nonempty_sources.insert((edge, name.into()));
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
            let labels = if labels_column.is_some() {
                use arrow::array::Array;
                let column = batch.column(props.len() + 1);
                let list = column.as_any().downcast_ref::<arrow::array::ListArray>()
                    .ok_or("logical node labels must be a string list")?;
                if list.is_null(row) { return Err("logical node labels must be non-null".into()); }
                let values = list.value(row);
                let strings = values.as_any().downcast_ref::<arrow::array::StringArray>()
                    .ok_or("logical node labels must contain strings")?;
                if strings.null_count() != 0 { return Err("logical node labels must contain non-null strings".into()); }
                Some(strings.iter().map(|value| value.unwrap().to_owned()).collect())
            } else { None };
            cache.records.insert(
                (edge, name.into(), id.clone()),
                Some(Record {
                    properties,
                    endpoints,
                    labels,
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
    fn generated_edge_key(&self, name: &str, src: &str, dst: &str) -> Option<Result<ElementId, String>> {
        self.mapping.edge_for_endpoints(name, src, dst).ok()?;
        self.generated_key(true, name)
    }
    fn edge_insert_defaults(&self, name: &str, src: &str, dst: &str)
        -> Option<Result<(BTreeMap<String, Value>, BTreeSet<String>), String>> {
        let mapping = self.mapping.edge_for_endpoints(name, src, dst).ok()?;
        let MappedSource::Table(table_name) = &mapping.source else { return None; };
        let address = (name.to_owned(), src.to_owned(), dst.to_owned());
        if let Some(defaults) = self.cache.lock().unwrap().edge_defaults.get(&address).cloned() {
            return Some(Ok(defaults));
        }
        Some((|| {
            let reference = datafusion::common::TableReference::from(table_name.as_str());
            let batch = self.host.query(HostRequest::new("SELECT column_name, column_default FROM duckdb_columns() WHERE lower(table_name)=lower($1) AND lower(schema_name)=lower(coalesce($2,current_schema())) AND lower(database_name)=lower(coalesce($3,current_database())) AND column_default IS NOT NULL")
                .parameters(vec![ScalarValue::Utf8(Some(reference.table().into())), ScalarValue::Utf8(reference.schema().map(str::to_owned)), ScalarValue::Utf8(reference.catalog().map(str::to_owned))]))?;
            let mut defaults = BTreeMap::new();
            let mut unsupported = BTreeSet::new();
            for row in 0..batch.num_rows() {
                let column = text(&batch, 0, row)?;
                if mapping.id_column.as_ref().is_some_and(|key| key.contains(&column)) { continue; }
                let expression = text(&batch, 1, row)?;
                for (property, _) in mapping.properties.iter().filter(|(_, physical)| **physical == column) {
                    if mapped_storage::constant_default(&expression) {
                        let value = self.host.query(HostRequest::new(format!("SELECT {expression} AS value")))?;
                        defaults.insert(property.clone(), crate::ir::catalog::array_value(value.column(0).as_ref(), 0, None));
                    } else {
                        unsupported.insert(property.clone());
                    }
                }
            }
            let result = (defaults, unsupported);
            self.cache.lock().unwrap().edge_defaults.insert(address, result.clone());
            Ok(result)
        })())
    }

    fn generated_key(&self, edge: bool, name: &str) -> Option<Result<ElementId, String>> {
        let (source, key) = if edge {
            let mapping = self.mapping.edge(name)?;
            (&mapping.source, mapping.id_column.as_ref()?)
        } else {
            let mapping = self.mapping.node(name)?;
            (&mapping.source, &mapping.id_column)
        };
        let MappedSource::Table(table_name) = source else { return None; };
        if key.len() != 1 { return None; }
        let reference = datafusion::common::TableReference::from(table_name.as_str());
        let result = (|| -> Result<Option<ElementId>, String> {
            // Cache the declaration, never its result: nextval()/UUID defaults
            // still execute once for every requested identity. The cache is
            // fenced with the other source metadata after storage effects.
            let address = (table_name.clone(), key.columns()[0].clone());
            let cached = self.cache.lock().unwrap().key_defaults.get(&address).cloned();
            let expression = if let Some(expression) = cached { expression } else {
            let defaults = self.host.query(HostRequest::new("SELECT column_default FROM duckdb_columns() WHERE lower(table_name)=lower($1) AND lower(column_name)=lower($2) AND lower(schema_name)=lower(coalesce($3,current_schema())) AND lower(database_name)=lower(coalesce($4,current_database())) AND column_default IS NOT NULL")
                .parameters(vec![ScalarValue::Utf8(Some(reference.table().into())), ScalarValue::Utf8(Some(key.columns()[0].clone())), ScalarValue::Utf8(reference.schema().map(str::to_owned)), ScalarValue::Utf8(reference.catalog().map(str::to_owned))]))?;
                let expression = if defaults.num_rows() == 0 { None } else { Some(text(&defaults, 0, 0)?) };
                self.cache.lock().unwrap().key_defaults.insert(address, expression.clone());
                expression
            };
            let Some(expression) = expression else { return Ok(None); };
            let sql = format!("SELECT {expression} AS generated_identity");
            let batch = self.host.query(HostRequest::new(&sql))?;
            self.cache.lock().unwrap().queries.push(sql);
            let value = ScalarValue::try_from_array(batch.column(0), 0).map_err(|e| e.to_string())?;
            Ok(Some(ElementId::new(value)?))
        })();
        match result { Ok(Some(key)) => Some(Ok(key)), Ok(None) => None, Err(error) => Some(Err(error)) }
    }
    fn native_state(&self, edge: bool, name: &str, id: &ElementId) -> Option<crate::ir::catalog::source::NativeState> {
        if edge || self.mapping.node(name)?.labels_column.is_none() { return None; }
        let record = self.record(false, name, id)?;
        Some(crate::ir::catalog::source::NativeState {
            node: None, public_id: None, labels: record.labels, cypher_id: None,
            null_keys: BTreeSet::new(), allow_null: false,
        })
    }
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
        cache.key_defaults.clear();
        cache.emptiness_checked.clear();
        cache.nonempty_sources.clear();
        cache.empty_lookups.clear();
        cache.edge_defaults.clear();
    }
    fn function(&self, name: &str, args: &[Value]) -> Option<Result<Value, String>> {
        if self.operators.overloads(name).is_empty() {
            return None;
        }
        Some(crate::ir::functions::host_execution::value(self.host.as_ref(), &self.operators.target_name(name), args))
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
            if matches!(src, MappedSource::Computed(_) | MappedSource::Cypher(_)) {
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
    fn engine_cost(&self) -> crate::ir::query_cost::EngineCost { self.host.engine_cost() }
    fn stats(&self) -> (usize, Vec<String>) {
        let cache = self.cache.lock().unwrap();
        (cache.rows, cache.queries.clone())
    }
    fn check(&self) -> Result<(), String> {
        self.cache.lock().unwrap().error.clone().map_or(Ok(()), Err)
    }
}

/// Partition delegates retain each source's bounded lookup/cache policy. Nodes
/// are read only through the primary delegate; relationship rows are combined
/// under their original logical type and globally unique relationship key.
#[derive(Debug)]
struct PartitionSources {
    sources: Vec<Arc<dyn GraphSource>>,
    handles: Mutex<HashMap<(String, ElementId, String), i64>>,
    error: Mutex<Option<String>>,
}
impl PartitionSources {
    fn owner(&self, edge: bool, name: &str, id: &ElementId) -> Option<&Arc<dyn GraphSource>> {
        if !edge { return Some(&self.sources[0]); }
        let mut found = None;
        for source in &self.sources {
            if source.exists(true, name, id) {
                if found.is_some() {
                    *self.error.lock().unwrap() = Some(format!("duplicate relationship identity across partitions of `{name}`"));
                    return None;
                }
                found = Some(source);
            }
        }
        found
    }
}
impl GraphSource for PartitionSources {
    fn generated_key(&self, edge: bool, name: &str) -> Option<Result<ElementId, String>> {
        self.sources[0].generated_key(edge, name)
    }
    fn generated_edge_key(&self, name: &str, src: &str, dst: &str) -> Option<Result<ElementId, String>> {
        self.sources.iter().find_map(|source| source.generated_edge_key(name, src, dst))
    }
    fn edge_insert_defaults(&self, name: &str, src: &str, dst: &str)
        -> Option<Result<(BTreeMap<String, Value>, BTreeSet<String>), String>> {
        self.sources.iter().find_map(|source| source.edge_insert_defaults(name, src, dst))
    }
    #[cfg(feature = "duckdb")]
    fn executor(&self) -> Option<Arc<Mutex<crate::ir::rel::sql::DuckDbExecutor>>> { self.sources[0].executor() }
    fn invalidate(&self) { for source in &self.sources { source.invalidate(); } }
    fn native_state(&self, edge: bool, name: &str, id: &ElementId) -> Option<crate::ir::catalog::source::NativeState> {
        self.owner(edge,name,id)?.native_state(edge,name,id)
    }
    fn function(&self,name:&str,args:&[Value])->Option<Result<Value,String>> { self.sources[0].function(name,args) }
    fn property_handle(&self,name:&str,id:&ElementId,key:&str)->i64 {
        let mut handles=self.handles.lock().unwrap(); let next=-(handles.len() as i64)-1;
        *handles.entry((name.into(),id.clone(),key.into())).or_insert(next)
    }
    fn property(&self,edge:bool,name:&str,id:&ElementId,key:&str)->Value {
        self.owner(edge,name,id).map_or(Value::Null,|source|source.property(edge,name,id,key))
    }
    fn ids(&self,edge:bool,name:&str)->Vec<ElementId> {
        if !edge { return self.sources[0].ids(false,name); }
        let ids = self.sources.iter().flat_map(|source|source.ids(true,name)).collect::<Vec<_>>();
        if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
            *self.error.lock().unwrap() = Some(format!("duplicate relationship identity across partitions of `{name}`"));
        }
        ids
    }
    fn endpoints(&self,name:&str,id:&ElementId)->Option<Endpoints> { self.owner(true,name,id)?.endpoints(name,id) }
    fn exists(&self,edge:bool,name:&str,id:&ElementId)->bool {
        if !edge { return self.sources[0].exists(false,name,id); }
        self.sources.iter().any(|source|source.exists(true,name,id))
    }
    fn prefetch_neighbors(&self,incoming:bool,nodes:&[(String,ElementId)],types:&[String]) {
        for source in &self.sources { source.prefetch_neighbors(incoming,nodes,types); }
    }
    fn neighbors(&self,incoming:bool,name:&str,id:&ElementId,types:&[String])->Vec<Neighbor> {
        self.sources.iter().flat_map(|source|source.neighbors(incoming,name,id,types)).collect()
    }
    fn prefetch(&self,values:&mut dyn Iterator<Item=&Value>) {
        let values=values.collect::<Vec<_>>();
        for source in &self.sources { source.prefetch(&mut values.iter().copied()); }
    }
    fn access_decisions(&self)->Vec<crate::ir::rel::statistics::OptimizerDecision> {
        self.sources.iter().flat_map(|source|source.access_decisions()).collect()
    }
    fn engine_cost(&self) -> crate::ir::query_cost::EngineCost {
        self.sources.first().map(|s| s.engine_cost()).unwrap_or_default()
    }
    fn stats(&self)->(usize,Vec<String>) {
        let mut total=(0,vec![]); for source in &self.sources {let (rows,queries)=source.stats();total.0+=rows;total.1.extend(queries);} total
    }
    fn check(&self)->Result<(),String> {
        if let Some(error) = self.error.lock().unwrap().clone() { return Err(error); }
        for source in &self.sources {source.check()?;} Ok(())
    }
}

pub fn attach(
    host: Arc<dyn HostRelational + Send + Sync>,
    mapping: Arc<GraphMapping>,
) -> Result<PropertyGraph, String> {
    attach_with_insert_defaults(host, mapping, true)
}
pub(crate) fn attach_with_insert_defaults(
    host: Arc<dyn HostRelational + Send + Sync>,
    mapping: Arc<GraphMapping>,
    load_insert_defaults: bool,
) -> Result<PropertyGraph, String> {
    let mut graph;
    let mut resolved = (*mapping).clone();
    // Runtime statements may write or observe a new snapshot. Only persistent
    // enforcement contracts survive here; snapshot proofs belong to caller-owned
    // immutable compile/execution scopes.
    resolved.set_constraint_scope(None);
    {
        let connection = host.as_ref();
        let mut bound_tables = BTreeSet::new();
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
                if !bound_tables.insert(t.clone()) { continue; }
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
            for partition in mapping.edge_partitions(&name) {
            if let MappedSource::Table(t) = &partition.source {
                if mapping.logical_source(t).is_some()
                    || mapping.collection_source(t).is_some()
                    || mapping.representation_source(t).is_some()
                {
                    continue;
                }
                if !bound_tables.insert(t.clone()) { continue; }
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
        }
        if let Some(snapshot) = mapping.statistics() {
            resolved
                .set_statistics(snapshot.clone())
                .map_err(|e| e.to_string())?;
        }
        graph = mapped_storage::metadata_with_insert_defaults(connection, Arc::new(resolved), load_insert_defaults)?;
    }
    let operators = crate::ir::functions::selected_operator_table().map_err(|e| e.to_string())?;
    let mappings = graph.mapping.as_ref().unwrap().single_partition_mappings();
    let sources = mappings.into_iter().map(|mapping| Arc::new(Source {
        host: host.clone(), operators: operators.clone(), mapping: Arc::new(mapping),
        cache: Default::default(), key_types: graph.key_types.clone(),
    }) as Arc<dyn GraphSource>).collect::<Vec<_>>();
    graph.source = if sources.len() == 1 { Some(sources[0].clone()) } else {
        Some(Arc::new(PartitionSources { sources, handles: Default::default(), error: Default::default() }))
    };
    Ok(graph)
}

#[cfg(all(test, feature = "duckdb"))]
#[path = "../../../../tests/unit/extracted/ir__rel__host__mapped_source__tests.rs"]
mod tests;
