
    use super::*;
    use arrow::array::Array as _;

    #[test]
    fn scalar_values_keep_width_and_null_validity() {
        for (value, expected) in [
            (Value::Byte(-12),ScalarValue::Int8(Some(-12))),
            (Value::Short(-300),ScalarValue::Int16(Some(-300))),
            (Value::Int(7),ScalarValue::Int32(Some(7))),
            (Value::Long(i64::MAX),ScalarValue::Int64(Some(i64::MAX))),
            (Value::UInt8(255),ScalarValue::UInt8(Some(255))),
            (Value::UInt16(65535),ScalarValue::UInt16(Some(65535))),
            (Value::UInt32(u32::MAX),ScalarValue::UInt32(Some(u32::MAX))),
            (Value::UInt64(u64::MAX),ScalarValue::UInt64(Some(u64::MAX))),
            (Value::Float32(1.25),ScalarValue::Float32(Some(1.25))),
            (Value::Float(-0.0),ScalarValue::Float64(Some(-0.0))),
            (Value::Bool(false),ScalarValue::Boolean(Some(false))),
            (Value::String("雪\0'".into()),ScalarValue::Utf8(Some("雪\0'".into()))),
        ] {
            let values = [Value::Null,value.clone(),Value::Null];
            let kind = infer_value_type(&values.iter().collect::<Vec<_>>()).unwrap();
            assert_eq!(kind,expected.data_type());
            assert_eq!(infer_property_data_type(&values),kind);
            let array=values_array(Language::Cypher,values.iter(),&kind).unwrap();
            assert_eq!(array.len(),3); assert_eq!(array.null_count(),2);
            assert_eq!(ScalarValue::try_from_array(&array,1).unwrap(),expected);
            assert!(array.is_null(0)); assert!(array.is_null(2));
        }
        let values=[Value::Scalar(ScalarValue::Int32(Some(8))),Value::Null];
        let array=values_array(Language::Cypher,values.iter(),&DataType::Int64).unwrap();
        assert_eq!(ScalarValue::try_from_array(&array,0).unwrap(),ScalarValue::Int64(Some(8)));
        assert!(array.is_null(1));
    }

    #[test]
    fn list_materialization_distinguishes_null_empty_and_null_elements() {
        use arrow::array::{Array, ListArray};
        let values=[Value::List(vec![]),Value::Null,Value::List(vec![Value::Int(1),Value::Null])];
        let field=Arc::new(Field::new("item",DataType::Int64,true));
        let array=values_array(Language::Cypher,values.iter(),&DataType::List(field)).unwrap();
        let list=array.as_any().downcast_ref::<ListArray>().unwrap();
        assert_eq!(list.value_offsets(),&[0,0,0,2]);
        assert!(!list.is_null(0)); assert!(list.is_null(1)); assert!(!list.is_null(2));
        assert_eq!(ScalarValue::try_from_array(list.values(),0).unwrap(),ScalarValue::Int64(Some(1)));
        assert!(list.values().is_null(1));
        assert_eq!(infer_value_type(&[]).unwrap(),DataType::Null);
        assert_eq!(infer_value_type(&[&Value::Null,&Value::Null]).unwrap(),DataType::Null);
        let nulls=values_array(Language::Cypher,[Value::Null,Value::Null].iter(),&DataType::Null).unwrap();
        assert_eq!(nulls.len(),2); assert_eq!(nulls.logical_null_count(),2);
    }

    #[test]
    fn incompatible_materialization_types_fail_instead_of_coercing_values() {
        for (value,kind) in [
            (Value::String("true".into()),DataType::Boolean),
            (Value::Float(1.5),DataType::Int64),
            (Value::String("1.5".into()),DataType::Float64),
            (Value::Int(-1),DataType::UInt64),
            (Value::Int(1),DataType::List(Arc::new(Field::new("item",DataType::Int64,true)))),
            (Value::Int(1),DataType::Binary),
        ] {
            assert!(values_array(Language::Cypher,[value].iter(),&kind).is_err(),"{kind}");
        }
        assert!(promote_type(DataType::Boolean,DataType::Int64).is_err());
        assert_eq!(promote_type(DataType::Int64,DataType::Float64).unwrap(),DataType::Float64);
        assert_eq!(promote_type(DataType::Float64,DataType::Int64).unwrap(),DataType::Float64);
        assert_eq!(promote_type(DataType::Boolean,DataType::Boolean).unwrap(),DataType::Boolean);
    }

    #[test]
    fn property_integer_conversion_rejects_overflow_and_float_truncation() {
        for value in [Value::Byte(7),Value::Short(7),Value::Int(7),Value::Long(7),
            Value::UInt8(7),Value::UInt16(7),Value::UInt32(7),Value::UInt64(7),
            Value::BigInt(7.into()),Value::UInt128(7.into())] {
            assert_eq!(value_to_int(&value),Some(7));
            assert_eq!(value_to_scalar(&value,&DataType::Int32,Language::Cypher).unwrap(),ScalarValue::Int32(Some(7)));
            assert_eq!(value_to_scalar(&value,&DataType::Int64,Language::Cypher).unwrap(),ScalarValue::Int64(Some(7)));
        }
        for value in [Value::UInt64(u64::MAX),Value::BigInt(num_bigint::BigInt::from(i64::MAX)+1),
            Value::Float(7.0),Value::String("7".into()),Value::Bool(true)] {
            assert_eq!(value_to_int(&value),None);
            assert!(value_to_scalar(&value,&DataType::Int64,Language::Cypher).is_err());
        }
        assert!(value_to_scalar(&Value::Long(i64::MAX),&DataType::Int32,Language::Cypher).is_err());
        for ty in [DataType::Boolean,DataType::Int32,DataType::Int64,DataType::Float64,DataType::Utf8] {
            assert_eq!(value_to_scalar(&Value::Null,&ty,Language::Cypher).unwrap(),ScalarValue::try_from(&ty).unwrap());
        }
    }

    #[test]
    fn property_projection_preserves_scalar_domains_and_requires_valid_metadata() {
        let native=ScalarValue::Decimal128(Some(123),10,2);
        assert_eq!(value_to_scalar(&Value::Scalar(native.clone()),&native.data_type(),Language::Cypher).unwrap(),native);
        assert!(value_to_scalar(&Value::Bool(true),&DataType::Float64,Language::Cypher).is_err());
        assert!(value_to_scalar(&Value::Int(1),&DataType::Boolean,Language::Cypher).is_err());
        assert!(value_to_scalar(&Value::Int(1),&DataType::Binary,Language::Cypher).is_err());
        for value in [Value::Float32(1.25),Value::Float(1.25),Value::BigDecimal("1.25".parse().unwrap())] {
            assert_eq!(materialized_value_to_f64(&value),Some(1.25));
        }
        let map=Value::Map(BTreeMap::from([("__tag".into(),Value::String("choice".into())),("field".into(),Value::Int(7))]));
        assert_eq!(union_tag_of(&map).as_deref(),Some("choice"));
        assert_eq!(struct_field_of(&map,"field"),Some(Value::Int(7)));
        assert_eq!(struct_field_of(&map,"missing"),None);
        for value in [Value::Null,Value::Map(BTreeMap::new()),Value::Map(BTreeMap::from([("__tag".into(),Value::Int(1))]))] {
            assert_eq!(union_tag_of(&value),None);
            assert_eq!(struct_field_of(&value,"field"),None);
        }
    }

    #[test]
    fn structured_property_discovery_uses_declared_order_and_skips_private_values() {
        let map = |entries: Vec<(&str,Value)>| Value::Map(entries.into_iter().map(|(k,v)|(k.into(),v)).collect());
        let rows = vec![None,Some("invalid".into()),
            Some(format!("{:?}",map(vec![("__tag",Value::String("union".into()))]))),
            Some(format!("{:?}",map(vec![("\0private",Value::Int(1))]))),
            Some(format!("{:?}",map(vec![("a",Value::Int(1)),("z",Value::Int(2)),
                (STRUCT_ORDER_KEY,Value::List(vec![Value::String("z".into()),Value::String("missing".into()),Value::Int(1),Value::String("a".into())]))]))),
        ];
        let field=Field::new("payload",DataType::Utf8,true).with_metadata(
            std::collections::HashMap::from([("orchiddb.value_type".into(),"value".into())]));
        let schema=Arc::new(Schema::new(vec![field]));
        let batch=RecordBatch::try_new(schema.clone(),vec![Arc::new(StringArray::from(rows))]).unwrap();
        let mut defs=BTreeMap::new();
        merge_property_defs(&mut defs,&schema,&[]).unwrap();
        merge_struct_field_defs(&mut defs,&batch).unwrap();
        assert!(defs["payload"].carries_union_tag);
        assert_eq!(defs["payload"].struct_fields,vec!["z","a"]);
        merge_struct_field_defs(&mut defs,&batch).unwrap();
        assert_eq!(defs["payload"].struct_fields,vec!["z","a"]);
        let incompatible=Schema::new(vec![Field::new("payload",DataType::Int64,true)]);
        assert!(merge_property_defs(&mut defs,&incompatible,&[]).unwrap_err().to_string().contains("mixed types"));
        merge_property_defs(&mut defs,&incompatible,&["payload"]).unwrap();
        assert!(list_shadow_candidates(&defs.into_values().collect::<Vec<_>>()).is_empty());
    }

    #[test]
    fn overlay_scans_preserve_tags_fields_and_null_properties_for_nodes_and_edges() {
        let graph=PropertyGraph::new();
        let props=BTreeMap::from([("payload".into(),Value::Map(BTreeMap::from([
            ("__tag".into(),Value::String("choice".into())),("field".into(),Value::Int(7)),
        ])))]);
        let a=graph.insert_node("P",props.clone());
        let b=graph.insert_node("P",BTreeMap::new());
        graph.insert_edge("R",&a,&b,props).unwrap();
        graph.insert_edge("R",&b,&a,BTreeMap::new()).unwrap();
        let defs=vec![PropertyDef{name:"payload".into(),data_type:DataType::Utf8,
            carries_union_tag:true,struct_fields:vec!["field".into(),"missing".into()]}];
        for edge in [false,true] {
            let ids=if edge {graph.edge_ids("R")} else {graph.node_ids("P").unwrap()};
            let batch=if edge {
                materialize_edge_scan_batch("v","R",&ids,&graph,&defs,edge_schema("v",&defs),Language::Cypher).unwrap()
            } else {
                materialize_node_scan_batch("v","P",&ids,&graph,&defs,node_schema("v",&defs),Language::Cypher).unwrap()
            };
            let cell=|name: String,row|ScalarValue::try_from_array(batch.column(batch.schema().index_of(&name).unwrap()),row).unwrap();
            assert_eq!(batch.num_rows(),2);
            assert_eq!(cell(union_tag_col("v","payload"),0),ScalarValue::Utf8(Some("choice".into())));
            assert_eq!(cell(struct_field_col("v","payload","field"),0),ScalarValue::Utf8(Some("7".into())));
            assert!(cell(struct_field_col("v","payload","missing"),0).is_null());
            assert!(cell(prop_col("v","payload"),1).is_null());
            assert!(cell(union_tag_col("v","payload"),1).is_null());
            assert!(materialize_property_array(&graph,edge,if edge {"R"} else {"P"},&ids,"payload",&DataType::Int8,Language::Cypher)
                .unwrap_err().to_string().contains("overlay data"));
        }
        assert!(materialize_edge_scan_batch("v","R",&[999_i64.into()],&graph,&defs,edge_schema("v",&defs),Language::Cypher)
            .unwrap_err().to_string().contains("has no endpoints"));
    }

#[test]
fn encoded_struct_fields_preserve_values_and_treat_absent_or_invalid_members_as_null() {
    let encode = |value: Value| Some(format!("{value:?}"));
    let values = vec![None, Some("not a value".into()), encode(Value::Int(7)),
        encode(Value::Map(BTreeMap::new())),
        encode(Value::Map(BTreeMap::from([("field".into(), Value::Null)]))),
        encode(Value::Map(BTreeMap::from([("field".into(), Value::Long(i64::MAX)), ("__tag".into(), Value::String("choice".into()))]))),
        encode(Value::Map(BTreeMap::from([("field".into(), Value::Bool(true)), ("__tag".into(), Value::Int(7))]))),
    ];
    let batch = RecordBatch::try_from_iter(vec![("payload", Arc::new(StringArray::from(values)) as ArrayRef)]).unwrap();
    let fields = property_struct_field_array(&batch, "payload", "field", 7, Language::Cypher).unwrap();
    let expected = [None, None, None, None, None, Some(i64::MAX.to_string()), Some("True".into())];
    for (row, value) in expected.into_iter().enumerate() {
        assert_eq!(ScalarValue::try_from_array(&fields, row).unwrap(), ScalarValue::Utf8(value));
    }
    let tags = property_union_tag_array(&batch, "payload", 7).unwrap();
    assert_eq!(tags.null_count(), 6);
    assert_eq!(ScalarValue::try_from_array(&tags, 5).unwrap(), ScalarValue::Utf8(Some("choice".into())));
    let absent = property_struct_field_array(&batch, "payload", "absent", 7, Language::Cypher).unwrap();
    assert_eq!(absent.null_count(), 7);
}

#[test]
fn encoded_property_fields_validate_source_types_and_missing_column_cardinality() {
    let batch = RecordBatch::try_from_iter(vec![("payload", Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef)]).unwrap();
    assert!(property_struct_field_array(&batch, "payload", "x", 2, Language::Cypher).unwrap_err().to_string().contains("is not text"));
    assert!(property_union_tag_array(&batch, "payload", 2).unwrap_err().to_string().contains("not a string column"));
    for rows in [0, 3] {
        for values in [property_struct_field_array(&batch, "missing", "x", rows, Language::Cypher).unwrap(),
            property_union_tag_array(&batch, "missing", rows).unwrap()] {
            assert_eq!(values.len(), rows);
            assert_eq!(values.null_count(), rows);
        }
    }
}

#[test]
fn base_property_encoding_preserves_raw_text_and_rejects_invalid_storage_types() {
    let metadata = std::collections::HashMap::from([
        ("orchiddb.value_type".into(), "value".into())]);
    let texts = Arc::new(StringArray::from(vec![Some("unencoded text"), None,
        Some("Int(7)")])) as ArrayRef;
    let batch = RecordBatch::try_new(Arc::new(Schema::new(vec![
        Field::new("payload", DataType::Utf8, true).with_metadata(metadata.clone())])),
        vec![texts]).unwrap();
    let decoded = property_array(&batch, "payload", &DataType::Utf8, 3, Language::Cypher).unwrap();
    assert_eq!(decoded.to_data(), StringArray::from(vec![Some("unencoded text"), None, Some("7")]).to_data());
    let error = property_array(&batch, "payload", &DataType::Int64, 3, Language::Cypher).unwrap_err();
    assert!(error.to_string().contains("expected Int64"), "{error}");
    let malformed = RecordBatch::try_new(Arc::new(Schema::new(vec![
        Field::new("payload", DataType::Int64, false).with_metadata(metadata)])),
        vec![Arc::new(Int64Array::from(vec![7]))]).unwrap();
    let error = property_array(&malformed, "payload", &DataType::Int64, 1, Language::Cypher).unwrap_err();
    assert!(error.to_string().contains("not a string column"), "{error}");
    for rows in [0, 3] {
        let absent = property_array(&batch, "absent", &DataType::UInt64, rows, Language::Cypher).unwrap();
        assert_eq!(absent.data_type(), &DataType::UInt64);
        assert_eq!(absent.len(), rows);
        assert_eq!(absent.null_count(), rows);
    }
}

#[test]
fn base_table_normalization_retains_structured_properties_and_endpoint_identity() {
    let payload = Value::Map(BTreeMap::from([("field".into(), Value::Int(7)), ("__tag".into(), Value::String("choice".into()))]));
    let values = Arc::new(StringArray::from(vec![Some(format!("{payload:?}")), None])) as ArrayRef;
    let field = Field::new("payload", DataType::Utf8, true).with_metadata(
        std::collections::HashMap::from([("orchiddb.value_type".into(), "value".into())]));
    let nodes = NodeTable { label: "Alpha".into(), batch: RecordBatch::try_new(Arc::new(Schema::new(vec![field.clone()])), vec![values.clone()]).unwrap() };
    let edges = EdgeTable { rel_type: "R".into(), src_label: "Alpha".into(), dst_label: "Beta".into(),
        batch: RecordBatch::try_new(Arc::new(Schema::new(vec![Field::new("__src_id", DataType::Int64, false), Field::new("__dst_id", DataType::Int64, false), field])),
            vec![Arc::new(Int64Array::from(vec![0, 1])), Arc::new(Int64Array::from(vec![1, 0])), values]).unwrap() };
    let props = vec![PropertyDef { name: "payload".into(), data_type: DataType::Utf8,
        carries_union_tag: true, struct_fields: vec!["field".into(), "missing".into()] }];
    for (output, first_id) in [
        (normalize_node_table("v", &nodes, &props, node_schema("v", &props), Language::Cypher).unwrap(), 0),
        (normalize_edge_table("v", &edges, 40, &props, edge_schema("v", &props), Language::Cypher).unwrap(), 40),
    ] {
        let cell = |name: String, row| ScalarValue::try_from_array(output.column(output.schema().index_of(&name).unwrap()), row).unwrap();
        assert_eq!(output.num_rows(), 2);
        assert_eq!(cell(id_col("v"), 0), ScalarValue::Int64(Some(first_id)));
        assert_eq!(cell(id_col("v"), 1), ScalarValue::Int64(Some(first_id + 1)));
        assert_eq!(cell(struct_field_col("v", "payload", "field"), 0), ScalarValue::Utf8(Some("7".into())));
        assert_eq!(cell(union_tag_col("v", "payload"), 0), ScalarValue::Utf8(Some("choice".into())));
        assert!(cell(struct_field_col("v", "payload", "missing"), 0).is_null());
        assert!(cell(struct_field_col("v", "payload", "field"), 1).is_null());
        if first_id == 40 {
            assert_eq!(cell(src_id_col("v"), 0), ScalarValue::Int64(Some(0)));
            assert_eq!(cell(dst_id_col("v"), 0), ScalarValue::Int64(Some(1)));
            assert_eq!(cell(src_label_col("v"), 0), ScalarValue::Utf8(Some("Alpha".into())));
            assert_eq!(cell(dst_label_col("v"), 0), ScalarValue::Utf8(Some("Beta".into())));
        }
    }
}

#[test]
fn base_edge_normalization_reports_missing_endpoint_columns() {
    for present in ["__src_id", "__dst_id"] {
        let batch = RecordBatch::try_from_iter(vec![(present, Arc::new(Int64Array::from(vec![0])) as ArrayRef)]).unwrap();
        let table = EdgeTable { rel_type: "R".into(), src_label: "P".into(), dst_label: "P".into(), batch };
        let error = normalize_edge_table("r", &table, 0, &[], edge_schema("r", &[]), Language::Cypher).unwrap_err().to_string();
        let missing = if present == "__src_id" { "__dst_id" } else { "__src_id" };
        assert!(error.contains(&format!("missing {missing}")), "{error}");
    }
}

#[test]
fn base_property_fallback_preserves_exact_arrow_cells_for_nodes_and_edges() {
    use arrow::array::{Int8Array, UInt64Array, Float32Array};
    for column in [
        Arc::new(Int8Array::from(vec![Some(-128), None])) as ArrayRef,
        Arc::new(UInt64Array::from(vec![Some(u64::MAX), None])) as ArrayRef,
        Arc::new(Float32Array::from(vec![Some(-0.0), None])) as ArrayRef,
    ] {
        let mut graph = PropertyGraph::new();
        graph.add_nodes(NodeTable { label: "P".into(), batch:
            RecordBatch::try_from_iter(vec![("payload", column.clone())]).unwrap() });
        graph.add_edges(EdgeTable { rel_type: "R".into(), src_label: "P".into(), dst_label: "P".into(), batch:
            RecordBatch::try_from_iter(vec![
                ("__src_id", Arc::new(Int64Array::from(vec![0, 1])) as ArrayRef),
                ("__dst_id", Arc::new(Int64Array::from(vec![1, 0])) as ArrayRef),
                ("payload", column.clone()),
            ]).unwrap() }).unwrap();
        for (edge, element) in [(false, "P"), (true, "R")] {
            let result = materialize_property_array(&graph, edge, element,
                &[0.into(), 1.into()], "payload", column.data_type(), Language::Cypher).unwrap();
            assert_eq!(result.to_data(), column.to_data());
            let error = materialize_property_array(&graph, edge, element,
                &[0.into()], "absent", column.data_type(), Language::Cypher).unwrap_err();
            assert!(error.to_string().contains("missing from its base schema"), "{error}");
        }
        graph.set_property(&Value::Node { label: "P".into(), id: 0.into() },
            "payload", Value::String("changed".into())).unwrap();
        let error = materialize_property_array(&graph, false, "P", &[0.into()],
            "payload", column.data_type(), Language::Cypher).unwrap_err();
        assert!(error.to_string().contains("overlay data"), "{error}");
        let untouched = materialize_property_array(&graph, false, "P", &[1.into()],
            "payload", column.data_type(), Language::Cypher).unwrap();
        assert_eq!(untouched.data_type(), column.data_type());
        assert!(untouched.is_null(0));
    }
}

fn cypher_scan_context(graph: &PropertyGraph) -> LoweringContext<'_> {
    LoweringContext {
        island_memo: None, graph, options: RelBackendOptions::default(),
        policy: GraphPlanPolicy::cypher(), language: Language::Cypher, scan_counter: 0,
        correlate_plan: None, rdf_typed_terms_used: false, scan_logical_labels: false,
        gremlin_label_binds: Default::default(), unordered_collects: Default::default(),
        unordered_distincts: Default::default(),
    }
}

#[test]
fn cypher_scan_labels_are_deduplicated_and_unsupported_predicates_decline() {
    let graph = PropertyGraph::new();
    let ctx = cypher_scan_context(&graph);
    let repeated = LabelExpr::AnyOf(vec!["B".into(), "A".into(), "A".into()]);
    assert_eq!(ctx.node_labels(&repeated).unwrap(), vec!["A", "B"]);
    assert_eq!(ctx.rel_types(&repeated).unwrap(), vec!["A", "B"]);
    assert_eq!(ctx.node_labels(&LabelExpr::AllOf(vec!["A".into()])).unwrap(), vec!["A"]);
    assert_eq!(ctx.rel_types(&LabelExpr::AllOf(vec!["R".into()])).unwrap(), vec!["R"]);
    for labels in [vec![], vec!["A".into(), "B".into()]] {
        assert!(ctx.node_labels(&LabelExpr::AllOf(labels.clone())).unwrap().is_empty());
        assert!(ctx.rel_types(&LabelExpr::AllOf(labels)).unwrap_err().to_string().contains("multi-type"));
    }
    let negated = LabelExpr::Not(Box::new(LabelExpr::label("A")));
    assert!(ctx.node_labels(&negated).unwrap_err().to_string().contains("negated label"));
    assert!(ctx.rel_types(&negated).unwrap_err().to_string().contains("negated relationship"));
    graph.insert_node("B", BTreeMap::new());
    graph.insert_node("A", BTreeMap::new());
    assert_eq!(ctx.node_labels(&LabelExpr::AllOf(vec!["A".into(), "B".into()])).unwrap(), vec!["A", "B"]);
}

#[test]
fn cypher_overlay_properties_decline_incompatible_types_across_elements() {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("A", BTreeMap::from([("payload".into(), Value::Int(7))]));
    let b = graph.insert_node("B", BTreeMap::from([("payload".into(), Value::String("seven".into()))]));
    graph.insert_edge("R", &a, &b, BTreeMap::from([("payload".into(), Value::Int(7))])).unwrap();
    graph.insert_edge("S", &b, &a, BTreeMap::from([("payload".into(), Value::String("seven".into()))])).unwrap();
    let ctx = cypher_scan_context(&graph);
    for labels in [vec!["A".into(), "B".into()], vec!["B".into(), "A".into()]] {
        assert!(ctx.node_property_defs(&labels).unwrap_err().to_string().contains("incompatible types across labels"));
    }
    for types in [vec!["R".into(), "S".into()], vec!["S".into(), "R".into()]] {
        assert!(ctx.edge_property_defs(&types).unwrap_err().to_string().contains("incompatible types across relationship types"));
    }
    assert_eq!(ctx.node_property_defs(&["A".into()]).unwrap()[0].data_type, DataType::Int32);
    assert_eq!(ctx.edge_property_defs(&["S".into()]).unwrap()[0].data_type, DataType::Utf8);
}

#[test]
fn cypher_temporal_properties_require_lossless_transport() {
    use crate::ir::temporal::{TemporalValue, transport};
    let date = TemporalValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());
    let value = Value::Temporal(date.clone());
    for kind in [DataType::Date32, transport::scalar(&date).unwrap().data_type(),
        transport::encoded_scalar(&date).unwrap().data_type()] {
        let result = value_to_scalar(&value, &kind, Language::Cypher).unwrap();
        assert_eq!(result.data_type(), kind);
        assert_eq!(transport::decode(&result).unwrap(), date);
        assert_eq!(value_to_scalar(&Value::Null, &kind, Language::Cypher).unwrap(),
            ScalarValue::try_from(&kind).unwrap());
        for invalid in [Value::String("2024-02-29".into()), Value::Int(19782)] {
            let error = value_to_scalar(&invalid, &kind, Language::Cypher).unwrap_err();
            assert!(error.to_string().contains("requires temporal or null"), "{error}");
        }
    }
    for nested in [value.clone(), Value::List(vec![value.clone()]),
        Value::Map(BTreeMap::from([("date".into(), value)]))] {
        let error = value_to_scalar(&nested, &DataType::Utf8, Language::Cypher).unwrap_err();
        assert!(error.to_string().contains("require a residual kernel"), "{error}");
    }
}

#[test]
fn cypher_property_inference_preserves_temporals_and_declines_mixed_scalars() {
    use crate::ir::temporal::{TemporalValue, transport};
    let date = TemporalValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());
    for (values, expected) in [
        (vec![Value::Null, Value::Temporal(date.clone())], Some(transport::scalar(&date).unwrap().data_type())),
        (vec![Value::Int(1), Value::Long(i64::MAX)], None),
        (vec![Value::Temporal(date), Value::String("2024-02-29".into())], None),
        (vec![Value::UInt64(u64::MAX), Value::Float(1.0)], None),
        (vec![Value::Null, Value::Null], Some(DataType::Utf8)),
    ] {
        let graph = PropertyGraph::new();
        let endpoints: Vec<_> = values.iter().map(|value| graph.insert_node("P",
            BTreeMap::from([("payload".into(), value.clone())]))).collect();
        for value in &values {
            graph.insert_edge("R", &endpoints[0], &endpoints[1],
                BTreeMap::from([("payload".into(), value.clone())])).unwrap();
        }
        for (edge, element) in [(false, "P"), (true, "R")] {
            let result = infer_element_property_type(&graph, edge, element, "payload", Language::Cypher);
            match &expected {
                Some(kind) => assert_eq!(&result.unwrap(), kind),
                None => assert!(result.unwrap_err().to_string().contains("Heterogeneous Cypher properties")),
            }
        }
    }
}

#[test]
fn cypher_structured_property_discovery_handles_implicit_order_and_bad_metadata() {
    let metadata = std::collections::HashMap::from([("orchiddb.value_type".into(), "value".into())]);
    let map = Value::Map(BTreeMap::from([
        ("z".into(), Value::Int(1)), ("a".into(), Value::Null),
        ("__internal".into(), Value::Int(9)),
    ]));
    let batch = RecordBatch::try_new(Arc::new(Schema::new(vec![
        Field::new("payload", DataType::Utf8, true).with_metadata(metadata.clone())])),
        vec![Arc::new(StringArray::from(vec![format!("{map:?}")]))]).unwrap();
    let mut defs = BTreeMap::new();
    merge_property_defs(&mut defs, batch.schema().as_ref(), &[]).unwrap();
    merge_struct_field_defs(&mut defs, &batch).unwrap();
    assert_eq!(defs["payload"].struct_fields, vec!["a", "z"]);
    let malformed = RecordBatch::try_new(Arc::new(Schema::new(vec![
        Field::new("payload", DataType::Int64, false).with_metadata(metadata)])),
        vec![Arc::new(Int64Array::from(vec![7]))]).unwrap();
    let error = merge_struct_field_defs(&mut defs, &malformed).unwrap_err();
    assert!(error.to_string().contains("structured property `payload` is not text"), "{error}");
}

#[test]
fn zero_column_values_preserve_bag_cardinality_and_validate_argument_widths() {
    for count in [0,1,3] {
        let rows=vec![vec![];count];
        let values=values_batch(Language::Cypher,&[],&rows).expect("zero-column VALUES is a bag of unit rows");
        assert_eq!(values.num_columns(),0);
        assert_eq!(values.num_rows(),count);
        let arguments=crate::ir::rel::host::function_arguments(0,&rows).unwrap();
        assert_eq!(arguments.num_columns(),0);
        assert_eq!(arguments.num_rows(),count);
    }
    for rows in [vec![vec![Value::Int(1)]],vec![vec![],vec![Value::Null]]] {
        assert!(values_batch(Language::Cypher,&[],&rows).is_err());
        assert!(crate::ir::rel::host::function_arguments(0,&rows).is_err());
    }
}

#[test]
fn source_column_lookup_requires_an_unambiguous_case_insensitive_match() {
    let schema=Schema::new(vec![Field::new("Id",DataType::Int64,true),Field::new("ID",DataType::Int64,true),Field::new("Name",DataType::Utf8,true)]);
    assert_eq!(schema_index(&schema,"Id"),Some(0));
    assert_eq!(schema_index(&schema,"ID"),Some(1));
    assert_eq!(schema_index(&schema,"id"),None);
    assert_eq!(schema_index(&schema,"name"),Some(2));
    assert_eq!(schema_index(&schema,"NAME"),Some(2));
    assert_eq!(schema_index(&schema,"missing"),None);
    assert_eq!(schema_index(&Schema::empty(),"id"),None);
}

fn zero_column_count_plan(count:usize,native_values:bool,mapped:bool) -> LoweredPlan {
    let root=Node::GraphReturn {fields:vec!["n".into()],result_form:ResultForm::RowSet,
        input:Node::GraphAggregate {group:vec![],aggs:vec![AggCall {kind:AggKind::CountRows,alias:"n".into(),arg:None,distinct:false}],fields:vec!["n".into()],
            input:Node::GraphValues {bindings:vec![],rows:vec![vec![];count],bulk:None}.boxed(),
        }.boxed(),
    };
    let plan=GraphPlan::new(GraphPlanPolicy::cypher(),root);
    let graph=PropertyGraph::new();
    RelBackend::with_options(RelBackendOptions {native_values,mapping:mapped.then(||Arc::new(Default::default())),..Default::default()}).lower(&plan,&graph).unwrap()
}

#[tokio::test]
async fn zero_column_values_lowering_preserves_count_in_sql() {
    for count in [0,1,3] {
        for native_values in [false,true] {
            for mapped in [false,true] {
                let lowered=zero_column_count_plan(count,native_values,mapped);
                let result=crate::ir::rel::sql::execute_lowered_sql(&mut crate::ir::rel::sql::DuckDbExecutor::default(),&lowered).await.unwrap();
                assert_eq!(result.batch.num_rows(),1);
                assert_eq!(arrow::util::display::array_value_to_string(result.batch.column(0),0).unwrap(),count.to_string(),"native={native_values} mapped={mapped}");
            }
        }
    }
}

#[cfg(feature="trino")]
#[tokio::test]
#[ignore="requires PostgreSQL and Trino"]
async fn zero_column_values_execute_on_postgres_and_trino() {
    use crate::ir::rel::sql::{SqlDialect,unparse_plan};
    use crate::execution::trino::{TrinoOptions,TrinoSession};
    use futures::StreamExt;
    let pg=std::env::var("GRAPH_PG_URL").expect("GRAPH_PG_URL");
    let endpoint=std::env::var("ORCHIDDB_TEST_TRINO_URL").or_else(|_|std::env::var("ORCHIDDB_TEST_OPA_URL")).expect("Trino URL");
    let trino=TrinoSession::source(TrinoOptions::new(endpoint,"orchid_test")).unwrap();
    for count in [0,1,3] {
        for native in [false,true] {
            let plan=zero_column_count_plan(count,native,true).plan;
            let sql=unparse_plan(plan.clone(),SqlDialect::Postgres).unwrap();
            let result=std::process::Command::new("psql").args([&pg,"-X","-A","-t","-v","ON_ERROR_STOP=1","-c",&sql]).output().unwrap();
            assert!(result.status.success(),"{sql}: {}",String::from_utf8_lossy(&result.stderr));
            assert_eq!(String::from_utf8(result.stdout).unwrap().trim(),count.to_string());
            let sql=unparse_plan(plan,SqlDialect::resolve("trino").unwrap()).unwrap();
            let mut stream=trino.execute(&sql,vec![]).await.unwrap();
            let mut rows=vec![];
            while let Some(batch)=stream.next().await {
                let batch=batch.unwrap();
                for index in 0..batch.num_rows() {rows.push(arrow::util::display::array_value_to_string(batch.column(0).as_ref(),index).unwrap());}
            }
            assert_eq!(rows,vec![count.to_string()],"{sql}");
        }
    }
}

#[test]
fn graph_values_lowering_rejects_width_mismatches_before_native_literals() {
    let map=Value::Map(BTreeMap::from([("a".into(),Value::Int(1))]));
    for native_values in [false,true] {
        for (bindings,rows) in [
            (vec!["x".into()],vec![vec![map.clone(),Value::Int(2)]]),
            (vec!["x".into(),"y".into()],vec![vec![map.clone()]]),
            (vec!["x".into()],vec![vec![map.clone()],vec![]]),
            (vec!["x".into()],vec![vec![Value::Int(1),Value::Int(2)]]),
        ] {
            let plan=GraphPlan::new(GraphPlanPolicy::cypher(),Node::GraphReturn {
                fields:vec!["x".into()],result_form:ResultForm::RowSet,
                input:Node::GraphValues{bindings,rows,bulk:None}.boxed(),
            });
            let result=RelBackend::with_options(RelBackendOptions{native_values,..Default::default()}).lower(&plan,&PropertyGraph::new());
            let error=result.err().expect("malformed VALUES rows must not be silently zipped to their bindings").to_string();
            assert!(error.contains("GraphValues row width does not match bindings"),"{error}");
        }
    }
}

#[test]
fn nominal_property_values_preserve_domains_and_refuse_storage_coercions() {
    use crate::ir::functions::domain;
    for (name, storage, plain) in [
        ("json", ScalarValue::Utf8(Some("{\"key\":1}".into())), Value::String("{\"key\":1}".into())),
        ("cypher_integer", ScalarValue::Decimal128(Some(9007199254740993), 38, 0), Value::Long(9007199254740993)),
    ] {
        let scalar = domain::scalar(name, storage.clone()).unwrap();
        let kind = scalar.data_type();
        let null = ScalarValue::try_from(&kind).unwrap();
        for (value, expected) in [
            (Value::Scalar(scalar.clone()), scalar),
            (Value::Null, null.clone()),
            (Value::Scalar(null.clone()), null),
        ] {
            assert_eq!(value_to_scalar(&value, &kind, Language::Cypher).unwrap(), expected);
        }
        for value in [plain, Value::Scalar(storage.clone()),
            Value::Scalar(domain::scalar("different_domain", storage).unwrap())] {
            assert!(value_to_scalar(&value, &kind, Language::Cypher).is_err(),
                "nominal type must not be inferred from storage: {name} {value:?}");
        }
    }
}

#[test]
fn node_and_edge_overlay_materialization_preserves_nominal_values_and_missing_properties() {
    use crate::ir::functions::domain;
    for scalar in [domain::json_scalar("{\"key\":1}").unwrap(),
        domain::scalar("cypher_integer", ScalarValue::Decimal128(Some(9007199254740993), 38, 0)).unwrap()] {
        let graph = PropertyGraph::new();
        let properties = [
            BTreeMap::from([("payload".into(), Value::Scalar(scalar.clone()))]),
            BTreeMap::from([("payload".into(), Value::Null)]),
            BTreeMap::new(),
        ];
        let nodes: Vec<_> = properties.iter().map(|p| graph.insert_node("P", p.clone())).collect();
        for p in &properties { graph.insert_edge("R", &nodes[0], &nodes[1], p.clone()).unwrap(); }
        let ctx = cypher_scan_context(&graph);
        for edge in [false, true] {
            let definitions = if edge { ctx.edge_property_defs(&["R".into()]) } else { ctx.node_property_defs(&["P".into()]) }.unwrap();
            assert_eq!(definitions.len(), 1);
            assert_eq!(definitions[0].data_type, scalar.data_type());
            let batch = if edge {
                materialize_edge_scan_batch("v", "R", &graph.edge_ids("R"), &graph, &definitions,
                    edge_schema("v", &definitions), Language::Cypher)
            } else {
                materialize_node_scan_batch("v", "P", &graph.node_ids("P").unwrap(), &graph, &definitions,
                    node_schema("v", &definitions), Language::Cypher)
            }.unwrap();
            let values = batch.column_by_name(&prop_col("v", "payload")).unwrap();
            assert_eq!(values.len(), 3);
            assert_eq!(values.null_count(), 2);
            assert_eq!(ScalarValue::try_from_array(values, 0).unwrap(), scalar);
            assert!(values.is_null(1));
            assert!(values.is_null(2));
            let wrong_domain = domain::data_type("different_domain", domain::descriptor(&scalar.data_type()).unwrap().1.clone());
            let ids = if edge { graph.edge_ids("R") } else { graph.node_ids("P").unwrap() };
            assert!(materialize_property_array(&graph, edge, if edge { "R" } else { "P" },
                &ids, "payload", &wrong_domain, Language::Cypher).is_err());
        }
    }
}

#[test]
fn mixed_temporal_property_kinds_preserve_values_in_both_scan_orders() {
    use crate::ir::temporal::{TemporalValue, transport};
    let date = Value::Temporal(TemporalValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()));
    let time = Value::Temporal(TemporalValue::LocalTime(chrono::NaiveTime::from_hms_nano_opt(12, 34, 56, 123456789).unwrap()));
    for values in [[date.clone(), time.clone()], [time, date]] {
        let graph = PropertyGraph::new();
        let nodes: Vec<_> = values.iter().map(|v| graph.insert_node("P",
            BTreeMap::from([("payload".into(), v.clone())]))).collect();
        for value in &values { graph.insert_edge("R", &nodes[0], &nodes[1],
            BTreeMap::from([("payload".into(), value.clone())])).unwrap(); }
        for (edge, label) in [(false, "P"), (true, "R")] {
            let kind = infer_element_property_type(&graph, edge, label, "payload", Language::Cypher).unwrap();
            assert!(transport::is_type(&kind));
            let ids = if edge { graph.edge_ids(label) } else { graph.node_ids(label).unwrap() };
            let array = materialize_property_array(&graph, edge, label, &ids, "payload", &kind, Language::Cypher).unwrap();
            for (row, expected) in values.iter().enumerate() {
                let Value::Temporal(expected) = expected else { panic!() };
                let actual = ScalarValue::try_from_array(&array, row).unwrap();
                assert_eq!(&transport::decode(&actual).unwrap(), expected);
            }
        }
    }
}

#[test]
fn narrow_graph_values_reject_out_of_range_values_without_turning_them_into_nulls() {
    for (kind,min,max) in [
        (DataType::Int8,i8::MIN as i64,i8::MAX as i64),
        (DataType::Int16,i16::MIN as i64,i16::MAX as i64),
        (DataType::Int32,i32::MIN as i64,i32::MAX as i64),
        (DataType::UInt8,0,u8::MAX as i64),
        (DataType::UInt16,0,u16::MAX as i64),
        (DataType::UInt32,0,u32::MAX as i64),
    ] {
        for native in [false,true] {
            let value=|v|if native {Value::Scalar(ScalarValue::Int64(Some(v)))} else {Value::Long(v)};
            let valid=vec![Value::Null,value(min),value(max)];
            let array=values_array(Language::Cypher,valid.iter(),&kind).unwrap();
            assert_eq!(array.data_type(),&kind);assert_eq!(array.null_count(),1);
            assert!(array.is_null(0));
            for (row,expected) in [(1,min),(2,max)] {
                assert_eq!(ScalarValue::try_from_array(&array,row).unwrap().cast_to(&DataType::Int64).unwrap(),ScalarValue::Int64(Some(expected)));
            }
            for bad in [min-1,max+1] {
                let values=vec![Value::Null,value(min),value(bad),value(max)];
                assert!(values_array(Language::Cypher,values.iter(),&kind).is_err(),"{kind:?} accepted {bad}; native={native}");
            }
        }
    }
}

#[test]
fn graph_values_keep_zero_column_cardinality_and_validate_every_row_width() {
    for count in [0,1,3] {
        let batch=values_batch(Language::Cypher,&[],&vec![vec![];count]).unwrap();
        assert_eq!(batch.num_columns(),0);assert_eq!(batch.num_rows(),count);
    }
    let bindings=vec!["a".into(),"b".into()];
    for row in [vec![],vec![Value::Int(1)],vec![Value::Null;3]] {
        let error=values_batch(Language::Cypher,&bindings,&[vec![Value::Null;2],row]).unwrap_err().to_string();
        assert!(error.contains("row width does not match bindings"),"{error}");
    }
    let batch=values_batch(Language::Cypher,&bindings,&vec![vec![Value::Null;2];3]).unwrap();
    assert_eq!(batch.num_rows(),3);
    for column in batch.columns(){assert_eq!(column.data_type(),&DataType::Null);assert_eq!(column.logical_null_count(),3);}
}
