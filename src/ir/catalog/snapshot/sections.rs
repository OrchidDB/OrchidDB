//! Paired graph-section encoders and decoders for snapshot layout changes.

use super::*;

// ---------------- section builders ----------------

pub(super) fn encode_nodes(nodes: &HashMap<String, NodeTable>) -> Result<Vec<u8>, String> {
    let mut b = Vec::new();
    let mut labels: Vec<&String> = nodes.keys().collect();
    labels.sort();
    put_u64(&mut b, labels.len() as u64);
    for label in labels {
        let table = &nodes[label];
        put_str(&mut b, label);
        put_str(&mut b, &table.label);
        put_bytes(&mut b, &encode_batch(&table.batch)?);
    }
    Ok(b)
}

pub(super) fn encode_edges(edges: &HashMap<String, EdgeTable>) -> Result<Vec<u8>, String> {
    let mut b = Vec::new();
    let mut rel_types: Vec<&String> = edges.keys().collect();
    rel_types.sort();
    put_u64(&mut b, rel_types.len() as u64);
    for rel_type in rel_types {
        let table = &edges[rel_type];
        put_str(&mut b, rel_type);
        put_str(&mut b, &table.rel_type);
        put_str(&mut b, &table.src_label);
        put_str(&mut b, &table.dst_label);
        put_bytes(&mut b, &encode_batch(&table.batch)?);
    }
    Ok(b)
}

pub(super) fn encode_edge_tables(edge_tables: &HashMap<String, Vec<EdgeTable>>) -> Result<Vec<u8>, String> {
    let mut b = Vec::new();
    let mut rel_types: Vec<&String> = edge_tables.keys().collect();
    rel_types.sort();
    put_u64(&mut b, rel_types.len() as u64);
    for rel_type in rel_types {
        let group = &edge_tables[rel_type];
        put_str(&mut b, rel_type);
        put_u64(&mut b, group.len() as u64);
        for table in group {
            put_str(&mut b, &table.rel_type);
            put_str(&mut b, &table.src_label);
            put_str(&mut b, &table.dst_label);
            put_bytes(&mut b, &encode_batch(&table.batch)?);
        }
    }
    Ok(b)
}

pub(super) fn encode_overlay(ov: &GraphOverlay) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();

    {
        let mut keys: Vec<(String, i64)> = ov.inserted_nodes.keys().cloned().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (label, id) in &keys {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
            put_map(&mut b, &ov.inserted_nodes[&(label.clone(), *id)]);
        }
        write_section(&mut out, OV_INSERTED_NODES, &b);
    }
    {
        let mut keys: Vec<(String, i64)> = ov.node_property_overrides.keys().cloned().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (label, id) in &keys {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
            put_map(&mut b, &ov.node_property_overrides[&(label.clone(), *id)]);
        }
        write_section(&mut out, OV_NODE_OVERRIDES, &b);
    }
    {
        let mut items: Vec<&(String, i64)> = ov.deleted_nodes.iter().collect();
        items.sort();
        let mut b = Vec::new();
        put_u64(&mut b, items.len() as u64);
        for (label, id) in items {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
        }
        write_section(&mut out, OV_DELETED_NODES, &b);
    }
    {
        let mut b = Vec::new();
        put_u64(&mut b, ov.inserted_edges.len() as u64);
        for ((rel_type, id), edge) in &ov.inserted_edges {
            put_str(&mut b, rel_type);
            put_i64(&mut b, *id);
            put_str(&mut b, &edge.src_label);
            put_i64(&mut b, edge.src_id);
            put_str(&mut b, &edge.dst_label);
            put_i64(&mut b, edge.dst_id);
            put_map(&mut b, &edge.properties);
        }
        write_section(&mut out, OV_INSERTED_EDGES, &b);
    }
    {
        let mut keys: Vec<(String, i64)> = ov.edge_property_overrides.keys().cloned().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (rel_type, id) in &keys {
            put_str(&mut b, rel_type);
            put_i64(&mut b, *id);
            put_map(
                &mut b,
                &ov.edge_property_overrides[&(rel_type.clone(), *id)],
            );
        }
        write_section(&mut out, OV_EDGE_OVERRIDES, &b);
    }
    {
        let mut items: Vec<&(String, i64)> = ov.deleted_edges.iter().collect();
        items.sort();
        let mut b = Vec::new();
        put_u64(&mut b, items.len() as u64);
        for (rel_type, id) in items {
            put_str(&mut b, rel_type);
            put_i64(&mut b, *id);
        }
        write_section(&mut out, OV_DELETED_EDGES, &b);
    }
    {
        let mut keys: Vec<&String> = ov.inserted_node_counts.keys().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for label in keys {
            put_str(&mut b, label);
            put_i64(
                &mut b,
                ov.inserted_node_counts.get(label).copied().unwrap_or(0),
            );
        }
        write_section(&mut out, OV_NODE_COUNTS, &b);
    }
    {
        let mut keys: Vec<&String> = ov.inserted_edge_counts.keys().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for rel_type in keys {
            put_str(&mut b, rel_type);
            put_i64(
                &mut b,
                ov.inserted_edge_counts.get(rel_type).copied().unwrap_or(0),
            );
        }
        write_section(&mut out, OV_EDGE_COUNTS, &b);
    }
    {
        let mut keys: Vec<(String, i64)> = ov.inserted_out_adj.keys().cloned().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (label, id) in &keys {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
            let adj = &ov.inserted_out_adj[&(label.clone(), *id)];
            put_u64(&mut b, adj.len() as u64);
            for (rel_type, row) in adj {
                put_str(&mut b, rel_type);
                put_i64(&mut b, *row);
            }
        }
        write_section(&mut out, OV_OUT_ADJ, &b);
    }
    {
        let mut keys: Vec<(String, i64)> = ov.inserted_in_adj.keys().cloned().collect();
        keys.sort();
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (label, id) in &keys {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
            let adj = &ov.inserted_in_adj[&(label.clone(), *id)];
            put_u64(&mut b, adj.len() as u64);
            for (rel_type, row) in adj {
                put_str(&mut b, rel_type);
                put_i64(&mut b, *row);
            }
        }
        write_section(&mut out, OV_IN_ADJ, &b);
    }
    {
        let mut items: Vec<&(String, i64)> = ov.replaced_node_properties.iter().collect();
        items.sort();
        let mut b = Vec::new();
        put_u64(&mut b, items.len() as u64);
        for (label, id) in items {
            put_str(&mut b, label);
            put_i64(&mut b, *id);
        }
        write_section(&mut out, OV_REPLACED_NODES, &b);
    }
    {
        let mut items: Vec<&(String, i64)> = ov.replaced_edge_properties.iter().collect();
        items.sort();
        let mut b = Vec::new();
        put_u64(&mut b, items.len() as u64);
        for (rel_type, id) in items {
            put_str(&mut b, rel_type);
            put_i64(&mut b, *id);
        }
        write_section(&mut out, OV_REPLACED_EDGES, &b);
    }
    for (tag, keys) in [
        (OV_INSERTED_NODE_KEYS, &ov.inserted_node_keys),
        (OV_OVERRIDE_NODE_KEYS, &ov.override_node_keys),
        (OV_INSERTED_EDGE_KEYS, &ov.inserted_edge_keys),
        (OV_OVERRIDE_EDGE_KEYS, &ov.override_edge_keys),
    ] {
        let mut b = Vec::new();
        put_u64(&mut b, keys.len() as u64);
        for (label, list) in keys {
            put_str(&mut b, label);
            put_str_list(&mut b, list);
        }
        write_section(&mut out, tag, &b);
    }

    let mut labels = Vec::new();
    put_u64(&mut labels, ov.node_label_sets.len() as u64);
    for ((storage, id), names) in &ov.node_label_sets {
        put_str(&mut labels, storage);
        put_i64(&mut labels, *id);
        put_str_list(&mut labels, &names.iter().cloned().collect::<Vec<_>>());
    }
    write_section(&mut out, 0x23, &labels);
    write_section(&mut out, 0x22, &binary::encode_value_bytes(&Value::Bool(ov.allow_null_property_values)));
    let nulls = Value::List(ov.edge_null_properties.iter().map(|((label, id), keys)| {
        Value::List(vec![Value::String(label.clone()), Value::Long(*id),
            Value::List(keys.iter().cloned().map(Value::String).collect())])
    }).collect());
    write_section(&mut out, 0x21, &binary::encode_value_bytes(&nulls));
    let mut state = Vec::new();
    let mut keys = std::collections::BTreeSet::new();
    keys.extend(ov.vertex_properties.keys().cloned());
    keys.extend(ov.public_ids.keys().filter(|(edge,_,_)| !edge).map(|(_,label,id)|(label.clone(),*id)));
    for key in keys {state.push(Value::List(vec![Value::String(key.0.clone()),Value::Long(key.1),ov.native_node_state(&key)]));}
    let edges = ov.public_ids.iter().filter(|((edge,_,_),_)|*edge).map(|((_,name,id),value)|Value::List(vec![Value::String(name.clone()),Value::Long(*id),value.clone()])).collect();
    write_section(&mut out, 0x20, &binary::encode_value_bytes(&Value::List(vec![Value::Long(ov.next_property_id),Value::List(state),Value::List(edges)])));
    Ok(out)
}

// ---------------- section parsers ----------------

pub(super) fn parse_nodes(payload: &[u8]) -> Result<HashMap<String, NodeTable>, String> {
    let mut r = Reader::new(payload);
    let n = r.count()?;
    let mut map = HashMap::with_capacity(n);
    for _ in 0..n {
        let key = r.str()?;
        let label = r.str()?;
        let batch = decode_batch(r.blob()?)?;
        map.insert(key, NodeTable { label, batch });
    }
    finish(&r)?;
    Ok(map)
}

pub(super) fn parse_edges(payload: &[u8]) -> Result<HashMap<String, EdgeTable>, String> {
    let mut r = Reader::new(payload);
    let n = r.count()?;
    let mut map = HashMap::with_capacity(n);
    for _ in 0..n {
        let key = r.str()?;
        let rel_type = r.str()?;
        let src_label = r.str()?;
        let dst_label = r.str()?;
        let batch = decode_batch(r.blob()?)?;
        map.insert(
            key,
            EdgeTable {
                rel_type,
                src_label,
                dst_label,
                batch,
            },
        );
    }
    finish(&r)?;
    Ok(map)
}

pub(super) fn parse_edge_tables(payload: &[u8]) -> Result<HashMap<String, Vec<EdgeTable>>, String> {
    let mut r = Reader::new(payload);
    let n = r.count()?;
    let mut map = HashMap::with_capacity(n);
    for _ in 0..n {
        let key = r.str()?;
        let m = r.count()?;
        let mut group = Vec::with_capacity(m);
        for _ in 0..m {
            let rel_type = r.str()?;
            let src_label = r.str()?;
            let dst_label = r.str()?;
            let batch = decode_batch(r.blob()?)?;
            group.push(EdgeTable {
                rel_type,
                src_label,
                dst_label,
                batch,
            });
        }
        map.insert(key, group);
    }
    finish(&r)?;
    Ok(map)
}

pub(super) fn parse_overlay(payload: &[u8]) -> Result<GraphOverlay, String> {
    let mut ov = GraphOverlay::default();
    let mut r = Reader::new(payload);
    while !r.is_empty() {
        let tag = r.u8()?;
        let sub = r.blob()?;
        let mut sr = Reader::new(sub);
        match tag {
            0x23 => {
                let count = sr.count()?;
                for _ in 0..count {
                    let storage = sr.str()?;
                    let id = sr.i64()?;
                    let names = decode_str_list(&mut sr)?;
                    ov.node_label_sets.insert((storage, id), names.into_iter().collect());
                }
            }
            0x22 => {
                let Value::Bool(enabled) = binary::decode_value_bytes(sub)? else {
                    return Err("Invalid null property feature".into());
                };
                ov.allow_null_property_values = enabled;
                continue;
            }
            0x21 => {
                let Value::List(edges) = binary::decode_value_bytes(sub)? else {
                    return Err("Invalid null edge properties".into());
                };
                for edge in edges {
                    let Value::List(fields) = edge else { return Err("Invalid null edge property record".into()); };
                    let [Value::String(label), Value::Long(id), Value::List(keys)] = fields.as_slice() else {
                        return Err("Invalid null edge property fields".into());
                    };
                    let keys = keys.iter().map(|key| match key {
                        Value::String(key) => Ok(key.clone()),
                        _ => Err("Invalid null edge property key".to_string()),
                    }).collect::<Result<_, _>>()?;
                    ov.edge_null_properties.insert((label.clone(), *id), keys);
                }
                continue;
            }
            0x20 => {
                let value = binary::decode_value_bytes(sub)?;
                let Value::List(fields) = value else {return Err("Invalid native property state".into())};
                let [Value::Long(next),Value::List(nodes),Value::List(edges)] = fields.as_slice() else {return Err("Invalid native property state fields".into())};
                ov.next_property_id = *next;
                for node in nodes {let Value::List(fields)=node else{return Err("Invalid native node".into())};let [Value::String(label),Value::Long(id),state]=fields.as_slice() else{return Err("Invalid native node fields".into())};ov.restore_native_node_state((label.clone(),*id),state)?;}
                for edge in edges {let Value::List(fields)=edge else{return Err("Invalid public edge".into())};let [Value::String(label),Value::Long(id),value]=fields.as_slice() else{return Err("Invalid public edge fields".into())};ov.public_ids.insert((true,label.clone(),*id),value.clone());}
                continue;
            }
            OV_INSERTED_NODES => {
                let n = sr.count()?;
                for _ in 0..n {
                    let label = sr.str()?;
                    let id = sr.i64()?;
                    let props = decode_map(&mut sr)?;
                    ov.inserted_nodes.insert((label, id), props);
                }
            }
            OV_NODE_OVERRIDES => {
                let n = sr.count()?;
                for _ in 0..n {
                    let label = sr.str()?;
                    let id = sr.i64()?;
                    let props = decode_map(&mut sr)?;
                    ov.node_property_overrides.insert((label, id), props);
                }
            }
            OV_DELETED_NODES => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.deleted_nodes.insert((sr.str()?, sr.i64()?));
                }
            }
            OV_INSERTED_EDGES => {
                let n = sr.count()?;
                for _ in 0..n {
                    let rel_type = sr.str()?;
                    let id = sr.i64()?;
                    let src_label = sr.str()?;
                    let src_id = sr.i64()?;
                    let dst_label = sr.str()?;
                    let dst_id = sr.i64()?;
                    let properties = decode_map(&mut sr)?;
                    ov.inserted_edges.insert(
                        (rel_type, id),
                        InsertedEdge {
                            src_label,
                            src_id,
                            dst_label,
                            dst_id,
                            properties,
                        },
                    );
                }
            }
            OV_EDGE_OVERRIDES => {
                let n = sr.count()?;
                for _ in 0..n {
                    let rel_type = sr.str()?;
                    let id = sr.i64()?;
                    let props = decode_map(&mut sr)?;
                    ov.edge_property_overrides.insert((rel_type, id), props);
                }
            }
            OV_DELETED_EDGES => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.deleted_edges.insert((sr.str()?, sr.i64()?));
                }
            }
            OV_NODE_COUNTS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.inserted_node_counts.insert(sr.str()?, sr.i64()?);
                }
            }
            OV_EDGE_COUNTS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.inserted_edge_counts.insert(sr.str()?, sr.i64()?);
                }
            }
            OV_OUT_ADJ | OV_IN_ADJ => {
                let n = sr.count()?;
                for _ in 0..n {
                    let label = sr.str()?;
                    let id = sr.i64()?;
                    let m = sr.count()?;
                    let mut adj = Vec::with_capacity(m);
                    for _ in 0..m {
                        adj.push((sr.str()?, sr.i64()?));
                    }
                    if tag == OV_OUT_ADJ {
                        ov.inserted_out_adj.insert((label, id), adj);
                    } else {
                        ov.inserted_in_adj.insert((label, id), adj);
                    }
                }
            }
            OV_REPLACED_NODES => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.replaced_node_properties.insert((sr.str()?, sr.i64()?));
                }
            }
            OV_REPLACED_EDGES => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.replaced_edge_properties.insert((sr.str()?, sr.i64()?));
                }
            }
            OV_INSERTED_NODE_KEYS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.inserted_node_keys
                        .insert(sr.str()?, decode_str_list(&mut sr)?);
                }
            }
            OV_OVERRIDE_NODE_KEYS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.override_node_keys
                        .insert(sr.str()?, decode_str_list(&mut sr)?);
                }
            }
            OV_INSERTED_EDGE_KEYS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.inserted_edge_keys
                        .insert(sr.str()?, decode_str_list(&mut sr)?);
                }
            }
            OV_OVERRIDE_EDGE_KEYS => {
                let n = sr.count()?;
                for _ in 0..n {
                    ov.override_edge_keys
                        .insert(sr.str()?, decode_str_list(&mut sr)?);
                }
            }
            _ => {}
        }
        if is_overlay_tag(tag) {
            finish(&sr)?;
        }
    }
    ov.rebuild_public_id_lookup();
    Ok(ov)
}
