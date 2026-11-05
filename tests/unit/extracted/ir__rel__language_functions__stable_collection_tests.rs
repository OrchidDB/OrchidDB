
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

#[test]
fn cypher_float_udf_executes_nullable_vectors_without_stringifying_results() {
    use arrow::array::{StringArray,Float64Array};
    let expression=cypher_to_float(col_exact("raw"));
    assert!(returns_finite_float(&expression));
    let Expr::ScalarFunction(call)=expression else {panic!("expected float callback")};
    let array=Arc::new(StringArray::from(vec![Some("1.25"),None,Some("bad"),Some("-0")])) as ArrayRef;
    let result=call.func.invoke_with_args(ScalarFunctionArgs {
        args:vec![ColumnarValue::Array(array)],
        arg_fields:vec![Arc::new(Field::new("raw",DataType::Utf8,true))],number_rows:4,
        return_field:Arc::new(Field::new("number",DataType::Float64,true)),config_options:Default::default(),
    }).unwrap();
    let ColumnarValue::Array(array)=result else {panic!("expected nullable vector")};
    let actual=array.as_any().downcast_ref::<Float64Array>().unwrap().iter()
        .map(|v|v.map(f64::to_bits)).collect::<Vec<_>>();
    assert_eq!(actual,vec![Some(1.25f64.to_bits()),None,None,Some((-0.0f64).to_bits())]);
}

#[test]
fn cypher_helpers_reject_invalid_protocol_shapes_before_evaluating_rows() {
    use arrow::array::{Int64Array,StringArray,BinaryArray};
    let text=||Arc::new(StringArray::from(vec!["value"])) as ArrayRef;
    let integer=||Arc::new(Int64Array::from(vec![1])) as ArrayRef;
    for arrays in [vec![],vec![integer()],vec![text(),text()]] {
        assert!(evaluate(CYPHER_TO_FLOAT,&arrays,1).unwrap_err().to_string().contains("nullable Utf8"));
    }
    for arrays in [vec![],vec![text()]] {
        assert!(evaluate(COLLECT_DISTINCT,&arrays,1).unwrap_err().to_string().contains("typed list"));
    }
    assert!(evaluate(LIST_COMPARE,&[],1).unwrap_err().to_string().contains("operation and two lists"));
    let null=Arc::new(StringArray::from(vec![None::<&str>])) as ArrayRef;
    assert!(evaluate(LIST_COMPARE,&[null,integer(),integer()],1).unwrap_err().to_string().contains("non-null operation"));
    assert!(evaluate(LIST_COMPARE,&[text(),integer(),integer()],1).unwrap_err().to_string().contains("invalid list comparison operation"));
    let binary=Arc::new(BinaryArray::from(vec![b"x".as_slice()])) as ArrayRef;
    assert!(evaluate(KEY,&[binary],1).unwrap_err().to_string().contains("unsupported Cypher key input"));
    assert!(evaluate(NODE_TEXT,&[text(),text(),text(),integer()],1).unwrap_err().to_string().contains("properties require a struct"));
    let props=Arc::new(StructArray::from(vec![(Arc::new(Field::new("amount",DataType::Int64,false)),integer())])) as ArrayRef;
    assert!(evaluate(NODE_TEXT,&[text(),text(),text(),props],1).unwrap_err().to_string().contains("rendered strings"));
}

#[test]
fn distinct_collection_retains_typed_empty_schema_and_ignores_null_lists() {
    let empty=ScalarValue::List(ScalarValue::new_list_nullable(&[],&DataType::Int64));
    let empty_input=empty.to_array_of_size(0).unwrap();
    let output=evaluate(COLLECT_DISTINCT,&[empty_input.clone()],0).unwrap();
    assert_eq!(output.len(),0);
    assert_eq!(output.data_type(),empty_input.data_type());
    let null=ScalarValue::try_from(empty.data_type()).unwrap().to_array_of_size(2).unwrap();
    let output=evaluate(COLLECT_DISTINCT,&[null],2).unwrap();
    assert_eq!(output.len(),2);
    for row in 0..2 { assert_eq!(ScalarValue::try_from_array(&output,row).unwrap(),empty); }
}

#[test]
fn cypher_float_udf_constant_folding_preserves_typed_null_and_negative_zero() {
    use datafusion::{common::DFSchema,logical_expr::simplify::SimplifyContext,
        optimizer::simplify_expressions::ExprSimplifier};
    let schema=DFSchema::empty();
    let simplifier=ExprSimplifier::new(SimplifyContext::default().with_schema(Arc::new(schema.clone())));
    for (input,expected) in [(Some("1.25"),Some(1.25f64)),(Some("invalid"),None),(None,None),(Some("-0"),Some(-0.0f64))] {
        let expression=cypher_to_float(lit(ScalarValue::Utf8(input.map(str::to_owned))));
        let simplified=simplifier.simplify(simplifier.coerce(expression,&schema).unwrap()).unwrap();
        let Expr::Literal(ScalarValue::Float64(actual),_)=simplified else {panic!("{simplified:?}")};
        assert_eq!(actual.map(f64::to_bits),expected.map(f64::to_bits));
    }
}
