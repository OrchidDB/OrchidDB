
    use super::*;
    #[test]
    fn distinct_collection_keeps_first_order_and_omits_nulls() {
        let input = ScalarValue::List(ScalarValue::new_list_nullable(&[
            ScalarValue::Int64(Some(3)), ScalarValue::Int64(None),
            ScalarValue::Int64(Some(1)), ScalarValue::Int64(Some(3)),
            ScalarValue::Int64(Some(2)), ScalarValue::Int64(Some(1)),
        ], &DataType::Int64));
        let array = input.to_array_of_size(1).unwrap();
        let actual = evaluate(COLLECT_DISTINCT, &[array], 1).unwrap();
        let expected = ScalarValue::List(ScalarValue::new_list_nullable(&[
            ScalarValue::Int64(Some(3)), ScalarValue::Int64(Some(1)), ScalarValue::Int64(Some(2)),
        ], &DataType::Int64));
        assert_eq!(ScalarValue::try_from_array(&actual, 0).unwrap(), expected);
    }
