
    use super::*;
    #[test]
    fn display_preserves_null_nodes_property_order_and_empty_values() {
        use arrow::array::StringArray;
        let strings = |values: Vec<Option<&str>>| -> ArrayRef { Arc::new(StringArray::from(values)) };
        let properties: ArrayRef = Arc::new(StructArray::from(vec![
            (Arc::new(Field::new("z", DataType::Utf8, true)), strings(vec![Some(""), None, Some("ignored")])),
            (Arc::new(Field::new("a", DataType::Utf8, true)), strings(vec![Some("True"), Some("text"), None])),
        ]));
        let output = evaluate(NODE_TEXT, &[
            strings(vec![Some("0"); 3]), strings(vec![Some("7"), Some("8"), None]),
            strings(vec![Some("P"); 3]), properties,
        ], 3).unwrap();
        let output = output.as_any().downcast_ref::<StringArray>().unwrap();
        assert_eq!(output.value(0), "{_ID: 0:7, _LABEL: P, z: , a: True}");
        assert_eq!(output.value(1), "{_ID: 0:8, _LABEL: P, a: text}");
        assert!(output.is_null(2));
    }
