use super::*;
use arrow::datatypes::DataType;

fn address(id: i64) -> RowAddress {
    ("people".into(), KeyColumns::from("id"), ElementId::new(ScalarValue::Int64(Some(id))).unwrap())
}

#[test]
fn physical_row_writes_combine_node_and_relationship_columns_without_duplicate_rows() {
    let mut rows = BTreeMap::new();
    let value = |n| ScalarValue::Int64(Some(n));
    merge_row(&mut rows, address(1), true, false, BTreeMap::from([("age".into(),value(40))])).unwrap();
    merge_row(&mut rows, address(1), true, false, BTreeMap::from([("age".into(),value(40)),("manager_id".into(),value(2))])).unwrap();
    merge_row(&mut rows, address(2), false, false, BTreeMap::from([("age".into(),value(50))])).unwrap();
    assert_eq!(rows.len(),2);
    assert_eq!(rows[&address(1)].values,BTreeMap::from([("age".into(),value(40)),("manager_id".into(),value(2))]));
    assert!(rows[&address(1)].existing);
    assert!(!rows[&address(2)].existing);
    let error = merge_row(&mut rows,address(1),true,false,BTreeMap::from([("age".into(),value(41))])).unwrap_err();
    assert!(error.contains("conflicting assignments") && error.contains("age"),"{error}");
    assert_eq!(rows[&address(1)].values["age"],value(40));
}

#[test]
fn incompatible_row_lifetimes_are_rejected_and_identical_deletes_coalesce() {
    for (existing,delete,next_existing,next_delete) in [(true,false,true,true),(true,false,false,false),(false,false,true,false),(true,true,true,false)] {
        let mut rows = BTreeMap::new();
        merge_row(&mut rows,address(1),existing,delete,BTreeMap::new()).unwrap();
        let error=merge_row(&mut rows,address(1),next_existing,next_delete,BTreeMap::new()).unwrap_err();
        assert!(error.contains("incompatible graph mutations"),"{error}");
        assert_eq!((rows[&address(1)].existing,rows[&address(1)].delete),(existing,delete));
    }
    let mut rows=BTreeMap::new();
    for _ in 0..2 { merge_row(&mut rows,address(1),true,true,BTreeMap::new()).unwrap(); }
    assert_eq!(rows.len(),1);
    assert!(rows[&address(1)].delete);
}

#[test]
fn constant_defaults_require_one_literal_expression_and_reject_dynamic_sources() {
    for expression in ["NULL","TRUE","'a''b'","DATE '2026-01-02'","((-(2 + 3)))","CAST(12 AS BIGINT)","1 + (2 * 3)","TRY_CAST('4' AS INTEGER)"] {
        assert!(constant_default(expression),"{expression}");
    }
    for expression in ["","(","1; SELECT 2","1 UNION SELECT 2","1, 2","1 AS renamed","nextval('ids')","current_timestamp","column_name","1 + column_name","column_name + 1","(SELECT 1)","*"] {
        assert!(!constant_default(expression),"{expression}");
    }
}

#[test]
fn metadata_text_accepts_all_arrow_string_layouts_and_rejects_null_or_numeric_cells() {
    use arrow::array::{StringArray, LargeStringArray, StringViewArray, Int64Array};
    let strings: Vec<ArrayRef> = vec![
        Arc::new(StringArray::from(vec![Some("café"),None])),
        Arc::new(LargeStringArray::from(vec![Some("café"),None])),
        Arc::new(StringViewArray::from(vec![Some("café"),None])),
    ];
    for array in strings {
        let batch=RecordBatch::try_from_iter(vec![("column_name",array)]).unwrap();
        assert_eq!(text(&batch,0,0).unwrap(),"café");
        assert_eq!(text(&batch,0,1).unwrap_err(),"Host metadata expected text");
    }
    let batch=RecordBatch::try_from_iter(vec![("column_name",Arc::new(Int64Array::from(vec![1])) as ArrayRef)]).unwrap();
    assert_eq!(text(&batch,0,0).unwrap_err(),"Host metadata expected text");
}

#[test]
fn mapped_property_values_preserve_exact_scalars_and_nullable_list_items() {
    let decimal = ScalarValue::Decimal128(Some(12345), 12, 3);
    assert_eq!(value_scalar(&Value::Scalar(decimal.clone())).unwrap(), decimal);
    for value in [Value::BigInt("123456789012345678901234567890123456789".parse().unwrap()),
        Value::UInt128("340282366920938463463374607431768211455".parse().unwrap()),
        Value::BigDecimal("12345678901234567890.000000000123".parse().unwrap())] {
        let expected = match &value {
            Value::BigInt(n) | Value::UInt128(n) => n.to_string(),
            Value::BigDecimal(n) => n.to_string(),
            _ => unreachable!(),
        };
        assert_eq!(value_scalar(&value).unwrap(), ScalarValue::Utf8(Some(expected)));
    }
    let list = value_scalar(&Value::List(vec![Value::Null, Value::Scalar(ScalarValue::Int8(Some(7))), Value::Null])).unwrap();
    let ScalarValue::List(array) = list else { panic!("expected list") };
    assert_eq!(array.value_type(), DataType::Int8);
    assert_eq!(array.value(0).len(), 3);
    for (row, expected) in [None, Some(7), None].into_iter().enumerate() {
        assert_eq!(ScalarValue::try_from_array(&array.value(0), row).unwrap(), ScalarValue::Int8(expected));
    }
    assert!(value_scalar(&Value::List(vec![Value::Int(7), Value::String("7".into())])).unwrap_err()
        .contains("heterogeneous list property"));
}

#[test]
fn mapped_temporal_lists_keep_dates_and_null_entries_in_physical_storage() {
    let date = crate::ir::temporal::TemporalValue::Date(chrono::NaiveDate::from_ymd_opt(1970, 1, 2).unwrap());
    let value = value_scalar(&Value::List(vec![Value::Temporal(date), Value::Null])).unwrap();
    let kind = DataType::List(Arc::new(Field::new("item", DataType::Date32, true)));
    let result = temporal_storage(value, &kind).unwrap().cast_to(&kind).unwrap();
    let ScalarValue::List(array) = result else { panic!("expected list") };
    assert_eq!(array.value_type(), DataType::Date32);
    assert_eq!(ScalarValue::try_from_array(&array.value(0), 0).unwrap(), ScalarValue::Date32(Some(1)));
    assert_eq!(ScalarValue::try_from_array(&array.value(0), 1).unwrap(), ScalarValue::Date32(None));
}

#[test]
fn mapped_null_temporal_lists_convert_to_the_declared_storage_type() {
    let date = crate::ir::temporal::TemporalValue::Date(chrono::NaiveDate::from_ymd_opt(1970, 1, 2).unwrap());
    let transport = crate::ir::temporal::transport::scalar(&date).unwrap().data_type();
    let input_kind = DataType::List(Arc::new(Field::new("item", transport, true)));
    let kind = DataType::List(Arc::new(Field::new("item", DataType::Date32, true)));
    let input = ScalarValue::try_from(&input_kind).unwrap();
    let result = temporal_storage(input.clone(), &kind).unwrap().cast_to(&kind).unwrap();
    assert_eq!(result.data_type(), kind);
    assert!(result.is_null());
    let dates = value_scalar(&Value::List(vec![Value::Temporal(date)])).unwrap();
    let nested = ScalarValue::List(ScalarValue::new_list_nullable(&[input, dates], &input_kind));
    let nested_kind = DataType::List(Arc::new(Field::new("item", kind.clone(), true)));
    let result = temporal_storage(nested, &nested_kind).unwrap().cast_to(&nested_kind).unwrap();
    let ScalarValue::List(array) = result else { panic!("expected nested list") };
    let null_list = ScalarValue::try_from_array(&array.value(0), 0).unwrap();
    assert_eq!(null_list.data_type(), kind);
    assert!(null_list.is_null());
    let ScalarValue::List(dates) = ScalarValue::try_from_array(&array.value(0), 1).unwrap() else { panic!("expected date list") };
    assert_eq!(ScalarValue::try_from_array(&dates.value(0), 0).unwrap(), ScalarValue::Date32(Some(1)));
}

#[cfg(feature = "duckdb")]
#[test]
fn mapped_temporal_list_writes_persist_typed_null_and_date_elements() {
    use crate::ir::rel::{mapping::NodeMapping, sql::DuckDbExecutor, host::legacy::ExecutorHost};
    use std::sync::Mutex;
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE people(id BIGINT PRIMARY KEY, dates DATE[]); INSERT INTO people VALUES(1, [DATE '2000-01-01'])").unwrap();
    let shared = Arc::new(Mutex::new(DuckDbExecutor::from_connection(db)));
    let mut mapping = GraphMapping::new();
    mapping.map_node(NodeMapping::table("Person", "people", "id").property("dates", "dates"));
    let mapping = Arc::new(mapping);
    let graph = crate::engine::mapped_source::attach(shared.clone(), mapping.clone()).unwrap();
    let host = ExecutorHost::new(shared);
    let node = Value::Node { label: "Person".into(), id: 1_i64.into() };
    let date = crate::ir::temporal::TemporalValue::Date(chrono::NaiveDate::from_ymd_opt(1970, 1, 2).unwrap());
    let transport = crate::ir::temporal::transport::scalar(&date).unwrap().data_type();
    let kind = DataType::List(Arc::new(Field::new("item", transport, true)));
    graph.set_property(&node, "dates", Value::Scalar(ScalarValue::try_from(&kind).unwrap())).unwrap();
    persist(&host, &graph, &mapping).unwrap();
    let actual = query(&host, "SELECT dates IS NULL AS is_null FROM people WHERE id=1").unwrap();
    assert_eq!(ScalarValue::try_from_array(actual.column(0), 0).unwrap(), ScalarValue::Boolean(Some(true)));
    graph.set_property(&node, "dates", Value::List(vec![Value::Temporal(date), Value::Null])).unwrap();
    persist(&host, &graph, &mapping).unwrap();
    let actual = query(&host, "SELECT dates FROM people WHERE id=1").unwrap();
    let ScalarValue::List(dates) = ScalarValue::try_from_array(actual.column(0), 0).unwrap() else { panic!("expected stored date list") };
    assert_eq!(ScalarValue::try_from_array(&dates.value(0), 0).unwrap(), ScalarValue::Date32(Some(1)));
    assert_eq!(ScalarValue::try_from_array(&dates.value(0), 1).unwrap(), ScalarValue::Date32(None));
}
