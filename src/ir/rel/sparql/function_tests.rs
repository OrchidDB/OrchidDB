use super::*;

fn invoke(
    name: &str,
    args: Vec<ColumnarValue>,
    return_type: DataType,
) -> datafusion::common::Result<Vec<ScalarValue>> {
    let function = DuckDbFunction {
        name: name.into(),
        return_type: return_type.clone(),
        signature: Signature::variadic_any(Volatility::Immutable),
        regexes: Default::default(),
    };
    invoke_function(&function, args)
}

fn invoke_function(
    function: &DuckDbFunction,
    args: Vec<ColumnarValue>,
) -> datafusion::common::Result<Vec<ScalarValue>> {
    let args = ScalarFunctionArgs {
        arg_fields: args
            .iter()
            .map(|arg| Arc::new(Field::new("arg", arg.data_type(), true)))
            .collect(),
        args,
        number_rows: 2,
        return_field: Arc::new(Field::new("result", function.return_type.clone(), true)),
        config_options: Default::default(),
    };
    let output = function.invoke_with_args(args)?.into_array(2)?;
    (0..2)
        .map(|row| ScalarValue::try_from_array(&output, row))
        .collect()
}

#[test]
fn residual_regex_prepares_once_across_batches_and_keeps_errors_lazy() {
    let function = DuckDbFunction {
        name: "regexp_full_match".into(),
        return_type: DataType::Boolean,
        signature: Signature::variadic_any(Volatility::Immutable),
        regexes: Default::default(),
    };
    let args = |text: Option<&str>, pattern: &str| {
        vec![
            ColumnarValue::Scalar(ScalarValue::Utf8(text.map(str::to_owned))),
            ColumnarValue::Scalar(ScalarValue::Utf8(Some(pattern.into()))),
        ]
    };
    for text in ["abc", "abcd", "abc"] {
        assert_eq!(
            invoke_function(&function, args(Some(text), "abc")).unwrap(),
            vec![ScalarValue::Boolean(Some(text == "abc")); 2]
        );
    }
    assert_eq!(function.regexes.len(), 1);
    assert_eq!(
        invoke_function(&function, args(None, "[")).unwrap(),
        vec![ScalarValue::Boolean(None); 2]
    );
    assert_eq!(
        function.regexes.len(),
        1,
        "null input must not compile an invalid pattern"
    );
    assert!(invoke_function(&function, args(Some("x"), "[")).is_err());
    assert!(invoke_function(&function, args(Some("y"), "[")).is_err());
    assert_eq!(function.regexes.len(), 2);
    assert_eq!(
        invoke_function(&function, args(Some("xyz"), "xyz")).unwrap(),
        vec![ScalarValue::Boolean(Some(true)); 2]
    );
    assert_eq!(function.regexes.len(), 3);
}

#[test]
fn residual_catalog_executes_aliases_nested_functions_and_nulls() {
    let strings = || ColumnarValue::Array(Arc::new(StringArray::from(vec![Some("hé"), None])));
    // Repeated batches exercise the shared catalogs with fresh input values.
    for _ in 0..3 {
        for name in ["length", "char_length"] {
            assert_eq!(
                invoke(name, vec![strings()], DataType::Int64).unwrap(),
                vec![ScalarValue::Int64(Some(2)), ScalarValue::Int64(None)]
            );
        }
        let needle = ColumnarValue::Scalar(ScalarValue::Utf8(Some("^h".into())));
        assert_eq!(
            invoke("regexp_matches", vec![strings(), needle], DataType::Boolean).unwrap(),
            vec![ScalarValue::Boolean(Some(true)), ScalarValue::Boolean(None)]
        );
        let list = ScalarValue::List(ScalarValue::new_list(
            &[ScalarValue::Int64(Some(7)), ScalarValue::Int64(Some(9))],
            &DataType::Int64,
            true,
        ));
        let index = ColumnarValue::Scalar(ScalarValue::Int64(Some(2)));
        assert_eq!(
            invoke(
                "list_extract",
                vec![ColumnarValue::Scalar(list), index],
                DataType::Int64
            )
            .unwrap(),
            vec![ScalarValue::Int64(Some(9)); 2]
        );
    }
    assert!(matches!(
        invoke("__missing", vec![strings()], DataType::Utf8),
        Err(DataFusionError::NotImplemented(_))
    ));
}
