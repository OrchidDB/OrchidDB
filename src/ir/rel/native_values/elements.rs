use super::*;
use crate::ir::ElementId;

pub(super) const FIELD: &str = "__orchiddb_element_v1";
#[derive(Default)]
pub(super) struct Context {
    pub graph: PropertyGraph,
    elements: BTreeMap<(bool, String, ElementId), Value>,
}
impl Context {
    pub fn attach(&mut self, descriptor: Value) -> Result<Value, String> {
        let Value::Map(fields) = &descriptor else {
            return Err("Invalid element cell".into());
        };
        let get = |key: &str| {
            fields
                .get(key)
                .cloned()
                .ok_or_else(|| format!("Missing element {key}"))
        };
        let text = |key: &str| match get(key)? {
            Value::String(s) => Ok(s),
            _ => Err(format!("Invalid element {key}")),
        };
        let key = |key: &str| -> Result<Value, String> {
            Ok(match get(key)? {
                Value::Map(map) => Value::List(
                    (0..map.len())
                        .map(|i| {
                            map.get(&format!("k{i}"))
                                .cloned()
                                .ok_or("Invalid composite key field")
                        })
                        .collect::<Result<_, _>>()?,
                ),
                value => value,
            })
        };
        let public_id = key("id")?;
        if public_id == Value::Null {
            return Ok(Value::Null);
        }
        let id = ElementId::try_from(fields.get("identity").unwrap_or(&public_id))?;
        let label = text("label")?;
        let edge = text("kind")? == "edge";
        let value = if edge {
            let src_label = text("src_label")?;
            let dst_label = text("dst_label")?;
            let src = key("src_id")?;
            let dst = key("dst_id")?;
            let src_id = ElementId::try_from(fields.get("src_identity").unwrap_or(&src))?;
            let dst_id = ElementId::try_from(fields.get("dst_identity").unwrap_or(&dst))?;
            // Endpoint values also need public identity after crossing a
            // second scalar boundary (for example outV() after fold()).
            for (label, id, identity) in [(&src_label, src, &src_id), (&dst_label, dst, &dst_id)] {
                self.attach(Value::Map(BTreeMap::from([
                    ("kind".into(), Value::String("node".into())),
                    ("id".into(), id),
                    ("identity".into(), Value::Scalar(identity.scalar().clone())),
                    ("label".into(), Value::String(label.clone())),
                    ("properties".into(), Value::Map(BTreeMap::new())),
                ])))?;
            }
            Value::Edge {
                rel_type: label.clone(),
                id: id.clone(),
                src_label,
                src_id,
                dst_label,
                dst_id,
                projected_properties: None,
            }
        } else {
            Value::Node {
                label: label.clone(),
                id: id.clone(),
            }
        };
        let Value::Map(mut properties) = get("properties")? else {
            return Err("Invalid element properties".into());
        };
        if fields.get("present_nulls") != Some(&Value::Bool(true)) {
            properties.retain(|_, v| *v != Value::Null);
        }
        self.graph
            .attach_element(&value, public_id, properties)
            .map_err(|e| e.to_string())?;
        if !edge {
            if let Some(state) = fields.get("native_node") {
                self.graph.restore_detached_node_state(&label, &id, state)?;
            }
            if let Some(Value::List(labels)) = fields.get("labels") {
                let labels = labels.iter().map(|label| match label {
                    Value::String(label) => Ok(label.clone()),
                    _ => Err("Invalid detached node label".to_string()),
                }).collect::<Result<Vec<_>, _>>()?;
                self.graph.set_node_labels(&value, labels).map_err(|e| e.to_string())?;
            }
        }
        let key = (edge, label, id);
        let mut descriptor = descriptor;
        // Multiple projected references may carry different property subsets.
        // Keep the hydrated union instead of replacing it with the last cell.
        if let Some(Value::Map(previous)) = self.elements.get(&key)
            && let Some(Value::Map(old_properties)) = previous.get("properties")
            && let Value::Map(fields) = &mut descriptor
            && let Some(Value::Map(properties)) = fields.get_mut("properties") {
            let mut merged = old_properties.clone();
            merged.extend(std::mem::take(properties));
            *properties = merged;
        }
        if let Some(Value::Map(previous)) = self.elements.get(&key)
            && let Value::Map(fields) = &mut descriptor {
            for (name,value) in previous { fields.entry(name.clone()).or_insert_with(||value.clone()); }
        }
        self.elements.insert(key, descriptor);
        Ok(value)
    }
    pub fn capture(&mut self, value: &Value, graph: &PropertyGraph) -> Result<(), String> {
        for (edge, label, id) in referenced_elements(value) {
            if self.elements.contains_key(&(edge, label.clone(), id.clone())) { continue; }
            let owner = if edge {
                let (src_label, src_id, dst_label, dst_id) = graph.live_edge_endpoints(&label, id.clone())
                    .ok_or_else(|| format!("Missing detached edge {label}"))?;
                Value::Edge {rel_type:label.clone(),id:id.clone(),src_label,src_id,dst_label,dst_id,projected_properties:None}
            } else {Value::Node {label:label.clone(),id:id.clone()}};
            let properties = graph.jvm_properties(&owner, &[]);
            // Cypher user properties may be named `id` without being Gremlin
            // property records. Use the existing graph output accessors, then
            // retain explicit null/property-record values from the native view.
            let keys = if edge { graph.edge_property_keys(&label) } else { graph.node_property_keys_with_id(&label) };
            let mut values = keys.into_iter().filter_map(|key| {
                let value = if edge { graph.edge_property(&label,id.clone(),&key) } else { graph.node_property(&label,id.clone(),&key) };
                (value != Value::Null).then_some((key,value))
            }).collect::<BTreeMap<_,_>>();
            values.extend(properties.iter().filter_map(|property| match property {
                Value::VertexProperty {key,value,..}|Value::Property{key,value,..} => Some((key.clone(),value.as_ref().clone())),
                _ => None,
            }));
            let mut fields = BTreeMap::from([
                ("kind".into(), Value::String(if edge {"edge"} else {"node"}.into())),
                ("label".into(), Value::String(label.clone())),
                ("identity".into(), Value::Scalar(id.scalar().clone())),
                ("id".into(), graph.element_public_id(&owner)),
                ("present_nulls".into(), Value::Bool(true)),
                ("properties".into(), Value::Map(values)),
            ]);
            if let Value::Edge {src_label,src_id,dst_label,dst_id,..} = &owner {
                for (prefix,label,id) in [("src",src_label,src_id),("dst",dst_label,dst_id)] {
                    fields.insert(format!("{prefix}_label"),Value::String(label.clone()));
                    fields.insert(format!("{prefix}_identity"),Value::Scalar(id.scalar().clone()));
                    fields.insert(format!("{prefix}_id"),graph.element_public_id(&Value::Node {label:label.clone(),id:id.clone()}));
                }
            } else {
                fields.insert("native_node".into(),graph.detached_node_state(&label,&id));
                fields.insert("labels".into(),Value::List(graph.node_labels(&label,id.clone()).into_iter().map(Value::String).collect()));
            }
            self.attach(Value::Map(fields))?;
        }
        Ok(())
    }
    pub fn decode(&mut self, text: &str) -> Result<Value, String> {
        let Some(text) = text.strip_prefix("g1:") else {
            return decode(text);
        };
        let Value::List(mut parts) = decode(text)? else {
            return Err("Invalid element carrier".into());
        };
        if parts.len() != 2 {
            return Err("Invalid element carrier".into());
        }
        let Value::List(elements) = parts.pop().unwrap() else {
            return Err("Invalid element context".into());
        };
        for element in elements {
            self.attach(element)?;
        }
        Ok(parts.pop().unwrap())
    }
    pub fn wire(&self, value: &Value) -> serde_json::Value {
        if *value == Value::Null {
            return serde_json::Value::Null;
        }
        let keys = referenced_elements(value);
        let elements = keys
            .iter()
            .filter_map(|key| self.elements.get(key).cloned())
            .collect::<Vec<_>>();
        let text = if elements.is_empty() {
            encode(value)
        } else {
            format!(
                "g1:{}",
                encode(&Value::List(vec![value.clone(), Value::List(elements)]))
            )
        };
        serde_json::json!({super::FIELD:text})
    }
}
fn referenced_elements(value: &Value) -> BTreeSet<(bool,String,ElementId)> {
        let mut keys = BTreeSet::new();
        let mut pending = vec![value];
        while let Some(value) = pending.pop() {
            match value {
                Value::Node { label, id } => {
                    keys.insert((false, label.clone(), id.clone()));
                }
                Value::Edge {
                    rel_type,
                    id,
                    src_label,
                    src_id,
                    dst_label,
                    dst_id,
                    ..
                } => {
                    keys.insert((true, rel_type.clone(), id.clone()));
                    keys.insert((false, src_label.clone(), src_id.clone()));
                    keys.insert((false, dst_label.clone(), dst_id.clone()));
                }
                Value::Path(items)
                | Value::List(items)
                | Value::Set(items)
                | Value::BulkSet(items) => pending.extend(items),
                Value::Map(map) => pending.extend(map.values()),
                Value::TypedMap(entries) => {
                    for (k, v) in entries {
                        pending.extend([k, v]);
                    }
                }
                Value::MapEntry(pair) => pending.extend([&pair.0, &pair.1]),
                Value::Property { owner, value, .. }
                | Value::VertexProperty { owner, value, .. } => {
                    pending.extend([owner.as_ref(), value.as_ref()])
                }
                Value::CardinalityValue { value, .. } => pending.push(value),
                _ => {}
            }
        }
        keys
}

impl LoweringContext<'_> {
    /// Preserve identity and the projected properties without fetching any rows
    /// from storage inside a scalar function.
    pub(in crate::ir::rel) fn native_element(
        &self,
        plan: &LogicalPlan,
        binding: &str,
    ) -> RelResult<Expr> {
        let shape = has_binding_shape(plan, binding)
            .ok_or_else(|| RelError::Unsupported("Expected element binding".into()))?;
        let keys = self.element_property_keys(plan, binding, shape);
        let mut props = keys
            .iter()
            .flat_map(|key| [lit(key.clone()), col_exact(prop_col(binding, key))])
            .collect::<Vec<_>>();
        if props.is_empty() {
            props.extend([lit("__unit"), lit(ScalarValue::Null)]);
        }
        let mut fields = vec![
            lit("kind"),
            lit(if shape == BindingShape::Edge {
                "edge"
            } else {
                "node"
            }),
            lit("id"),
            col_exact(id_col(binding)),
            lit("label"),
            col_exact(label_col(binding)),
            lit("properties"),
            df_core::named_struct(props),
        ];
        if shape == BindingShape::Edge {
            fields.extend([
                lit("src_id"),
                col_exact(src_id_col(binding)),
                lit("src_label"),
                col_exact(src_label_col(binding)),
                lit("dst_id"),
                col_exact(dst_id_col(binding)),
                lit("dst_label"),
                col_exact(dst_label_col(binding)),
            ]);
        }
        Ok(function(
            VALUE,
            value_type(),
            vec![
                lit(r#"{"Binding":"value"}"#),
                df_core::named_struct(vec![
                    lit("value"),
                    df_core::named_struct(vec![lit(FIELD), df_core::named_struct(fields)]),
                ]),
            ],
        ))
    }
}
