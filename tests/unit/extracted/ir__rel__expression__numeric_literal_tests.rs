
    use super::*;
    #[test]
    fn float_comparisons_accept_only_exact_integer_constants() {
        for n in [0_i64, 1, -1, 9007199254740992, i64::MIN] {
            assert!(exact_float_literal(&lit(n), &DataType::Float64));
        }
        for n in [9007199254740993_i64, i64::MAX, -9007199254740993] {
            assert!(!exact_float_literal(&lit(n), &DataType::Float64));
        }
        assert!(!exact_float_literal(&lit(16777217_i64), &DataType::Float32));
        assert!(!exact_float_literal(&datafusion::logical_expr::col("id"), &DataType::Float64));
    }

    #[test]
    fn all_integer_widths_require_exact_binary_float_representation() {
        // Binary32 has 24 significant bits; binary64 has 53. Powers of two
        // remain exact beyond those boundaries, but adjacent odd integers do not.
        for (value, binary32, binary64) in [
            (ScalarValue::Int8(Some(i8::MIN)), true, true),
            (ScalarValue::Int8(Some(i8::MAX)), true, true),
            (ScalarValue::UInt8(Some(u8::MAX)), true, true),
            (ScalarValue::Int16(Some(i16::MIN)), true, true),
            (ScalarValue::Int16(Some(i16::MAX)), true, true),
            (ScalarValue::UInt16(Some(u16::MAX)), true, true),
            (ScalarValue::Int32(Some(1 << 24)), true, true),
            (ScalarValue::Int32(Some((1 << 24) + 1)), false, true),
            (ScalarValue::Int32(Some(-((1 << 24) + 1))), false, true),
            (ScalarValue::Int32(Some(i32::MIN)), true, true),
            (ScalarValue::Int32(Some(i32::MAX)), false, true),
            (ScalarValue::UInt32(Some(1 << 31)), true, true),
            (ScalarValue::UInt32(Some(u32::MAX)), false, true),
            (ScalarValue::Int64(Some(1 << 53)), true, true),
            (ScalarValue::Int64(Some((1 << 53) + 1)), false, false),
            (ScalarValue::Int64(Some(i64::MIN)), true, true),
            (ScalarValue::Int64(Some(i64::MAX)), false, false),
            (ScalarValue::UInt64(Some(1 << 63)), true, true),
            (ScalarValue::UInt64(Some((1 << 63) + 1)), false, false),
            (ScalarValue::UInt64(Some(u64::MAX)), false, false),
        ] {
            let expression = lit(value.clone());
            assert_eq!(exact_float_literal(&expression, &DataType::Float32), binary32, "{value:?}");
            assert_eq!(exact_float_literal(&expression, &DataType::Float64), binary64, "{value:?}");
            assert!(!exact_float_literal(&expression, &DataType::Int64));
        }
    }

    #[test]
    fn exact_float_promotion_does_not_assume_nulls_or_dynamic_values_are_integers() {
        for value in [
            ScalarValue::Int8(None), ScalarValue::Int16(None), ScalarValue::Int32(None),
            ScalarValue::Int64(None), ScalarValue::UInt8(None), ScalarValue::UInt16(None),
            ScalarValue::UInt32(None), ScalarValue::UInt64(None), ScalarValue::Null,
            ScalarValue::Float32(Some(1.0)), ScalarValue::Float64(Some(1.0)),
            ScalarValue::Decimal128(Some(1), 10, 0), ScalarValue::Boolean(Some(true)),
            ScalarValue::Utf8(Some("1".into())),
        ] {
            for target in [DataType::Float32, DataType::Float64] {
                assert!(!exact_float_literal(&lit(value.clone()), &target), "{value:?}");
            }
        }
        for target in [DataType::Float32, DataType::Float64] {
            let column = datafusion::logical_expr::col("unknown_integer");
            assert!(!exact_float_literal(&column, &target));
            assert!(!exact_float_literal(&(column + lit(0_i64)), &target));
        }
    }
