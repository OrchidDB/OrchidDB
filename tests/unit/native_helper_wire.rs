use super::*;
use arrow::array::{FixedSizeListArray, LargeListArray, ListArray, StructArray};
use arrow::datatypes::{DataType, Field, Int64Type};

#[test]
fn embedded_native_helper_reports_unknown_operations_and_invalid_specs() {
    let connection=Connection::open_in_memory().unwrap();
    register(&connection).unwrap();
    register(&connection).unwrap();
    for (kind,spec,needle) in [("unknown","{}","unknown native scalar operation"),
        ("value","invalid-json","expected value"),("predicate","invalid-json","expected value"),
        ("aggregate","invalid-json","expected value"),("project","invalid-json","expected value"),
        ("sort","invalid-json","expected value")] {
        let sql=format!("SELECT __orchiddb_native_eval('{kind}','{spec}',struct_pack(argument := 7),0,0)");
        let error=connection.query_row(&sql,[],|row|row.get::<_,String>(0)).unwrap_err();
        assert!(error.to_string().contains(needle),"{kind}: {error}");
    }
    let result=connection.query_row("SELECT 42",[],|row|row.get::<_,i64>(0)).unwrap();
    assert_eq!(result,42,"failed helper calls must not invalidate the session");
}

#[test]
fn embedded_native_helper_preserves_cypher_equality_and_unwind_items() {
    use base64::Engine;
    use crate::ir::value::Value as GraphValue;
    let connection=Connection::open_in_memory().unwrap();register(&connection).unwrap();
    let key=|value:&str|connection.query_row(&format!(
        "SELECT __orchiddb_native_eval('key','true',struct_pack(argument := {value}),0,0)"),[],|row|row.get::<_,String>(0)).unwrap();
    assert_eq!(key("7::BIGINT"),key("7.0::DOUBLE"));
    assert_ne!(key("7::BIGINT"),key("'7'"));
    let raw=connection.query_row("SELECT __orchiddb_native_eval('items','',struct_pack(argument := [7::BIGINT,NULL,7::BIGINT]),0,0)",[],|row|row.get::<_,String>(0)).unwrap();
    let values:Value=serde_json::from_str(&raw).unwrap();
    let values=values.as_array().unwrap();assert_eq!(values.len(),3);
    assert!(values[1].is_null());
    for index in [0,2] {
        let bytes=base64::engine::general_purpose::STANDARD.decode(values[index][native::FIELD].as_str().unwrap()).unwrap();
        assert_eq!(crate::ir::catalog::snapshot::binary::decode_value_bytes(&bytes).unwrap(),GraphValue::Long(7));
    }
}

#[test]
fn native_wire_fallback_preserves_unsigned_domain_and_rejects_unsupported_binary() {
    use base64::Engine;
    use crate::ir::value::Value as GraphValue;
    for (input,expected) in [
        (ScalarValue::UInt8(Some(255)),GraphValue::UInt8(255)),
        (ScalarValue::UInt16(Some(65535)),GraphValue::UInt16(65535)),
        (ScalarValue::UInt32(Some(u32::MAX)),GraphValue::UInt32(u32::MAX)),
        (ScalarValue::UInt64(Some(u64::MAX)),GraphValue::UInt64(u64::MAX)),
    ] {
        let result=wire(input).unwrap();
        let bytes=base64::engine::general_purpose::STANDARD.decode(result[native::FIELD].as_str().unwrap()).unwrap();
        assert_eq!(crate::ir::catalog::snapshot::binary::decode_value_bytes(&bytes).unwrap(),expected);
    }
    for input in [ScalarValue::Binary(Some(vec![0,255])),ScalarValue::LargeBinary(Some(vec![0,255]))] {
        assert!(wire(input).unwrap_err().to_string().contains("unsupported host helper input"));
    }
}

#[test]
fn native_wire_preserves_scalar_width_nulls_and_string_storage() {
    for (value,tag,text) in [
        (ScalarValue::Int8(Some(-128)),"TINYINT","-128"),
        (ScalarValue::Int16(Some(32767)),"SMALLINT","32767"),
        (ScalarValue::Int32(Some(i32::MIN)),"INTEGER","-2147483648"),
        (ScalarValue::Int64(Some(i64::MAX)),"BIGINT","9223372036854775807"),
        (ScalarValue::Float32(Some(1.25)),"FLOAT","1.25"),
        (ScalarValue::Float64(Some(-1.5)),"DOUBLE","-1.5"),
    ] {
        assert_eq!(wire(value).unwrap(),json!({"__orchiddb_number":[tag,text]}));
    }
    for value in [ScalarValue::Utf8(Some("雪\0'".into())),
        ScalarValue::LargeUtf8(Some("雪\0'".into())),ScalarValue::Utf8View(Some("雪\0'".into()))] {
        assert_eq!(wire(value).unwrap(),json!("雪\0'"));
    }
    for value in [ScalarValue::Null,ScalarValue::Boolean(None),ScalarValue::Int64(None),ScalarValue::LargeUtf8(None)] {
        assert_eq!(wire(value).unwrap(),Value::Null);
    }
    assert_eq!(wire(ScalarValue::Boolean(Some(false))).unwrap(),json!(false));
}

#[test]
fn native_wire_retains_nested_list_nulls_and_struct_field_names() {
    let input=vec![Some(vec![Some(7_i64),None])];
    let expected=json!([{"__orchiddb_number":["BIGINT","7"]},null]);
    for value in [
        ScalarValue::List(Arc::new(ListArray::from_iter_primitive::<Int64Type,_,_>(input.clone()))),
        ScalarValue::LargeList(Arc::new(LargeListArray::from_iter_primitive::<Int64Type,_,_>(input.clone()))),
        ScalarValue::FixedSizeList(Arc::new(FixedSizeListArray::from_iter_primitive::<Int64Type,_,_>(input,2))),
    ] {
        assert_eq!(wire(value.clone()).unwrap(),expected);
        let array=value.to_array().unwrap();
        let structure=StructArray::new(vec![Arc::new(Field::new("quoted.field",array.data_type().clone(),true))].into(),vec![array],None);
        assert_eq!(wire(ScalarValue::Struct(Arc::new(structure))).unwrap(),json!({"quoted.field":expected}));
    }
    let empty=ScalarValue::List(Arc::new(ListArray::from_iter_primitive::<Int64Type,_,_>(vec![Some(Vec::<Option<i64>>::new())])));
    assert_eq!(wire(empty).unwrap(),json!([]));
    let null=ScalarValue::try_from(&DataType::List(Arc::new(Field::new("item",DataType::Int64,true)))).unwrap();
    assert_eq!(wire(null).unwrap(),Value::Null);
}
