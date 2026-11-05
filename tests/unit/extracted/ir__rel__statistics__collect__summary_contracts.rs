
    use super::*;
    use serde_json::json;

fn generator()->Generator {
    Generator::new(json!({"dialect":"duckdb","tables":[{"name":"items","columns":[{"name":"id","data_type":"int64"}]}]})).unwrap()
}

#[test]
fn statistics_truncated_lists_record_original_lengths_without_inventing_observations() {
    let mut generator=Generator::new(json!({"tables":[{"name":"items","columns":[{"name":"xs","data_type":"list:int64"}]}]})).unwrap();
    generator.sources[0].rows=vec![BTreeMap::from([
        ("xs".into(),json!((0..64).collect::<Vec<_>>())),
        ("__statistics_length_xs".into(),json!(100)),
    ]),BTreeMap::from([("xs".into(),json!([])),("__statistics_length_xs".into(),json!(0))])];
    let snapshot=generator.finish();
    let list=snapshot.sources["items"].columns["xs"].list.as_ref().unwrap();
    assert_eq!(list.total_lengths,100);assert_eq!(list.observed_elements,64);
    assert_eq!(list.empty_lists,1);assert!(list.truncated);
}

#[test]
fn statistics_snapshot_budget_discards_distributions_then_oversized_sources() {
    let mut generator=generator();
    generator.sources[0].name="x".repeat(MAX_SNAPSHOT_BYTES);
    generator.sources[0].rows=vec![BTreeMap::from([("id".into(),json!(7))])];
    let snapshot=generator.finish();
    assert!(!snapshot.report.complete);assert!(snapshot.sources.is_empty());
    assert!(snapshot.report.notes.iter().any(|note|note.contains("Snapshot size budget")));
    assert!(serde_json::to_vec(&snapshot).unwrap().len()<MAX_SNAPSHOT_BYTES);
    snapshot.validate().unwrap();
}

#[test]
fn statistics_sampling_scopes_catalog_names_and_uses_each_dialects_sampling_clause() {
    for (dialect,name) in [("duckdb","\"catalog'one\".\"schema'one\".\"table'one\""),
        ("postgres","\"schema'one\".\"table'one\"")] {
        let mut generator=Generator::new(json!({"dialect":dialect,"tables":[{"name":name,"columns":[]}]})).unwrap();
        let metadata=generator.next().unwrap();
        assert!(metadata.sql.contains("'table''one'"));assert!(metadata.sql.contains("'schema''one'"));
        if dialect=="duckdb" {assert!(metadata.sql.contains("'catalog''one'"));}
        generator.submit(&metadata.id,vec![BTreeMap::from([("rows".into(),json!(1000000))])],None,true).unwrap();
        let sample=generator.next().unwrap();
        assert!(sample.sql.contains("1 AS __statistics_row"));
        assert!(sample.sql.contains(if dialect=="duckdb" {"USING SAMPLE"}else{"TABLESAMPLE SYSTEM"}));
        assert!(sample.sql.ends_with(&format!("LIMIT {}",sample.max_rows)));
        assert!(sample.timeout_ms<=TIMEOUT_MS);assert!(sample.max_bytes<=1024*1024);
    }
}

#[test]
fn statistics_wide_sources_prioritize_graph_keys_properties_and_collection_inputs() {
    let columns=(0..80).map(|n|json!({"name":format!("c{n}"),"data_type":"int64"})).collect::<Vec<_>>();
    let generator=Generator::new(json!({"tables":[{"name":"items","columns":columns}],
        "nodes":[{"table":"items","id":"c79","properties":{"value":"c78"}}],
        "collection_sources":[{"column":"c77","parent_columns":{"parent":"c76"}}]})).unwrap();
    let selected=generator.sources[0].selected.iter().map(|i|generator.sources[0].schema.field(*i).name().as_str()).collect::<Vec<_>>();
    assert_eq!(selected.len(),64);
    for field in ["c79","c78","c77","c76"] {assert!(selected.contains(&field));}
    let snapshot=generator.finish();assert!(snapshot.report.notes.iter().any(|note|note.contains("selected 64 of 80 columns")));
    let sources=(0..20).map(|n|json!({"name":format!("t{n}"),"columns":[]})).collect::<Vec<_>>();
    let mut generator=Generator::new(json!({"tables":sources})).unwrap();
    assert_eq!(generator.next().unwrap().kind,"sample","wide catalogs reserve sampling work for every source");
}

#[test]
fn statistics_deadline_and_each_budget_stop_without_issuing_more_requests() {
    for limit in 0..6 {
        let mut generator=generator();
        match limit {
            0=>generator.started=Instant::now()-std::time::Duration::from_millis(TIMEOUT_MS+1),
            1=>generator.report.accepted_rows=MAX_ROWS,
            2=>generator.report.accepted_bytes=MAX_BYTES,
            3=>generator.report.requests=MAX_REQUESTS,
            4=>generator.elements=MAX_ELEMENTS,
            _=>generator.retained_bytes=MAX_RETAINED_BYTES,
        }
        let requests=generator.report.requests;
        assert!(generator.exhausted());assert!(generator.next().is_none());
        assert_eq!(generator.report.requests,requests);
        assert_eq!(generator.report.stopped,if limit==0 {"collection deadline reached"}else{"collection budget reached"});
    }
}

#[test]
fn statistics_pending_request_survives_wrong_receipts_and_parses_finite_metadata() {
    let mut generator=generator();
    assert_eq!(generator.submit("missing",vec![],None,true).unwrap_err(),"no outstanding statistics request");
    let request=generator.next().unwrap();
    assert_eq!(request.kind,"metadata");
    assert_eq!(generator.submit("wrong",vec![],None,true).unwrap_err(),"statistics request id mismatch");
    assert_eq!(generator.next().unwrap().id,request.id);
    for (rows,bytes,expected_rows,expected_bytes) in [(json!("12.5"),json!("64"),Some(12.5),Some(64.0)),
        (json!("NaN"),json!(-1),None,None)] {
        generator.submit(&request.id,vec![BTreeMap::from([("rows".into(),rows),("bytes".into(),bytes)])],None,false).unwrap();
        assert_eq!(generator.sources[0].estimated_rows,expected_rows);
        assert_eq!(generator.sources[0].estimated_bytes,expected_bytes);
    }
    generator.report.accepted_rows=MAX_ROWS;
    assert_eq!(generator.next().unwrap().id,request.id,"an outstanding request remains resumable");
    generator.submit(&request.id,vec![BTreeMap::from([("rows".into(),json!(100))])],None,true).unwrap();
    assert_eq!(generator.report.stopped,"sample budget reached");
    assert_eq!(generator.report.accepted_rows,MAX_ROWS);
    assert!(generator.next().is_none());
}

#[test]
fn statistics_generator_rejects_incomplete_or_ambiguous_source_metadata() {
    for (request,message) in [
        (json!({"dialect":"unknown","tables":[]}),"supported dialect"),
        (json!({}),"requires tables"),
        (json!({"tables":[{}]}),"source missing name"),
        (json!({"tables":[{"name":"a"}]}),"source missing columns"),
        (json!({"tables":[{"name":"a","columns":[{}]}]}),"column name missing"),
        (json!({"tables":[{"name":"a","columns":[{"name":"id"}]}]}),"column type missing"),
        (json!({"tables":[{"name":"a","columns":[]},{"name":"a","columns":[]}]}),"duplicate statistics source"),
    ] {
        let error=Generator::new(request).err().expect("metadata must be rejected");
        assert!(error.contains(message),"{error}");
    }
}

#[cfg(feature="duckdb")]
#[test]
fn statistics_previews_bound_nested_payloads_without_corrupting_nulls_or_unicode() {
    let connection=duckdb::Connection::open_in_memory().unwrap();
    for datatype in [DataType::List(std::sync::Arc::new(Field::new("item",DataType::Utf8,true))),
        DataType::LargeList(std::sync::Arc::new(Field::new("item",DataType::Utf8,true)))] {
        let expression=preview("payload",&datatype,"duckdb");
        let sql=format!("SELECT to_json({expression})::VARCHAR FROM (SELECT list_transform(range(70), i -> CASE WHEN i=0 THEN NULL ELSE repeat('雪',300) END) AS payload)");
        let result=connection.query_row(&sql,[],|row|row.get::<_,String>(0)).unwrap();
        let result:Value=serde_json::from_str(&result).unwrap();let values=result.as_array().unwrap();
        assert_eq!(values.len(),64);assert!(values[0].is_null());
        assert!(values.iter().skip(1).all(|value|value.as_str()==Some("雪".repeat(256).as_str())));
        assert_eq!(preview("payload",&datatype,"postgres"),"NULL");
    }
    let datatype=DataType::Struct(vec![Field::new("odd\"name",DataType::Utf8,true),Field::new("number",DataType::Int64,true)].into());
    let expression=preview("payload",&datatype,"duckdb");
    let sql=format!("SELECT to_json({expression})::VARCHAR FROM (SELECT struct_pack(\"odd\"\"name\" := repeat('x',300), number := 7::BIGINT) AS payload)");
    let result=connection.query_row(&sql,[],|row|row.get::<_,String>(0)).unwrap();
    assert_eq!(serde_json::from_str::<Value>(&result).unwrap(),json!({"odd\"name":"x".repeat(256),"number":7}));
    let sql=format!("SELECT {expression} IS NULL FROM (SELECT NULL::STRUCT(\"odd\"\"name\" VARCHAR, number BIGINT) AS payload)");
    assert!(connection.query_row(&sql,[],|row|row.get::<_,bool>(0)).unwrap());
}

    #[test]
    fn scalar_histograms_preserve_frequency_nulls_and_numeric_order() {
        let values=vec![Value::Null,json!(10),json!(2),json!(2),json!(-1)];
        let result=summarize(&values,"int64",true,&mut 0);
        assert_eq!((result.observations,result.nulls,result.sample_distinct),(5,1,3));
        assert_eq!(result.estimated_distinct,Some(3.0));
        assert_eq!(result.minimum.as_deref(),Some("-1"));
        assert_eq!(result.maximum.as_deref(),Some("10"));
        assert_eq!(result.histogram,vec!["-1","2","2","10"]);
        assert_eq!(result.frequent[0],("2".into(),2));
        assert_eq!(result.average_width,6.0/5.0);
        let empty=summarize(&[],"int64",false,&mut 0);
        assert_eq!(empty.estimated_distinct,None);assert_eq!(empty.average_width,0.0);
        assert!(empty.histogram.is_empty());assert!(empty.minimum.is_none());
        let many=summarize(&(0..130).map(|n|json!(n)).collect::<Vec<_>>(),"int64",false,&mut 0);
        assert_eq!(many.histogram.len(),64);assert_eq!(many.frequent.len(),32);
        assert_eq!(many.sample_distinct,130);
    }

    #[test]
    fn nested_list_summaries_count_parent_presence_and_respect_global_budget() {
        let values=vec![json!([1,1,null]),json!([]),Value::Null,json!([{"x":2},{"x":2,"y":3}])];
        let mut elements=0;
        let summary=summarize(&values,"list",true,&mut elements);
        assert!(summary.minimum.is_none());assert!(summary.maximum.is_none());
        assert!(summary.frequent.is_empty());assert!(summary.histogram.is_empty());
        let list=summary.list.unwrap();
        assert_eq!((list.parents,list.null_lists,list.empty_lists,list.total_lengths,list.observed_elements),(4,1,1,5,5));
        assert_eq!(elements,5);assert_eq!(list.lengths,vec![0,2,3]);
        assert_eq!(list.parents_containing["1"],1);assert_eq!(list.parents_containing["x:2"],1);
        assert_eq!(list.elements["$element"].nulls,1);assert_eq!(list.elements["x"].observations,2);
        let mut elements=MAX_ELEMENTS-1;
        let truncated=summarize(&[json!([1,2]),json!([3])],"list",true,&mut elements).list.unwrap();
        assert!(truncated.truncated);assert_eq!(elements,MAX_ELEMENTS);
        assert_eq!(truncated.total_lengths,3);assert_eq!(truncated.observed_elements,1);
        assert_eq!(truncated.elements["$element"].estimated_distinct,None);
        let bounded=summarize(&(0..130).map(|n|json!([n])).collect::<Vec<_>>(),"list",true,&mut 0).list.unwrap();
        assert_eq!(bounded.parents_containing.len(),128);assert_eq!(bounded.lengths.len(),64);
        assert!(bounded.lengths.iter().all(|n|*n==1));
    }

    #[test]
    fn tuple_summaries_treat_missing_values_as_null_and_keep_joint_counts() {
        let rows=[json!({"a":1,"b":2}),json!({"a":1,"b":2}),json!({"a":1}),json!({"a":1,"b":null})]
            .into_iter().map(|v|serde_json::from_value::<BTreeMap<String,Value>>(v).unwrap()).collect::<Vec<_>>();
        let summary=group(&rows,&["a".into(),"b".into()]);
        assert_eq!((summary.observations,summary.sample_distinct),(4,2));
        assert_eq!(summary.frequent,vec![(vec!["1".into(),"2".into()],2),(vec!["1".into(),"null".into()],2)]);
        assert_eq!(key_columns(&json!("a")),vec!["a"]);
        assert_eq!(key_columns(&json!(["a",1,"b"])),vec!["a","b"]);
        assert!(key_columns(&Value::Null).is_empty());
    }

    #[test]
    fn numeric_bounds_do_not_round_large_integers_or_decimal_neighbors() {
        use std::cmp::Ordering::{Less,Equal,Greater};
        for (left,right,expected) in [
            ("-9007199254740993","-9007199254740992",Less),
            ("9007199254740993","9007199254740992",Greater),
            ("1.00000000000000000001","1",Greater),
            ("1e100","9e99",Greater),("1.0","1",Equal),
            ("\"a\"","\"b\"",Less),("-Infinity","Infinity",Less),
        ] {assert_eq!(numeric_cmp(left,right),expected,"{left}, {right}");}
    }

#[test]
fn empty_block_samples_retry_with_a_bounded_prefix_on_each_sampling_dialect() {
    for dialect in ["duckdb","postgres"] {
        let mut generator=Generator::new(json!({"dialect":dialect,"tables":[{"name":"items","columns":[{"name":"id","data_type":"int64"}]}]})).unwrap();
        let metadata=generator.next().unwrap();
        generator.submit(&metadata.id,vec![BTreeMap::from([("rows".into(),json!(64000))])],None,true).unwrap();
        let sample=generator.next().unwrap();
        assert!(sample.sql.contains("SAMPLE"));
        generator.submit(&sample.id,vec![],None,false).unwrap();
        assert_eq!(generator.next().unwrap().id,sample.id,"pending request is replayed until complete");
        generator.submit(&sample.id,vec![],None,true).unwrap();
        let retry=generator.next().unwrap();
        assert!(!retry.sql.contains("SAMPLE"));
        assert_eq!(retry.kind,"sample");
        assert!(retry.sql.ends_with(&format!("LIMIT {}",retry.max_rows)));
        assert!(retry.max_bytes<=sample.max_bytes);
        generator.submit(&retry.id,vec![BTreeMap::from([("id".into(),json!(7))])],None,true).unwrap();
        assert!(generator.next().is_none());
        let snapshot=generator.finish();
        assert!(snapshot.report.complete);
        assert_eq!(snapshot.sources["items"].columns["id"].observations,1);
        assert_eq!(snapshot.sources["items"].estimated_rows,Some(1.0));
    }
}

#[test]
fn empty_sample_errors_and_exhausted_request_budgets_do_not_trigger_unbounded_retries() {
    for error in [None,Some("sample failed".to_string())] {
        let mut generator=generator();
        let metadata=generator.next().unwrap();
        generator.submit(&metadata.id,vec![BTreeMap::from([("rows".into(),json!(64000))])],None,true).unwrap();
        let sample=generator.next().unwrap();
        if error.is_none() {generator.report.requests=MAX_REQUESTS;}
        generator.submit(&sample.id,vec![],error.clone(),true).unwrap();
        assert!(generator.next().is_none());
        let snapshot=generator.finish();
        assert!(!snapshot.report.complete);
        assert_eq!(snapshot.sources["items"].estimated_rows,Some(64000.0));
    }
}

#[test]
fn statistics_empty_sample_retry_terminates_and_nonempty_samples_keep_metadata_counts() {
    for (sample_has_rows,prefix_error) in [(true,false),(false,false),(false,true)] {
        let mut generator=generator();
        let metadata=generator.next().unwrap();
        generator.submit(&metadata.id,vec![BTreeMap::from([("rows".into(),json!(64000))])],None,true).unwrap();
        let sample=generator.next().unwrap();
        // An empty transport batch is not EOF and cannot schedule a retry.
        generator.submit(&sample.id,vec![],None,false).unwrap();
        assert_eq!(generator.next().unwrap().id,sample.id);
        if sample_has_rows {
            generator.submit(&sample.id,vec![BTreeMap::from([("id".into(),json!(7))])],None,false).unwrap();
        }
        generator.submit(&sample.id,vec![],None,true).unwrap();
        if !sample_has_rows {
            let retry=generator.next().unwrap();
            assert!(!retry.sql.contains("SAMPLE"));
            generator.submit(&retry.id,vec![],prefix_error.then(||"prefix failed".into()),true).unwrap();
        }
        assert!(generator.next().is_none(),"no repeated fallback after empty prefix or prefix error");
        let snapshot=generator.finish();
        assert_eq!(snapshot.report.requests,if sample_has_rows {2}else{3});
        assert_eq!(snapshot.report.complete,!prefix_error);
        assert_eq!(snapshot.sources["items"].estimated_rows,
            Some(if sample_has_rows || prefix_error {64000.0}else{0.0}));
        assert_eq!(snapshot.sources["items"].sample_rows,u64::from(sample_has_rows));
    }
}
