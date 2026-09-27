//! Typed source keys used as element identities, never surrogate row numbers.
use datafusion::common::ScalarValue;
use std::{cmp::Ordering, fmt, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ElementId(Arc<ScalarValue>);

impl ElementId {
    /// A key is either one scalar or an ordered tuple of non-null scalars.
    /// Struct field names are positional and canonical, independent of the
    /// physical column names used by a particular mapping.
    pub fn new(value: ScalarValue) -> Result<Self, String> {
        use arrow::array::Array;
        if let ScalarValue::Struct(array) = &value {
            if array.len() != 1 || array.is_null(0) {
                return Err("element identity cannot be null".into());
            }
            if is_identity_union(&value.data_type()) {
                let values = array
                    .columns()
                    .iter()
                    .map(|column| ScalarValue::try_from_array(column, 0).map_err(|e| e.to_string()))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut live = values.into_iter().filter(|v| !v.is_null());
                let key = live.next().ok_or("null element identity")?;
                if live.next().is_some() {
                    return Err("ambiguous identity union".into());
                }
                return Self::new(key);
            }
            return Self::from_components(
                array
                    .columns()
                    .iter()
                    .map(|column| ScalarValue::try_from_array(column, 0).map_err(|e| e.to_string()))
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        if value.is_null() || !super::rel::mapping::is_scalar_identity_type(&value.data_type()) {
            return Err(format!(
                "element identity requires non-null scalar components, got {}",
                value.data_type()
            ));
        }
        Ok(Self(Arc::new(value)))
    }
    pub fn from_components(values: Vec<ScalarValue>) -> Result<Self, String> {
        use arrow::{array::StructArray, datatypes::Field};
        if values.is_empty() {
            return Err("element identity cannot be empty".into());
        }
        if values.len() == 1 {
            return Self::new(values.into_iter().next().unwrap());
        }
        if values
            .iter()
            .any(|v| v.is_null() || !super::rel::mapping::is_scalar_identity_type(&v.data_type()))
        {
            return Err("composite identity components must be non-null scalars".into());
        }
        let fields = values
            .iter()
            .enumerate()
            .map(|(i, v)| Arc::new(Field::new(format!("k{i}"), v.data_type(), true)))
            .collect::<Vec<_>>();
        let arrays = values
            .iter()
            .map(|v| v.to_array().map_err(|e| e.to_string()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self(Arc::new(ScalarValue::Struct(Arc::new(
            StructArray::new(fields.into(), arrays, None),
        )))))
    }
    pub fn components(&self) -> Vec<ScalarValue> {
        match self.scalar() {
            ScalarValue::Struct(array) => array
                .columns()
                .iter()
                .map(|column| {
                    ScalarValue::try_from_array(column, 0).expect("validated key component")
                })
                .collect(),
            value => vec![value.clone()],
        }
    }
    /// Cast each component independently. Never flatten a tuple into text.
    pub fn cast_to(&self, kind: &arrow::datatypes::DataType) -> Result<Self, String> {
        use arrow::datatypes::DataType;
        let values = self.components();
        let types = match kind {
            DataType::Struct(fields) if !is_identity_union(kind) => {
                fields.iter().map(|f| f.data_type()).collect()
            }
            _ => vec![kind],
        };
        if values.len() != types.len() {
            return Err("element identity arity does not match its mapping".into());
        }
        Self::from_components(
            values
                .into_iter()
                .zip(types)
                .map(|(v, t)| v.cast_to(t).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?,
        )
    }
    pub fn scalar(&self) -> &ScalarValue {
        &self.0
    }
    pub fn as_i64(&self) -> Option<i64> {
        match self.scalar() {
            ScalarValue::Int64(Some(id)) => Some(*id),
            _ => None,
        }
    }
}
impl From<i64> for ElementId {
    fn from(id: i64) -> Self {
        Self(Arc::new(ScalarValue::Int64(Some(id))))
    }
}
impl From<&ElementId> for ElementId {
    fn from(id: &ElementId) -> Self {
        id.clone()
    }
}
impl fmt::Display for ElementId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.scalar().fmt(f)
    }
}
impl PartialOrd for ElementId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ElementId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.scalar()
            .data_type()
            .cmp(&other.scalar().data_type())
            .then_with(|| {
                self.scalar()
                    .partial_cmp(other.scalar())
                    .expect("same scalar key type")
            })
    }
}

impl ElementId {
    /// Lossless Arrow encoding for transport and typed collection membership.
    pub fn encode(&self) -> Vec<u8> {
        if let Some(id) = self.as_i64() {
            let mut bytes = vec![0];
            bytes.extend_from_slice(&id.to_le_bytes());
            return bytes;
        }
        let mut bytes = vec![1];
        bytes.extend_from_slice(&encode_scalar(self.scalar()));
        bytes
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.first() == Some(&0) && bytes.len() == 9 {
            return Ok(i64::from_le_bytes(bytes[1..].try_into().unwrap()).into());
        }
        let bytes = bytes.strip_prefix(&[1]).ok_or("invalid key encoding")?;
        Self::new(decode_scalar(bytes)?)
    }
    pub fn to_value(&self) -> super::Value {
        use super::Value;
        match self.scalar() {
            ScalarValue::Struct(_) => Value::List(
                self.components()
                    .into_iter()
                    .map(|v| Self::new(v).expect("validated component").to_value())
                    .collect(),
            ),
            ScalarValue::Boolean(Some(v)) => Value::Bool(*v),
            ScalarValue::Int8(Some(v)) => Value::Byte(*v),
            ScalarValue::Int16(Some(v)) => Value::Short(*v),
            ScalarValue::Int32(Some(v)) => Value::Int(*v as i64),
            ScalarValue::Int64(Some(v)) => Value::Int(*v),
            ScalarValue::Utf8(Some(v))
            | ScalarValue::LargeUtf8(Some(v))
            | ScalarValue::Utf8View(Some(v)) => Value::String(v.clone()),
            ScalarValue::Float32(Some(v)) => Value::Float32(*v),
            ScalarValue::Float64(Some(v)) => Value::Float(*v),
            _ => Value::Scalar(self.scalar().clone()),
        }
    }
}

pub(crate) fn decode_scalar(bytes: &[u8]) -> Result<ScalarValue, String> {
    use arrow::ipc::reader::StreamReader;
    let mut reader =
        StreamReader::try_new(std::io::Cursor::new(bytes), None).map_err(|e| e.to_string())?;
    let batch = reader
        .next()
        .ok_or("missing scalar key")?
        .map_err(|e| e.to_string())?;
    if batch.num_columns() != 1 || batch.num_rows() != 1 || reader.next().is_some() {
        return Err("invalid scalar key batch".into());
    }
    ScalarValue::try_from_array(batch.column(0), 0).map_err(|e| e.to_string())
}

impl TryFrom<&super::Value> for ElementId {
    type Error = String;
    fn try_from(value: &super::Value) -> Result<Self, String> {
        use super::Value;
        if let Value::List(parts) = value {
            return Self::from_components(
                parts
                    .iter()
                    .map(|part| {
                        if matches!(part, Value::List(_)) {
                            return Err("nested composite identities are not supported".into());
                        }
                        Self::try_from(part).and_then(|key| {
                            if key.components().len() != 1 {
                                return Err("nested composite identities are not supported".into());
                            }
                            Ok(key.scalar().clone())
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        Self::new(match value {
            Value::Scalar(v) => v.clone(),
            Value::BigInt(v) | Value::UInt128(v) => ScalarValue::Utf8(Some(v.to_string())),
            Value::BigDecimal(v) => ScalarValue::Utf8(Some(v.to_string())),
            Value::Int(v) | Value::Long(v) => ScalarValue::Int64(Some(*v)),
            Value::Byte(v) => ScalarValue::Int8(Some(*v)),
            Value::Short(v) => ScalarValue::Int16(Some(*v)),
            Value::UInt8(v) => ScalarValue::UInt8(Some(*v)),
            Value::UInt16(v) => ScalarValue::UInt16(Some(*v)),
            Value::UInt32(v) => ScalarValue::UInt32(Some(*v)),
            Value::UInt64(v) => ScalarValue::UInt64(Some(*v)),
            Value::Float32(v) => ScalarValue::Float32(Some(*v)),
            Value::Float(v) => ScalarValue::Float64(Some(*v)),
            Value::Bool(v) => ScalarValue::Boolean(Some(*v)),
            Value::String(v) => ScalarValue::Utf8(Some(v.clone())),
            _ => return Err(format!("unsupported element key {}", value.type_name())),
        })
    }
}
impl Default for ElementId {
    fn default() -> Self {
        0_i64.into()
    }
}
impl serde::Serialize for ElementId {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if let Some(id) = self.as_i64() {
            return s.serialize_i64(id);
        }
        use serde::ser::SerializeStruct;
        let mut state = s.serialize_struct("ElementId", 1)?;
        state.serialize_field("scalar_key", &self.encode())?;
        state.end()
    }
}

pub(crate) fn encode_scalar(value: &ScalarValue) -> Vec<u8> {
    use arrow::{
        array::RecordBatch,
        datatypes::{Field, Schema},
        ipc::writer::StreamWriter,
    };
    let array = value.to_array().expect("validated scalar");
    let schema = Arc::new(Schema::new(vec![Field::new(
        "key",
        array.data_type().clone(),
        true,
    )]));
    let batch = RecordBatch::try_new(schema.clone(), vec![array]).expect("scalar batch");
    let mut bytes = Vec::new();
    let mut writer = StreamWriter::try_new(&mut bytes, &schema).expect("scalar schema");
    writer.write(&batch).expect("scalar IPC");
    writer.finish().expect("scalar IPC");
    bytes
}

#[cfg(feature = "duckdb")]
impl duckdb::ToSql for ElementId {
    fn to_sql(&self) -> duckdb::Result<duckdb::types::ToSqlOutput<'_>> {
        Ok(duckdb::types::ToSqlOutput::Owned(
            duckdb::types::Value::Blob(self.encode()),
        ))
    }
}
#[cfg(feature = "duckdb")]
impl duckdb::types::FromSql for ElementId {
    fn column_result(value: duckdb::types::ValueRef<'_>) -> duckdb::types::FromSqlResult<Self> {
        match value {
            duckdb::types::ValueRef::BigInt(id) => Ok(id.into()),
            duckdb::types::ValueRef::Blob(bytes) => {
                Self::decode(bytes).map_err(|_| duckdb::types::FromSqlError::InvalidType)
            }
            _ => Err(duckdb::types::FromSqlError::InvalidType),
        }
    }
}

impl PartialEq<i64> for ElementId {
    fn eq(&self, other: &i64) -> bool {
        self.as_i64() == Some(*other)
    }
}

/// Heterogeneous graph scans carry a typed, sparse union of identities. Each
/// variant retains its original Arrow type; only the type name is fingerprinted.
pub(crate) fn is_identity_union(kind: &arrow::datatypes::DataType) -> bool {
    matches!(kind, arrow::datatypes::DataType::Struct(fields) if !fields.is_empty() &&
        fields.iter().all(|f| f.name().starts_with("__orchid_key_")))
}
pub(crate) fn identity_variant(kind: &arrow::datatypes::DataType) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "__orchid_key_{:x}",
        Sha256::digest(format!("{kind:?}").as_bytes())
    )
}
pub(crate) fn identity_union_type(
    types: impl IntoIterator<Item = arrow::datatypes::DataType>,
) -> arrow::datatypes::DataType {
    use arrow::datatypes::{DataType, Field};
    let mut fields = std::collections::BTreeMap::new();
    for kind in types {
        if is_identity_union(&kind) {
            let DataType::Struct(existing) = kind else {
                unreachable!()
            };
            for field in &existing {
                fields.insert(field.name().clone(), field.clone());
            }
        } else {
            let name = identity_variant(&kind);
            fields.insert(name.clone(), Arc::new(Field::new(name, kind, true)));
        }
    }
    DataType::Struct(fields.into_values().collect::<Vec<_>>().into())
}
