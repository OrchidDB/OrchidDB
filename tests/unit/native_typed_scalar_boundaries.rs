use super::*;

#[test]
fn negative_epoch_timestamps_preserve_scale_and_zone() {
    for (value, expected, zoned) in [
        (ScalarValue::TimestampSecond(Some(-1),None),"1969-12-31 23:59:59",false),
        (ScalarValue::TimestampMillisecond(Some(-1),None),"1969-12-31 23:59:59.999",false),
        (ScalarValue::TimestampMicrosecond(Some(-1),None),"1969-12-31 23:59:59.999999",false),
        (ScalarValue::TimestampNanosecond(Some(-1),None),"1969-12-31 23:59:59.999999999",false),
        (ScalarValue::TimestampSecond(Some(-1),Some("UTC".into())),"1969-12-31 23:59:59 +00:00",true),
    ] {
        let actual = scalar(value,&mut Context::default()).unwrap();
        match actual {
            Value::Temporal(crate::ir::temporal::TemporalValue::LocalDateTime(value)) => {assert!(!zoned);assert_eq!(value.to_string(),expected);},
            Value::Temporal(crate::ir::temporal::TemporalValue::DateTime(value,_)) => {assert!(zoned);assert_eq!(value.to_string(),expected);},
            other => panic!("unexpected timestamp: {other:?}"),
        }
    }
    assert!(scalar(ScalarValue::TimestampSecond(Some(i64::MAX),None),&mut Context::default()).unwrap_err().contains("temporal range"));
    assert_eq!(scalar(ScalarValue::TimestampNanosecond(None,Some("UTC".into())),&mut Context::default()).unwrap(),Value::Null);
}

fn domain(name: &str, payload: ScalarValue) -> ScalarValue {
    let field = Arc::new(Field::new(name,payload.data_type(),true));
    ScalarValue::Struct(Arc::new(StructArray::new(vec![field].into(),vec![payload.to_array_of_size(1).unwrap()],None)))
}

#[test]
fn temporal_domains_enforce_the_same_declared_kind_for_arrow_and_json() {
    let text = ScalarValue::Utf8(Some("2020-01-02".into()));
    let valid = scalar(domain("__orchiddb_domain_cypher_temporal_date",text.clone()),&mut Context::default()).unwrap();
    assert_eq!(valid,Value::Temporal(crate::ir::temporal::parse("date","2020-01-02").unwrap()));
    assert_eq!(scalar(domain("__orchiddb_domain_cypher_temporal_date",ScalarValue::Utf8(None)),&mut Context::default()).unwrap(),Value::Null);
    assert!(scalar(domain("__orchiddb_domain_cypher_temporal_date",ScalarValue::Int64(Some(1))),&mut Context::default()).unwrap_err().contains("Invalid temporal domain payload"));
    let date = ScalarValue::Date32(Some(18263));
    assert_eq!(scalar(domain("__orchiddb_domain_cypher_temporal_date",date.clone()),&mut Context::default()).unwrap(),valid);
    assert!(scalar(domain("__orchiddb_domain_cypher_temporal_datetime",date),&mut Context::default()).is_err());
    for kind in ["datetime","localdatetime","time","duration"] {
        let name = format!("__orchiddb_domain_cypher_temporal_{kind}");
        let json = serde_json::json!({name.clone():"2020-01-02"});
        let expected = from_json(&json,&mut Context::default()).unwrap_err();
        assert_eq!(scalar(domain(&name,text.clone()),&mut Context::default()).unwrap_err(),expected);
    }
}

#[test]
fn recursive_edge_projection_preserves_path_identity_and_property_data() {
    let graph=PropertyGraph::new();
    let a=graph.insert_node("N",BTreeMap::from([("name".into(),Value::String("a".into()))]));
    let b=graph.insert_node("N",BTreeMap::from([("name".into(),Value::String("b".into()))]));
    let edge=graph.insert_edge("R",&a,&b,BTreeMap::from([("weight".into(),Value::Int(7)),("hidden".into(),Value::Int(9))])).unwrap();
    for items in [vec![],vec![a.clone()],vec![a.clone(),edge.clone(),b.clone()]] {
        let path=Value::Path(items.clone());
        let mut context=Context::default();context.capture(&path,&graph).unwrap();
        for names in [vec![],vec!["weight"],vec!["missing","weight"]] {
            let keys=Value::List(names.iter().map(|name|Value::String((*name).into())).collect());
            let result=evaluate("path_project_edges",&[values_array(&[path.clone()],&context),values_array(&[keys],&context)],1).unwrap();
            let mut decoded=Context::default();
            let actual=scalar(ScalarValue::try_from_array(&result,0).unwrap(),&mut decoded).unwrap();
            let Value::Path(values)=actual else {panic!("expected path")};
            assert_eq!(values.len(),items.len());
            for (original,projected) in items.iter().zip(&values) {
                let mut expected=original.clone();
                if let Value::Edge{projected_properties,..}=&mut expected {
                    *projected_properties=Some(names.iter().map(|s|(*s).into()).collect());
                }
                assert_eq!(*projected,expected);
                assert_eq!(context.cypher_identity(original),decoded.cypher_identity(projected));
            }
            if let Some(edge)=values.get(1) {
                let Value::Edge{rel_type,id,..}=edge else {panic!("edge identity")};
                assert_eq!(decoded.graph.edge_property(rel_type,id.clone(),"weight"),Value::Int(7));
                assert_eq!(decoded.graph.edge_property(rel_type,id.clone(),"hidden"),Value::Int(9));
            }
        }
    }
}

#[test]
fn arrow_local_times_preserve_units_nulls_and_reject_out_of_day_values() {
    let cases: &[(fn(Option<i64>)->ScalarValue, i64, &str)] = &[
        (|n|ScalarValue::Time32Second(n.map(|n|n as i32)),1,"23:59:59"),
        (|n|ScalarValue::Time32Millisecond(n.map(|n|n as i32)),1000,"23:59:59.999"),
        (ScalarValue::Time64Microsecond,1_000_000,"23:59:59.999999"),
        (ScalarValue::Time64Nanosecond,1_000_000_000,"23:59:59.999999999"),
    ];
    for (value, scale, last) in cases {
        for (ticks,text) in [(0,"00:00:00"),(86400*scale-1,*last)] {
            assert_eq!(scalar(value(Some(ticks)),&mut Context::default()).unwrap(),Value::Temporal(crate::ir::temporal::parse("localtime",text).unwrap()));
        }
        assert_eq!(scalar(value(None),&mut Context::default()).unwrap(),Value::Null);
        for ticks in [-1,86400*scale] {
            assert!(scalar(value(Some(ticks)),&mut Context::default()).unwrap_err().contains("local time range"));
        }
    }
    assert!(scalar(ScalarValue::Time64Nanosecond(Some(i64::MAX)),&mut Context::default()).is_err());
    assert!(scalar(ScalarValue::Time64Microsecond(Some(i64::MIN)),&mut Context::default()).is_err());
}
