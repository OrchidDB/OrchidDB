use super::*;

#[test]
fn collection_type_unification_widens_only_compatible_integer_domains() {
    for left in [DataType::Int8, DataType::Int16, DataType::Int32, DataType::Int64] {
        for right in [DataType::Int8, DataType::Int16, DataType::Int32, DataType::Int64] {
            let expected = if left == right { left.clone() } else { DataType::Int64 };
            assert_eq!(nullable_collection_type(&left, &right), Some(expected));
        }
    }
    let metadata = std::collections::HashMap::from([("source".into(), "left".into())]);
    let left_field = Field::new("item", DataType::Int8, false).with_metadata(metadata.clone());
    let left = DataType::List(Arc::new(left_field));
    let right = DataType::List(Arc::new(Field::new("other", DataType::Int32, true)));
    assert_eq!(nullable_collection_type(&left, &right), Some(DataType::List(Arc::new(
        Field::new("item", DataType::Int64, true).with_metadata(metadata)))));
    for kind in [DataType::Boolean, DataType::Date32, left.clone(), DataType::Struct(vec![Field::new("x", DataType::Utf8, true)].into())] {
        assert_eq!(nullable_collection_type(&DataType::Null, &kind), Some(kind.clone()));
        assert_eq!(nullable_collection_type(&kind, &DataType::Null), Some(kind));
    }
    for (left, right) in [(DataType::Int64, DataType::Float64), (DataType::Int64, DataType::UInt64),
        (DataType::Utf8, DataType::Boolean),
        (left, DataType::List(Arc::new(Field::new("item", DataType::Boolean, true))))] {
        assert_eq!(nullable_collection_type(&left, &right), None);
    }
}

fn call(name: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call { name: name.into(), args }
}

#[test]
fn literal_conditionals_prune_only_unreachable_arguments() {
    let poison = call("effectful", vec![]);
    let unknown = IrExpr::Binding("unknown".into());
    let null = IrExpr::Lit(Lit::Null);
    let value = IrExpr::Lit(Lit::Int(7));
    let typed_null = IrExpr::Lit(Lit::Scalar(ScalarValue::Int64(None)));
    for (name, input, expected) in [
        ("fn.nvl2", vec![typed_null.clone(), poison.clone(), value.clone()], vec![typed_null.clone(), null.clone(), value.clone()]),
        ("FN.NVL2", vec![value.clone(), value.clone(), poison.clone()], vec![value.clone(), value.clone(), null.clone()]),
        ("fn.coalesce", vec![typed_null, unknown.clone(), value.clone(), poison.clone()], vec![IrExpr::Lit(Lit::Scalar(ScalarValue::Int64(None))), unknown.clone(), value.clone(), null.clone()]),
        ("fn.nvl", vec![value.clone(), poison.clone()], vec![value.clone(), null.clone()]),
        ("fn.ifnull", vec![value.clone(), poison.clone()], vec![value.clone(), null.clone()]),
    ] {
        assert_eq!(prune_literal_conditional_branches(&call(name, input)), Some(call(name, expected)), "{name}");
    }
    for expr in [
        unknown.clone(),
        call("fn.coalesce", vec![]),
        call("fn.coalesce", vec![null.clone(), unknown.clone()]),
        call("fn.coalesce", vec![value.clone(), null.clone()]),
        call("fn.nvl2", vec![unknown.clone(), value.clone(), poison.clone()]),
        call("fn.nvl2", vec![null.clone(), poison.clone()]),
        call("fn.nvl", vec![unknown, poison.clone()]),
        call("fn.ifnull", vec![null.clone(), poison.clone()]),
        call("unrelated", vec![value, poison]),
    ] {
        assert!(prune_literal_conditional_branches(&expr).is_none(), "{expr:?}");
    }
}
