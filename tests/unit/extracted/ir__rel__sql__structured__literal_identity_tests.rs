
    use super::*;

fn map_type()->DataType {
    DataType::Map(Arc::new(Field::new("entries",DataType::Struct(vec![
        Field::new("key",DataType::Utf8,false),Field::new("value",DataType::Int64,true)].into()),false)),false)
}

#[test]
fn structured_map_transport_preserves_entry_order_empty_maps_and_null_values() {
    let ty=map_type();
    for payload in [json!([]),json!([{"key":"b","value":"2"},{"key":"a","value":null}])] {
        let value=decode(&payload,&ty).unwrap();assert_eq!(value.data_type(),ty);
        assert_eq!(encode(&value).unwrap(),payload);
    }
    assert!(decode(&Value::Null,&ty).unwrap().is_null());
    for payload in [json!({}),json!([{"key":null,"value":"2"}]),json!([{"key":"a","value":"not-an-integer"}])] {
        assert!(decode(&payload,&ty).is_err());
    }
}

#[cfg(feature="duckdb")]
#[test]
fn structured_map_literals_execute_empty_and_nullable_entries() {
    let connection=duckdb::Connection::open_in_memory().unwrap();
    for (payload,count) in [(json!([]),0_i64),(json!([{"key":"a'雪","value":"7"},{"key":"missing","value":null}]),2)] {
        let value=decode(&payload,&map_type()).unwrap();let expression=literal(SqlDialect::DuckDb,&value).unwrap();
        let sql=format!("SELECT cardinality({expression}), ({expression})['a''雪'], ({expression})['missing']");
        let result=connection.query_row(&sql,[],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,Option<i64>>(1)?,row.get::<_,Option<i64>>(2)?))).unwrap();
        assert_eq!(result,(count,if count==0 {None}else{Some(7)},None));
    }
}

#[test]
fn structured_literal_native_function_preserves_scalar_values_and_return_types() {
    use datafusion::logical_expr::{ScalarFunctionArgs,ColumnarValue,ExprSchemable};
    for value in [decode(&json!([]),&map_type()).unwrap(),
        decode(&json!([{"key":"a","value":"7"}]),&map_type()).unwrap(),
        ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Int64(Some(7)),ScalarValue::Int64(None)],&DataType::Int64))] {
        for dialect in [SqlDialect::DuckDb,SqlDialect::Postgres] {
            let expression=literal_expr(&value,dialect).unwrap();
            assert_eq!(expression.get_type(&datafusion::common::DFSchema::empty()).unwrap(),value.data_type());
            let Expr::ScalarFunction(call)=expression else {panic!()};
            for rows in [0,3] {
                let actual=call.func.invoke_with_args(ScalarFunctionArgs{args:vec![],arg_fields:vec![],number_rows:rows,
                    return_field:Arc::new(Field::new("value",value.data_type(),true)),config_options:Default::default()}).unwrap();
                let ColumnarValue::Scalar(actual)=actual else {panic!("constant must stay scalar")};assert_eq!(actual,value);
            }
        }
    }
}

#[test]
fn structured_literal_unsupported_scalars_fail_and_null_scalars_remain_sql_null() {
    for value in [ScalarValue::Int64(Some(7)),ScalarValue::Boolean(Some(true)),ScalarValue::Utf8(Some("text".into()))] {
        assert!(literal(SqlDialect::DuckDb,&value).unwrap_err().to_string().contains("structured literal"));
    }
    for value in [ScalarValue::Int64(None),ScalarValue::Utf8(None),ScalarValue::try_from(&map_type()).unwrap()] {
        for dialect in [SqlDialect::DuckDb,SqlDialect::Postgres] {assert_eq!(literal(dialect,&value).unwrap(),"NULL");}
    }
}

#[test]
fn structured_json_domain_leaves_distinguish_sql_null_from_json_null() {
    let ty=crate::ir::functions::domain::json_type();
    for text in ["null","{\"a\":[1,null]}","\"雪\""] {
        let value=crate::ir::functions::domain::json_scalar(text).unwrap();
        let encoded=encode(&value).unwrap();assert_eq!(encoded,Value::String(text.into()));
        assert_eq!(decode(&encoded,&ty).unwrap(),value);
    }
    let null=ScalarValue::try_from(&ty).unwrap();
    assert_eq!(encode(&null).unwrap(),Value::Null);assert_eq!(decode(&Value::Null,&ty).unwrap(),null);
    for malformed in [json!(7),json!({"a":1}),json!([])] {
        assert!(decode(&malformed,&ty).unwrap_err().to_string().contains("serialized JSON domain leaf"));
    }
}

#[test]
fn structured_list_and_binary_layouts_roundtrip_empty_and_nullable_values() {
    let field=Arc::new(Field::new("item",DataType::Int64,true));
    for ty in [DataType::List(field.clone()),DataType::LargeList(field.clone()),DataType::FixedSizeList(field,3)] {
        let expected=json!(["1",null,"3"]);
        let value=decode(&expected,&ty).unwrap();assert_eq!(value.data_type(),ty);assert_eq!(encode(&value).unwrap(),expected);
        assert!(decode(&json!({}),&ty).is_err());
    }
    for ty in [DataType::Binary,DataType::LargeBinary,DataType::BinaryView] {
        for text in ["\\x","\\x00ff27"] {
            let value=decode(&json!(text),&ty).unwrap();assert_eq!(value.data_type(),ty);
            assert_eq!(encode(&value).unwrap(),json!(text));
        }
    }
}

#[test]
fn structured_union_transport_preserves_selected_null_tag_and_rejects_unknown_tags() {
    use arrow::datatypes::{UnionFields,UnionMode};
    let fields=UnionFields::try_new(vec![0,3],vec![Field::new("number",DataType::Int64,true),Field::new("text",DataType::Utf8,true)]).unwrap();
    for mode in [UnionMode::Dense,UnionMode::Sparse] {
        let ty=DataType::Union(fields.clone(),mode);
        for payload in [json!({"tag":"number","value":null}),json!({"tag":"number","value":"7"}),json!({"tag":"text","value":"雪"})] {
            let value=decode(&payload,&ty).unwrap();assert_eq!(encode(&value).unwrap(),payload);
            assert!(literal(SqlDialect::Postgres,&value).unwrap().contains("AS JSONB"));
        }
        for payload in [json!({}),json!({"tag":"missing","value":"7"}),json!({"tag":7})] {
            assert!(decode(&payload,&ty).is_err());
        }
        let null=ScalarValue::Union(None,fields.clone(),mode);
        assert_eq!(encode(&null).unwrap(),Value::Null);
        for dialect in [SqlDialect::DuckDb,SqlDialect::Postgres] {assert_eq!(literal(dialect,&null).unwrap(),"NULL");}
    }
}

#[cfg(feature="duckdb")]
#[test]
fn structured_union_literals_execute_with_null_members_and_quoted_field_names() {
    use arrow::datatypes::{UnionFields,UnionMode};
    let fields=UnionFields::try_new(vec![0,3],vec![Field::new("odd\"name",DataType::Int64,true),Field::new("text",DataType::Utf8,true)]).unwrap();
    let connection=duckdb::Connection::open_in_memory().unwrap();
    for member in [ScalarValue::Int64(None),ScalarValue::Int64(Some(7))] {
        let value=ScalarValue::Union(Some((0,Box::new(member.clone()))),fields.clone(),UnionMode::Dense);
        let expression=literal(SqlDialect::DuckDb,&value).unwrap();
        let sql=format!("SELECT union_tag({expression})::VARCHAR, union_extract({expression}, 'odd\"name')");
        let (tag,actual)=connection.query_row(&sql,[],|row|Ok((row.get::<_,String>(0)?,row.get::<_,Option<i64>>(1)?))).unwrap();
        assert_eq!(tag,"odd\"name");
        let ScalarValue::Int64(expected)=member else {panic!()};assert_eq!(actual,expected);
    }
}
    #[test]
    fn separately_lowered_structured_constants_keep_semantic_function_identity() {
        let value = ScalarValue::new_list_nullable(&[ScalarValue::Int64(Some(1))], &DataType::Int64);
        let value = ScalarValue::List(value);
        let first = literal_expr(&value, SqlDialect::DuckDb).unwrap();
        let second = literal_expr(&value, SqlDialect::DuckDb).unwrap();
        assert_eq!(first, second);
        let different = ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Int64(Some(2))], &DataType::Int64));
        assert_ne!(first, literal_expr(&different, SqlDialect::DuckDb).unwrap());
    }
