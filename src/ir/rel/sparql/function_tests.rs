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
    };
    let args = ScalarFunctionArgs {
        arg_fields: args
            .iter()
            .map(|arg| Arc::new(Field::new("arg", arg.data_type(), true)))
            .collect(),
        args,
        number_rows: 2,
        return_field: Arc::new(Field::new("result", return_type, true)),
        config_options: Default::default(),
    };
    let output = function.invoke_with_args(args)?.into_array(2)?;
    (0..2)
        .map(|row| ScalarValue::try_from_array(&output, row))
        .collect()
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
