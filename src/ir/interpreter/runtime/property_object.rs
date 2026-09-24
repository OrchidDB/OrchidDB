//! value_map / element_map / property_map / properties_list.
//!
//! Extracted from `interpreter.rs` lines 2708..2793.

use crate::ir::catalog::PropertyGraph;
use crate::ir::value::Value;

pub(crate) fn eval_property_object(name: &str, args: &[Value], graph: &PropertyGraph) -> Value {
    let target = args.first().cloned().unwrap_or(Value::Null);
    let keys = match args.get(1) {Some(Value::List(keys)) => keys.iter().filter_map(|v|if let Value::String(s)=v {Some(s.clone())} else {None}).collect::<Vec<_>>(),_=>vec![]};
    let props = graph.properties(&target,&keys);
    if name == "properties_list" {return Value::List(props)}
    if !matches!(target,Value::Node{..}|Value::Edge{..}|Value::VertexProperty{..}) {return Value::Null}
    let single = !matches!(target,Value::Node{..}) || name == "element_map" || bool_arg(args.get(4),false);
    let mut map=std::collections::BTreeMap::new();
    for property in props {
        let (key,value)=match &property {Value::VertexProperty{key,value,..}|Value::Property{key,value,..}=>(key.clone(),value.as_ref().clone()),_=>continue};
        let value=if name=="property_map" {property} else {value};
        if single {map.entry(key).or_insert(value);} else {let entry=map.entry(key).or_insert_with(||Value::List(vec![]));if let Value::List(items)=entry {items.push(value)}}
    }
    if name == "element_map" || name == "value_map_tokens" {
        let mut entries=vec![];
        if bool_arg(args.get(2),true) {entries.push((Value::Token("id".into()),graph.element_public_id(&target)));}
        if bool_arg(args.get(3),true) {let label=match &target {Value::Node{label,..}=>label,Value::Edge{rel_type,..}=>rel_type,Value::VertexProperty{key,..}=>key,_=>unreachable!()};entries.push((Value::Token("label".into()),Value::String(label.clone())));}
        if name=="element_map" {add_endpoint_tokens(&mut entries,&target,graph)}
        entries.extend(map.into_iter().map(|(key,value)|(Value::String(key),value)));
        Value::TypedMap(entries)
    } else {Value::Map(map)}
}

pub(crate) fn requested_property_values(target:&Value,keys:&[String],graph:&PropertyGraph)->Value {
    if let Value::Map(map)=target {return Value::List(if keys.is_empty(){map.values().cloned().collect()}else{keys.iter().filter_map(|k|map.get(k).cloned()).collect()})}
    Value::List(graph.properties(target,keys).into_iter().filter_map(|p|match p {Value::VertexProperty{value,..}|Value::Property{value,..}=>Some(*value),_=>None}).collect())
}

fn bool_arg(value: Option<&Value>, default: bool) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        _ => default,
    }
}

pub(crate) fn eval_property_element(args: &[Value]) -> Value {
    match args.first() { Some(Value::VertexProperty {owner,..}|Value::Property {owner,..})=>owner.as_ref().clone(), _=>Value::Null }
}

fn add_endpoint_tokens(entries: &mut Vec<(Value, Value)>, value: &Value, graph: &PropertyGraph) {
    if let Value::Edge { src_label, src_id, dst_label, dst_id, .. } = value {
        entries.push((Value::Direction("OUT".into()), endpoint_token(graph, src_label, *src_id)));
        entries.push((Value::Direction("IN".into()), endpoint_token(graph, dst_label, *dst_id)));
    }
}

fn endpoint_token(graph: &PropertyGraph, label: &str, id: i64) -> Value {
    let node = Value::Node { label: label.to_string(), id };
    Value::TypedMap(vec![
        (Value::Token("id".into()), super::graph::gremlin_user_id(graph, &node)),
        (Value::Token("label".into()), Value::String(label.to_string())),
    ])
}
