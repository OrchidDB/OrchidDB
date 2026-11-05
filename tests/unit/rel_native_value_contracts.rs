use super::*;
use serde_json::json;

#[test]
fn heterogeneous_property_carriers_decode_before_scalar_comparison() {
    for (tag, field, payload, expected) in [
        (1, "integer", json!({"__orchiddb_number":["BIGINT","29"]}), Value::Long(29)),
        (2, "string", json!("Ada"), Value::String("Ada".into())),
        (3, "boolean", json!(true), Value::Bool(true)),
    ] {
        let mut carrier = json!({"__cypher_scalar_tag":{"__orchiddb_number":["TINYINT",tag.to_string()]},"integer":null,"string":null,"boolean":null});
        carrier[field] = payload;
        assert_eq!(from_json(&carrier, &mut Context::default()).unwrap(), expected);
    }
}

fn element(id: i64, properties: serde_json::Value) -> serde_json::Value {
    json!({elements::FIELD: {"kind":"node", "id":id,"label":"Person","properties":properties}})
}

#[test]
fn current_projection_preserves_productive_map_null_and_drops_absent_element_property() {
    let expression = serde_json::to_string(&IrExpr::property("current", "name", crate::ir::policy::PropertyMissing::DropUnproductive)).unwrap();
    assert_eq!(project(&expression, &json!({"current":{}}), 0, 0).unwrap(), json!([null]));
    assert_eq!(project(&expression, &json!({"current":{"Name":"case-sensitive"}}), 0, 0).unwrap(), json!([null]));
    assert_eq!(project(&expression, &json!({"current":element(1,json!({}))}), 0, 0).unwrap(), json!([]));
    let result = project(&expression, &json!({"current":element(1,json!({"name":"Ada"}))}), 0, 0).unwrap();
    assert_eq!(json(&result[0]).unwrap(), json!({"type":"string","value":"Ada"}));
    let null = serde_json::to_string(&IrExpr::Lit(Lit::Null)).unwrap();
    assert_eq!(project(&null, &json!({}), 0, 0).unwrap(), json!([]));
}

#[test]
fn equality_keys_follow_language_numeric_and_element_identity_rules() {
    let integer = json!({"__orchiddb_number":["INTEGER","1"]});
    let double = json!({"__orchiddb_number":["DOUBLE","1"]});
    assert_eq!(key(&integer,true).unwrap(),key(&double,true).unwrap());
    assert_ne!(key(&integer,false).unwrap(),key(&double,false).unwrap());
    let a = element(1,json!({"name":"Ada"}));
    let b = element(1,json!({"age":30}));
    for cypher in [false,true] { assert_eq!(key(&a,cypher).unwrap(),key(&b,cypher).unwrap()); }
}

#[test]
fn scalar_round_trip_retains_union_of_projected_element_properties() {
    let input = json!([element(1,json!({"name":"Ada"})),element(1,json!({"age":30}))]);
    let mut context = Context::default();
    let Value::List(values) = from_json(&input,&mut context).unwrap() else { panic!("expected values") };
    let output=json(&context.wire(&values[1])).unwrap();
    assert_eq!(output["properties"]["name"],json!({"type":"string","value":"Ada"}));
    assert_eq!(output["properties"]["age"]["value"],json!(30));
}

#[test]
fn sort_reuses_language_comparator_and_returns_original_row_permutation() {
    let spec=SortSpecification {
        keys:vec![SortKey {expr:IrExpr::Call{name:"gremlin_order_key".into(),args:vec![IrExpr::binding("value")]},dir:SortDir::Asc,nulls:NullsOrder::ProviderDefined}],
        scopes:vec!["scope".into()],
    };
    let input=json!([
        {"payload":"ten","scope":{"value":{"__orchiddb_number":["INTEGER","10"]}}},
        {"payload":"two","scope":{"value":{"__orchiddb_number":["DOUBLE","2"]}}},
        {"payload":"negative","scope":{"value":{"__orchiddb_number":["BIGINT","-1"]}}},
    ]);
    assert_eq!(sort(&serde_json::to_string(&spec).unwrap(),&input).unwrap(),vec![2,1,0]);
}

#[test]
fn program_arrow_boundary_preserves_null_bulk_and_property_record_context() {
    use crate::ir::catalog::Cardinality;
    use crate::ir::policy::{GraphPlanPolicy, ResultForm};
    let graph = PropertyGraph::default();
    graph.enable_null_property_values(true);
    let node = graph.insert_node("Person", BTreeMap::new());
    graph.set_element_public_id(&node, Value::String("person-1".into())).unwrap();
    graph.set_node_labels(&node, ["Person".into(), "Employee".into()]).unwrap();
    let property = graph.set_vertex_property(&node,"name".into(),Value::Null,Cardinality::List,
        BTreeMap::from([("since".into(),Value::Int(2000))])).unwrap();
    graph.set_vertex_property_public_id(&property,Value::Long(73)).unwrap();
    let mut row = KernelRow::new().with("current",node.clone());
    row.bulk = 2;
    let rows = vec![row,KernelRow::new().with("current",Value::Null)];
    let fields=vec!["current".into()];
    let batch=result_batch(&fields,ResultForm::TraverserStream,&rows,&graph,&GraphPlanPolicy::gremlin()).unwrap();
    assert_eq!(batch.num_rows(),3);
    assert!(batch.column(0).is_null(2));
    let decoded=decode_rows(&batch,&fields,&graph).unwrap();
    assert_eq!(decoded[0].get("current"),node);
    assert_eq!(decoded[1].get("current"),node);
    assert_eq!(decoded[2].get("current"),Value::Null);
    let structs=batch.column(0).as_any().downcast_ref::<arrow::array::StructArray>().unwrap();
    let strings=structs.column(0).as_any().downcast_ref::<arrow::array::StringArray>().unwrap();
    let mut context=Context::default();
    let node=context.decode(strings.value(0)).unwrap();
    let properties=context.graph.properties(&node,&["name".into()]);
    assert_eq!(properties.len(),1);
    assert_eq!(context.graph.element_public_id(&properties[0]),Value::Long(73));
    assert_eq!(context.graph.properties(&properties[0],&["since".into()]).len(),1);
    let Value::Node{label,id}=node else {panic!("node")};
    assert_eq!(context.graph.node_labels(&label,id),vec!["Employee".to_string(),"Person".to_string()]);
}

#[test]
fn detached_output_keeps_user_id_properties_separate_from_identity() {
    let graph = PropertyGraph::default();
    let node = graph.insert_node("Person", BTreeMap::from([("id".into(),Value::Int(42))]));
    let other = graph.insert_node("Person", BTreeMap::new());
    let edge = graph.insert_edge("KNOWS", &node, &other, BTreeMap::from([("id".into(),Value::Int(73))])).unwrap();
    let mut context = Context::default();
    context.capture(&Value::List(vec![node.clone(),edge.clone()]), &graph).unwrap();
    assert_eq!(json(&context.wire(&node)).unwrap()["properties"]["id"],json!({"type":"int","value":42}));
    assert_eq!(json(&context.wire(&edge)).unwrap()["properties"]["id"],json!({"type":"int","value":73}));
}

#[test]
fn nested_graph_reference_domains_keep_relationship_identity() {
    let fields = json!({"label":"Y","id":7,"src_label":"A","src_id":1,"dst_label":"B","dst_id":2});
    let reference = json!({"__orchiddb_domain_graph_edge": fields});
    let mut descriptor = fields.clone();
    descriptor["kind"] = json!("edge");
    descriptor["properties"] = json!({});
    let attached = json!({elements::FIELD: descriptor});
    assert_eq!(key(&json!([reference]), true).unwrap(), key(&json!([attached]), true).unwrap());
    let mut context = Context::default();
    let decoded = from_json(&reference, &mut context).unwrap();
    assert!(matches!(decoded, Value::Edge { ref rel_type, ref src_label, ref dst_label, .. }
        if rel_type == "Y" && src_label == "A" && dst_label == "B"));
    assert_eq!(from_json(&json!({"__orchiddb_domain_graph_node":null}), &mut context).unwrap(), Value::Null);
}

#[test]
fn temporal_domains_preserve_kinds_across_nested_sql_scalar_inputs() {
    let mut context = Context::default();
    for (kind, text) in [("date", "2020-01-02"), ("localtime", "12:31:14.645876123"), ("duration", "P1M2DT3S")] {
        let value = crate::ir::temporal::parse(kind, text).unwrap();
        let input = json!({"property": {"__orchiddb_domain_cypher_temporal": value.encode()}});
        let Value::Map(decoded) = from_json(&input, &mut context).unwrap() else { panic!("map") };
        assert_eq!(decoded["property"], Value::Temporal(value));
    }
    assert!(matches!(from_json(&json!({"__orchiddb_domain_cypher_temporal":null}), &mut context).unwrap(), Value::Null));
    let date = from_json(&json!({"__orchiddb_domain_cypher_temporal":"2020-01-02"}), &mut context).unwrap();
    assert_eq!(date, Value::Temporal(crate::ir::temporal::parse("date", "2020-01-02").unwrap()));
}

#[test]
fn detached_source_identity_survives_empty_native_property_state() {
    let mut graph = PropertyGraph::default();
    let node = graph.insert_node("Person", BTreeMap::new());
    graph.source_keys = true;
    let expected = graph.element_public_id(&node);
    let mut captured = Context::default();
    captured.capture(&node, &graph).unwrap();
    assert_eq!(captured.graph.element_public_id(&node), expected);
    let text = captured.wire(&node)[FIELD].as_str().unwrap().to_string();
    let mut decoded = Context::default();
    let value = decoded.decode(&text).unwrap();
    assert_eq!(decoded.graph.element_public_id(&value), expected);
}

#[test]
fn cypher_native_temporal_domains_validate_kind_payload_and_timestamp_scale() {
    for (kind, text) in [
        ("date", "2024-02-29"), ("localtime", "12:34:56.123456789"),
        ("time", "12:34:56+01:00"), ("localdatetime", "2024-02-29T12:34:56"),
        ("datetime", "2024-02-29T12:34:56Z"), ("duration", "P2DT3S"),
    ] {
        let temporal = crate::ir::temporal::parse(kind, text).unwrap();
        let key = format!("__orchiddb_domain_cypher_temporal_{kind}");
        for payload in [json!(temporal.encode()), serde_json::Value::Null] {
            let expected = if payload.is_null() { Value::Null } else { Value::Temporal(temporal.clone()) };
            assert_eq!(from_json(&json!({key.clone():payload}), &mut Context::default()).unwrap(), expected);
        }
        assert!(from_json(&json!({key.clone():42}), &mut Context::default()).unwrap_err().contains("Invalid temporal domain payload"));
        let other = if kind == "date" { "localtime" } else { "date" };
        let wrong = format!("__orchiddb_domain_cypher_temporal_{other}");
        assert!(from_json(&json!({wrong:temporal.encode()}), &mut Context::default()).unwrap_err().contains("declared kind"));
    }
    let fixed = crate::ir::temporal::parse("datetime", "2024-02-29T12:34:56+01:00").unwrap();
    assert_eq!(from_json(&json!({"__orchiddb_domain_cypher_temporal_datetime_fixed":fixed.encode()}), &mut Context::default()).unwrap(), Value::Temporal(fixed));
    for (scale, text) in [(1,"1969-12-31T23:59:59"), (1000,"1969-12-31T23:59:59.999"),
        (1_000_000,"1969-12-31T23:59:59.999999"), (1_000_000_000,"1969-12-31T23:59:59.999999999")] {
        for zoned in [false, true] {
            let text = if zoned { format!("{text}Z") } else { text.to_string() };
            let expected = crate::ir::temporal::parse(if zoned { "datetime" } else { "localdatetime" }, &text).unwrap();
            assert_eq!(from_json(&json!({"__orchiddb_timestamp":[-1,scale,zoned]}), &mut Context::default()).unwrap(), Value::Temporal(expected));
        }
    }
    for (payload, message) in [(json!([null,1,false]),"ticks"), (json!([1,2,false]),"scale"),
        (json!([1,1,null]),"zone"), (json!([i64::MAX,1,false]),"range")] {
        assert!(from_json(&json!({"__orchiddb_timestamp":payload}), &mut Context::default()).unwrap_err().contains(message));
    }
}

#[test]
fn cypher_native_numeric_transport_rejects_invalid_payloads_without_losing_precision() {
    for (kind, text, expected) in [
        ("TINYINT", "-128", Value::Byte(-128)),
        ("SMALLINT", "32767", Value::Short(32767)),
        ("INTEGER", "-2147483648", Value::Int(-2147483648)),
        // The shared value model stores Int as i64, including Cypher integers.
        ("INTEGER", "9223372036854775807", Value::Int(i64::MAX)),
        ("BIGINT", "9223372036854775807", Value::Long(i64::MAX)),
        ("HUGEINT", "9223372036854775807", Value::Long(i64::MAX)),
        ("HUGEINT", "9223372036854775808", Value::BigInt("9223372036854775808".parse().unwrap())),
        ("DECIMAL(38,9)", "12345678901234567890.123456789", Value::BigDecimal("12345678901234567890.123456789".parse().unwrap())),
        ("FLOAT", "1.5", Value::Float32(1.5)),
        ("DOUBLE", "-2.25", Value::Float(-2.25)),
    ] {
        let input = json!({"__orchiddb_number":[kind,text]});
        assert_eq!(from_json(&input, &mut Context::default()).unwrap(), expected);
        let encoded = Context::default().wire(&expected);
        assert_eq!(key(&input, true).unwrap(), key(&encoded, true).unwrap(), "{kind} lost numeric identity");
    }
    for (payload, message) in [
        (json!([null,"1"]), "Missing numeric type"), (json!(["BIGINT",null]), "Missing numeric value"),
        (json!(["TINYINT","128"]), "Invalid byte"), (json!(["SMALLINT","32768"]), "Invalid short"),
        (json!(["INTEGER","9223372036854775808"]), "Invalid integer"), (json!(["BIGINT","9223372036854775808"]), "Invalid long"),
        (json!(["HUGEINT","x"]), "Invalid huge integer"), (json!(["DECIMAL(38,9)","x"]), "Invalid decimal"),
        (json!(["FLOAT","x"]), "Invalid float"), (json!(["DOUBLE","x"]), "Invalid double"),
        (json!(["UNKNOWN","1"]), "Unsupported numeric type"),
    ] {
        let input = json!({"__orchiddb_number":payload});
        assert!(from_json(&input, &mut Context::default()).unwrap_err().contains(message));
        assert!(key(&input, true).is_err(), "malformed values must not acquire a Cypher equality key");
    }
    for (text, expected) in [("Infinity",f64::INFINITY),("-Infinity",f64::NEG_INFINITY),("NaN",f64::NAN)] {
        let Value::Float(value) = from_json(&json!({"__orchiddb_float":text}), &mut Context::default()).unwrap() else {panic!("float")};
        assert!(if expected.is_nan() {value.is_nan()} else {value == expected});
    }
    for payload in [json!("infinity"),json!(0),json!(null)] {
        assert!(from_json(&json!({"__orchiddb_float":payload}), &mut Context::default()).unwrap_err().contains("Invalid native float"));
    }
}

#[test]
fn cypher_native_evaluation_rejects_invalid_bindings_clocks_and_predicate_types() {
    let expr = serde_json::to_string(&IrExpr::binding("x")).unwrap();
    for value in [json!(true), json!(false), json!(null)] {
        assert_eq!(evaluate(&expr, &json!({"x":value.clone()}), true, 0, 0).unwrap(), value);
    }
    assert!(evaluate(&expr, &json!({"x":1}), true, 0, 0).unwrap_err().to_string().contains("boolean predicate"));
    for invalid in [json!(null),json!([]),json!(1)] {
        assert!(evaluate(&expr, &invalid, false, 0, 0).unwrap_err().to_string().contains("bindings must be a struct"));
    }
    for (statement,transaction,message) in [(i64::MAX,0,"statement"),(0,i64::MAX,"transaction")] {
        assert!(evaluate(&expr, &json!({"x":1}), false, statement, transaction).unwrap_err().to_string().contains(message));
    }
    assert!(evaluate("{", &json!({}), false, 0, 0).is_err());
    for (input,message) in [(json!({FIELD:42}),"Invalid native value storage"),
        (json!({FIELD:"not base64"}),""),
        (json!({"__cypher_scalar_tag":7,"integer":1,"string":null,"boolean":null}),"Invalid heterogeneous scalar tag")] {
        let error = evaluate(&expr, &json!({"x":input}), false, 0, 0).unwrap_err();
        assert!(!error.to_string().is_empty());
        if !message.is_empty() { assert!(error.to_string().contains(message)); }
    }
}

#[test]
fn cypher_native_literals_are_recognized_only_from_exact_constant_constructors() {
    let value = Value::Map(BTreeMap::from([
        ("list".into(), Value::List(vec![Value::Long(i64::MAX), Value::Null, Value::String("雪\0'".into())])),
        ("flag".into(), Value::Bool(false)),
    ]));
    let expression = literal(&value).unwrap();
    assert_eq!(literal_value(&expression), Some(value.clone()));
    assert_eq!(from_json(&Context::default().wire(&value), &mut Context::default()).unwrap(), value);
    for expression in [
        lit(1_i64), col_exact("runtime_value"), literal(&Value::Null).unwrap(),
        df_core::named_struct(vec![lit("ordinary_user_field"),lit(encode(&value))]),
        df_core::named_struct(vec![lit(FIELD),col_exact("runtime_value")]),
        df_core::named_struct(vec![lit(FIELD),lit("not base64")]),
        df_core::named_struct(vec![lit(FIELD),lit(STANDARD.encode([255_u8]))]),
    ] {
        assert_eq!(literal_value(&expression), None, "must not decode runtime or malformed constructor: {expression}");
    }
}

#[test]
fn cypher_native_list_transport_preserves_nested_values_and_null_cardinality() {
    for (input, expected) in [
        (json!(null), vec![]),
        (json!([]), vec![]),
        (json!([null]), vec![Value::Null]),
        (json!([1,null,[2,3],"雪"]), vec![Value::Int(1),Value::Null,
            Value::List(vec![Value::Int(2),Value::Int(3)]),Value::String("雪".into())]),
    ] {
        let result = items(&input).unwrap();
        let decoded = from_json(&result, &mut Context::default()).unwrap();
        assert_eq!(decoded, Value::List(expected));
    }
    let expression = serde_json::to_string(&IrExpr::binding("x")).unwrap();
    let nested = json!({"map":{"list":[null,true,"雪"]},"large":{"__orchiddb_number":["HUGEINT","9223372036854775808"]}});
    let encoded = evaluate(&expression, &json!({"x":nested.clone()}), false, -1, 1).unwrap();
    assert_eq!(from_json(&encoded, &mut Context::default()).unwrap(), from_json(&nested, &mut Context::default()).unwrap());
}
