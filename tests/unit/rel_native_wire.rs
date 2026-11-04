use super::*;
use arrow::datatypes::{TimeUnit, i256};

#[test]
fn floating_literals_roundtrip_every_ieee_category_without_json_coercion() {
    for bits in [
        0_u64,
        1,
        0x8000000000000000,
        0x0010000000000000,
        0x7fefffffffffffff,
        0x7ff0000000000000,
        0xfff0000000000000,
        0x7ff8000000000001,
        0xfff8000000000011,
    ] {
        let literal = Lit::Float(f64::from_bits(bits));
        let encoded = serde_json::to_string(&literal).unwrap();
        let Lit::Float(decoded) = serde_json::from_str(&encoded).unwrap() else {
            panic!("float became another literal")
        };
        assert_eq!(decoded.to_bits(), bits);
    }
    for bad in [
        serde_json::json!({"Float":-1}),
        serde_json::json!({"Float":1.5}),
        serde_json::json!({"Float":null}),
        serde_json::json!({"Float":"NaN"}),
    ] {
        assert!(serde_json::from_value::<Lit>(bad).is_err());
    }
}

#[test]
fn scalar_literal_wire_preserves_arrow_domains_and_typed_nulls() {
    let mut values = vec![
        ScalarValue::Boolean(Some(true)),
        ScalarValue::Int8(Some(i8::MIN)),
        ScalarValue::UInt8(Some(u8::MAX)),
        ScalarValue::Int16(Some(i16::MIN)),
        ScalarValue::UInt16(Some(u16::MAX)),
        ScalarValue::Int32(Some(i32::MIN)),
        ScalarValue::UInt32(Some(u32::MAX)),
        ScalarValue::Int64(Some(i64::MIN)),
        ScalarValue::UInt64(Some(u64::MAX)),
        ScalarValue::Float32(Some(-0.0)),
        ScalarValue::Float64(Some(f64::INFINITY)),
        ScalarValue::Utf8(Some("雪\0text".into())),
        ScalarValue::LargeUtf8(Some("large".into())),
        ScalarValue::Utf8View(Some("view".into())),
        ScalarValue::Binary(Some(vec![0, 255, 1])),
        ScalarValue::LargeBinary(Some(vec![255, 0])),
        ScalarValue::FixedSizeBinary(2, Some(vec![0, 255])),
        ScalarValue::Date32(Some(-1)),
        ScalarValue::Date64(Some(-86400000)),
        ScalarValue::TimestampSecond(Some(-1), None),
        ScalarValue::TimestampMillisecond(Some(1234), Some("UTC".into())),
        ScalarValue::TimestampMicrosecond(Some(1234567), None),
        ScalarValue::TimestampNanosecond(Some(1234567890), Some("UTC".into())),
        ScalarValue::Time32Second(Some(1)),
        ScalarValue::Time32Millisecond(Some(1001)),
        ScalarValue::Time64Microsecond(Some(1000001)),
        ScalarValue::Time64Nanosecond(Some(1000000001)),
        ScalarValue::DurationSecond(Some(-1)),
        ScalarValue::DurationMillisecond(Some(-1)),
        ScalarValue::DurationMicrosecond(Some(-1)),
        ScalarValue::DurationNanosecond(Some(-1)),
        ScalarValue::IntervalYearMonth(Some(-12)),
        ScalarValue::Decimal128(Some(123456789012345678901234567890), 38, 9),
        ScalarValue::Decimal256(Some(i256::from_i128(i128::MAX)), 76, 12),
    ];
    let list = ScalarValue::List(ScalarValue::new_list(
        &[ScalarValue::Int64(Some(1)), ScalarValue::Int64(None)],
        &DataType::Int64,
        true,
    ));
    values.push(list.clone());
    values.push(ScalarValue::Struct(Arc::new(
        arrow::array::StructArray::new(
            vec![
                Field::new("nested", list.data_type(), true),
                Field::new("name", DataType::Utf8, true),
            ]
            .into(),
            vec![
                list.to_array().unwrap(),
                ScalarValue::Utf8(None).to_array().unwrap(),
            ],
            None,
        ),
    )));
    let mut null_types = values
        .iter()
        .map(ScalarValue::data_type)
        .collect::<Vec<_>>();
    null_types.extend([
        DataType::Null,
        DataType::Timestamp(TimeUnit::Nanosecond, None),
    ]);
    values.extend(
        null_types
            .iter()
            .map(|ty| ScalarValue::try_from(ty).unwrap()),
    );
    for value in values {
        let encoded = serde_json::to_value(Lit::Scalar(value.clone())).unwrap();
        let Lit::Scalar(decoded) = serde_json::from_value(encoded.clone()).unwrap() else {
            panic!("scalar became another literal")
        };
        assert_eq!(decoded.data_type(), value.data_type());
        assert_eq!(
            serde_json::to_value(Lit::Scalar(decoded)).unwrap(),
            encoded,
            "{value:?}"
        );
    }
}

#[test]
fn scalar_literal_wire_rejects_corruption_and_non_scalar_values() {
    for encoded in [
        "not base64!".to_string(),
        STANDARD.encode([255, 255]),
        encode(&Value::Int(7)),
        encode(&Value::List(vec![])),
    ] {
        assert!(serde_json::from_value::<Lit>(serde_json::json!({"Scalar":encoded})).is_err());
    }
    let error = serde_json::from_value::<Lit>(serde_json::json!({"Scalar":encode(&Value::Int(7))}))
        .unwrap_err();
    assert!(error.to_string().contains("Expected scalar literal"));
}
