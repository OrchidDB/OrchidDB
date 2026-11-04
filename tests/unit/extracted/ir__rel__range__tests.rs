
    use super::*;
    #[test]
    fn range_provider_retains_exact_boundaries_for_sql_lowering() {
        for (start, stop, step) in [
            (i64::MAX - 1, i64::MAX, 1),
            (i64::MIN + 1, i64::MIN, -1),
            (i64::MAX, i64::MIN, i64::MIN),
        ] {
            let provider = IntegerRange::new(start, stop, step, "n".into());
            assert_eq!(
                (provider.start, provider.stop, provider.step),
                (start, stop, step)
            );
            assert_eq!(provider.schema().field(0).data_type(), &DataType::Int64);
        }
    }
