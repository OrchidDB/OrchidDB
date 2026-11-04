
    use super::*;

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
