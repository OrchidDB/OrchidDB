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
