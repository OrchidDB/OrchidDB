//! DuckDB source-table adapter for the shared graph runtime.
pub(super) use crate::ir::{
    ElementId, Value,
    catalog::{EdgeTable, NodeTable, PropertyGraph},
    rel::mapping::{GraphMapping, KeyColumns, MappedSource},
};
pub(super) use arrow::{
    array::{ArrayRef, RecordBatch},
    datatypes::{Field, Schema},
};
pub(super) use datafusion::common::ScalarValue;
pub(super) use duckdb::{
    Connection,
    vtab::{arrow::ArrowVTab, arrow_recordbatch_to_query_params},
};
pub(super) use std::{collections::BTreeMap, sync::Arc};

pub(super) fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
pub(super) fn table(name: &str) -> String {
    datafusion::common::TableReference::from(name)
        .to_vec()
        .iter()
        .map(|part| quote(part))
        .collect::<Vec<_>>()
        .join(".")
}
pub(super) fn source(source: &MappedSource) -> String {
    match source {
        MappedSource::Table(name) => table(name),
        MappedSource::Query(sql) => format!("({sql})"),
    }
}
pub(super) fn resolved_source(mapping: &GraphMapping, src: &MappedSource, filters: &[datafusion::logical_expr::Expr]) -> Result<String, String> {
    if let Some(sql) = mapping.derived_source_sql(src, filters).map_err(|e|e.to_string())? {
        return Ok(format!("({sql})"));
    }
    Ok(match src {
        MappedSource::Table(name) => table(&mapping.resolve_table(name, filters)),
        MappedSource::Query(_) => source(src),
    })
}

pub(super) fn query(connection: &Connection, sql: &str) -> Result<RecordBatch, String> {
    let mut statement = connection.prepare(sql).map_err(|e| e.to_string())?;
    let result = statement.query_arrow([]).map_err(|e| e.to_string())?;
    let schema = result.get_schema();
    let batches = result.collect::<Vec<_>>();
    arrow::compute::concat_batches(&schema, &batches).map_err(|e| e.to_string())
}
pub(super) fn keys(batch: &RecordBatch, column: usize) -> Result<Vec<ElementId>, String> {
    (0..batch.num_rows())
        .map(|row| {
            ElementId::new(
                ScalarValue::try_from_array(batch.column(column), row)
                    .map_err(|e| e.to_string())?,
            )
        })
        .collect()
}
pub(super) fn property_projection(props: &BTreeMap<String, String>) -> Vec<String> {
    props
        .iter()
        .map(|(name, column)| format!("{} AS {}", quote(column), quote(name)))
        .collect()
}
pub(super) fn metadata(
    connection: &Connection,
    mapping: Arc<GraphMapping>,
) -> Result<PropertyGraph, String> {
    mapping.validate_foreign_keys().map_err(|e| e.to_string())?;
    let mut graph = PropertyGraph::new();
    for label in mapping.labels() {
        let m = mapping.node(&label).unwrap();
        let mut projection = vec![format!("{} AS __key", m.id_column.sql(None))];
        projection.extend(property_projection(&m.properties));
        let batch = query(
            connection,
            &format!(
                "SELECT {} FROM {} WHERE false",
                projection.join(","),
                resolved_source(&mapping, &m.source, &[])?
            ),
        )?;
        graph
            .key_types
            .insert((false, label.clone()), batch.column(0).data_type().clone());
        let ids = keys(&batch, 0)?;
        let props = batch
            .project(&(1..batch.num_columns()).collect::<Vec<_>>())
            .map_err(|e| e.to_string())?;
        graph
            .add_keyed_nodes(
                NodeTable {
                    label,
                    batch: props,
                },
                ids,
            )
            .map_err(|e| e.to_string())?;
    }
    for rel_type in mapping.rel_types() {
        let m = mapping.edge(&rel_type).unwrap();
        let mut projection = vec![
            format!(
                "{} AS __key",
                m.id_column.as_ref().unwrap_or(&m.src_column).sql(None)
            ),
            format!("{} AS __src_id", m.src_column.sql(None)),
            format!("{} AS __dst_id", m.dst_column.sql(None)),
        ];
        projection.extend(property_projection(&m.properties));
        let batch = query(
            connection,
            &format!(
                "SELECT {} FROM {} WHERE false",
                projection.join(","),
                resolved_source(&mapping, &m.source, &[])?
            ),
        )?;
        graph.key_types.insert(
            (true, rel_type.clone()),
            batch.column(0).data_type().clone(),
        );
        let ids = keys(&batch, 0)?;
        let props = batch
            .project(&(1..batch.num_columns()).collect::<Vec<_>>())
            .map_err(|e| e.to_string())?;
        let mut arrays = props.columns().to_vec();
        for (index, label) in [(0, &m.src_label), (1, &m.dst_label)] {
            let kind = graph
                .key_types
                .get(&(false, label.clone()))
                .ok_or_else(|| format!("unmapped endpoint label `{label}`"))?;
            arrays[index] =
                arrow::compute::cast(&arrays[index], kind).map_err(|e| e.to_string())?;
            for row in 0..batch.num_rows() {
                let key = ElementId::new(
                    ScalarValue::try_from_array(&arrays[index], row).map_err(|e| e.to_string())?,
                )?;
                if !graph.node_is_live(label, key) {
                    return Err(format!("edge `{rel_type}` has an unmapped endpoint"));
                }
            }
        }
        let fields = props
            .schema()
            .fields()
            .iter()
            .zip(&arrays)
            .map(|(field, array)| {
                Field::new(field.name(), array.data_type().clone(), field.is_nullable())
            })
            .collect::<Vec<_>>();
        let props = RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays)
            .map_err(|e| e.to_string())?;
        graph
            .add_keyed_edges(
                EdgeTable {
                    rel_type,
                    src_label: m.src_label.clone(),
                    dst_label: m.dst_label.clone(),
                    batch: props,
                },
                ids,
            )
            .map_err(|e| e.to_string())?;
    }
    for edge in [false, true] {
        for name in if edge {
            mapping.rel_types()
        } else {
            mapping.labels()
        } {
            let (source, props, key) = if edge {
                let m = mapping.edge(&name).unwrap();
                (
                    &m.source,
                    &m.properties,
                    m.id_column.as_ref().unwrap_or(&m.src_column),
                )
            } else {
                let m = mapping.node(&name).unwrap();
                (&m.source, &m.properties, &m.id_column)
            };
            let MappedSource::Table(table_name) = source else {
                continue;
            };
            let mut statement=connection.prepare("SELECT column_name, column_default FROM duckdb_columns() WHERE table_name = ? AND schema_name = coalesce(?, current_schema()) AND column_default IS NOT NULL").map_err(|e|e.to_string())?;
            let rows = statement
                .query_map(
                    duckdb::params![
                        datafusion::common::TableReference::from(table_name.as_str()).table(),
                        datafusion::common::TableReference::from(table_name.as_str()).schema()
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                )
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            for (column, expression) in rows {
                if key.contains(&column) {
                    continue;
                }
                for (property, _) in props.iter().filter(|(_, c)| **c == column) {
                    if constant_default(&expression) {
                        let batch = query(connection, &format!("SELECT {expression} AS value"))?;
                        let value =
                            crate::ir::catalog::array_value(batch.column(0).as_ref(), 0, None);
                        graph
                            .mapped_defaults
                            .entry((edge, name.clone()))
                            .or_default()
                            .insert(property.clone(), value);
                    } else {
                        graph
                            .unsupported_defaults
                            .entry((edge, name.clone()))
                            .or_default()
                            .insert(property.clone());
                    }
                }
            }
        }
    }
    graph.mapping = Some(mapping);
    Ok(graph)
}
pub(super) fn register(connection: &Connection) -> Result<(), String> {
    let exists: i64 = connection.query_row("SELECT count(*) FROM duckdb_functions() WHERE function_name = '__orchiddb_write_values'",[],|row|row.get(0)).map_err(|e|e.to_string())?;
    if exists == 0 {
        connection
            .register_table_function::<ArrowVTab>("__orchiddb_write_values")
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(super) fn value_scalar(value: &Value) -> Result<ScalarValue, String> {
    if matches!(value, Value::Null) {
        return Ok(ScalarValue::Null);
    }
    match value {
        Value::BigDecimal(v) => Ok(ScalarValue::Utf8(Some(v.to_string()))),
        Value::BigInt(v) | Value::UInt128(v) => Ok(ScalarValue::Utf8(Some(v.to_string()))),
        _ => ElementId::try_from(value).map(|id| id.scalar().clone()),
    }
}
fn bind_batch(values: Vec<(String, ScalarValue)>) -> Result<RecordBatch, String> {
    let fields = values
        .iter()
        .map(|(name, value)| Field::new(name, value.data_type(), true))
        .collect::<Vec<_>>();
    let arrays = values
        .into_iter()
        .map(|(_, value)| value.to_array().map_err(|e| e.to_string()))
        .collect::<Result<Vec<ArrayRef>, _>>()?;
    RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).map_err(|e| e.to_string())
}
/// MATCH SIMPLE lets any nullable FK component disconnect the relationship.
/// Preserve shared primary-key components and other required FK columns.
fn nullable_fk_columns(
    connection: &Connection,
    name: &str,
    fk: &KeyColumns,
    key: &KeyColumns,
) -> Result<Vec<String>, String> {
    let reference = datafusion::common::TableReference::from(name);
    let mut statement = connection.prepare("SELECT column_name, is_nullable FROM duckdb_columns() WHERE table_name = ? AND schema_name = coalesce(?, current_schema()) AND database_name = coalesce(?, current_database())").map_err(|e| e.to_string())?;
    let columns = statement
        .query_map(
            duckdb::params![reference.table(), reference.schema(), reference.catalog()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
        )
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    let result = columns
        .into_iter()
        .filter_map(|(column, nullable)| {
            (nullable && fk.contains(&column) && !key.contains(&column)).then_some(column)
        })
        .collect::<Vec<_>>();
    if result.is_empty() {
        return Err("cannot unlink relationship: its foreign key has no nullable non-primary-key component; replace the relationship or delete the child".into());
    }
    Ok(result)
}

// Write ownership is physical, not graph-shaped: a node and several FK edges
// can all contribute columns to one row.
type RowAddress = (String, KeyColumns, ElementId);
#[derive(Debug)]
struct RowWrite {
    existing: bool,
    delete: bool,
    values: BTreeMap<String, ScalarValue>,
}
fn merge_row(
    rows: &mut BTreeMap<RowAddress, RowWrite>,
    address: RowAddress,
    existing: bool,
    delete: bool,
    values: BTreeMap<String, ScalarValue>,
) -> Result<(), String> {
    use std::collections::btree_map::Entry;
    match rows.entry(address) {
        Entry::Vacant(entry) => {
            entry.insert(RowWrite {
                existing,
                delete,
                values,
            });
        }
        Entry::Occupied(mut entry) => {
            let row = entry.get_mut();
            if row.delete != delete || row.existing != existing {
                return Err("incompatible graph mutations target the same physical row".into());
            }
            for (column, value) in values {
                if row.values.get(&column).is_some_and(|old| old != &value) {
                    return Err(format!(
                        "conflicting assignments to physical column `{column}`"
                    ));
                }
                row.values.insert(column, value);
            }
        }
    }
    Ok(())
}

// (child table, FK column, referenced table, referenced key column).
type Reference = (String, KeyColumns, String, KeyColumns);
fn references(mapping: &GraphMapping) -> Result<Vec<Reference>, String> {
    let mut refs = std::collections::BTreeSet::new();
    for name in mapping.rel_types() {
        let m = mapping.edge(&name).unwrap();
        let MappedSource::Table(child_table) = &m.source else {
            continue;
        };
        let endpoints = if let Some((_, _, parent, fk)) = m.foreign_key_columns() {
            vec![(parent, fk)]
        } else {
            vec![
                (m.src_label.as_str(), &m.src_column),
                (m.dst_label.as_str(), &m.dst_column),
            ]
        };
        for (label, column) in endpoints {
            let parent = mapping
                .node(label)
                .ok_or_else(|| format!("unmapped endpoint `{label}`"))?;
            if let MappedSource::Table(parent_table) = &parent.source {
                refs.insert((
                    child_table.clone(),
                    column.clone(),
                    parent_table.clone(),
                    parent.id_column.clone(),
                ));
            }
        }
    }
    Ok(refs.into_iter().collect())
}

/// Construct dependency waves before executing any SQL. This handles FK inserts,
/// updates that release an old parent, and child-before-parent deletion. Cycles
/// requiring deferred constraints are rejected rather than inserting NULL first.
fn write_order(
    connection: &Connection,
    rows: &BTreeMap<RowAddress, RowWrite>,
    refs: &[Reference],
    schemas: &BTreeMap<String, Arc<Schema>>,
) -> Result<Vec<Vec<RowAddress>>, String> {
    use std::collections::BTreeSet;
    let deleting_tables = rows
        .iter()
        .filter(|(_, row)| row.delete)
        .map(|((table, key, _), _)| (table.as_str(), key))
        .collect::<BTreeSet<_>>();
    let mut dependencies = rows
        .keys()
        .map(|key| (key.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for (address, row) in rows {
        let (table_name, key_column, key) = address;
        let relevant = refs
            .iter()
            .filter(|r| &r.0 == table_name)
            .collect::<Vec<_>>();
        let needs_old = row.existing
            && relevant
                .iter()
                .any(|r| deleting_tables.contains(&(r.2.as_str(), &r.3)));
        let old = if needs_old {
            let columns = relevant.iter().map(|r| r.1.sql(None)).collect::<Vec<_>>();
            let input = bind_batch(key_column.values(key)?.into_iter().collect())?;
            let sql = format!(
                "SELECT {} FROM {} WHERE {} IN (SELECT {} FROM __orchiddb_write_values(?, ?))",
                columns.join(","),
                table(table_name),
                key_column.sql(None),
                key_column.sql(None)
            );
            let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
            let reader = statement
                .query_arrow(arrow_recordbatch_to_query_params(input))
                .map_err(|e| e.to_string())?;
            let schema = reader.get_schema();
            Some(
                arrow::compute::concat_batches(&schema, &reader.collect::<Vec<_>>())
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        for (index, reference) in relevant.iter().enumerate() {
            let (_, fk, parent_table, parent_key) = reference;
            let old_value = match &old {
                Some(batch) if batch.num_rows() == 1 => {
                    ScalarValue::try_from_array(batch.column(index), 0)
                        .map_err(|e| e.to_string())?
                }
                _ => ScalarValue::Null,
            };
            let components = |value: &ScalarValue| -> Result<Vec<ScalarValue>, String> {
                match value {
                    ScalarValue::Struct(array) => array
                        .columns()
                        .iter()
                        .map(|column| {
                            ScalarValue::try_from_array(column, 0).map_err(|e| e.to_string())
                        })
                        .collect(),
                    value if fk.len() == 1 => Ok(vec![value.clone()]),
                    _ if value.is_null() => Ok(vec![ScalarValue::Null; fk.len()]),
                    _ => Err("foreign key result has wrong arity".into()),
                }
            };
            let old_parts = components(&old_value)?;
            let new_parts = if row.delete {
                vec![ScalarValue::Null; fk.len()]
            } else {
                fk.columns()
                    .iter()
                    .zip(&old_parts)
                    .map(|(column, old)| row.values.get(column).unwrap_or(old).clone())
                    .collect()
            };
            let parent_address = |values: Vec<ScalarValue>| -> Result<Option<RowAddress>, String> {
                // SQL MATCH SIMPLE: any null component means no relationship.
                if values.iter().any(ScalarValue::is_null) {
                    return Ok(None);
                }
                let kind = parent_key.data_type(&schemas[parent_table])?;
                Ok(Some((
                    parent_table.clone(),
                    parent_key.clone(),
                    ElementId::from_components(values)?.cast_to(&kind)?,
                )))
            };
            if let Some(parent) = parent_address(new_parts)? {
                if let Some(parent_row) = rows.get(&parent) {
                    if parent_row.delete {
                        return Err("cannot delete a parent still referenced by a child row".into());
                    }
                    if !parent_row.existing {
                        dependencies.get_mut(address).unwrap().insert(parent);
                    }
                }
            }
            if let Some(parent) = parent_address(old_parts)? {
                if rows.get(&parent).is_some_and(|r| r.delete) {
                    dependencies
                        .get_mut(&parent)
                        .unwrap()
                        .insert(address.clone());
                }
            }
        }
    }
    let mut waves = Vec::new();
    while !dependencies.is_empty() {
        let ready = dependencies
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(key, _)| key.clone())
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return Err("cyclic mapped writes require deferred foreign-key constraints, which are not supported".into());
        }
        for key in &ready {
            dependencies.remove(key);
        }
        let ready_set = ready.iter().cloned().collect::<BTreeSet<_>>();
        for deps in dependencies.values_mut() {
            deps.retain(|key| !ready_set.contains(key));
        }
        waves.push(ready);
    }
    Ok(waves)
}

pub(super) fn persist(
    connection: &Connection,
    graph: &PropertyGraph,
    mapping: &GraphMapping,
) -> Result<(), String> {
    graph.validate_mapped_changes()?;
    let pending = graph.pending_changes();
    let mut rows = BTreeMap::<RowAddress, RowWrite>::new();
    for edge in [false, true] {
        let touched = if edge { &pending.edges } else { &pending.nodes };
        for (name, key) in touched {
            let (mapped_source, id_column, props) = if edge {
                let m = mapping
                    .edge(name)
                    .ok_or_else(|| format!("unmapped relationship `{name}`"))?;
                (
                    &m.source,
                    m.id_column.as_ref().unwrap_or(&m.src_column),
                    &m.properties,
                )
            } else {
                let m = mapping
                    .node(name)
                    .ok_or_else(|| format!("unmapped label `{name}`"))?;
                (&m.source, &m.id_column, &m.properties)
            };
            let MappedSource::Table(table_name) = mapped_source else {
                return Err(format!("query-backed mapping `{name}` is read-only"));
            };
            if mapping.representation_source(table_name).is_some() {
                return Err(format!("representation source `{table_name}` is read-only"));
            }
            if mapping.collection_source(table_name).is_some() {
                return Err(format!("collection source `{table_name}` is read-only"));
            }
            if mapping.logical_source(table_name).is_some() {
                return Err(format!("logical source `{table_name}` is read-only; write to its physical table and refresh layout statistics"));
            }
            let live = if edge {
                graph.live_edge_endpoints(name, key.clone()).is_some()
            } else {
                graph.node_is_live(name, key.clone())
            };
            let mut values = id_column.values(key)?;
            if live {
                for (property, column) in props {
                    if id_column.contains(column) {
                        continue;
                    }
                    let value = if edge {
                        graph.edge_property(name, key.clone(), property)
                    } else {
                        graph.node_property(name, key.clone(), property)
                    };
                    values.insert(column.clone(), value_scalar(&value)?);
                }
            }
            if edge {
                let m = mapping.edge(name).unwrap();
                if let Some((child, child_key, _, fk)) = m.foreign_key_columns() {
                    // Removing a child already removes its FK edge. Removing an
                    // edge alone never deletes the child row or its properties.
                    if !graph.node_is_live(child, key.clone()) {
                        continue;
                    }
                    if live {
                        let (_, src, _, dst) = graph
                            .edge_endpoints(name, key.clone())
                            .ok_or("missing edge endpoints")?;
                        let parent = if m.foreign_key
                            == Some(crate::ir::rel::mapping::ForeignKeyEndpoint::Source)
                        {
                            dst
                        } else {
                            src
                        };
                        for (column, value) in fk.values(&parent)? {
                            if values.get(&column).is_some_and(|old| old != &value) {
                                return Err(format!(
                                    "relationship conflicts with child key component `{column}`"
                                ));
                            }
                            values.insert(column, value);
                        }
                    } else {
                        for column in nullable_fk_columns(connection, table_name, fk, child_key)? {
                            values.insert(column, ScalarValue::Null);
                        }
                    }
                    merge_row(
                        &mut rows,
                        (table_name.clone(), child_key.clone(), key.clone()),
                        graph.base_exists(false, child, key),
                        false,
                        values,
                    )?;
                    continue;
                }
                if live {
                    let (_, src, _, dst) = graph
                        .edge_endpoints(name, key.clone())
                        .ok_or("missing edge endpoints")?;
                    for (column, value) in m
                        .src_column
                        .values(&src)?
                        .into_iter()
                        .chain(m.dst_column.values(&dst)?)
                    {
                        if values.get(&column).is_some_and(|old| old != &value) {
                            return Err(format!("conflicting endpoint/key component `{column}`"));
                        }
                        values.insert(column, value);
                    }
                }
            }
            let existing = graph.base_exists(edge, name, key);
            // A create-then-delete in one statement has no physical row.
            if !live && !existing {
                continue;
            }
            merge_row(
                &mut rows,
                (table_name.clone(), id_column.clone(), key.clone()),
                existing,
                !live,
                values,
            )?;
        }
    }
    // Newly inserted child rows have no relationship unless one was explicitly
    // created. This also prevents a database FK default from inventing an edge
    // invisible to the statement's graph overlay.
    for name in mapping.rel_types() {
        let m = mapping.edge(&name).unwrap();
        if let (MappedSource::Table(table_name), Some((_, child_key, _, fk))) =
            (&m.source, m.foreign_key_columns())
        {
            for ((target, key_column, id), row) in &mut rows {
                if target == table_name && key_column == child_key && !row.existing && !row.delete {
                    for column in fk.columns() {
                        row.values
                            .entry(column.clone())
                            .or_insert(ScalarValue::Null);
                    }
                    if fk
                        .columns()
                        .iter()
                        .all(|c| row.values.get(c).is_some_and(|v| !v.is_null()))
                        && !pending.edges.contains(&(name.clone(), id.clone()))
                    {
                        return Err(format!(
                            "creation of this child requires an explicit `{name}` relationship in the same statement"
                        ));
                    }
                }
            }
        }
    }
    let refs = references(mapping)?;
    let mut schemas = BTreeMap::new();
    for table_name in rows.keys().map(|r| &r.0).chain(refs.iter().map(|r| &r.2)) {
        if !schemas.contains_key(table_name) {
            schemas.insert(
                table_name.clone(),
                query(
                    connection,
                    &format!("SELECT * FROM {} WHERE false", table(table_name)),
                )?
                .schema(),
            );
        }
    }
    for ((table_name, _, _), row) in &mut rows {
        for (column, value) in &mut row.values {
            let kind = schemas[table_name]
                .field_with_name(column)
                .map_err(|e| e.to_string())?
                .data_type();
            *value = value.cast_to(kind).map_err(|e| e.to_string())?;
        }
    }
    graph.check_source()?;
    let waves = write_order(connection, &rows, &refs, &schemas)?;
    for wave in waves {
        let mut writes = BTreeMap::<String, Vec<RecordBatch>>::new();
        for address in wave {
            let (table_name, key_column, _) = &address;
            let row = rows.remove(&address).unwrap();
            let columns = row.values.keys().map(|c| quote(c)).collect::<Vec<_>>();
            let relation = "__orchiddb_write_values(?, ?)";
            let target = table(table_name);
            let predicate = key_column
                .columns()
                .iter()
                .map(|column| format!("target.{} = incoming.{}", quote(column), quote(column)))
                .collect::<Vec<_>>()
                .join(" AND ");
            let sql = if row.delete {
                format!(
                    "DELETE FROM {target} AS target USING {relation} AS incoming WHERE {predicate}"
                )
            } else if row.existing {
                let assignments = columns
                    .iter()
                    .filter(|c| !key_column.columns().iter().any(|key| quote(key) == **c))
                    .map(|c| format!("{c}=incoming.{c}"))
                    .collect::<Vec<_>>();
                if assignments.is_empty() {
                    continue;
                }
                format!(
                    "UPDATE {target} AS target SET {} FROM {relation} AS incoming WHERE {predicate}",
                    assignments.join(",")
                )
            } else {
                format!(
                    "INSERT INTO {target} ({}) SELECT {} FROM {relation}",
                    columns.join(","),
                    columns.join(",")
                )
            };
            writes
                .entry(sql)
                .or_default()
                .push(bind_batch(row.values.into_iter().collect())?);
        }
        for (sql, batches) in writes {
            let batch = arrow::compute::concat_batches(&batches[0].schema(), &batches)
                .map_err(|e| e.to_string())?;
            connection
                .execute(&sql, arrow_recordbatch_to_query_params(batch))
                .map_err(|e| e.to_string())?;
        }
    }
    graph.clear_pending_changes();
    Ok(())
}

fn constant_default(expression: &str) -> bool {
    use datafusion::sql::sqlparser::{
        ast::{Expr, SelectItem, SetExpr, Statement},
        dialect::DuckDbDialect,
        parser::Parser,
    };
    fn constant(expr: &Expr) -> bool {
        match expr {
            Expr::Value(_) | Expr::TypedString { .. } => true,
            Expr::Nested(e) | Expr::UnaryOp { expr: e, .. } | Expr::Cast { expr: e, .. } => {
                constant(e)
            }
            Expr::BinaryOp { left, right, .. } => constant(left) && constant(right),
            _ => false,
        }
    }
    let Ok(statements) = Parser::parse_sql(&DuckDbDialect {}, &format!("SELECT {expression}"))
    else {
        return false;
    };
    let [Statement::Query(query)] = statements.as_slice() else {
        return false;
    };
    let SetExpr::Select(select) = query.body.as_ref() else {
        return false;
    };
    matches!(select.projection.as_slice(),[SelectItem::UnnamedExpr(expr)] if constant(expr))
}
