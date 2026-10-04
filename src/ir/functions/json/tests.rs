use super::*;
fn j(text: &str) -> ScalarValue {
    domain::json_scalar(text).unwrap()
}
fn s(text: &str) -> ScalarValue {
    ScalarValue::Utf8(Some(text.into()))
}
fn eval(op: &str, args: Vec<ScalarValue>) -> Result<ScalarValue> {
    let types = args.iter().map(ScalarValue::data_type).collect::<Vec<_>>();
    validate(op, &types)?;
    let literal = if op == "transform" {
        args.get(1)
    } else if op == "value" {
        args.get(2)
    } else {
        None
    }
    .map(|value| {
        if domain::is_json(&value.data_type()) {
            domain::json_text(value)
        } else {
            text(value)
        }
    })
    .transpose()?
    .flatten();
    let ty = return_type(op, &types, literal.as_deref())?;
    evaluate(op, &args, &ty)
}
fn doc(value: ScalarValue) -> Value {
    document(&value).unwrap().unwrap()
}
#[test]
fn missing_json_null_and_sql_null_remain_distinct() {
    let d = j(r#"{"a":null}"#);
    let missing = eval("query", vec![d.clone(), s("$.missing")]).unwrap();
    assert!(missing.is_null());
    let null_json = eval("query", vec![d.clone(), s("$.a")]).unwrap();
    assert!(!null_json.is_null());
    assert_eq!(doc(null_json), Value::Null);
    assert_eq!(
        eval("exists", vec![d.clone(), s("$.a")]).unwrap(),
        ScalarValue::Boolean(Some(true))
    );
    assert_eq!(eval("type", vec![d, s("$.a")]).unwrap(), s("null"));
    assert!(
        eval("parse", vec![ScalarValue::Utf8(None)])
            .unwrap()
            .is_null()
    );
}
#[test]
fn paths_handle_quotes_pointers_negative_indices_and_escapes() {
    let d = j(r#"{"a/b":{"~key":[1,2,3]},"quoted.key":"ok"}"#);
    assert_eq!(
        doc(eval("query", vec![d.clone(), s("/a~1b/~0key/1")]).unwrap()),
        serde_json::json!(2)
    );
    assert_eq!(
        doc(eval("query", vec![d.clone(), s("$['a/b']['~key'][-1]")]).unwrap()),
        serde_json::json!(3)
    );
    assert_eq!(
        eval("value", vec![d, s("$[\"quoted.key\"]")]).unwrap(),
        s("ok")
    );
    assert!(path::parse("/bad~2escape").is_err());
    assert!(path::parse("$.x.size()").is_err());
}
#[test]
fn query_multi_paths_always_return_arrays() {
    let d = j(r#"{"a":[{"n":1},{"n":2},{"n":3}],"b":{"n":9}}"#);
    assert_eq!(
        doc(eval("query", vec![d.clone(), s("$.a[?(@.n >= 2)].n")]).unwrap()),
        serde_json::json!([2, 3])
    );
    assert_eq!(
        doc(eval("query", vec![d.clone(), s("$.a[?(@.n == 1)].n")]).unwrap()),
        serde_json::json!([1])
    );
    assert_eq!(
        doc(eval("query", vec![d.clone(), s("$..n")]).unwrap()),
        serde_json::json!([1, 2, 3, 9])
    );
    assert_eq!(
        doc(eval("query", vec![d, s("$.a[0:3:2].n")]).unwrap()),
        serde_json::json!([1, 3])
    );
    assert!(path::parse("$[::0]").is_err());
}
#[test]
fn typed_value_and_transform_preserve_nested_schema() {
    let d = j(r#"{"id":9007199254740993,"tags":["a",null],"enabled":true}"#);
    assert_eq!(
        eval("value", vec![d.clone(), s("$.id"), s("int64")]).unwrap(),
        ScalarValue::Int64(Some(9007199254740993))
    );
    let result = eval(
        "transform",
        vec![
            d,
            s(r#"{"enabled":"boolean","id":"int64","missing":"string","tags":["string"]}"#),
        ],
    )
    .unwrap();
    let ScalarValue::Struct(values) = result else {
        panic!()
    };
    assert_eq!(values.column(1).data_type(), &DataType::Int64);
    assert!(values.column(2).is_null(0));
    assert!(eval("value", vec![j("{\"x\":\"bad\"}"), s("$.x"), s("int64")]).is_err());
}
#[test]
fn constructors_embed_documents_and_preserve_sql_null_as_json_null() {
    assert_eq!(
        doc(eval("array", vec![s("hello"), j("[1]"), ScalarValue::Null]).unwrap()),
        serde_json::json!(["hello", [1], null])
    );
    assert_eq!(
        doc(eval(
            "object",
            vec![
                s("a"),
                ScalarValue::Int64(Some(1)),
                s("a"),
                ScalarValue::Null
            ]
        )
        .unwrap()),
        serde_json::json!({"a":null})
    );
    assert!(eval("object", vec![ScalarValue::Utf8(None), s("x")]).is_err());
    assert_eq!(doc(eval("array", vec![]).unwrap()), serde_json::json!([]));
    assert_eq!(doc(eval("object", vec![]).unwrap()), serde_json::json!({}));
}
#[test]
fn mutation_has_explicit_missing_parent_and_array_policies() {
    let original = j(r#"{"a":[1,2],"b":5}"#);
    assert_eq!(
        doc(eval("set", vec![original.clone(), s("$.x"), ScalarValue::Null]).unwrap()),
        serde_json::json!({"a":[1,2],"b":5,"x":null})
    );
    assert_eq!(
        doc(eval("set", vec![original.clone(), s("$.missing.x"), j("3")]).unwrap()),
        doc(original.clone())
    );
    assert_eq!(
        doc(eval("insert", vec![original.clone(), s("$.a[1]"), j("9")]).unwrap()),
        serde_json::json!({"a":[1,9,2],"b":5})
    );
    assert_eq!(
        doc(eval("replace", vec![original.clone(), s("$.missing"), j("9")]).unwrap()),
        doc(original.clone())
    );
    assert_eq!(
        doc(eval("remove", vec![original, s("$.a[0]"), s("$.b")]).unwrap()),
        serde_json::json!({"a":[2]})
    );
    assert!(eval("set", vec![j("[1,2]"), s("$[*]"), j("0")]).is_err());
}
#[test]
fn equality_and_containment_do_not_round_numbers_or_recurse_arbitrarily() {
    assert_eq!(
        eval("equals", vec![j("1"), j("1.0")]).unwrap(),
        ScalarValue::Boolean(Some(true))
    );
    assert_eq!(
        eval("equals", vec![j("9007199254740993"), j("9007199254740992")]).unwrap(),
        ScalarValue::Boolean(Some(false))
    );
    assert_eq!(
        eval(
            "contains",
            vec![j(r#"{"a":[1,2,3],"b":4}"#), j(r#"{"a":[2,2]}"#)]
        )
        .unwrap(),
        ScalarValue::Boolean(Some(true))
    );
    assert_eq!(
        eval("contains", vec![j(r#"{"a":{"b":1}}"#), j(r#"{"b":1}"#)]).unwrap(),
        ScalarValue::Boolean(Some(false))
    );
    assert_eq!(
        eval("contains", vec![j("[1,2]"), j("2")]).unwrap(),
        ScalarValue::Boolean(Some(true))
    );
}
#[test]
fn containment_primitive_array_exception_is_root_only() {
    assert_eq!(
        eval("contains", vec![j("[[1]]"), j("[1]")]).unwrap(),
        ScalarValue::Boolean(Some(false))
    );
    assert_eq!(
        eval("contains", vec![j(r#"{"v":[1]}"#), j(r#"{"v":1}"#)]).unwrap(),
        ScalarValue::Boolean(Some(false))
    );
    assert_eq!(
        eval("contains", vec![j(r#"[{"a":1}]"#), j(r#"{"a":1}"#)]).unwrap(),
        ScalarValue::Boolean(Some(false))
    );
    assert_eq!(
        eval("contains", vec![j("[1]"), j("1")]).unwrap(),
        ScalarValue::Boolean(Some(true))
    );
}

#[test]
fn merge_patch_replaces_arrays_and_removes_object_keys() {
    assert_eq!(
        doc(eval(
            "merge_patch",
            vec![
                j(r#"{"a":1,"b":{"x":2,"y":3},"c":[1]}"#),
                j(r#"{"a":null,"b":{"x":4},"c":[2]}"#)
            ]
        )
        .unwrap()),
        serde_json::json!({"b":{"x":4,"y":3},"c":[2]})
    );
}
#[test]
fn expansion_preserves_duplicate_occurrences_and_tree_paths() {
    let ScalarValue::List(rows) = eval("elements", vec![j("[3,3,null]")]).unwrap() else {
        panic!()
    };
    assert_eq!(rows.value(0).len(), 3);
    let row = ScalarValue::try_from_array(rows.value(0).as_ref(), 2).unwrap();
    let ScalarValue::Struct(row) = row else {
        panic!()
    };
    assert_eq!(
        ScalarValue::try_from_array(row.column(1), 0).unwrap(),
        ScalarValue::Int64(Some(2))
    );
    assert!(!row.column(0).is_null(0));
    let ScalarValue::List(rows) = eval("tree", vec![j(r#"{"a":[null]}"#)]).unwrap() else {
        panic!()
    };
    assert_eq!(rows.value(0).len(), 3);
    let ScalarValue::Struct(row) = ScalarValue::try_from_array(rows.value(0).as_ref(), 2).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        ScalarValue::try_from_array(row.column(3), 0).unwrap(),
        s("$[\"a\"][0]")
    );
    assert!(eval("elements", vec![j("null")]).is_err());
    let ScalarValue::List(empty) =
        eval("entries", vec![null(&domain::json_type()).unwrap()]).unwrap()
    else {
        panic!()
    };
    assert_eq!(empty.value(0).len(), 0);
}
#[test]
fn native_arrow_execution_preserves_domain_and_rows() {
    let f = function("json.parse").unwrap();
    let input = Arc::new(arrow::array::StringArray::from(vec![
        Some("null"),
        None,
        Some("{\"a\":1}"),
    ])) as ArrayRef;
    let ColumnarValue::Array(output) = f
        .invoke_with_args(ScalarFunctionArgs {
            args: vec![ColumnarValue::Array(input)],
            arg_fields: vec![Arc::new(Field::new("", DataType::Utf8, true))],
            number_rows: 3,
            return_field: Arc::new(Field::new("", domain::json_type(), true)),
            config_options: Default::default(),
        })
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(output.data_type(), &domain::json_type());
    assert!(!output.is_null(0));
    assert!(output.is_null(1));
    assert_eq!(
        document(&ScalarValue::try_from_array(output.as_ref(), 2).unwrap()).unwrap(),
        Some(serde_json::json!({"a":1}))
    );
}
#[test]
fn aggregates_merge_typed_states_and_keep_duplicate_array_members() {
    use datafusion::logical_expr::Accumulator;
    let mut a = aggregate::JsonAccumulator {
        object: false,
        values: vec![],
        ..Default::default()
    };
    a.update_batch(&[Arc::new(arrow::array::Int64Array::from(vec![
        Some(1),
        None,
        Some(1),
    ]))])
    .unwrap();
    let state = a.state().unwrap()[0].to_array_of_size(1).unwrap();
    let mut b = aggregate::JsonAccumulator {
        object: false,
        values: vec![],
        ..Default::default()
    };
    b.merge_batch(&[state]).unwrap();
    assert_eq!(doc(b.evaluate().unwrap()), serde_json::json!([1, null, 1]));
    let mut object = aggregate::JsonAccumulator {
        object: true,
        values: vec![],
        ..Default::default()
    };
    object
        .update_batch(&[
            Arc::new(arrow::array::StringArray::from(vec!["k", "k"])),
            Arc::new(arrow::array::Int64Array::from(vec![1, 2])),
        ])
        .unwrap();
    assert_eq!(doc(object.evaluate().unwrap()), serde_json::json!({"k":2}));
}

#[test]
fn typed_transform_preserves_missing_json_fields_and_rejects_numeric_coercions() {
    let result = eval(
        "transform",
        vec![
            j(r#"{"present":null}"#),
            j(r#"{"missing":"json","present":"json"}"#),
        ],
    )
    .unwrap();
    let ScalarValue::Struct(row) = result else {
        panic!()
    };
    assert!(row.column_by_name("missing").unwrap().is_null(0));
    assert!(!row.column_by_name("present").unwrap().is_null(0));
    for value in ["1.0", "1e2", "\"1\"", "9223372036854775808"] {
        assert!(
            eval("value", vec![j(value), s("$"), s("int64")]).is_err(),
            "{value}"
        );
    }
    assert!(eval("value", vec![j("1e99999"), s("$"), s("float64")]).is_err());
}
#[test]
fn path_fields_do_not_coerce_to_indices_and_filters_group_correctly() {
    assert!(
        eval("query", vec![j("[7]"), s("$[\"0\"]")])
            .unwrap()
            .is_null()
    );
    assert_eq!(
        doc(eval("query", vec![j("[7]"), s("/0")]).unwrap()),
        serde_json::json!(7)
    );
    assert!(eval("query", vec![j("[7]"), s("/00")]).unwrap().is_null());
    let input = j(r#"[{"a":1,"b":2},{"a":1,"b":3},{"a":2,"b":3}]"#);
    assert_eq!(
        doc(eval("query", vec![input, s("$[?((@.a == 1) && (@.b == 2))]")]).unwrap()),
        serde_json::json!([{"a":1,"b":2}])
    );
    assert_eq!(
        eval("equals", vec![j("1e99999"), j("10e99998")]).unwrap(),
        ScalarValue::Boolean(Some(true))
    );
}

#[tokio::test]
async fn ordered_aggregates_execute_with_declared_order_and_last_key_wins() {
    let context = datafusion::prelude::SessionContext::new();
    for name in aggregate_names() {
        context.register_udaf(aggregate(name).unwrap().as_ref().clone());
    }
    let batches = context.sql("SELECT __orchiddb_json_array_agg(value ORDER BY ordering) AS a, __orchiddb_json_object_agg('key', value ORDER BY ordering) AS o FROM (VALUES (3,30), (1,10), (2,20)) AS t(ordering,value)").await.unwrap().collect().await.unwrap();
    assert_eq!(
        doc(ScalarValue::try_from_array(batches[0].column(0), 0).unwrap()),
        serde_json::json!([10, 20, 30])
    );
    assert_eq!(
        doc(ScalarValue::try_from_array(batches[0].column(1), 0).unwrap()),
        serde_json::json!({"key":30})
    );
}

#[tokio::test]
async fn distinct_aggregate_keeps_sql_null_and_json_null_as_distinct_inputs() {
    let context = datafusion::prelude::SessionContext::new();
    context.register_udaf(aggregate("json.array_agg").unwrap().as_ref().clone());
    context.register_udf(function("json.parse").unwrap().as_ref().clone());
    let batches = context.sql("SELECT __orchiddb_json_array_agg(DISTINCT __orchiddb_json_parse(v)) FROM (VALUES (CAST(NULL AS VARCHAR)), ('null'), ('null')) AS t(v)").await.unwrap().collect().await.unwrap();
    assert_eq!(
        doc(ScalarValue::try_from_array(batches[0].column(0), 0).unwrap()),
        serde_json::json!([null, null])
    );
}

#[test]
fn runtime_construction_omits_graph_map_metadata_and_preserves_empty_records() {
    use crate::ir::value::{STRUCT_ORDER_KEY, Value as GraphValue};
    let input = GraphValue::Map(BTreeMap::from([
        ("name".into(), GraphValue::String("Ada".into())),
        (
            STRUCT_ORDER_KEY.into(),
            GraphValue::List(vec![GraphValue::String("name".into())]),
        ),
    ]));
    let GraphValue::Scalar(output) = runtime_call("json.array", &[input]).unwrap() else {
        panic!("JSON domain result");
    };
    assert_eq!(doc(output), serde_json::json!([{"name":"Ada"}]));
    let empty = eval("transform", vec![j("{}"), s("{}")]).unwrap();
    let ScalarValue::Struct(empty) = empty else {
        panic!("struct result");
    };
    assert_eq!(empty.len(), 1);
    assert_eq!(empty.num_columns(), 0);
}
