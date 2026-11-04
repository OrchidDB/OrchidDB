
use super::*;
#[test]
fn canonical_grammar_calendar_and_range_are_proven_without_constructing_dates() {
    for text in ["0001-01-01", "9999-12-31", "2000-02-29", "2024-02-29"] {
        assert!(canonical_date(text), "{text}");
    }
    for text in [
        "0000-01-01",
        "1900-02-29",
        "2023-02-29",
        "2024-04-31",
        "2024-00-01",
        "2024-01-00",
        "2024-1-01",
        "20240101",
        "2024-W01-1",
        "2024-001",
        "+10000-01-01",
        "é024-01-01",
    ] {
        assert!(!canonical_date(text), "{text}");
    }
}

#[test]
fn canonical_date_native_scalar_supports_string_widths_and_preserves_typed_nulls() {
    for value in [
        ScalarValue::Utf8(Some("1970-01-01".into())),
        ScalarValue::LargeUtf8(Some("1970-01-01".into())),
        ScalarValue::Utf8View(Some("1970-01-01".into())),
    ] {
        assert!(safe_value(&value));
        assert_eq!(
            domain::storage(&native_date(value).unwrap()).unwrap(),
            ScalarValue::Date32(Some(0))
        );
    }
    for value in [
        ScalarValue::Null,
        ScalarValue::Utf8(None),
        ScalarValue::LargeUtf8(None),
        ScalarValue::Utf8View(None),
    ] {
        assert!(safe_value(&value));
        let result = native_date(value).unwrap();
        assert_eq!(
            result.data_type(),
            domain::data_type("cypher_temporal", DataType::Date32)
        );
        assert!(result.is_null());
    }
    for value in [
        ScalarValue::Int64(Some(1)),
        ScalarValue::Utf8(Some("not a date".into())),
    ] {
        assert!(!safe_value(&value));
        assert!(native_date(value).is_err());
    }
}

#[test]
fn lowered_date_callback_preserves_scalar_array_and_empty_batch_shapes() {
    use arrow::array::{ArrayRef, StringArray};
    use datafusion::logical_expr::ScalarFunctionArgs;
    let graph = PropertyGraph::new();
    let context = LoweringContext {
        island_memo: None,
        graph: &graph,
        options: RelBackendOptions::default(),
        policy: crate::ir::policy::GraphPlanPolicy::cypher(),
        language: Language::Cypher,
        scan_counter: 0,
        correlate_plan: None,
        rdf_typed_terms_used: false,
        scan_logical_labels: false,
        gremlin_label_binds: Default::default(),
        unordered_collects: Default::default(),
        unordered_distincts: Default::default(),
    };
    let plan = LogicalPlanBuilder::empty(true)
        .project(vec![lit("1970-01-01").alias("text")])
        .unwrap()
        .build()
        .unwrap();
    let expression = IrExpr::Call {
        name: "cypher_temporal.date".into(),
        args: vec![IrExpr::binding("text")],
    };
    let Expr::ScalarFunction(call) = lower_runtime_date(&context, &plan, &expression).unwrap()
    else {
        panic!("native date callback missing")
    };
    let target = domain::data_type("cypher_temporal", DataType::Date32);
    for (input, expected) in [
        (
            ColumnarValue::Scalar(ScalarValue::Utf8(Some("1970-01-01".into()))),
            vec![Some(0)],
        ),
        (
            ColumnarValue::Array(Arc::new(StringArray::from(vec![
                Some("1970-01-01"),
                None,
                Some("1970-01-02"),
            ])) as ArrayRef),
            vec![Some(0), None, Some(1)],
        ),
        (
            ColumnarValue::Array(
                Arc::new(StringArray::from(Vec::<Option<&str>>::new())) as ArrayRef
            ),
            vec![],
        ),
    ] {
        let scalar = matches!(&input, ColumnarValue::Scalar(_));
        let output = call
            .func
            .invoke_with_args(ScalarFunctionArgs {
                args: vec![input],
                arg_fields: vec![Arc::new(Field::new("text", DataType::Utf8, true))],
                number_rows: expected.len(),
                return_field: Arc::new(Field::new("date", target.clone(), true)),
                config_options: Default::default(),
            })
            .unwrap();
        assert_eq!(matches!(&output, ColumnarValue::Scalar(_)), scalar);
        let values = match output {
            ColumnarValue::Scalar(value) => vec![value],
            ColumnarValue::Array(array) => {
                assert_eq!(array.data_type(), &target);
                (0..array.len())
                    .map(|index| ScalarValue::try_from_array(&array, index).unwrap())
                    .collect()
            }
        };
        let physical = values
            .iter()
            .map(|value| domain::storage(value).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            physical,
            expected
                .into_iter()
                .map(ScalarValue::Date32)
                .collect::<Vec<_>>()
        );
    }
}
