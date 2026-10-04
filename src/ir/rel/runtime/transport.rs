//! Lossless Arrow transport for native traversers. Binding names and common
//! scalar/element values are columns, not serialized Row maps. The opaque
//! child is a compatibility escape for nested language values and Arrow scalars.
use super::{Result, Row, Value, failure};
use crate::ir::{
    catalog::snapshot::binary::{decode_value_bytes, encode_value},
    identity::ElementId,
};
use arrow::{array::*, buffer::OffsetBuffer, datatypes::*};
use datafusion::common::ScalarValue;
use std::sync::{Arc, OnceLock};

fn id_fields() -> Fields {
    static FIELDS: OnceLock<Fields> = OnceLock::new();
    FIELDS
        .get_or_init(|| {
            vec![
                Field::new("integer", DataType::Int64, true),
                Field::new("string", DataType::Utf8, true),
                Field::new("other", DataType::Binary, true),
            ]
            .into()
        })
        .clone()
}
fn node_fields() -> Fields {
    static FIELDS: OnceLock<Fields> = OnceLock::new();
    FIELDS
        .get_or_init(|| {
            vec![
                Field::new("label", DataType::Utf8, false),
                Field::new("id", DataType::Struct(id_fields()), false),
            ]
            .into()
        })
        .clone()
}
fn edge_fields() -> Fields {
    static FIELDS: OnceLock<Fields> = OnceLock::new();
    FIELDS
        .get_or_init(|| {
            vec![
                Field::new("type", DataType::Utf8, false),
                Field::new("id", DataType::Struct(id_fields()), false),
                Field::new("source", DataType::Struct(node_fields()), false),
                Field::new("target", DataType::Struct(node_fields()), false),
            ]
            .into()
        })
        .clone()
}
fn value_fields() -> UnionFields {
    static FIELDS: OnceLock<UnionFields> = OnceLock::new();
    FIELDS
        .get_or_init(|| {
            let types = [
                DataType::Null,
                DataType::Boolean,
                DataType::Int8,
                DataType::UInt8,
                DataType::Int16,
                DataType::UInt16,
                DataType::Int64,
                DataType::UInt32,
                DataType::Int64,
                DataType::UInt64,
                DataType::Float32,
                DataType::Float64,
                DataType::Utf8,
                DataType::Struct(node_fields()),
                DataType::Struct(edge_fields()),
                DataType::Binary,
            ];
            let names = [
                "null", "bool", "byte", "u8", "short", "u16", "int", "u32", "long", "u64", "f32",
                "float", "string", "node", "edge", "opaque",
            ];
            UnionFields::try_new(
                0..16,
                names
                    .into_iter()
                    .zip(types)
                    .map(|(name, ty)| Field::new(name, ty, true)),
            )
            .expect("fixed union types")
        })
        .clone()
}
fn entries_fields() -> Fields {
    static FIELDS: OnceLock<Fields> = OnceLock::new();
    FIELDS
        .get_or_init(|| {
            vec![
                Field::new("key", DataType::Utf8, false),
                Field::new(
                    "value",
                    DataType::Union(value_fields(), UnionMode::Dense),
                    true,
                ),
            ]
            .into()
        })
        .clone()
}
pub(super) fn schema() -> SchemaRef {
    static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
    SCHEMA
        .get_or_init(|| {
            Arc::new(Schema::new(vec![
                Field::new(
                    "bindings",
                    DataType::Map(
                        Arc::new(Field::new(
                            "entries",
                            DataType::Struct(entries_fields()),
                            false,
                        )),
                        false,
                    ),
                    false,
                ),
                Field::new("bulk", DataType::UInt64, false),
            ]))
        })
        .clone()
}
fn encode_ids(ids: &[&ElementId]) -> StructArray {
    let mut integers = Int64Builder::new();
    let mut strings = StringBuilder::new();
    let mut other = BinaryBuilder::new();
    for id in ids {
        match id.scalar() {
            ScalarValue::Int64(Some(value)) => {
                integers.append_value(*value);
                strings.append_null();
                other.append_null();
            }
            ScalarValue::Utf8(Some(value)) => {
                integers.append_null();
                strings.append_value(value);
                other.append_null();
            }
            _ => {
                integers.append_null();
                strings.append_null();
                other.append_value(id.encode());
            }
        }
    }
    StructArray::new(
        id_fields(),
        vec![
            Arc::new(integers.finish()),
            Arc::new(strings.finish()),
            Arc::new(other.finish()),
        ],
        None,
    )
}
fn encode_nodes(nodes: &[(&str, &ElementId)]) -> StructArray {
    StructArray::new(
        node_fields(),
        vec![
            Arc::new(StringArray::from_iter_values(
                nodes.iter().map(|(label, _)| *label),
            )),
            Arc::new(encode_ids(
                &nodes.iter().map(|(_, id)| *id).collect::<Vec<_>>(),
            )),
        ],
        None,
    )
}
fn tag(value: &Value) -> usize {
    match value {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Byte(_) => 2,
        Value::UInt8(_) => 3,
        Value::Short(_) => 4,
        Value::UInt16(_) => 5,
        Value::Int(_) => 6,
        Value::UInt32(_) => 7,
        Value::Long(_) => 8,
        Value::UInt64(_) => 9,
        Value::Float32(_) => 10,
        Value::Float(_) => 11,
        Value::String(_) => 12,
        Value::Node { .. } => 13,
        Value::Edge {
            projected_properties: None,
            ..
        } => 14,
        _ => 15,
    }
}
pub(super) fn encode_rows(rows: Vec<Row>) -> Result<RecordBatch> {
    let mut keys = StringBuilder::new();
    let mut offsets = vec![0_i32];
    let mut bulk = Vec::with_capacity(rows.len());
    let mut type_ids = Vec::new();
    let mut value_offsets = Vec::new();
    let mut groups: [Vec<Value>; 16] = Default::default();
    for row in rows {
        bulk.push(row.bulk);
        for (key, value) in row.bindings {
            let kind = tag(&value);
            keys.append_value(key);
            type_ids.push(kind as i8);
            value_offsets.push(i32::try_from(groups[kind].len()).map_err(failure)?);
            groups[kind].push(value);
        }
        offsets.push(i32::try_from(type_ids.len()).map_err(failure)?);
    }
    macro_rules! primitive {
        ($index:expr, $variant:ident, $array:ty) => {
            Arc::new(<$array>::from_iter_values(groups[$index].iter().map(
                |value| match value {
                    Value::$variant(value) => *value,
                    _ => unreachable!(),
                },
            ))) as ArrayRef
        };
    }
    let nodes = groups[13]
        .iter()
        .map(|v| match v {
            Value::Node { label, id } => (label.as_str(), id),
            _ => unreachable!(),
        })
        .collect::<Vec<_>>();
    let mut rels = Vec::new();
    let mut ids = Vec::new();
    let mut sources = Vec::new();
    let mut targets = Vec::new();
    for edge in &groups[14] {
        let Value::Edge {
            rel_type,
            id,
            src_label,
            src_id,
            dst_label,
            dst_id,
            ..
        } = edge
        else {
            unreachable!()
        };
        rels.push(rel_type.as_str());
        ids.push(id);
        sources.push((src_label.as_str(), src_id));
        targets.push((dst_label.as_str(), dst_id));
    }
    let edges = StructArray::new(
        edge_fields(),
        vec![
            Arc::new(StringArray::from(rels)),
            Arc::new(encode_ids(&ids)),
            Arc::new(encode_nodes(&sources)),
            Arc::new(encode_nodes(&targets)),
        ],
        None,
    );
    let mut opaque = BinaryBuilder::new();
    let mut buffer = Vec::new();
    for value in &groups[15] {
        buffer.clear();
        encode_value(&mut buffer, value);
        opaque.append_value(&buffer);
    }
    let children: Vec<ArrayRef> = vec![
        Arc::new(NullArray::new(groups[0].len())),
        Arc::new(BooleanArray::from_iter(groups[1].iter().map(|v| {
            if let Value::Bool(v) = v {
                Some(*v)
            } else {
                unreachable!()
            }
        }))),
        primitive!(2, Byte, Int8Array),
        primitive!(3, UInt8, UInt8Array),
        primitive!(4, Short, Int16Array),
        primitive!(5, UInt16, UInt16Array),
        primitive!(6, Int, Int64Array),
        primitive!(7, UInt32, UInt32Array),
        primitive!(8, Long, Int64Array),
        primitive!(9, UInt64, UInt64Array),
        primitive!(10, Float32, Float32Array),
        primitive!(11, Float, Float64Array),
        Arc::new(StringArray::from_iter_values(groups[12].iter().map(|v| {
            if let Value::String(v) = v {
                v.as_str()
            } else {
                unreachable!()
            }
        }))),
        Arc::new(encode_nodes(&nodes)),
        Arc::new(edges),
        Arc::new(opaque.finish()),
    ];
    let values = UnionArray::try_new(
        value_fields(),
        type_ids.into(),
        Some(value_offsets.into()),
        children,
    )?;
    let entries = StructArray::new(
        entries_fields(),
        vec![Arc::new(keys.finish()), Arc::new(values)],
        None,
    );
    let bindings = MapArray::try_new(
        Arc::new(Field::new(
            "entries",
            DataType::Struct(entries_fields()),
            false,
        )),
        OffsetBuffer::new(offsets.into()),
        entries,
        None,
        false,
    )?;
    Ok(RecordBatch::try_new(
        schema(),
        vec![Arc::new(bindings), Arc::new(UInt64Array::from(bulk))],
    )?)
}
fn array<T: Array + 'static>(value: &dyn Array) -> Result<&T> {
    value
        .as_any()
        .downcast_ref()
        .ok_or_else(|| failure("Invalid native Arrow transport array"))
}
fn decode_id(ids: &StructArray, index: usize) -> Result<ElementId> {
    let integers = array::<Int64Array>(ids.column(0).as_ref())?;
    let strings = array::<StringArray>(ids.column(1).as_ref())?;
    let other = array::<BinaryArray>(ids.column(2).as_ref())?;
    if !integers.is_null(index) {
        Ok(integers.value(index).into())
    } else if !strings.is_null(index) {
        ElementId::new(ScalarValue::Utf8(Some(strings.value(index).into()))).map_err(failure)
    } else if !other.is_null(index) {
        ElementId::decode(other.value(index)).map_err(failure)
    } else {
        Err(failure("Null element identity in native transport"))
    }
}
fn decode_node(nodes: &StructArray, index: usize) -> Result<(String, ElementId)> {
    Ok((
        array::<StringArray>(nodes.column(0).as_ref())?
            .value(index)
            .into(),
        decode_id(array::<StructArray>(nodes.column(1).as_ref())?, index)?,
    ))
}
fn decode_value(values: &UnionArray, index: usize) -> Result<Value> {
    let kind = values.type_id(index);
    let offset = values.value_offset(index);
    let child = values.child(kind).as_ref();
    macro_rules! primitive {
        ($variant:ident, $array:ty) => {
            Value::$variant(array::<$array>(child)?.value(offset))
        };
    }
    Ok(match kind {
        0 => Value::Null,
        1 => primitive!(Bool, BooleanArray),
        2 => primitive!(Byte, Int8Array),
        3 => primitive!(UInt8, UInt8Array),
        4 => primitive!(Short, Int16Array),
        5 => primitive!(UInt16, UInt16Array),
        6 => primitive!(Int, Int64Array),
        7 => primitive!(UInt32, UInt32Array),
        8 => primitive!(Long, Int64Array),
        9 => primitive!(UInt64, UInt64Array),
        10 => primitive!(Float32, Float32Array),
        11 => primitive!(Float, Float64Array),
        12 => Value::String(array::<StringArray>(child)?.value(offset).into()),
        13 => {
            let (label, id) = decode_node(array::<StructArray>(child)?, offset)?;
            Value::Node { label, id }
        }
        14 => {
            let edges = array::<StructArray>(child)?;
            let (src_label, src_id) =
                decode_node(array::<StructArray>(edges.column(2).as_ref())?, offset)?;
            let (dst_label, dst_id) =
                decode_node(array::<StructArray>(edges.column(3).as_ref())?, offset)?;
            Value::Edge {
                rel_type: array::<StringArray>(edges.column(0).as_ref())?
                    .value(offset)
                    .into(),
                id: decode_id(array::<StructArray>(edges.column(1).as_ref())?, offset)?,
                src_label,
                src_id,
                dst_label,
                dst_id,
                projected_properties: None,
            }
        }
        15 => decode_value_bytes(array::<BinaryArray>(child)?.value(offset)).map_err(failure)?,
        _ => return Err(failure("Unknown native value type")),
    })
}
pub(super) fn decode_rows(batch: &RecordBatch) -> Result<Vec<Row>> {
    if batch.schema().as_ref() != schema().as_ref() {
        return Err(failure("Invalid traverser batch schema"));
    }
    let bindings = array::<MapArray>(batch.column(0).as_ref())?;
    let keys = array::<StringArray>(bindings.keys().as_ref())?;
    let values = array::<UnionArray>(bindings.values().as_ref())?;
    let bulk = array::<UInt64Array>(batch.column(1).as_ref())?;
    (0..batch.num_rows())
        .map(|i| {
            if bindings.is_null(i) || bulk.is_null(i) {
                return Err(failure("Null traverser transport field"));
            }
            let start = bindings.value_offsets()[i] as usize;
            let end = bindings.value_offsets()[i + 1] as usize;
            let bindings = (start..end)
                .map(|index| Ok((keys.value(index).to_string(), decode_value(values, index)?)))
                .collect::<Result<_>>()?;
            Ok(Row {
                bindings,
                bulk: bulk.value(i),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
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
}
