
    use super::*;

    #[test]
    fn outer_unnest_empty_or_null_lists_include_null_in_domain() {
        let empty = ScalarValue::List(ScalarValue::new_list(&[], &DataType::Int64, true));
        let null = ScalarValue::try_from(&empty.data_type()).unwrap();
        for source in [empty, null] {
            let input = LogicalPlanBuilder::empty(true)
                .project(vec![lit(source).alias("step")]).unwrap().build().unwrap();
            let mut options = datafusion::common::UnnestOptions::default();
            options.preserve_nulls = true;
            let plan = LogicalPlanBuilder::from(input)
                .unnest_column_with_options(Column::new_unqualified("step"), options)
                .unwrap().build().unwrap();
            assert_eq!(finite_values(&plan, "step"), Some(vec![ScalarValue::Int64(None)]));
        }
    }
