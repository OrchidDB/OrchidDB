//! DuckDB source-table adapter for the shared graph runtime.
pub(super) use crate::ir::{
    ElementId, Value,
    catalog::{EdgeTable, NodeTable, PropertyGraph},
    rel::mapping::{GraphMapping, MappedSource},
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
    let mut graph = PropertyGraph::new();
    for label in mapping.labels() {
        let m = mapping.node(&label).unwrap();
        let mut projection = vec![format!("{} AS __key", quote(&m.id_column))];
        projection.extend(property_projection(&m.properties));
        let batch = query(
            connection,
            &format!(
                "SELECT {} FROM {} WHERE false",
                projection.join(","),
                source(&m.source)
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
                quote(m.id_column.as_ref().unwrap_or(&m.src_column))
            ),
            format!("{} AS __src_id", quote(&m.src_column)),
            format!("{} AS __dst_id", quote(&m.dst_column)),
        ];
        projection.extend(property_projection(&m.properties));
        let batch = query(
            connection,
            &format!(
                "SELECT {} FROM {} WHERE false",
                projection.join(","),
                source(&m.source)
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
                if &column == key {
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
pub(super) fn persist(
    connection: &Connection,
    graph: &PropertyGraph,
    mapping: &GraphMapping,
) -> Result<(), String> {
    graph.validate_mapped_changes()?;
    let pending = graph.pending_changes();
    let mut writes: Vec<(String, Vec<RecordBatch>)> = Vec::new();
    let mut schemas = BTreeMap::new();
    // Deletions precede inserts so source foreign keys see the same graph semantics.
    for deleting in [true, false] {
        for edge in if deleting {
            [true, false]
        } else {
            [false, true]
        } {
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
                let live = if edge {
                    graph.live_edge_endpoints(name, key.clone()).is_some()
                } else {
                    graph.node_is_live(name, key.clone())
                };
                if deleting == live {
                    continue;
                }
                let mut values = BTreeMap::from([(id_column.clone(), key.scalar().clone())]);
                if live {
                    for (property, column) in props {
                        let value = if edge {
                            graph.edge_property(name, key.clone(), property)
                        } else {
                            graph.node_property(name, key.clone(), property)
                        };
                        let scalar = value_scalar(&value)?;
                        if column == id_column {
                            continue;
                        }
                        values.insert(column.clone(), scalar);
                    }
                    if edge {
                        let m = mapping.edge(name).unwrap();
                        let (_, src, _, dst) = graph
                            .edge_endpoints(name, key.clone())
                            .ok_or("missing edge endpoints")?;
                        values.insert(m.src_column.clone(), src.scalar().clone());
                        values.insert(m.dst_column.clone(), dst.scalar().clone());
                    }
                }
                let target_schema = if let Some(schema) = schemas.get(table_name) {
                    Arc::clone(schema)
                } else {
                    let schema = query(
                        connection,
                        &format!("SELECT * FROM {} WHERE false", table(table_name)),
                    )?
                    .schema();
                    schemas.insert(table_name.clone(), schema.clone());
                    schema
                };
                for (column, value) in &mut values {
                    let kind = target_schema
                        .field_with_name(column)
                        .map_err(|e| e.to_string())?
                        .data_type();
                    *value = value.cast_to(kind).map_err(|e| e.to_string())?;
                }
                let columns = values.keys().map(|c| quote(c)).collect::<Vec<_>>();
                let batch = bind_batch(values.into_iter().collect())?;
                let relation = "__orchiddb_write_values(?, ?)";
                let target = table(table_name);
                let predicate = format!(
                    "target.{} = incoming.{}",
                    quote(id_column),
                    quote(id_column)
                );
                let existing = graph.base_exists(edge, name, key);
                let sql = if !live {
                    format!(
                        "DELETE FROM {target} AS target USING {relation} AS incoming WHERE {predicate}"
                    )
                } else if existing {
                    let assignments = columns
                        .iter()
                        .filter(|c| **c != quote(id_column))
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
                if let Some((_, batches)) =
                    writes.iter_mut().find(|(statement, _)| statement == &sql)
                {
                    batches.push(batch);
                } else {
                    writes.push((sql, vec![batch]));
                }
            }
        }
    }
    graph.check_source()?;
    for (sql, batches) in writes {
        let batch = arrow::compute::concat_batches(&batches[0].schema(), &batches)
            .map_err(|e| e.to_string())?;
        connection
            .execute(&sql, arrow_recordbatch_to_query_params(batch))
            .map_err(|e| e.to_string())?;
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
