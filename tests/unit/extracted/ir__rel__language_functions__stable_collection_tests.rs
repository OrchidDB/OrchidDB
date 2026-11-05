
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

#[test]
fn cypher_list_comparison_preserves_three_valued_results_across_list_layouts() {
    let list = |values: &[Option<i64>]| ScalarValue::List(ScalarValue::new_list_nullable(
        &values.iter().map(|v| ScalarValue::Int64(*v)).collect::<Vec<_>>(), &DataType::Int64));
    let cases: Vec<(Vec<Option<i64>>, Vec<Option<i64>>, [Option<bool>;6])> = vec![
        (vec![],vec![],[Some(true),Some(false),Some(false),Some(true),Some(false),Some(true)]),
        (vec![Some(1)],vec![Some(1),Some(2)],[Some(false),Some(true),Some(true),Some(true),Some(false),Some(false)]),
        (vec![Some(1),None],vec![Some(1),Some(2)],[None,None,None,None,None,None]),
        (vec![Some(1),None],vec![Some(2),None],[Some(false),Some(true),Some(true),Some(true),Some(false),Some(false)]),
        (vec![None,Some(2)],vec![None,Some(3)],[Some(false),Some(true),None,None,None,None]),
        (vec![Some(i64::MAX),Some(0)],vec![Some(i64::MAX-1),Some(0)],[Some(false),Some(true),Some(false),Some(false),Some(true),Some(true)]),
    ];
    for (left,right,expected) in cases {
        let left=list(&left);let right=list(&right);
        let item=Arc::new(Field::new("item",DataType::Int64,true));
        let mut kinds=vec![left.data_type(),DataType::LargeList(item.clone())];
        if let (ScalarValue::List(a),ScalarValue::List(b))=(&left,&right) {
            if a.value_length(0)==2 && b.value_length(0)==2 {kinds.push(DataType::FixedSizeList(item,2));}
        }
        for kind in kinds {
            let a=left.cast_to(&kind).unwrap().to_array().unwrap();
            let b=right.cast_to(&kind).unwrap().to_array().unwrap();
            for (op,expected) in ["eq","neq","lt","lte","gt","gte"].into_iter().zip(expected) {
                let operation=ScalarValue::Utf8(Some(op.into())).to_array().unwrap();
                let result=evaluate(LIST_COMPARE,&[operation,a.clone(),b.clone()],1).unwrap();
                assert_eq!(ScalarValue::try_from_array(&result,0).unwrap(),ScalarValue::Boolean(expected),"{kind:?} {left:?} {op} {right:?}");
            }
        }
    }
    let null=ScalarValue::try_from(list(&[]).data_type()).unwrap().to_array().unwrap();
    for op in ["eq","neq","lt","lte","gt","gte"] {
        let result=evaluate(LIST_COMPARE,&[ScalarValue::Utf8(Some(op.into())).to_array().unwrap(),null.clone(),list(&[]).to_array().unwrap()],1).unwrap();
        assert!(result.is_null(0));
    }
}

#[test]
fn cypher_distinct_collection_keeps_first_numeric_representation_and_exact_decimals() {
    let input=ScalarValue::List(ScalarValue::new_list_nullable(&[
        ScalarValue::Float64(Some(-0.0)),ScalarValue::Float64(Some(0.0)),ScalarValue::Float64(None),
        ScalarValue::Float64(Some(2.5)),ScalarValue::Float64(Some(f64::INFINITY)),
        ScalarValue::Float64(Some(2.5)),ScalarValue::Float64(Some(f64::NEG_INFINITY)),
    ],&DataType::Float64));
    let result=evaluate(COLLECT_DISTINCT,&[input.to_array().unwrap()],1).unwrap();
    let ScalarValue::List(list)=ScalarValue::try_from_array(&result,0).unwrap() else {panic!("list")};
    let array=list.value(0);
    let values=array.as_any().downcast_ref::<arrow::array::Float64Array>().unwrap();
    assert_eq!(values.values().iter().map(|n|n.to_bits()).collect::<Vec<_>>(),vec![(-0.0_f64).to_bits(),2.5_f64.to_bits(),f64::INFINITY.to_bits(),f64::NEG_INFINITY.to_bits()]);
    let ty=DataType::Decimal128(38,9);
    let high=10_i128.pow(37);
    let values=[Some(high),Some(high+1),None,Some(high),Some(-high)];
    let input=ScalarValue::List(ScalarValue::new_list_nullable(&values.into_iter().map(|n|ScalarValue::Decimal128(n,38,9)).collect::<Vec<_>>(),&ty));
    let result=evaluate(COLLECT_DISTINCT,&[input.to_array().unwrap()],1).unwrap();
    let expected=ScalarValue::List(ScalarValue::new_list_nullable(&[high,high+1,-high].into_iter().map(|n|ScalarValue::Decimal128(Some(n),38,9)).collect::<Vec<_>>(),&ty));
    assert_eq!(ScalarValue::try_from_array(&result,0).unwrap(),expected);
}
