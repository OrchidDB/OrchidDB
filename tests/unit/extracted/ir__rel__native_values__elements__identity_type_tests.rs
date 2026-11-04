
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
