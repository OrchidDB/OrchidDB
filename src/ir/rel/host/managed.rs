//! DuckDB-owned managed graph rows with the existing incremental value codec.
//! Entity identities/endpoints/labels stay relational and indexed; the payload
//! preserves native property records without introducing another storage codec.
use super::{HostRelational, HostRequest, SharedHost};
use crate::ir::{
    ElementId, Value,
    catalog::{EdgeTable, NodeTable, PropertyGraph, incremental::IncrementalRecord},
};
use arrow::{
    array::{ArrayRef, RecordBatch},
    datatypes::{DataType, Field, Schema},
};
use datafusion::common::ScalarValue;
use std::{collections::BTreeMap, sync::Arc};
mod source;
#[derive(Debug, Clone)]
pub struct ManagedStore {
    pub table: String,
}
impl ManagedStore {
    pub fn new(table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
        }
    }
    pub(super) fn sql_table(&self) -> String {
        super::mapped_storage::table(&self.table)
    }
    /// Execute only during the caller's effect phase, never while binding.
    pub fn create(&self, host: &dyn HostRelational) -> Result<(), String> {
        host.execute(HostRequest::new(format!("CREATE TABLE IF NOT EXISTS {} (kind INTEGER NOT NULL,name VARCHAR NOT NULL,id BIGINT NOT NULL,payload BLOB NOT NULL,live BOOLEAN NOT NULL,src_name VARCHAR,src_id BIGINT,dst_name VARCHAR,dst_id BIGINT,cypher_id BIGINT,labels VARCHAR[],properties VARCHAR,property_next BIGINT,public_key VARCHAR,allow_null BOOLEAN,PRIMARY KEY(kind,name,id))",self.sql_table())))
    }
    pub fn reset(&self, host: &dyn HostRelational) -> Result<(), String> {
        host.execute(HostRequest::new(format!(
            "DELETE FROM {}",
            self.sql_table()
        )))
    }
    /// Persist complete records for touched entities and allocator metadata in
    /// the caller's transaction. A later statement attaches a fresh source.
    pub fn persist(&self, host: &dyn HostRelational, graph: &PropertyGraph) -> Result<(), String> {
        use base64::Engine;
        let records = graph.durable_records()?;
        let mut rows = Vec::with_capacity(records.len());
        for record in records {
            let edge = record.kind == 2;
            let live = if record.kind == 1 {
                graph.node_is_live(&record.name, record.id.clone())
            } else if edge {
                graph
                    .live_edge_endpoints(&record.name, record.id.clone())
                    .is_some()
            } else {
                false
            };
            let endpoints = if edge {
                graph.edge_endpoints(&record.name, record.id.clone())
            } else {
                None
            };
            let value = if edge {
                endpoints
                    .as_ref()
                    .map(|(src_label, src_id, dst_label, dst_id)| Value::Edge {
                        rel_type: record.name.clone(),
                        id: record.id.clone(),
                        src_label: src_label.clone(),
                        src_id: src_id.clone(),
                        dst_label: dst_label.clone(),
                        dst_id: dst_id.clone(),
                        projected_properties: None,
                    })
            } else {
                Some(Value::Node {
                    label: record.name.clone(),
                    id: record.id.clone(),
                })
            };
            let properties = if live {
                let keys = if edge {
                    graph.edge_property_keys(&record.name)
                } else {
                    graph.node_property_keys_with_id(&record.name)
                };
                let properties = keys
                    .into_iter()
                    .map(|key| {
                        let value = if edge {
                            graph.edge_property(&record.name, record.id.clone(), &key)
                        } else {
                            graph.node_property(&record.name, record.id.clone(), &key)
                        };
                        (key, value)
                    })
                    .filter(|(_, v)| *v != Value::Null)
                    .collect();
                Some(base64::engine::general_purpose::STANDARD.encode(
                    crate::ir::catalog::snapshot::binary::encode_value_bytes(&Value::Map(
                        properties,
                    )),
                ))
            } else {
                None
            };
            let labels = if record.kind == 1 {
                graph.node_labels(&record.name, record.id.clone())
            } else {
                vec![]
            };
            let labels = ScalarValue::new_list(
                &labels
                    .into_iter()
                    .map(|s| ScalarValue::Utf8(Some(s)))
                    .collect::<Vec<_>>(),
                &DataType::Utf8,
                true,
            );
            rows.push(vec![
                ScalarValue::Int32(Some(record.kind)),
                ScalarValue::Utf8(Some(record.name.clone())),
                ScalarValue::Int64(Some(
                    record
                        .id
                        .as_i64()
                        .ok_or("Managed storage requires internal integer identities")?,
                )),
                ScalarValue::Binary(Some(record.payload)),
                ScalarValue::Boolean(Some(live)),
                ScalarValue::Utf8(endpoints.as_ref().map(|e| e.0.clone())),
                ScalarValue::Int64(endpoints.as_ref().and_then(|e| e.1.as_i64())),
                ScalarValue::Utf8(endpoints.as_ref().map(|e| e.2.clone())),
                ScalarValue::Int64(endpoints.as_ref().and_then(|e| e.3.as_i64())),
                ScalarValue::Int64(value.as_ref().and_then(|v| graph.cypher_id(v))),
                ScalarValue::List(labels),
                ScalarValue::Utf8(properties),
                ScalarValue::Int64(Some(graph.property_allocator())),
                ScalarValue::Utf8(value.as_ref().map(|v| {
                    crate::ir::catalog::properties::public_id_key(&graph.element_public_id(v))
                })),
                ScalarValue::Boolean(Some(graph.supports_null_property_values())),
            ]);
        }
        // Statement-independent allocator/write policy also survives empty
        // imports. Kind zero is host metadata, never an entity codec record.
        rows.push(vec![
            ScalarValue::Int32(Some(0)),
            ScalarValue::Utf8(Some(String::new())),
            ScalarValue::Int64(Some(0)),
            ScalarValue::Binary(Some(vec![])),
            ScalarValue::Boolean(Some(false)),
            ScalarValue::Utf8(None),
            ScalarValue::Int64(None),
            ScalarValue::Utf8(None),
            ScalarValue::Int64(None),
            ScalarValue::Int64(None),
            ScalarValue::List(ScalarValue::new_list(&[], &DataType::Utf8, true)),
            ScalarValue::Utf8(None),
            ScalarValue::Int64(Some(graph.property_allocator())),
            ScalarValue::Utf8(None),
            ScalarValue::Boolean(Some(graph.supports_null_property_values())),
        ]);
        let names = [
            "kind",
            "name",
            "id",
            "payload",
            "live",
            "src_name",
            "src_id",
            "dst_name",
            "dst_id",
            "cypher_id",
            "labels",
            "properties",
            "property_next",
            "public_key",
            "allow_null",
        ];
        let mut arrays = Vec::<ArrayRef>::new();
        for column in 0..names.len() {
            arrays.push(
                ScalarValue::iter_to_array(rows.iter().map(|row| row[column].clone()))
                    .map_err(|e| e.to_string())?,
            );
        }
        let schema = Arc::new(Schema::new(
            names
                .iter()
                .zip(&arrays)
                .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
                .collect::<Vec<_>>(),
        ));
        let batch = RecordBatch::try_new(schema, arrays).map_err(|e| e.to_string())?;
        graph.check_source()?;
        // The complete replacement row is already normalized by the shared
        // codec. Apply its keyed removal/insertion in the caller's transaction;
        // avoid an aggregate-based conflict plan over nested Arrow properties.
        host.execute(HostRequest::new(format!("DELETE FROM {} AS target USING __orchiddb_managed_changes AS incoming WHERE target.kind=incoming.kind AND target.name=incoming.name AND target.id=incoming.id",self.sql_table())).relation("__orchiddb_managed_changes",batch.clone()))?;
        host.execute(
            HostRequest::new(format!(
                "INSERT INTO {} SELECT * FROM __orchiddb_managed_changes",
                self.sql_table()
            ))
            .relation("__orchiddb_managed_changes", batch),
        )?;
        graph.invalidate_source_cache();
        graph.clear_pending_changes();
        Ok(())
    }
    /// Bind only catalog/allocator metadata. Entity payloads are fetched lazily.
    pub fn attach(&self, host: SharedHost) -> Result<PropertyGraph, String> {
        let metadata = host.query(HostRequest::new(format!(
            "SELECT kind,name,id,payload FROM {} WHERE kind IN (3,4) ORDER BY kind,name",
            self.sql_table()
        )))?;
        let records = records(&metadata)?;
        let mut graph = PropertyGraph::new();
        graph.apply_incremental_records(&records)?;
        for record in records {
            if record.kind == 3 {
                let fields = graph
                    .node_property_keys_with_id(&record.name)
                    .into_iter()
                    .map(|key| Field::new(key, crate::ir::rel::native_values::value_type(), true))
                    .collect::<Vec<_>>();
                graph.add_nodes(NodeTable {
                    label: record.name,
                    batch: RecordBatch::new_empty(Arc::new(Schema::new(fields))),
                });
            }
        }
        // The small endpoint-group catalog supports repeated public edge labels.
        let endpoints=host.query(HostRequest::new(format!("SELECT DISTINCT name,src_name,dst_name FROM {} WHERE kind=2 AND live ORDER BY name,src_name,dst_name",self.sql_table())))?;
        for row in 0..endpoints.num_rows() {
            let name = text(&endpoints, 0, row)?;
            let mut fields = vec![
                Field::new("__src_id", DataType::Int64, false),
                Field::new("__dst_id", DataType::Int64, false),
            ];
            fields.extend(
                graph
                    .edge_property_keys(&name)
                    .into_iter()
                    .map(|key| Field::new(key, crate::ir::rel::native_values::value_type(), true)),
            );
            graph
                .add_edges(EdgeTable {
                    rel_type: name,
                    src_label: text(&endpoints, 1, row)?,
                    dst_label: text(&endpoints, 2, row)?,
                    batch: RecordBatch::new_empty(Arc::new(Schema::new(fields))),
                })
                .map_err(|e| e.to_string())?;
        }
        let alloc = host.query(HostRequest::new(format!(
            "SELECT property_next,allow_null FROM {} ORDER BY (kind=0) DESC,property_next DESC LIMIT 1",
            self.sql_table()
        )))?;
        if alloc.num_rows() > 0 {
            if let ScalarValue::Int64(Some(next)) =
                ScalarValue::try_from_array(alloc.column(0), 0).map_err(|e| e.to_string())?
            {
                graph.set_property_allocator(next);
            }
            if let ScalarValue::Boolean(Some(enabled)) =
                ScalarValue::try_from_array(alloc.column(1), 0).map_err(|e| e.to_string())?
            {
                graph.enable_null_property_values(enabled);
            }
        }
        graph.source_keys = false;
        graph.source = Some(Arc::new(source::Source::new(self.clone(), host)));
        graph.clear_pending_changes();
        Ok(graph)
    }
    /// Relational source for the compiler: identity and endpoint columns retain
    /// native SQL types, while `properties` is the existing value-map carrier.
    pub fn relation_sql(&self, edge: bool) -> String {
        format!(
            "SELECT name,id,src_name,src_id,dst_name,dst_id,cypher_id,labels,struct_pack(__orchiddb_value_v1 := properties) AS properties FROM {} WHERE kind={} AND live",
            self.sql_table(),
            if edge { 2 } else { 1 }
        )
    }
}
pub(super) fn text(batch: &RecordBatch, column: usize, row: usize) -> Result<String, String> {
    match ScalarValue::try_from_array(batch.column(column), row).map_err(|e| e.to_string())? {
        ScalarValue::Utf8(Some(v))
        | ScalarValue::Utf8View(Some(v))
        | ScalarValue::LargeUtf8(Some(v)) => Ok(v),
        _ => Err("Managed source expected text".into()),
    }
}
pub(super) fn records(batch: &RecordBatch) -> Result<Vec<IncrementalRecord>, String> {
    (0..batch.num_rows())
        .map(|row| {
            let kind = match ScalarValue::try_from_array(batch.column(0), row)
                .map_err(|e| e.to_string())?
            {
                ScalarValue::Int32(Some(v)) => v,
                _ => return Err("Managed record kind must be INTEGER".into()),
            };
            let id = ElementId::new(
                ScalarValue::try_from_array(batch.column(2), row).map_err(|e| e.to_string())?,
            )?;
            let payload = match ScalarValue::try_from_array(batch.column(3), row)
                .map_err(|e| e.to_string())?
            {
                ScalarValue::Binary(Some(v))
                | ScalarValue::BinaryView(Some(v))
                | ScalarValue::LargeBinary(Some(v)) => v,
                _ => return Err("Managed record payload must be BLOB".into()),
            };
            Ok(IncrementalRecord {
                kind,
                name: text(batch, 1, row)?,
                id,
                payload,
            })
        })
        .collect()
}

#[cfg(all(test, feature = "duckdb"))]
#[path = "../../../../tests/unit/extracted/ir__rel__host__managed__tests.rs"]
mod tests;
