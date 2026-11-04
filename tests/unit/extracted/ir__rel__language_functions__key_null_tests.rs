
    use super::*;

#[test]
fn cypher_key_structs_use_field_identity_and_preserve_nested_nulls() {
    use arrow::array::{StructArray,Int64Array,StringArray,ListArray,LargeListArray,FixedSizeListArray};
    use arrow::buffer::{OffsetBuffer,ScalarBuffer};
    let structure=|reverse| {
        let mut fields=vec![
            (Arc::new(Field::new("number",DataType::Int64,true)),Arc::new(Int64Array::from(vec![Some(7),None])) as ArrayRef),
            (Arc::new(Field::new("text",DataType::Utf8,true)),Arc::new(StringArray::from(vec![Some("雪"),None])) as ArrayRef),
        ];
        if reverse {fields.reverse();}
        StructArray::from(fields)
    };
    let expected=Value::List(vec![
        Value::Map(std::collections::BTreeMap::from([("number".into(),Value::Int(7)),("text".into(),Value::String("雪".into()))])),
        Value::Map(std::collections::BTreeMap::from([("number".into(),Value::Null),("text".into(),Value::Null)])),
    ]);
    let mut keys=vec![];
    for reverse in [false,true] {
        let items=Arc::new(structure(reverse)) as ArrayRef;
        let field=Arc::new(Field::new("item",items.data_type().clone(),true));
        let list=ScalarValue::List(Arc::new(ListArray::new(field.clone(),OffsetBuffer::new(ScalarBuffer::from(vec![0_i32,2])),items.clone(),None)));
        let large=ScalarValue::LargeList(Arc::new(LargeListArray::new(field.clone(),OffsetBuffer::new(ScalarBuffer::from(vec![0_i64,2])),items.clone(),None)));
        let fixed=ScalarValue::FixedSizeList(Arc::new(FixedSizeListArray::new(field,2,items,None)));
        for scalar in [list.clone(),large,fixed] {assert_eq!(value(scalar).unwrap(),expected);}
        keys.push(evaluate(KEY,&[list.to_array().unwrap()],1).unwrap());
    }
    assert_eq!(keys[0].to_data(),keys[1].to_data());
}

#[test]
fn cypher_key_lowering_keeps_native_integer_columns_and_rejects_unsupported_storage() {
    let plan=datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
    for value in [ScalarValue::Boolean(Some(true)),ScalarValue::Int8(Some(1)),ScalarValue::Int16(Some(1)),
        ScalarValue::Int32(Some(1)),ScalarValue::Int64(Some(1))] {
        let expression=lit(value);assert_eq!(key(expression.clone(),&plan).unwrap(),expression);
    }
    for datatype in [DataType::Binary,DataType::LargeBinary,DataType::Timestamp(arrow::datatypes::TimeUnit::Microsecond,None)] {
        assert!(!key_type(&datatype));
        let error=key(lit(ScalarValue::try_from(&datatype).unwrap()),&plan).unwrap_err();
        assert!(error.to_string().contains("Cypher key type requires native values"));
    }
    assert!(!key_type(&DataType::Struct(vec![Field::new("__internal",DataType::Int64,true)].into())));
    assert!(!key_type(&crate::ir::functions::domain::json_type()));
}

#[tokio::test]
async fn native_carrier_key_lowering_keeps_null_absent_and_numbers_equivalent() {
    let plan=datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
    let mut keys=vec![];
    for (index,value) in [Value::Null,Value::Long(7),Value::Float(7.0)].iter().enumerate() {
        let expression=super::super::native_values::literal(value).unwrap();
        keys.push(key(expression,&plan).unwrap().alias(format!("key{index}")));
    }
    let plan=datafusion::logical_expr::LogicalPlanBuilder::from(plan).project(keys).unwrap().build().unwrap();
    let batches=super::super::execute_test_plan(plan).await.unwrap();
    assert_eq!(batches.len(),1);let batch=&batches[0];assert_eq!(batch.num_rows(),1);
    assert!(batch.column(0).is_null(0));
    assert!(!batch.column(1).is_null(0));
    assert_eq!(ScalarValue::try_from_array(batch.column(1),0).unwrap(),ScalarValue::try_from_array(batch.column(2),0).unwrap());
}

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
