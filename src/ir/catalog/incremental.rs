//! Incremental overlay record codec for [`PropertyGraph`].
//!
//! Unlike [`super::snapshot`], which serializes the whole graph (base Arrow
//! tables plus the entire session overlay), this module captures only the
//! overlay state of a caller-supplied set of *touched* entities. Each record
//! is a self-contained, checksummed, versioned blob that can be applied over
//! an unchanged base-table checkpoint to reconstruct exactly the targeted
//! overlay entries and per-label / per-rel-type metadata.
//!
//! Four record kinds exist:
//! * `1` — node overlay record (`name` = label, `id` = node id)
//! * `2` — edge overlay record (`name` = rel type, `id` = edge row)
//! * `3` — per-touched-node-label metadata (`name` = label, `id` = 0)
//! * `4` — per-touched-edge-type metadata (`name` = rel type, `id` = 0)
//!
//! Each entity record carries the current inserted properties/endpoints,
//! override properties, and the deleted/replaced flags *including their
//! absence*, so applying a record replaces (rather than merges) the targeted
//! overlay entry. Metadata records carry insert allocation counters and the
//! ordered inserted/override property keys.
//!
//! Record payloads are `Value::List` structures of `[tag, value]` pairs,
//! encoded with the snapshot value codec (bit-exact for every `Value`
//! variant, including NaN payloads). The payload is prefixed by a version
//! and a SHA-256 checksum over `version ++ kind ++ name ++ id ++ body` so
//! corrupted or swapped records are rejected.

use std::collections::{BTreeMap, BTreeSet};

use sha2::{Digest, Sha256};

use super::snapshot::{decode_value_bytes, encode_value_bytes};
use super::{GraphOverlay, InsertedEdge, PropertyGraph};
use crate::ir::value::Value;

/// Wire-format version for incremental records.
const VERSION: u32 = 1;
/// Length in bytes of the per-record SHA-256 checksum.
const CHECKSUM_LEN: usize = 32;
/// Fixed header size: 4-byte version + 32-byte checksum.
const HEADER_LEN: usize = 4 + CHECKSUM_LEN;

// Record kinds.
const KIND_NODE: i32 = 1;
const KIND_EDGE: i32 = 2;
const KIND_NODE_META: i32 = 3;
const KIND_EDGE_META: i32 = 4;

// Entity-record field tags (node and edge records share these).
const T_INSERTED: i64 = 1;
const T_OVERRIDES: i64 = 2;
const T_DELETED: i64 = 3;
const T_REPLACED: i64 = 4;
const T_OUT_POSITION: i64 = 5;
const T_IN_POSITION: i64 = 6;

// Metadata-record field tags.
const T_COUNT: i64 = 1;
const T_INSERTED_KEYS: i64 = 2;
const T_OVERRIDE_KEYS: i64 = 3;

const T_NATIVE: i64 = 7;
const T_NULL_VALUES: i64 = 9;
const T_LABELS: i64 = 10;
const T_CYPHER_ID: i64 = 11;
const ENTITY_TAGS: [i64; 8] = [T_INSERTED, T_OVERRIDES, T_DELETED, T_REPLACED,T_NATIVE,T_NULL_VALUES,T_LABELS,T_CYPHER_ID];
const T_NULL_PROPERTIES: i64 = 8;
const EDGE_TAGS: [i64; 10] = [
    T_CYPHER_ID,
    T_NULL_VALUES,
    T_NULL_PROPERTIES,
    T_NATIVE,
    T_INSERTED,
    T_OVERRIDES,
    T_DELETED,
    T_REPLACED,
    T_OUT_POSITION,
    T_IN_POSITION,
];
const META_TAGS: [i64; 3] = [T_COUNT, T_INSERTED_KEYS, T_OVERRIDE_KEYS];

/// A single self-contained overlay delta record.
#[derive(Debug, Clone)]
pub(crate) struct IncrementalRecord {
    pub kind: i32,
    pub name: String,
    pub id: i64,
    pub payload: Vec<u8>,
}

/// Build a `[tag, value]` field for a record payload list.
fn field(tag: i64, value: Value) -> Value {
    Value::List(vec![Value::Int(tag), value])
}

/// Compute the SHA-256 checksum binding a record to its identity.
///
/// The checksum covers the version, kind, name and id so that records whose
/// payloads have been swapped, or whose identity fields have been altered,
/// fail validation.
fn record_checksum(kind: i32, name: &str, id: i64, body: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(VERSION.to_le_bytes());
    hasher.update(kind.to_le_bytes());
    hasher.update((name.len() as u64).to_le_bytes());
    hasher.update(name.as_bytes());
    hasher.update(id.to_le_bytes());
    hasher.update(body);
    hasher.finalize().into()
}

fn encode_record(kind: i32, name: &str, id: i64, body: &Value) -> IncrementalRecord {
    let body_bytes = encode_value_bytes(body);
    let checksum = record_checksum(kind, name, id, &body_bytes);
    let mut payload = Vec::with_capacity(HEADER_LEN + body_bytes.len());
    payload.extend_from_slice(&VERSION.to_le_bytes());
    payload.extend_from_slice(&checksum);
    payload.extend_from_slice(&body_bytes);
    IncrementalRecord {
        kind,
        name: name.to_string(),
        id,
        payload,
    }
}

/// Decode and verify a record payload, returning the `Value::List` body.
fn decode_body(record: &IncrementalRecord) -> Result<Value, String> {
    if record.payload.len() < HEADER_LEN {
        return Err("incremental record payload too short".to_string());
    }
    let version = u32::from_le_bytes(record.payload[0..4].try_into().unwrap());
    if version != VERSION {
        return Err(format!("unsupported incremental record version {version}"));
    }
    let stored: [u8; CHECKSUM_LEN] = record.payload[4..HEADER_LEN].try_into().unwrap();
    let body = &record.payload[HEADER_LEN..];
    let computed = record_checksum(record.kind, &record.name, record.id, body);
    if stored != computed {
        return Err("incremental record checksum mismatch".to_string());
    }
    decode_value_bytes(body)
}

/// Parse a record payload list into its `[tag, value]` fields, rejecting
/// unknown tags, malformed entries and duplicates.
fn decode_fields<'a>(body: &'a Value, allowed: &[i64]) -> Result<BTreeMap<i64, &'a Value>, String> {
    let Value::List(items) = body else {
        return Err("incremental record payload must be a list".to_string());
    };
    let mut fields = BTreeMap::new();
    for item in items {
        let Value::List(pair) = item else {
            return Err("incremental record field must be a tag/value pair".to_string());
        };
        if pair.len() != 2 {
            return Err("incremental record field must have exactly two elements".to_string());
        }
        let Value::Int(tag) = &pair[0] else {
            return Err("incremental record field tag must be an integer".to_string());
        };
        if !allowed.contains(tag) {
            return Err(format!("unknown incremental record field tag {tag}"));
        }
        if fields.insert(*tag, &pair[1]).is_some() {
            return Err(format!("duplicate incremental record field tag {tag}"));
        }
    }
    Ok(fields)
}

fn decode_props(value: &Value) -> Result<BTreeMap<String, Value>, String> {
    match value {
        Value::Map(map) => Ok(map.clone()),
        other => Err(format!(
            "incremental record expected a property map, got {}",
            other.type_name()
        )),
    }
}

fn decode_flag(value: &Value) -> Result<bool, String> {
    match value {
        Value::Bool(flag) => Ok(*flag),
        other => Err(format!(
            "incremental record expected a flag, got {}",
            other.type_name()
        )),
    }
}

fn decode_count(value: &Value) -> Result<i64, String> {
    match value {
        Value::Int(count) if *count >= 0 => Ok(*count),
        Value::Int(count) => Err(format!("incremental record has negative count {count}")),
        other => Err(format!(
            "incremental record expected an allocation count, got {}",
            other.type_name()
        )),
    }
}

fn decode_key_list(value: &Value) -> Result<Vec<String>, String> {
    let Value::List(items) = value else {
        return Err("incremental record expected a key list".to_string());
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::String(key) => out.push(key.clone()),
            other => {
                return Err(format!(
                    "incremental record expected a string key, got {}",
                    other.type_name()
                ));
            }
        }
    }
    Ok(out)
}

fn decode_inserted_edge(value: &Value) -> Result<InsertedEdge, String> {
    let Value::List(items) = value else {
        return Err("incremental record inserted edge must be a list".to_string());
    };
    if items.len() != 5 {
        return Err("incremental record inserted edge must have five elements".to_string());
    }
    let string_at = |item: &Value, what: &str| match item {
        Value::String(s) => Ok(s.clone()),
        other => Err(format!(
            "incremental record {what} must be a string, got {}",
            other.type_name()
        )),
    };
    let id_at = |item: &Value, what: &str| match item {
        Value::Int(v) if *v >= 0 => Ok(*v),
        Value::Int(v) => Err(format!("incremental record {what} is negative ({v})")),
        other => Err(format!(
            "incremental record {what} must be an integer, got {}",
            other.type_name()
        )),
    };
    Ok(InsertedEdge {
        src_label: string_at(&items[0], "src_label")?,
        src_id: id_at(&items[1], "src_id")?,
        dst_label: string_at(&items[2], "dst_label")?,
        dst_id: id_at(&items[3], "dst_id")?,
        properties: decode_props(&items[4])?,
    })
}

/// Rebuild the inserted adjacency caches from `inserted_edges`. Deleted
/// inserted edges are still indexed; reads filter them via `edge_is_live`,
/// matching the behaviour of `insert_edge`.
type EdgePositions = BTreeMap<(String, i64), usize>;

fn adjacency_positions(ov: &GraphOverlay) -> (EdgePositions, EdgePositions) {
    let positions = |adj: &std::collections::HashMap<(String, i64), Vec<(String, i64)>>| {
        adj.values()
            .flat_map(|edges| edges.iter().enumerate().map(|(i, edge)| (edge.clone(), i)))
            .collect()
    };
    (
        positions(&ov.inserted_out_adj),
        positions(&ov.inserted_in_adj),
    )
}

fn rebuild_inserted_adjacency(
    ov: &mut GraphOverlay,
    outgoing: &EdgePositions,
    incoming: &EdgePositions,
) {
    ov.inserted_out_adj.clear();
    ov.inserted_in_adj.clear();
    for ((rel_type, id), edge) in &ov.inserted_edges {
        ov.inserted_out_adj
            .entry((edge.src_label.clone(), edge.src_id))
            .or_default()
            .push((rel_type.clone(), *id));
        ov.inserted_in_adj
            .entry((edge.dst_label.clone(), edge.dst_id))
            .or_default()
            .push((rel_type.clone(), *id));
    }
    for edges in ov.inserted_out_adj.values_mut() {
        edges.sort_by_key(|edge| outgoing.get(edge).copied().unwrap_or(usize::MAX));
    }
    for edges in ov.inserted_in_adj.values_mut() {
        edges.sort_by_key(|edge| incoming.get(edge).copied().unwrap_or(usize::MAX));
    }
}

impl PropertyGraph {
    /// Serialize overlay records for the given `nodes` and `edges` (each a
    /// set of `(name, id)` pairs). Only the touched entities and the touched
    /// labels / rel-types are encoded; the base Arrow tables are never read.
    pub(crate) fn incremental_records(
        &self,
        nodes: &BTreeSet<(String, i64)>,
        edges: &BTreeSet<(String, i64)>,
    ) -> Result<Vec<IncrementalRecord>, String> {
        let ov = self.overlay.borrow();
        let mut records = Vec::new();

        for key in nodes {
            if key.1 < 0 {
                return Err(format!(
                    "touched node `{}` has negative id {}",
                    key.0, key.1
                ));
            }
            let body = encode_node_body(key, &ov);
            records.push(encode_record(KIND_NODE, &key.0, key.1, &Value::List(body)));
        }

        for key in edges {
            if key.1 < 0 {
                return Err(format!(
                    "touched edge `{}` has negative id {}",
                    key.0, key.1
                ));
            }
            let body = encode_edge_body(key, &ov);
            records.push(encode_record(KIND_EDGE, &key.0, key.1, &Value::List(body)));
        }

        let mut labels: BTreeSet<&str> = BTreeSet::new();
        for (label, _) in nodes {
            labels.insert(label.as_str());
        }
        for label in labels {
            let body = encode_node_meta_body(label, &ov);
            records.push(encode_record(KIND_NODE_META, label, 0, &Value::List(body)));
        }

        let mut rel_types: BTreeSet<&str> = BTreeSet::new();
        for (rel_type, _) in edges {
            rel_types.insert(rel_type.as_str());
        }
        for rel_type in rel_types {
            let body = encode_edge_meta_body(rel_type, &ov);
            records.push(encode_record(
                KIND_EDGE_META,
                rel_type,
                0,
                &Value::List(body),
            ));
        }

        Ok(records)
    }

    /// Apply a set of incremental records, replacing the targeted overlay
    /// entries and metadata. The base Arrow tables and derived caches are
    /// left untouched; the inserted adjacency caches are rebuilt once at the
    /// end.
    pub(crate) fn apply_incremental_records(
        &mut self,
        records: &[IncrementalRecord],
    ) -> Result<(), String> {
        let mut ov = self.overlay.borrow_mut();
        let (mut outgoing, mut incoming) = adjacency_positions(&ov);
        for record in records {
            apply_one(&mut ov, record)?;
            if record.kind == KIND_EDGE {
                let body = decode_body(record)?;
                let fields = decode_fields(&body, &EDGE_TAGS)?;
                if fields.contains_key(&T_INSERTED) {
                    for (tag, positions) in [
                        (T_OUT_POSITION, &mut outgoing),
                        (T_IN_POSITION, &mut incoming),
                    ] {
                        let value = fields
                            .get(&tag)
                            .ok_or("inserted edge is missing adjacency order")?;
                        let position =
                            usize::try_from(decode_count(value)?).map_err(|e| e.to_string())?;
                        positions.insert((record.name.clone(), record.id), position);
                    }
                }
            }
        }
        rebuild_inserted_adjacency(&mut ov, &outgoing, &incoming);
        ov.rebuild_public_id_lookup();
        Ok(())
    }
}

fn encode_node_body(key: &(String, i64), ov: &GraphOverlay) -> Vec<Value> {
    let mut fields = vec![field(T_NATIVE, ov.native_node_state(key)), field(T_NULL_VALUES, Value::Bool(ov.allow_null_property_values))];
    if let Some(id) = ov.cypher_ids.get(&(false, key.0.clone(), key.1)) { fields.push(field(T_CYPHER_ID, Value::Long(*id))); }
    if let Some(labels) = ov.node_label_sets.get(key) {
        fields.push(field(T_LABELS, Value::List(labels.iter().cloned().map(Value::String).collect())));
    }
    if let Some(props) = ov.inserted_nodes.get(key) {
        fields.push(field(T_INSERTED, Value::Map(props.clone())));
    }
    if let Some(props) = ov.node_property_overrides.get(key) {
        fields.push(field(T_OVERRIDES, Value::Map(props.clone())));
    }
    if ov.deleted_nodes.contains(key) {
        fields.push(field(T_DELETED, Value::Bool(true)));
    }
    if ov.replaced_node_properties.contains(key) {
        fields.push(field(T_REPLACED, Value::Bool(true)));
    }
    fields
}

fn encode_edge_body(key: &(String, i64), ov: &GraphOverlay) -> Vec<Value> {
    let mut fields = vec![field(T_NATIVE, ov.public_ids.get(&(true,key.0.clone(),key.1)).cloned().unwrap_or(Value::Null))];
    if let Some(id) = ov.cypher_ids.get(&(true, key.0.clone(), key.1)) { fields.push(field(T_CYPHER_ID, Value::Long(*id))); }
    fields.push(field(T_NULL_VALUES, Value::Bool(ov.allow_null_property_values)));
    if let Some(keys) = ov.edge_null_properties.get(key) {
        fields.push(field(T_NULL_PROPERTIES, Value::List(keys.iter().cloned().map(Value::String).collect())));
    }
    if let Some(edge) = ov.inserted_edges.get(key) {
        let inserted = Value::List(vec![
            Value::String(edge.src_label.clone()),
            Value::Int(edge.src_id),
            Value::String(edge.dst_label.clone()),
            Value::Int(edge.dst_id),
            Value::Map(edge.properties.clone()),
        ]);
        fields.push(field(T_INSERTED, inserted));
        for (tag, adjacent) in [
            (
                T_OUT_POSITION,
                ov.inserted_out_adj
                    .get(&(edge.src_label.clone(), edge.src_id)),
            ),
            (
                T_IN_POSITION,
                ov.inserted_in_adj
                    .get(&(edge.dst_label.clone(), edge.dst_id)),
            ),
        ] {
            if let Some(position) =
                adjacent.and_then(|edges| edges.iter().position(|candidate| candidate == key))
            {
                fields.push(field(tag, Value::Int(position as i64)));
            }
        }
    }
    if let Some(props) = ov.edge_property_overrides.get(key) {
        fields.push(field(T_OVERRIDES, Value::Map(props.clone())));
    }
    if ov.deleted_edges.contains(key) {
        fields.push(field(T_DELETED, Value::Bool(true)));
    }
    if ov.replaced_edge_properties.contains(key) {
        fields.push(field(T_REPLACED, Value::Bool(true)));
    }
    fields
}

fn key_list_field(tag: i64, list: &[String]) -> Value {
    field(
        tag,
        Value::List(list.iter().cloned().map(Value::String).collect()),
    )
}

fn encode_node_meta_body(label: &str, ov: &GraphOverlay) -> Vec<Value> {
    let mut fields = Vec::new();
    if let Some(count) = ov.inserted_node_counts.get(label) {
        fields.push(field(T_COUNT, Value::Int(*count)));
    }
    if let Some(keys) = ov.inserted_node_keys.get(label) {
        fields.push(key_list_field(T_INSERTED_KEYS, keys));
    }
    if let Some(keys) = ov.override_node_keys.get(label) {
        fields.push(key_list_field(T_OVERRIDE_KEYS, keys));
    }
    fields
}

fn encode_edge_meta_body(rel_type: &str, ov: &GraphOverlay) -> Vec<Value> {
    let mut fields = Vec::new();
    if let Some(count) = ov.inserted_edge_counts.get(rel_type) {
        fields.push(field(T_COUNT, Value::Int(*count)));
    }
    if let Some(keys) = ov.inserted_edge_keys.get(rel_type) {
        fields.push(key_list_field(T_INSERTED_KEYS, keys));
    }
    if let Some(keys) = ov.override_edge_keys.get(rel_type) {
        fields.push(key_list_field(T_OVERRIDE_KEYS, keys));
    }
    fields
}

fn apply_one(ov: &mut GraphOverlay, record: &IncrementalRecord) -> Result<(), String> {
    match record.kind {
        KIND_NODE => {
            if record.id < 0 {
                return Err(format!(
                    "node overlay record `{}` has negative id {}",
                    record.name, record.id
                ));
            }
            apply_node(ov, record)
        }
        KIND_EDGE => {
            if record.id < 0 {
                return Err(format!(
                    "edge overlay record `{}` has negative id {}",
                    record.name, record.id
                ));
            }
            apply_edge(ov, record)
        }
        KIND_NODE_META => {
            if record.id != 0 {
                return Err(format!(
                    "node metadata record `{}` must have id 0, got {}",
                    record.name, record.id
                ));
            }
            apply_node_meta(ov, record)
        }
        KIND_EDGE_META => {
            if record.id != 0 {
                return Err(format!(
                    "edge metadata record `{}` must have id 0, got {}",
                    record.name, record.id
                ));
            }
            apply_edge_meta(ov, record)
        }
        other => Err(format!("unknown incremental record kind {other}")),
    }
}

fn apply_node(ov: &mut GraphOverlay, record: &IncrementalRecord) -> Result<(), String> {
    let body = decode_body(record)?;
    let fields = decode_fields(&body, &ENTITY_TAGS)?;
    let key = (record.name.clone(), record.id);
    if let Some(value) = fields.get(&T_CYPHER_ID) {
        let Value::Long(id) = value else { return Err("Invalid Cypher identity".into()); };
        ov.cypher_ids.insert((false, key.0.clone(), key.1), *id);
    }
    if let Some(value) = fields.get(&T_NULL_VALUES) {
        ov.allow_null_property_values = decode_flag(value)?;
    }

    if let Some(state) = fields.get(&T_NATIVE) {ov.restore_native_node_state(key.clone(),state)?;}
    ov.node_label_sets.remove(&key);
    if let Some(value) = fields.get(&T_LABELS) {
        let Value::List(labels) = value else { return Err("Invalid node label set".into()); };
        let labels = labels.iter().map(|v| match v {
            Value::String(s) => Ok(s.clone()), _ => Err("Invalid node label".to_string())
        }).collect::<Result<_, _>>()?;
        ov.node_label_sets.insert(key.clone(), labels);
    }
    ov.inserted_nodes.remove(&key);
    ov.node_property_overrides.remove(&key);
    ov.deleted_nodes.remove(&key);
    ov.replaced_node_properties.remove(&key);

    if let Some(value) = fields.get(&T_INSERTED) {
        ov.inserted_nodes.insert(key.clone(), decode_props(value)?);
    }
    if let Some(value) = fields.get(&T_OVERRIDES) {
        ov.node_property_overrides
            .insert(key.clone(), decode_props(value)?);
    }
    if let Some(value) = fields.get(&T_DELETED) {
        if decode_flag(value)? {
            ov.deleted_nodes.insert(key.clone());
        }
    }
    if let Some(value) = fields.get(&T_REPLACED) {
        if decode_flag(value)? {
            ov.replaced_node_properties.insert(key);
        }
    }
    Ok(())
}

fn apply_edge(ov: &mut GraphOverlay, record: &IncrementalRecord) -> Result<(), String> {
    let body = decode_body(record)?;
    let fields = decode_fields(&body, &EDGE_TAGS)?;
    let key = (record.name.clone(), record.id);
    if let Some(value) = fields.get(&T_CYPHER_ID) {
        let Value::Long(id) = value else { return Err("Invalid Cypher identity".into()); };
        ov.cypher_ids.insert((true, key.0.clone(), key.1), *id);
    }
    if let Some(value) = fields.get(&T_NULL_VALUES) {
        ov.allow_null_property_values = decode_flag(value)?;
    }

    ov.public_ids.remove(&(true,key.0.clone(),key.1));
    if let Some(value) = fields.get(&T_NATIVE).filter(|v| ***v != Value::Null) {ov.public_ids.insert((true,key.0.clone(),key.1),(*value).clone());}
    ov.edge_null_properties.remove(&key);
    if let Some(value) = fields.get(&T_NULL_PROPERTIES) {
        let Value::List(keys) = value else { return Err("Invalid null property keys".into()); };
        let keys = keys.iter().map(|key| match key {
            Value::String(key) => Ok(key.clone()),
            _ => Err("Invalid null property key".to_string()),
        }).collect::<Result<_, _>>()?;
        ov.edge_null_properties.insert(key.clone(), keys);
    }
    ov.inserted_edges.remove(&key);
    ov.edge_property_overrides.remove(&key);
    ov.deleted_edges.remove(&key);
    ov.replaced_edge_properties.remove(&key);

    if let Some(value) = fields.get(&T_INSERTED) {
        ov.inserted_edges
            .insert(key.clone(), decode_inserted_edge(value)?);
    }
    if let Some(value) = fields.get(&T_OVERRIDES) {
        ov.edge_property_overrides
            .insert(key.clone(), decode_props(value)?);
    }
    if let Some(value) = fields.get(&T_DELETED) {
        if decode_flag(value)? {
            ov.deleted_edges.insert(key.clone());
        }
    }
    if let Some(value) = fields.get(&T_REPLACED) {
        if decode_flag(value)? {
            ov.replaced_edge_properties.insert(key);
        }
    }
    Ok(())
}

fn apply_node_meta(ov: &mut GraphOverlay, record: &IncrementalRecord) -> Result<(), String> {
    let body = decode_body(record)?;
    let fields = decode_fields(&body, &META_TAGS)?;
    let label = record.name.clone();

    ov.inserted_node_counts.remove(&label);
    ov.inserted_node_keys.remove(&label);
    ov.override_node_keys.remove(&label);

    if let Some(value) = fields.get(&T_COUNT) {
        ov.inserted_node_counts
            .insert(label.clone(), decode_count(value)?);
    }
    if let Some(value) = fields.get(&T_INSERTED_KEYS) {
        ov.inserted_node_keys
            .insert(label.clone(), decode_key_list(value)?);
    }
    if let Some(value) = fields.get(&T_OVERRIDE_KEYS) {
        ov.override_node_keys
            .insert(label.clone(), decode_key_list(value)?);
    }
    Ok(())
}

fn apply_edge_meta(ov: &mut GraphOverlay, record: &IncrementalRecord) -> Result<(), String> {
    let body = decode_body(record)?;
    let fields = decode_fields(&body, &META_TAGS)?;
    let rel_type = record.name.clone();

    ov.inserted_edge_counts.remove(&rel_type);
    ov.inserted_edge_keys.remove(&rel_type);
    ov.override_edge_keys.remove(&rel_type);

    if let Some(value) = fields.get(&T_COUNT) {
        ov.inserted_edge_counts
            .insert(rel_type.clone(), decode_count(value)?);
    }
    if let Some(value) = fields.get(&T_INSERTED_KEYS) {
        ov.inserted_edge_keys
            .insert(rel_type.clone(), decode_key_list(value)?);
    }
    if let Some(value) = fields.get(&T_OVERRIDE_KEYS) {
        ov.override_edge_keys
            .insert(rel_type.clone(), decode_key_list(value)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::sync::Arc;

    use arrow::array::{ArrayRef, Int64Array, StringArray};

    use crate::ir::catalog::PropertyGraph;
    use crate::ir::value::Value;
    use crate::ir::{edges_from_columns, nodes_from_columns};

    fn base_graph() -> PropertyGraph {
        let mut g = PropertyGraph::new();
        g.add_nodes(nodes_from_columns(
            "P",
            vec![
                (
                    "name",
                    Arc::new(StringArray::from(vec!["a", "b", "c"])) as ArrayRef,
                ),
                ("age", Arc::new(Int64Array::from(vec![1, 2, 3])) as ArrayRef),
            ],
        ));
        g.add_edges(edges_from_columns(
            "LIKES",
            "P",
            "P",
            vec![0, 1],
            vec![1, 2],
            vec![],
        ))
        .unwrap();
        g
    }

    fn touched_sets(g: &PropertyGraph) -> (BTreeSet<(String, i64)>, BTreeSet<(String, i64)>) {
        let ov = g.overlay.borrow();
        let mut nodes = BTreeSet::new();
        nodes.extend(ov.inserted_nodes.keys().cloned());
        nodes.extend(ov.node_property_overrides.keys().cloned());
        nodes.extend(ov.deleted_nodes.iter().cloned());
        nodes.extend(ov.replaced_node_properties.iter().cloned());
        let mut edges = BTreeSet::new();
        edges.extend(ov.inserted_edges.keys().cloned());
        edges.extend(ov.edge_property_overrides.keys().cloned());
        edges.extend(ov.deleted_edges.iter().cloned());
        edges.extend(ov.replaced_edge_properties.iter().cloned());
        (nodes, edges)
    }

    fn node(label: &str, id: i64) -> Value {
        Value::Node {
            label: label.to_string(),
            id,
        }
    }

    #[test]
    fn apply_restores_overlay_over_checkpoint() {
        let g = base_graph();

        // Inserted node (id 3) and edge (id 2).
        assert_eq!(g.insert_node("P", BTreeMap::new()), node("P", 3));
        g.set_property(&node("P", 3), "name", Value::String("d".into()))
            .unwrap();
        let e = g
            .insert_edge(
                "LIKES",
                &node("P", 3),
                &node("P", 0),
                BTreeMap::from([("since".to_string(), Value::Int(2020))]),
            )
            .unwrap();
        assert!(matches!(e, Value::Edge { id: 2, .. }));

        // Property override on base node 0.
        g.set_property(&node("P", 0), "age", Value::Int(99))
            .unwrap();
        // Whole-bag replacement on base node 1 (drops `age`).
        g.set_properties(
            &node("P", 1),
            BTreeMap::from([("name".to_string(), Value::String("B".into()))]),
            true,
        )
        .unwrap();
        // Detach-delete node 2, which removes its incident edge (row 1).
        g.delete_value(&node("P", 2), true).unwrap();

        let (nodes, edges) = touched_sets(&g);
        let records = g.incremental_records(&nodes, &edges).unwrap();

        let mut restored = base_graph();
        restored.apply_incremental_records(&records).unwrap();

        // Inserted node/edge restored over the checkpoint.
        assert_eq!(restored.node_ids("P").unwrap(), vec![0i64, 1, 3]);
        assert_eq!(
            restored.node_property("P", 3, "name"),
            Value::String("d".into())
        );
        assert_eq!(restored.edge_ids("LIKES"), vec![0i64, 2]);
        assert_eq!(
            restored.edge_endpoints("LIKES", 2),
            Some(("P".to_string(), 3, "P".to_string(), 0))
        );
        assert_eq!(
            restored.edge_property("LIKES", 2, "since"),
            Value::Int(2020)
        );

        // Property override.
        assert_eq!(restored.node_property("P", 0, "age"), Value::Int(99));
        assert_eq!(
            restored.node_property("P", 0, "name"),
            Value::String("a".into())
        );

        // Whole-bag replacement.
        assert_eq!(
            restored.node_property("P", 1, "name"),
            Value::String("B".into())
        );
        assert_eq!(restored.node_property("P", 1, "age"), Value::Null);

        // Deletion.
        assert_eq!(restored.node_property("P", 2, "name"), Value::Null);
        assert!(restored.out_edges("P", 3, &[]).contains(&(
            "LIKES".to_string(),
            2,
            "P".to_string(),
            0
        )));
        assert!(restored.in_edges("P", 0, &[]).contains(&(
            "LIKES".to_string(),
            2,
            "P".to_string(),
            3
        )));

        // Counters survive the checkpoint (id gaps preserved: node 2 deleted,
        // node 3 inserted -> next node 4, next edge 3).
        assert_eq!(restored.insert_node("P", BTreeMap::new()), node("P", 4));
        let e3 = restored
            .insert_edge("LIKES", &node("P", 4), &node("P", 0), BTreeMap::new())
            .unwrap();
        assert!(matches!(e3, Value::Edge { id: 3, .. }));
    }

    #[test]
    fn apply_preserves_inserted_and_override_key_metadata() {
        let mut g = PropertyGraph::new();
        g.add_nodes(nodes_from_columns(
            "P",
            vec![("x", Arc::new(Int64Array::from(vec![1])) as ArrayRef)],
        ));
        g.insert_node("P", BTreeMap::from([("a".to_string(), Value::Int(1))]));
        g.set_property(&node("P", 0), "b", Value::Int(2)).unwrap();

        let (nodes, edges) = touched_sets(&g);
        let records = g.incremental_records(&nodes, &edges).unwrap();

        let mut restored = PropertyGraph::new();
        restored.add_nodes(nodes_from_columns(
            "P",
            vec![("x", Arc::new(Int64Array::from(vec![1])) as ArrayRef)],
        ));
        restored.apply_incremental_records(&records).unwrap();

        assert_eq!(
            restored.node_property_keys("P"),
            vec!["x".to_string(), "a".to_string(), "b".to_string()]
        );
        // Counter preserved: next node is id 2 (base 1 row + 1 insert).
        assert_eq!(restored.insert_node("P", BTreeMap::new()), node("P", 2));
    }

    #[test]
    fn incremental_refresh_preserves_cross_type_adjacency_order() {
        let graph = PropertyGraph::new();
        let a = graph.insert_node("P", BTreeMap::new());
        let b = graph.insert_node("P", BTreeMap::new());
        graph.insert_edge("Z", &a, &b, BTreeMap::new()).unwrap();
        let checkpoint = graph.snapshot_encode().unwrap();
        graph.insert_edge("A", &a, &b, BTreeMap::new()).unwrap();
        graph.insert_edge("Z", &a, &b, BTreeMap::new()).unwrap();
        let (nodes, edges) = touched_sets(&graph);
        let records = graph.incremental_records(&nodes, &edges).unwrap();
        let mut restored = PropertyGraph::snapshot_decode(&checkpoint).unwrap();
        restored.apply_incremental_records(&records).unwrap();
        assert_eq!(
            graph.out_edges("P", 0, &[]),
            restored.out_edges("P", 0, &[])
        );
        assert_eq!(graph.in_edges("P", 1, &[]), restored.in_edges("P", 1, &[]));
    }

    #[test]
    fn corrupted_record_is_rejected() {
        let g = base_graph();
        g.insert_node("P", BTreeMap::new());
        let (nodes, edges) = touched_sets(&g);
        let records = g.incremental_records(&nodes, &edges).unwrap();

        // Flip a checksum byte: structure is intact but the checksum must
        // no longer match.
        let mut corrupted = records.clone();
        corrupted[0].payload[4] ^= 0xFF;

        let mut restored = base_graph();
        assert!(restored.apply_incremental_records(&corrupted).is_err());
    }

    #[test]
    fn swapped_payloads_are_rejected() {
        let g = base_graph();
        g.insert_node("P", BTreeMap::from([("a".to_string(), Value::Int(1))]));
        g.insert_node("P", BTreeMap::from([("b".to_string(), Value::Int(2))]));
        let (nodes, edges) = touched_sets(&g);
        let records = g.incremental_records(&nodes, &edges).unwrap();

        // The first two records are the two node overlay records (ids 3, 4).
        assert!(records.len() >= 2);
        assert_eq!(records[0].kind, 1);
        assert_eq!(records[1].kind, 1);

        let mut swapped = records.clone();
        let payload0 = swapped[0].payload.clone();
        swapped[0].payload = swapped[1].payload.clone();
        swapped[1].payload = payload0;

        let mut restored = base_graph();
        assert!(restored.apply_incremental_records(&swapped).is_err());
    }

    #[test]
    fn unknown_kind_and_negative_id_are_rejected() {
        let g = base_graph();
        g.insert_node("P", BTreeMap::new());
        let (nodes, edges) = touched_sets(&g);
        let records = g.incremental_records(&nodes, &edges).unwrap();

        let mut unknown_kind = records[0].clone();
        unknown_kind.kind = 99;
        let mut restored = base_graph();
        assert!(restored.apply_incremental_records(&[unknown_kind]).is_err());

        let mut negative_id = records[0].clone();
        negative_id.id = -1;
        assert!(restored.apply_incremental_records(&[negative_id]).is_err());
    }

    #[test]
    fn nan_and_value_variants_round_trip_exactly() {
        let mut g = PropertyGraph::new();
        g.add_nodes(nodes_from_columns(
            "P",
            vec![("x", Arc::new(Int64Array::from(vec![1])) as ArrayRef)],
        ));
        let nan = f64::NAN;
        g.insert_node(
            "P",
            BTreeMap::from([
                ("f".to_string(), Value::Float(nan)),
                (
                    "list".to_string(),
                    Value::List(vec![Value::Float(nan), Value::Int(7)]),
                ),
                ("neg".to_string(), Value::Float(-0.0)),
            ]),
        );

        let (nodes, edges) = touched_sets(&g);
        let records = g.incremental_records(&nodes, &edges).unwrap();

        let mut restored = PropertyGraph::new();
        restored.add_nodes(nodes_from_columns(
            "P",
            vec![("x", Arc::new(Int64Array::from(vec![1])) as ArrayRef)],
        ));
        restored.apply_incremental_records(&records).unwrap();

        match restored.node_property("P", 1, "f") {
            Value::Float(v) => assert!(v.is_nan()),
            other => panic!("expected Float NaN, got {other:?}"),
        }
        match restored.node_property("P", 1, "list") {
            Value::List(items) => match &items[0] {
                Value::Float(v) => assert!(v.is_nan()),
                other => panic!("expected Float NaN in list, got {other:?}"),
            },
            other => panic!("expected List, got {other:?}"),
        }
        match restored.node_property("P", 1, "neg") {
            Value::Float(v) => assert_eq!(v.to_bits(), (-0.0f64).to_bits()),
            other => panic!("expected Float, got {other:?}"),
        }
    }
}
