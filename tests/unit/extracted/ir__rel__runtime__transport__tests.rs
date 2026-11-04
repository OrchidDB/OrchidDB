
    use super::*;
    fn bytes(rows: Vec<Row>) -> Vec<(u64, Vec<u8>)> {
        rows.into_iter()
            .map(|row| {
                let mut bytes = Vec::new();
                encode_value(&mut bytes, &Value::Map(row.bindings));
                (row.bulk, bytes)
            })
            .collect()
    }
    #[test]
    fn typed_transport_preserves_values_bulk_absence_and_arrow_slices() {
        let tuple = ElementId::from_components(vec![
            ScalarValue::Int64(Some(9)),
            ScalarValue::Utf8(Some("key".into())),
        ])
        .unwrap();
        let values = vec![
            Value::Null,
            Value::Bool(true),
            Value::Byte(-8),
            Value::UInt8(255),
            Value::Short(-42),
            Value::UInt16(65535),
            Value::Int(i64::MIN),
            Value::UInt32(u32::MAX),
            Value::Long(i64::MAX),
            Value::UInt64(u64::MAX),
            Value::Float32(-0.0),
            Value::Float(f64::from_bits(0x7ff8000000000001)),
            Value::String("hello 🪷".into()),
            Value::Node {
                label: "N".into(),
                id: 42.into(),
            },
            Value::Node {
                label: "N".into(),
                id: ElementId::new(ScalarValue::Utf8(Some("42".into()))).unwrap(),
            },
            Value::Node {
                label: "N".into(),
                id: tuple.clone(),
            },
            Value::Edge {
                rel_type: "R".into(),
                id: tuple,
                src_label: "N".into(),
                src_id: 42.into(),
                dst_label: "N".into(),
                dst_id: 43.into(),
                projected_properties: None,
            },
            Value::List(vec![
                Value::Long(9),
                Value::Map([("nested".into(), Value::Null)].into()),
            ]),
            Value::Path(vec![Value::String("x".into())]),
            Value::BigDecimal("1.00".parse().unwrap()),
            Value::Scalar(ScalarValue::Decimal128(Some(1200), 10, 2)),
        ];
        let mut rows = values
            .into_iter()
            .enumerate()
            .map(|(i, v)| Row {
                bindings: [(format!("v{}", i % 3), v)].into(),
                bulk: if i == 0 { u64::MAX } else { i as u64 },
            })
            .collect::<Vec<_>>();
        rows.push(Row::new());
        rows.push(Row {
            bindings: Default::default(),
            bulk: 0,
        });
        let encoded = encode_rows(rows.clone()).unwrap();
        assert!(matches!(encoded.column(0).data_type(), DataType::Map(..)));
        assert_eq!(bytes(decode_rows(&encoded).unwrap()), bytes(rows.clone()));
        assert_eq!(
            bytes(decode_rows(&encoded.slice(3, 8)).unwrap()),
            bytes(rows[3..11].to_vec())
        );
        let pieces = [
            encoded.slice(0, 8),
            encoded.slice(8, encoded.num_rows() - 8),
        ];
        let joined = arrow::compute::concat_batches(&schema(), &pieces).unwrap();
        assert_eq!(bytes(decode_rows(&joined).unwrap()), bytes(rows));
        assert!(
            decode_rows(&encode_rows(vec![]).unwrap())
                .unwrap()
                .is_empty()
        );
        assert!(
            decode_rows(&RecordBatch::new_empty(schema()))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn transport_is_valid_arrow_ipc_and_common_values_need_no_opaque_payload() {
        let batch = encode_rows(vec![Row::new().with("x", Value::Int(7)).with(
            "n",
            Value::Node {
                label: "N".into(),
                id: 12.into(),
            },
        )])
        .unwrap();
        let maps = array::<MapArray>(batch.column(0).as_ref()).unwrap();
        let values = array::<UnionArray>(maps.values().as_ref()).unwrap();
        assert_eq!(values.child(15).len(), 0);
        let mut buffer = Vec::new();
        {
            let mut writer =
                arrow::ipc::writer::StreamWriter::try_new(&mut buffer, &schema()).unwrap();
            writer.write(&batch).unwrap();
            writer.finish().unwrap();
        }
        let decoded = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(buffer), None)
            .unwrap()
            .next()
            .unwrap()
            .unwrap();
        assert_eq!(
            bytes(decode_rows(&decoded).unwrap()),
            bytes(decode_rows(&batch).unwrap())
        );
    }
    #[test]
    fn transport_accepts_host_sparse_unions_and_nullable_schema_fields() {
        let rows = vec![
            Row::new().with("x", Value::Int(7)).with("n", Value::Null),
            Row {
                bindings: [("x".into(), Value::String("seven".into()))].into(),
                bulk: 4,
            },
        ];
        let emitted = encode_host_rows(rows.clone()).unwrap();
        assert_eq!(emitted.schema(), host_schema());
        assert_eq!(bytes(decode_rows(&emitted).unwrap()), bytes(rows.clone()));
        let batch = encode_rows(rows.clone()).unwrap();
        let bindings = array::<MapArray>(batch.column(0).as_ref()).unwrap();
        let dense = array::<UnionArray>(bindings.values().as_ref()).unwrap();
        let mut children = value_fields()
            .iter()
            .map(|(tag, _)| {
                let indices = UInt32Array::from(
                    (0..dense.len())
                        .map(|i| (dense.type_id(i) == tag).then(|| dense.value_offset(i) as u32))
                        .collect::<Vec<_>>(),
                );
                arrow::compute::take(dense.child(tag).as_ref(), &indices, None).unwrap()
            })
            .collect::<Vec<_>>();
        children[0] = Arc::new(Int32Array::from(vec![None; dense.len()]));
        let fields = UnionFields::try_new(
            0..16,
            value_fields().iter().map(|(tag, field)| {
                if tag == 0 {
                    field.as_ref().clone().with_data_type(DataType::Int32)
                } else {
                    field.as_ref().clone()
                }
            }),
        )
        .unwrap();
        let sparse = UnionArray::try_new(fields, dense.type_ids().clone(), None, children).unwrap();
        let entries = StructArray::new(
            vec![
                Arc::new(Field::new("key", DataType::Utf8, false)),
                Arc::new(Field::new("value", sparse.data_type().clone(), true)),
            ]
            .into(),
            vec![bindings.keys().clone(), Arc::new(sparse)],
            None,
        );
        let map = MapArray::new(
            Arc::new(Field::new("entries", entries.data_type().clone(), false)),
            bindings.offsets().clone(),
            entries,
            None,
            false,
        );
        let schema = Arc::new(Schema::new(vec![
            Field::new("bindings", map.data_type().clone(), true),
            Field::new("bulk", DataType::UInt64, true),
        ]));
        let host =
            RecordBatch::try_new(schema, vec![Arc::new(map), batch.column(1).clone()]).unwrap();
        assert_eq!(bytes(decode_rows(&host).unwrap()), bytes(rows));
    }
