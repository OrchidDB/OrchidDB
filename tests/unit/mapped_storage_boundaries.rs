use super::*;

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
