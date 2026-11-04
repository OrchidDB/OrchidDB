
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
