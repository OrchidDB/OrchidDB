use super::*;
use crate::ir::ElementId;

pub(super) const FIELD: &str = "__orchiddb_element_v1";
#[derive(Default)]
pub(super) struct Context {
    pub graph: PropertyGraph,
    elements: BTreeMap<(bool, String, ElementId), Value>,
}
// JSON itself cannot distinguish SQL INTEGER from BIGINT. Retain the declared
// widths inside element descriptors so nested graph values preserve public IDs.
fn restore_identity_type(value: Value, kind: &DataType) -> Result<Value, String> {
    if value == Value::Null { return Ok(value); }
    match (value, kind) {
        (Value::Map(mut values), DataType::Struct(fields)) => {
            for field in fields {
                if let Some(value) = values.remove(field.name()) {
                    values.insert(field.name().clone(), restore_identity_type(value, field.data_type())?);
                }
            }
            Ok(Value::Map(values))
        }
        (Value::List(values), DataType::List(field) | DataType::LargeList(field) | DataType::FixedSizeList(field, _)) =>
            Ok(Value::List(values.into_iter().map(|value| restore_identity_type(value, field.data_type())).collect::<Result<_, _>>()?)),
        (value, kind) if matches!(kind, DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 | DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 | DataType::Float32 | DataType::Float64) => {
            let text = match value {
                Value::Byte(n) => n.to_string(), Value::Short(n) => n.to_string(),
                Value::Int(n) | Value::Long(n) => n.to_string(),
                Value::UInt8(n) => n.to_string(), Value::UInt16(n) => n.to_string(),
                Value::UInt32(n) => n.to_string(), Value::UInt64(n) => n.to_string(),
                Value::Float(n) => n.to_string(), Value::Float32(n) => n.to_string(),
                Value::Scalar(value) => return Ok(Value::Scalar(value.cast_to(kind).map_err(|error| error.to_string())?)),
                _ => return Err("Invalid numeric element identity".into()),
            };
            let scalar = ScalarValue::Utf8(Some(text)).cast_to(kind).map_err(|error| error.to_string())?;
            Ok(Value::Scalar(scalar))
        }
        (value, _) => Ok(value),
    }
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
            let mut value = get(key)?;
            if let Some(Value::String(encoded)) = fields.get(&format!("{key}_type")) {
                let kind: DataType = encoded.parse().map_err(|error: arrow::error::ArrowError| error.to_string())?;
                value = restore_identity_type(value, &kind)?;
            }
            Ok(match value {
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
            .attach_element(&value, public_id.clone(), properties)
            .map_err(|e| e.to_string())?;
        if let Some(Value::String(record)) = fields.get("native_record") {
            // Managed SQL scans select the existing complete entity record.
            // Decode it inside this scalar context so multi-properties, meta-
            // properties and public identities survive the SQL boundary.
            let payload = STANDARD.decode(record).map_err(|error| error.to_string())?;
            self.graph.apply_incremental_records(&[
                crate::ir::catalog::incremental::IncrementalRecord {
                    kind: if edge { 2 } else { 1 }, name: label.clone(), id: id.clone(), payload,
                },
            ])?;
        }
        if !edge {
            if let Some(state) = fields.get("native_node") {
                let public_identity = self.graph.element_public_id(&value);
                self.graph.restore_detached_node_state(&label, &id, state)?;
                // Source keys are public identities even when the source has
                // no overlay public-id entry. Restoring its property-record
                // state must not replace that explicit descriptor ID with a
                // synthetic label#offset identity.
                self.graph.attach_element(&value, public_identity, BTreeMap::new()).map_err(|e| e.to_string())?;
            }
            if let Some(Value::List(labels)) = fields.get("labels") {
                let labels = labels.iter().map(|label| match label {
                    Value::String(label) => Ok(label.clone()),
                    _ => Err("Invalid detached node label".to_string()),
                }).collect::<Result<Vec<_>, _>>()?;
                self.graph.set_node_labels(&value, labels).map_err(|e| e.to_string())?;
            }
        }
        if fields.get("source_keys") == Some(&Value::Bool(true)) {
            self.graph.stabilize_mapped_property_ids(&value);
        }
        if fields.get("deleted") == Some(&Value::Bool(true)) {
            self.graph.mark_detached_deleted(&value);
        }
        let mut descriptor = descriptor;
        if let Value::Map(fields) = &mut descriptor
            && (fields.remove("native_record").is_some()
                || fields.get("source_keys") == Some(&Value::Bool(true))) {
            // The storage record is needed only to hydrate this context.
            // Emit the canonical detached state, not another full copy of the
            // record alongside its already-decoded properties on every hop.
            fields.insert("id".into(), self.graph.element_public_id(&value));
            if !edge {
                fields.insert("native_node".into(), self.graph.detached_node_state(&label, &id));
            }
        }
        let key = (edge, label, id);
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
        for ((edge, label, id), owner) in element_owners(value) {
            if self.elements.contains_key(&(edge, label.clone(), id.clone())) { continue; }
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
                ("source_keys".into(), Value::Bool(graph.source_keys)),
                ("label".into(), Value::String(label.clone())),
                ("identity".into(), Value::Scalar(id.scalar().clone())),
                ("id".into(), graph.element_public_id(&owner)),
                ("present_nulls".into(), Value::Bool(true)),
                ("deleted".into(), Value::Bool(if edge {
                    graph.live_edge_endpoints(&label, id.clone()).is_none()
                } else { !graph.node_is_live(&label, id.clone()) })),
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
    pub fn encode(&self, value: &Value) -> Option<String> {
        if *value == Value::Null { return None; }
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
        Some(text)
    }
    pub fn wire(&self, value: &Value) -> serde_json::Value {
        self.encode(value).map(|text|serde_json::json!({super::FIELD:text})).unwrap_or(serde_json::Value::Null)
    }
}
fn referenced_elements(value: &Value) -> BTreeSet<(bool,String,ElementId)> {
    element_owners(value).into_keys().collect()
}
fn element_owners(value: &Value) -> BTreeMap<(bool,String,ElementId), Value> {
        let mut keys = BTreeMap::new();
        let mut pending = vec![value];
        while let Some(value) = pending.pop() {
            match value {
                Value::Node { label, id } => {
                    keys.insert((false, label.clone(), id.clone()), value.clone());
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
                    keys.insert((true, rel_type.clone(), id.clone()), value.clone());
                    keys.insert((false, src_label.clone(), src_id.clone()), Value::Node { label:src_label.clone(), id:src_id.clone() });
                    keys.insert((false, dst_label.clone(), dst_id.clone()), Value::Node { label:dst_label.clone(), id:dst_id.clone() });
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
            lit("source_keys"),
            lit(self.graph.source_keys || self.options.mapping.is_some()),
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
        for key in if shape == BindingShape::Edge { vec!["id", "src_id", "dst_id"] } else { vec!["id"] } {
            let column = match key { "src_id" => src_id_col(binding), "dst_id" => dst_id_col(binding), _ => id_col(binding) };
            let kind = col_exact(column).get_type(plan.schema())?;
            fields.extend([lit(format!("{key}_type")), lit(kind.to_string())]);
        }
        if self.language == Language::Cypher {
            return Ok(super::typed::call("element", vec![df_core::named_struct(fields)]));
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

#[cfg(test)]
mod identity_type_tests {
    use super::*;
    #[test]
    fn declared_identity_width_survives_nested_json_descriptors() {
        let mut context = Context::default();
        let node = context.attach(Value::Map(BTreeMap::from([
            ("kind".into(), Value::String("node".into())),
            ("label".into(), Value::String("software".into())),
            ("id".into(), Value::Long(3)),
            ("id_type".into(), Value::String(DataType::Int32.to_string())),
            ("properties".into(), Value::Map(BTreeMap::new())),
        ]))).unwrap();
        assert!(matches!(context.graph.element_public_id(&node), Value::Scalar(ScalarValue::Int32(Some(3)))));
        let Value::Node { id, .. } = node else { panic!("node") };
        assert_eq!(id.scalar().data_type(), DataType::Int32);
        let input = Value::Map(BTreeMap::from([("k0".into(), Value::Long(2)), ("k1".into(), Value::Long(3))]));
        let ty = DataType::Struct(vec![Field::new("k0", DataType::Int16, false), Field::new("k1", DataType::Int64, false)].into());
        let Value::Map(result) = restore_identity_type(input, &ty).unwrap() else { panic!("composite key") };
        assert!(matches!(result["k0"], Value::Scalar(ScalarValue::Int16(Some(2)))));
        assert!(matches!(result["k1"], Value::Scalar(ScalarValue::Int64(Some(3)))));
    }
}
