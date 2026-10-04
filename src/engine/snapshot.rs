//! Exact side-effect observations without planning five graph queries.
use super::*;
use crate::ir::runtime::output::gremlin_typed_value;
use crate::ir::value::{STRUCT_ORDER_KEY, STRUCT_TYPES_KEY};
use serde::Serialize;
use std::collections::BTreeSet;

/// The same versioned, typed rows exposed by Cypher result transport. Consumers
/// compare sets of rows; properties include their value, so replacement counts
/// as one removed and one added property. This observes state, not write counts.
#[derive(Debug, Clone, Serialize)]
pub struct CypherStateSnapshot {
    pub nodes: Vec<Vec<serde_json::Value>>,
    pub relationships: Vec<Vec<serde_json::Value>>,
    pub labels: Vec<Vec<serde_json::Value>>,
    pub node_properties: Vec<Vec<serde_json::Value>>,
    pub edge_properties: Vec<Vec<serde_json::Value>>,
}
impl GraphEngine {
    /// Read one consistent state snapshot with Cypher identity/label/property
    /// semantics. Existing transactions see their own writes. For mapped graphs
    /// this explicitly reads source tables; normal query execution stays lazy.
    pub fn cypher_state_snapshot(&mut self) -> EngineResult<CypherStateSnapshot> {
        if self.failed_transaction {
            return Err("transaction failed; roll it back".into());
        }
        let Some(mapping) = self.mapping.clone() else {
            self.refresh()?;
            return observe(&self.graph, None);
        };
        let placeholder = Connection::open_in_memory().map_err(|e| e.to_string())?;
        let storage = std::mem::replace(&mut self.storage, placeholder);
        let mut connection_lease = MappedConnectionLease {
            target: &mut self.storage,
            executor: sql::DuckDbExecutor::from_connection(storage),
        };
        let automatic = !connection_lease.executor.in_transaction();
        if automatic {
            connection_lease
                .executor
                .begin()
                .map_err(|e| e.to_string())?;
        }
        let shared = Arc::new(std::sync::Mutex::new(std::mem::take(
            &mut connection_lease.executor,
        )));
        let mut executor_lease = MappedExecutorLease {
            target: &mut connection_lease.executor,
            shared: shared.clone(),
            automatic,
            finished: false,
        };
        let result = mapped_source::attach(shared.clone(), mapping.clone()).and_then(|graph| {
            let qualified = mapped_identity_text(&graph, &shared, &mapping)?;
            observe(&graph, Some(&qualified))
        });
        executor_lease.finished = true;
        drop(executor_lease);
        if automatic {
            if result.is_ok() {
                if let Err(error) = connection_lease.executor.commit() {
                    let _ = connection_lease.executor.rollback();
                    return Err(error.to_string());
                }
            } else {
                let _ = connection_lease.executor.rollback();
            }
        }
        result
    }
}
type QualifiedIdentities = BTreeMap<(bool, String, crate::ir::ElementId), Value>;

// Existing mapped id() SQL observations render provider-qualified identifiers,
// while native property observations retain scalar identity. Preserve both
// public representations exactly, including the database's cast formatting.
fn mapped_identity_text(
    graph: &PropertyGraph,
    shared: &Arc<std::sync::Mutex<sql::DuckDbExecutor>>,
    mapping: &GraphMapping,
) -> EngineResult<QualifiedIdentities> {
    use mapped_storage::{keys, query, resolved_source};
    let mut executor = shared.lock().map_err(|e| e.to_string())?;
    let connection = executor.connection().map_err(|e| e.to_string())?;
    let mut result = BTreeMap::new();
    for edge in [false, true] {
        let names = if edge {
            graph.edge_rel_order()
        } else {
            graph.node_label_order()
        };
        for (index, name) in names.iter().enumerate() {
            let (mapped_source, key) = if edge {
                let m = mapping.edge(name).unwrap();
                (&m.source, m.id_column.as_ref().unwrap_or(&m.src_column))
            } else {
                let m = mapping.node(name).unwrap();
                (&m.source, &m.id_column)
            };
            let table_id = if edge {
                graph.node_label_order().len() + 2 * index
            } else {
                index
            };
            let filter = if edge {
                mapping
                    .edge(name)
                    .and_then(|m| m.foreign_key_columns())
                    .map(|(_, _, _, fk)| format!(" WHERE {}", fk.present_sql()))
                    .unwrap_or_default()
            } else {
                String::new()
            };
            // Use the same typed, length-delimited tuple token as relational
            // ID() rendering, with DuckDB providing each component's text form.
            let schema = query(
                connection,
                &format!("SELECT * FROM {} WHERE false", resolved_source(mapping, mapped_source, &[])?),
            )?
            .schema();
            let projections = key
                .columns()
                .iter()
                .map(|column| {
                    let column_sql = mapped_storage::quote(column);
                    Ok(
                        match schema
                            .field_with_name(column)
                            .map_err(|e| e.to_string())?
                            .data_type()
                        {
                            arrow::datatypes::DataType::Binary
                            | arrow::datatypes::DataType::LargeBinary
                            | arrow::datatypes::DataType::BinaryView
                            | arrow::datatypes::DataType::FixedSizeBinary(_)
                                if key.len() > 1 =>
                            {
                                format!("lower(hex({column_sql}))")
                            }
                            _ => format!("CAST({column_sql} AS VARCHAR)"),
                        },
                    )
                })
                .collect::<Result<Vec<_>, String>>()?;
            let batch = query(
                connection,
                &format!(
                    "SELECT {}, {} FROM {}{filter}",
                    key.sql(None),
                    projections.join(","),
                    resolved_source(mapping, mapped_source, &[])?
                ),
            )?;
            for (row, id) in keys(&batch, 0)?.into_iter().enumerate() {
                let parts = id.components();
                let mut text = if key.len() > 1 {
                    format!("tuple{}:", key.len())
                } else {
                    String::new()
                };
                for (index, part) in parts.iter().enumerate() {
                    let Value::String(rendered) = crate::ir::catalog::array_value(
                        batch.column(index + 1).as_ref(),
                        row,
                        None,
                    ) else {
                        return Err("missing mapped identity text".into());
                    };
                    if key.len() > 1 {
                        text.push_str(&format!(
                            "{}:{}:",
                            crate::ir::identity::identity_variant(&part.data_type()),
                            rendered.chars().count()
                        ));
                    }
                    text.push_str(&rendered);
                }
                result.insert(
                    (edge, name.clone(), id),
                    Value::String(format!("{table_id}:{text}")),
                );
            }
        }
    }
    Ok(result)
}
fn observe(
    graph: &PropertyGraph,
    qualified: Option<&QualifiedIdentities>,
) -> EngineResult<CypherStateSnapshot> {
    let mut result = CypherStateSnapshot {
        nodes: vec![],
        relationships: vec![],
        labels: vec![],
        node_properties: vec![],
        edge_properties: vec![],
    };
    let encode = |value: &Value| gremlin_typed_value(value, graph);
    let identity = |element: &Value| {
        graph.source_identity(element).unwrap_or_else(|| {
            graph
                .cypher_id(element)
                .map(Value::Int)
                .unwrap_or(Value::Null)
        })
    };
    let mut labels = BTreeSet::new();
    for label in graph.labels() {
        let ids = graph.node_ids(&label).map_err(|e| e.to_string())?;
        let nodes = ids
            .iter()
            .map(|id| Value::Node {
                label: label.clone(),
                id: id.clone(),
            })
            .collect::<Vec<_>>();
        if let Some(source) = &graph.source {
            source.prefetch(&mut nodes.iter());
        }
        let keys = graph.node_property_keys_with_id(&label);
        for node in nodes {
            let Value::Node { id, .. } = &node else {
                unreachable!()
            };
            let identity = encode(&identity(&node));
            let element_identity = qualified
                .and_then(|ids| ids.get(&(false, label.clone(), id.clone())))
                .map(&encode)
                .unwrap_or_else(|| identity.clone());
            result.nodes.push(vec![element_identity]);
            labels.extend(graph.node_labels(&label, id.clone()));
            for key in &keys {
                if matches!(key.as_str(), STRUCT_ORDER_KEY | STRUCT_TYPES_KEY) {
                    continue;
                }
                let value = graph.node_property(&label, id.clone(), key);
                if value != Value::Null {
                    result.node_properties.push(vec![
                        identity.clone(),
                        encode(&Value::String(key.clone())),
                        encode(&value),
                    ]);
                }
            }
        }
    }
    result.labels = labels
        .into_iter()
        .map(|label| vec![encode(&Value::String(label))])
        .collect();
    for rel_type in graph.rel_types() {
        let ids = graph.edge_ids(&rel_type);
        let keys = graph.edge_property_keys(&rel_type);
        for id in ids {
            let Some((src_label, src_id, dst_label, dst_id)) =
                graph.edge_endpoints(&rel_type, id.clone())
            else {
                continue;
            };
            // MATCH ()-[r]->() only observes relationships with live endpoints.
            if !graph.node_is_live(&src_label, src_id.clone())
                || !graph.node_is_live(&dst_label, dst_id.clone())
            {
                continue;
            }
            let edge = Value::Edge {
                rel_type: rel_type.clone(),
                id: id.clone(),
                src_label,
                src_id,
                dst_label,
                dst_id,
                projected_properties: None,
            };
            let identity = encode(&identity(&edge));
            let element_identity = qualified
                .and_then(|ids| ids.get(&(true, rel_type.clone(), id.clone())))
                .map(&encode)
                .unwrap_or_else(|| identity.clone());
            result.relationships.push(vec![element_identity]);
            for key in &keys {
                if matches!(key.as_str(), STRUCT_ORDER_KEY | STRUCT_TYPES_KEY) {
                    continue;
                }
                let value = graph.edge_property(&rel_type, id.clone(), key);
                if value != Value::Null {
                    result.edge_properties.push(vec![
                        identity.clone(),
                        encode(&Value::String(key.clone())),
                        encode(&value),
                    ]);
                }
            }
        }
    }
    graph.check_source()?;
    Ok(result)
}
