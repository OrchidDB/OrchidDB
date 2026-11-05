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

#[derive(Debug)]
struct WriteOrderHost(Option<RecordBatch>);
impl HostRelational for WriteOrderHost {
    fn query(&self, _: HostRequest) -> Result<RecordBatch, String> {
        self.0.clone().ok_or_else(|| "new rows must not query old foreign keys".into())
    }
    fn execute(&self, _: HostRequest) -> Result<(), String> {
        panic!("dependency planning must not execute writes")
    }
}

fn composite_write_fixture() -> (RowAddress, RowAddress, Vec<Reference>, BTreeMap<String, Arc<Schema>>) {
    let parent_key = KeyColumns::from(["tenant", "id"]);
    let parent = ("parents".into(), parent_key.clone(),
        ElementId::from_components(vec![ScalarValue::Int64(Some(1)), ScalarValue::Int64(Some(2))]).unwrap());
    let child = ("children".into(), KeyColumns::from("id"), ElementId::from(10_i64));
    let refs = vec![("children".into(), KeyColumns::from(["tenant", "parent"]), "parents".into(), parent_key)];
    let schemas = BTreeMap::from([("parents".into(), Arc::new(Schema::new(vec![
        Field::new("tenant", DataType::Int64, false), Field::new("id", DataType::Int64, false)])))]);
    (parent, child, refs, schemas)
}

#[test]
fn mapped_write_order_respects_composite_dependencies_and_match_simple_nulls() {
    let (parent, child, refs, schemas) = composite_write_fixture();
    for fk in [Some(2), None] {
        let rows = BTreeMap::from([
            (parent.clone(), RowWrite { existing: false, delete: false, values: BTreeMap::new() }),
            (child.clone(), RowWrite { existing: false, delete: false, values: BTreeMap::from([
                ("tenant".into(), ScalarValue::Int64(Some(1))), ("parent".into(), ScalarValue::Int64(fk))]) }),
        ]);
        let waves = write_order(&WriteOrderHost(None), &rows, &refs, &schemas).unwrap();
        if fk.is_some() { assert_eq!(waves, vec![vec![parent.clone()], vec![child.clone()]]); }
        else { assert_eq!(waves, vec![vec![child.clone(), parent.clone()]]); }
    }
    let first = address(1); let second = address(2);
    let rows = BTreeMap::from([
        (first.clone(), RowWrite {existing:false, delete:false, values:BTreeMap::from([("parent".into(),ScalarValue::Int64(Some(2)))])}),
        (second.clone(), RowWrite {existing:false, delete:false, values:BTreeMap::from([("parent".into(),ScalarValue::Int64(Some(1)))])}),
    ]);
    let refs = vec![("people".into(),KeyColumns::from("parent"),"people".into(),KeyColumns::from("id"))];
    let schemas = BTreeMap::from([("people".into(),Arc::new(Schema::new(vec![Field::new("id",DataType::Int64,false)])))]);
    assert_eq!(write_order(&WriteOrderHost(None),&rows,&refs,&schemas).unwrap(),vec![vec![first,second]],
        "mapping cycles are submitted together for the physical backend to validate");
}

#[cfg(feature = "duckdb")]
#[test]
fn mapped_parent_deletion_requires_child_deletion_or_unlink_first() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE children(id BIGINT, tenant BIGINT, parent BIGINT); INSERT INTO children VALUES(10,1,2)").unwrap();
    let host = crate::ir::rel::host::legacy::ConnectionHost(&db);
    let (parent, child, refs, schemas) = composite_write_fixture();
    for delete_child in [true, false] {
        let rows = BTreeMap::from([
            (parent.clone(), RowWrite {existing:true, delete:true, values:BTreeMap::new()}),
            (child.clone(), RowWrite {existing:true, delete:delete_child,
                values:if delete_child {BTreeMap::new()} else {BTreeMap::from([("parent".into(),ScalarValue::Int64(None))])}}),
        ]);
        assert_eq!(write_order(&host,&rows,&refs,&schemas).unwrap(),vec![vec![child.clone()],vec![parent.clone()]]);
    }
    let rows = BTreeMap::from([
        (parent, RowWrite {existing:true, delete:true, values:BTreeMap::new()}),
        (child, RowWrite {existing:true, delete:false, values:BTreeMap::new()}),
    ]);
    assert!(write_order(&host,&rows,&refs,&schemas).unwrap_err().contains("still referenced by a child row"));
    let result = query(&host,"SELECT count(*) FROM children").unwrap();
    assert_eq!(ScalarValue::try_from_array(result.column(0),0).unwrap(),ScalarValue::Int64(Some(1)));
}

#[test]
fn mapped_write_order_rejects_a_scalar_result_for_a_composite_foreign_key() {
    let (parent, child, refs, schemas) = composite_write_fixture();
    let rows = BTreeMap::from([
        (parent,RowWrite {existing:true,delete:true,values:BTreeMap::new()}),
        (child,RowWrite {existing:true,delete:true,values:BTreeMap::new()}),
    ]);
    let batch = bind_batch(vec![("bad_fk".into(),ScalarValue::Int64(Some(2)))]).unwrap();
    assert_eq!(write_order(&WriteOrderHost(Some(batch)),&rows,&refs,&schemas).unwrap_err(),
        "foreign key result has wrong arity");
}

#[cfg(feature = "duckdb")]
#[test]
fn mapped_metadata_rejects_partition_keys_with_incompatible_physical_types() {
    use crate::ir::rel::mapping::{NodeMapping, EdgeMapping};
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE a(id BIGINT); CREATE TABLE b(id BIGINT); CREATE TABLE c(id BIGINT); CREATE TABLE ab(id BIGINT, src BIGINT, dst BIGINT); CREATE TABLE ac(id VARCHAR, src BIGINT, dst BIGINT)").unwrap();
    let host = crate::ir::rel::host::legacy::ConnectionHost(&db);
    let mut mapping = GraphMapping::new();
    for (label, table) in [("A","a"),("B","b"),("C","c")] {
        mapping.map_node(NodeMapping::table(label,table,"id"));
    }
    mapping.map_edge_partition(EdgeMapping::table("R","ab","src","dst","A","B").with_id("id"));
    mapping.map_edge_partition(EdgeMapping::table("R","ac","src","dst","A","C").with_id("id"));
    let error = metadata(&host,Arc::new(mapping)).unwrap_err();
    assert!(error.contains("partitions need a common key type"),"{error}");
}

#[cfg(feature = "duckdb")]
#[test]
fn mapped_metadata_loads_shared_defaults_once_without_evaluating_volatile_defaults() {
    use crate::ir::rel::mapping::NodeMapping;
    #[derive(Debug)]
    struct ObservedHost<'a> {
        connection: &'a duckdb::Connection,
        defaults_reads: std::cell::Cell<usize>,
    }
    impl HostRelational for ObservedHost<'_> {
        fn query(&self, request:HostRequest) -> Result<RecordBatch,String> {
            if request.sql.starts_with("SELECT column_name, column_default") {
                self.defaults_reads.set(self.defaults_reads.get()+1);
            }
            crate::ir::rel::host::legacy::ConnectionHost(self.connection).query(request)
        }
        fn execute(&self, _:HostRequest) -> Result<(),String> {panic!("metadata must not execute writes")}
    }
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE SEQUENCE default_ids; CREATE TABLE people(id BIGINT PRIMARY KEY, fixed BIGINT DEFAULT (2+3), generated BIGINT DEFAULT nextval('default_ids'))").unwrap();
    let host = ObservedHost {connection:&db,defaults_reads:std::cell::Cell::new(0)};
    let mut mapping = GraphMapping::new();
    for label in ["Person","Alias"] {
        mapping.map_node(NodeMapping::table(label,"people","id").property("answer","fixed").property("generated","generated"));
    }
    let mapping = Arc::new(mapping);
    let graph = metadata(&host,mapping.clone()).unwrap();
    assert_eq!(host.defaults_reads.get(),1);
    for label in ["Person","Alias"] {
        assert_eq!(graph.mapped_defaults[&(false,label.into())]["answer"],Value::Int(5));
        assert!(graph.unsupported_defaults[&(false,label.into())].contains("generated"));
    }
    let next = query(&host,"SELECT nextval('default_ids')").unwrap();
    assert_eq!(ScalarValue::try_from_array(next.column(0),0).unwrap(),ScalarValue::Int64(Some(1)));
    host.defaults_reads.set(0);
    let unloaded = metadata_with_insert_defaults(&host,mapping,false).unwrap();
    assert!(unloaded.mapped_defaults.is_empty());
    assert!(unloaded.unsupported_defaults.is_empty());
    assert_eq!(host.defaults_reads.get(),0);
}
