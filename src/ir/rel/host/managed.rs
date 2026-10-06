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
mod tests {
    use super::super::{legacy::ExecutorHost, observation::observe};
    use super::*;
    use crate::ir::{
        catalog::{Cardinality, import::import_graph},
        rel::sql::DuckDbExecutor,
    };
    use std::sync::Mutex;
    fn host() -> SharedHost {
        Arc::new(ExecutorHost(Arc::new(Mutex::new(
            DuckDbExecutor::from_connection(duckdb::Connection::open_in_memory().unwrap()),
        ))))
    }
    fn cypher(
        host: SharedHost,
        store: &ManagedStore,
        query: &str,
    ) -> Result<Vec<crate::ir::runtime::Row>, String> {
        use crate::ir::rel::runtime::{
            CompileOptions, KernelState, SubplanRunner, compile_for_host, host::tests::HostRunner,
        };
        let graph = store.attach(host.clone())?;
        let plan = crate::language::cypher::preparation::prepare(
            query,
            &BTreeMap::new(),
            Some(graph.procedures.as_ref()),
        )
        .map_err(|e| e.message)?;
        let compiled = compile_for_host(
            &plan,
            &graph,
            CompileOptions {
                sql_islands: false,
                ..Default::default()
            },
        )
        .map_err(|e| e.message)?;
        let runner = Arc::new(HostRunner::default());
        let mut state = KernelState::for_host(graph, runner.clone());
        host.execute(HostRequest::new("BEGIN TRANSACTION"))?;
        let result = runner
            .run(&compiled, &mut state)
            .map_err(|e| e.to_string())
            .and_then(|rows| {
                store.persist(host.as_ref(), &state.graph)?;
                Ok(rows)
            });
        host.execute(HostRequest::new(if result.is_ok() {
            "COMMIT"
        } else {
            "ROLLBACK"
        }))?;
        result
    }
    #[test]
    fn managed_cypher_reuses_mutation_kernels_for_sequential_merge_and_labels() {
        let host = host();
        let store = ManagedStore::new("managed_graph");
        store.create(host.as_ref()).unwrap();
        let rows=cypher(host.clone(),&store,"CREATE (n:A {k:1}) WITH n MERGE (m:A {k:1}) ON MATCH SET m.hit=true RETURN id(n)=id(m) AS same").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("same"), Value::Bool(true));
        let rows=cypher(host.clone(),&store,"MATCH (n:A) SET n:Employee:Active WITH n MATCH (m:Employee:Active) RETURN count(m) AS total").unwrap();
        assert_eq!(rows[0].get("total"), Value::Long(1));
        let rows = cypher(
            host.clone(),
            &store,
            "MATCH (n:Employee:Active) RETURN count(n) AS total",
        )
        .unwrap();
        assert_eq!(rows[0].get("total"), Value::Long(1));
        cypher(host.clone(), &store, "MATCH (n:Employee) REMOVE n:Active").unwrap();
        let rows = cypher(
            host.clone(),
            &store,
            "MATCH (n:Active) RETURN count(n) AS total",
        )
        .unwrap();
        assert_eq!(rows[0].get("total"), Value::Long(0));
        cypher(host.clone(),&store,"UNWIND [1,1,2] AS x MERGE (n:N {k:x}) ON CREATE SET n.created=x ON MATCH SET n.hits=coalesce(n.hits,0)+1 RETURN n.k AS k").unwrap();
        let rows = cypher(
            host.clone(),
            &store,
            "MATCH (n:N) RETURN n.k AS k,n.hits AS hits ORDER BY k",
        )
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get("hits"), Value::Int(1));
        let snapshot = observe(&store.attach(host).unwrap(), None).unwrap();
        assert_eq!(snapshot.nodes.len(), 3);
        assert_eq!(snapshot.labels.len(), 3);
    }
    #[test]
    fn managed_cypher_delete_path_detach_and_failed_statement_are_atomic() {
        let host = host();
        let store = ManagedStore::new("managed_graph");
        store.create(host.as_ref()).unwrap();
        cypher(host.clone(), &store, "CREATE (a:A)-[:R]->(b:B)").unwrap();
        let before =
            serde_json::to_value(observe(&store.attach(host.clone()).unwrap(), None).unwrap())
                .unwrap();
        assert!(cypher(host.clone(), &store, "MATCH (a:A) DELETE a").is_err());
        assert_eq!(
            serde_json::to_value(observe(&store.attach(host.clone()).unwrap(), None).unwrap())
                .unwrap(),
            before
        );
        assert!(cypher(host.clone(), &store, "CREATE (n:Bad) SET n.invalid=[{x:1}]").is_err());
        assert_eq!(
            serde_json::to_value(observe(&store.attach(host.clone()).unwrap(), None).unwrap())
                .unwrap(),
            before
        );
        cypher(host.clone(), &store, "MATCH p=(a:A)-[:R]->(b:B) DELETE p").unwrap();
        let snapshot = observe(&store.attach(host.clone()).unwrap(), None).unwrap();
        assert!(snapshot.nodes.is_empty());
        assert!(snapshot.relationships.is_empty());
        cypher(host.clone(), &store, "CREATE (:A)-[:R]->(:B)").unwrap();
        cypher(host.clone(), &store, "MATCH (a:A) DETACH DELETE a").unwrap();
        let snapshot = observe(&store.attach(host).unwrap(), None).unwrap();
        assert_eq!(snapshot.nodes.len(), 1);
        assert!(snapshot.relationships.is_empty());
    }
    #[test]
    fn nested_mutation_publishes_native_hydration_with_overlay() {
        let host = host();
        let store = ManagedStore::new("managed_graph");
        store.create(host.as_ref()).unwrap();
        let fixture = import_graph(&serde_json::json!({"nodes":[{"id":42,"label":"person","properties":{"name":"alice","_partition":"a"}}],"edges":[]})).unwrap();
        store.persist(host.as_ref(), &fixture).unwrap();
        let parent = store.attach(host.clone()).unwrap();
        let node = Value::Node { label: "person".into(), id: parent.node_ids("person").unwrap()[0].clone() };
        // A correlated body is the first operation to hydrate this entity.
        let child = parent.clone();
        child.set_vertex_property(&node, "name", Value::String("bob".into()), Cardinality::Single, BTreeMap::new()).unwrap();
        parent.restore_execution_overlay(&child);
        let properties = parent.properties(&node, &["name".into()]);
        assert!(matches!(&properties[0], Value::VertexProperty { value, .. } if **value == Value::String("bob".into())));
        store.persist(host.as_ref(), &parent).unwrap();
        let reopened = store.attach(host).unwrap();
        let properties = reopened.properties(&node, &["name".into()]);
        assert!(matches!(&properties[0], Value::VertexProperty { value, .. } if **value == Value::String("bob".into())));
        let Value::Node { label, id } = node else { unreachable!() };
        assert_eq!(reopened.node_property(&label, id, "_partition"), Value::String("a".into()));
    }

    #[test]
    fn managed_records_preserve_identity_properties_labels_and_allocator() {
        let host = host();
        let store = ManagedStore::new("managed_graph");
        store.create(host.as_ref()).unwrap();
        let graph=import_graph(&serde_json::json!({"nodes":[{"id":42,"id_type":"Long","label":"person","properties":{},"property_records":[{"id":91,"id_type":"Long","key":"name","value":"one","meta":{"since":2020},"meta_types":{"since":"Integer"}},{"id":92,"id_type":"Long","key":"name","value":"two","meta":{}}]},{"id":"other","label":"other","properties":{"age":3}}],"edges":[{"id":7,"id_type":"Long","label":"knows","src":42,"dst":"other","properties":{"weight":0.5}}]})).unwrap();
        let person = graph
            .find_element_by_public_id(&Value::Long(42), false)
            .unwrap();
        graph
            .set_node_labels(&person, vec!["person".into(), "Employee".into()])
            .unwrap();
        store.persist(host.as_ref(), &graph).unwrap();
        let graph = store.attach(host.clone()).unwrap();
        let person = graph
            .find_element_by_public_id(&Value::Long(42), false)
            .unwrap();
        let Value::Node { label, id } = &person else {
            panic!()
        };
        assert_eq!(
            graph.node_labels(label, id.clone()),
            vec!["Employee", "person"]
        );
        let props = graph.properties(&person, &["name".into()]);
        assert_eq!(props.len(), 2);
        assert_eq!(graph.element_public_id(&props[0]), Value::Long(91));
        let meta = graph.properties(&props[0], &[]);
        assert_eq!(meta.len(), 1);
        let added = graph
            .set_vertex_property(
                &person,
                "name",
                Value::String("three".into()),
                Cardinality::List,
                BTreeMap::new(),
            )
            .unwrap();
        if let Value::VertexProperty { id, .. } = added {
            assert!(id >= 93);
        } else {
            panic!()
        }
        let snapshot = observe(&graph, None).unwrap();
        assert_eq!(snapshot.nodes.len(), 2);
        assert_eq!(snapshot.relationships.len(), 1);
        store.persist(host.as_ref(), &graph).unwrap();
        let restored = store.attach(host).unwrap();
        let person = restored
            .find_element_by_public_id(&Value::Long(42), false)
            .unwrap();
        assert_eq!(restored.properties(&person, &["name".into()]).len(), 3);
    }
    #[test]
    fn managed_updates_use_caller_transaction_and_retain_other_properties() {
        let host = host();
        let store = ManagedStore::new("managed_graph");
        store.create(host.as_ref()).unwrap();
        let graph = PropertyGraph::new();
        let node = graph.insert_node(
            "A",
            BTreeMap::from([
                ("x".into(), Value::Int(1)),
                ("y".into(), Value::String("keep".into())),
            ]),
        );
        store.persist(host.as_ref(), &graph).unwrap();
        host.execute(HostRequest::new("BEGIN TRANSACTION")).unwrap();
        let graph = store.attach(host.clone()).unwrap();
        graph.set_property(&node, "x", Value::Int(2)).unwrap();
        store.persist(host.as_ref(), &graph).unwrap();
        let changed = store.attach(host.clone()).unwrap();
        assert_eq!(changed.node_property("A", 0.into(), "x"), Value::Int(2));
        assert_eq!(
            changed.node_property("A", 0.into(), "y"),
            Value::String("keep".into())
        );
        host.execute(HostRequest::new("ROLLBACK")).unwrap();
        let restored = store.attach(host.clone()).unwrap();
        assert_eq!(restored.node_property("A", 0.into(), "x"), Value::Int(1));
        let b = restored.insert_node("B", BTreeMap::new());
        let c = restored.insert_node("C", BTreeMap::new());
        restored
            .insert_edge("R", &node, &b, BTreeMap::new())
            .unwrap();
        restored.insert_edge("R", &b, &c, BTreeMap::new()).unwrap();
        store.persist(host.as_ref(), &restored).unwrap();
        let restored = store.attach(host.clone()).unwrap();
        assert_eq!(restored.edge_ids("R").len(), 2);
        let d = restored.insert_node("D", BTreeMap::new());
        restored.insert_edge("R", &c, &d, BTreeMap::new()).unwrap();
        store.persist(host.as_ref(), &restored).unwrap();
        assert_eq!(store.attach(host).unwrap().edge_ids("R").len(), 3);
    }
}
