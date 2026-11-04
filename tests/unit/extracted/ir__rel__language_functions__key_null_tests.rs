
    use super::*;

    #[test]
    fn native_signed_integer_groups_preserve_language_equivalence() {
        // Adjacent integers above f64's exact range must remain separate.
        let values = vec![None, Some(i64::MIN), Some(-1), Some(0), Some(1),
            Some(9_007_199_254_740_992), Some(9_007_199_254_740_993),
            Some(i64::MAX), Some(1), None];
        let input = Arc::new(arrow::array::Int64Array::from(values.clone())) as ArrayRef;
        let encoded = evaluate(KEY, &[input], values.len()).unwrap();
        let encoded = encoded.as_any().downcast_ref::<arrow::array::BinaryArray>().unwrap();
        for (i, left) in values.iter().enumerate() {
            assert_eq!(encoded.is_null(i), left.is_none());
            for (j, right) in values.iter().enumerate() {
                let equal = if encoded.is_null(i) || encoded.is_null(j) {
                    encoded.is_null(i) && encoded.is_null(j)
                } else { encoded.value(i) == encoded.value(j) };
                assert_eq!(equal, left == right);
            }
        }
        for ty in [DataType::Float64, DataType::Utf8, DataType::Binary] {
            assert!(!native_grouping_type(&ty));
        }
    }

    #[test]
    fn untyped_and_typed_nulls_produce_null_keys() {
        for array in [
            Arc::new(arrow::array::NullArray::new(3)) as ArrayRef,
            Arc::new(arrow::array::Int64Array::from(vec![None, None, None])) as ArrayRef,
        ] {
            let keys = evaluate(KEY, &[array], 3).unwrap();
            assert_eq!(keys.null_count(), 3);
        }
    }
