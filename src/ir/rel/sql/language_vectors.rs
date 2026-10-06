//! Read-only DuckDB vector transport for the language UDFs. duckdb-rs's
//! Arrow reader currently leaves lists, structs and decimals unimplemented.
use arrow::{
    array::ArrayRef,
    datatypes::{DataType, Field},
};
use datafusion::common::ScalarValue;
use duckdb::{core::DataChunkHandle, ffi};
use std::{ffi::CStr, sync::Arc};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
struct Type(ffi::duckdb_logical_type);
impl Drop for Type {
    fn drop(&mut self) {
        unsafe { ffi::duckdb_destroy_logical_type(&mut self.0) }
    }
}

// All pointers are borrowed from the callback's input chunk. Child lengths and
// offsets come from DuckDB; no pointer or borrowed buffer escapes this module.
unsafe fn data_type(t: ffi::duckdb_logical_type) -> Result<DataType> {
    use duckdb::core::LogicalTypeId as L;
    Ok(match L::from(unsafe { ffi::duckdb_get_type_id(t) }) {
        L::SqlNull => DataType::Null,
        L::Boolean => DataType::Boolean,
        L::Tinyint => DataType::Int8,
        L::Smallint => DataType::Int16,
        L::Integer => DataType::Int32,
        L::Bigint => DataType::Int64,
        L::UTinyint => DataType::UInt8,
        L::USmallint => DataType::UInt16,
        L::UInteger => DataType::UInt32,
        L::UBigint => DataType::UInt64,
        L::Float => DataType::Float32,
        L::Double => DataType::Float64,
        L::Varchar => DataType::Utf8,
        L::Decimal => DataType::Decimal128(unsafe { ffi::duckdb_decimal_width(t) }, unsafe {
            ffi::duckdb_decimal_scale(t)
        }
            as i8),
        L::List => {
            let child = Type(unsafe { ffi::duckdb_list_type_child_type(t) });
            DataType::List(Arc::new(Field::new(
                "item",
                unsafe { data_type(child.0)? },
                true,
            )))
        }
        L::Struct => {
            let mut fields = Vec::new();
            for i in 0..unsafe { ffi::duckdb_struct_type_child_count(t) } {
                let name = unsafe { ffi::duckdb_struct_type_child_name(t, i) };
                let text = unsafe { CStr::from_ptr(name) }
                    .to_string_lossy()
                    .into_owned();
                unsafe { ffi::duckdb_free(name.cast()) };
                let child = Type(unsafe { ffi::duckdb_struct_type_child_type(t, i) });
                fields.push(Field::new(text, unsafe { data_type(child.0)? }, true));
            }
            DataType::Struct(fields.into())
        }
        other => return Err(format!("unsupported language vector type {other:?}").into()),
    })
}
unsafe fn cell(v: ffi::duckdb_vector, t: &DataType, row: usize) -> Result<ScalarValue> {
    let valid = unsafe { ffi::duckdb_vector_get_validity(v) };
    if *t == DataType::Null
        || !valid.is_null() && !unsafe { ffi::duckdb_validity_row_is_valid(valid, row as u64) }
    {
        return Ok(ScalarValue::try_from(t)?);
    }
    let ptr = unsafe { ffi::duckdb_vector_get_data(v) };
    macro_rules! get {
        ($ty:ty) => {
            unsafe { *ptr.cast::<$ty>().add(row) }
        };
    }
    Ok(match t {
        DataType::Boolean => ScalarValue::Boolean(Some(get!(bool))),
        DataType::Int8 => ScalarValue::Int8(Some(get!(i8))),
        DataType::Int16 => ScalarValue::Int16(Some(get!(i16))),
        DataType::Int32 => ScalarValue::Int32(Some(get!(i32))),
        DataType::Int64 => ScalarValue::Int64(Some(get!(i64))),
        DataType::UInt8 => ScalarValue::UInt8(Some(get!(u8))),
        DataType::UInt16 => ScalarValue::UInt16(Some(get!(u16))),
        DataType::UInt32 => ScalarValue::UInt32(Some(get!(u32))),
        DataType::UInt64 => ScalarValue::UInt64(Some(get!(u64))),
        DataType::Float32 => ScalarValue::Float32(Some(get!(f32))),
        DataType::Float64 => ScalarValue::Float64(Some(get!(f64))),
        DataType::Utf8 => {
            let mut s = get!(ffi::duckdb_string_t);
            let len = unsafe { ffi::duckdb_string_t_length(s) } as usize;
            let bytes = unsafe {
                std::slice::from_raw_parts(ffi::duckdb_string_t_data(&mut s).cast::<u8>(), len)
            };
            ScalarValue::Utf8(Some(std::str::from_utf8(bytes)?.into()))
        }
        DataType::Decimal128(width, scale) => {
            let n = match width {
                0..=4 => get!(i16) as i128,
                5..=9 => get!(i32) as i128,
                10..=18 => get!(i64) as i128,
                _ => {
                    let n = get!(ffi::duckdb_hugeint);
                    ((n.upper as i128) << 64) | n.lower as i128
                }
            };
            ScalarValue::Decimal128(Some(n), *width, *scale)
        }
        DataType::List(field) => {
            let entry = get!(ffi::duckdb_list_entry);
            let size = unsafe { ffi::duckdb_list_vector_get_size(v) };
            if entry
                .offset
                .checked_add(entry.length)
                .is_none_or(|end| end > size)
            {
                return Err("invalid DuckDB list bounds".into());
            }
            let child = unsafe { ffi::duckdb_list_vector_get_child(v) };
            let values = (entry.offset..entry.offset + entry.length)
                .map(|i| unsafe { cell(child, field.data_type(), i as usize) })
                .collect::<Result<Vec<_>>>()?;
            ScalarValue::List(ScalarValue::new_list(&values, field.data_type(), true))
        }
        DataType::Struct(fields) => {
            let columns = fields
                .iter()
                .enumerate()
                .map(|(i, f)| unsafe {
                    cell(
                        ffi::duckdb_struct_vector_get_child(v, i as u64),
                        f.data_type(),
                        row,
                    )?
                    .to_array()
                    .map_err(Into::into)
                })
                .collect::<Result<Vec<_>>>()?;
            ScalarValue::Struct(Arc::new(arrow::array::StructArray::try_new(
                fields.clone(),
                columns,
                None,
            )?))
        }
        _ => return Err(format!("unsupported language vector {t}").into()),
    })
}
// Build struct children by column, avoiding one StructArray allocation per row.
unsafe fn array(v: ffi::duckdb_vector, ty: &DataType, len: usize) -> Result<ArrayRef> {
    if len == 0 {
        return Ok(arrow::array::new_empty_array(ty));
    }
    if let DataType::Struct(fields) = ty {
        let columns = fields
            .iter()
            .enumerate()
            .map(|(i, f)| unsafe {
                array(
                    ffi::duckdb_struct_vector_get_child(v, i as u64),
                    f.data_type(),
                    len,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let validity = unsafe { ffi::duckdb_vector_get_validity(v) };
        let nulls = if validity.is_null() {
            None
        } else {
            Some(arrow::buffer::NullBuffer::from(
                (0..len)
                    .map(|r| unsafe { ffi::duckdb_validity_row_is_valid(validity, r as u64) })
                    .collect::<Vec<_>>(),
            ))
        };
        return Ok(Arc::new(arrow::array::StructArray::try_new(
            fields.clone(),
            columns,
            nulls,
        )?));
    }
    let cells = (0..len)
        .map(|r| unsafe { cell(v, ty, r) })
        .collect::<Result<Vec<_>>>()?;
    Ok(ScalarValue::iter_to_array(cells)?)
}
pub(super) fn arrays(input: &DataChunkHandle) -> Result<Vec<ArrayRef>> {
    (0..input.num_columns())
        .map(|i| unsafe {
            let v = ffi::duckdb_data_chunk_get_vector(input.get_ptr(), i as u64);
            let t = Type(ffi::duckdb_vector_get_column_type(v));
            let ty = data_type(t.0)?;
            if matches!(
                ty,
                DataType::Boolean
                    | DataType::Int8
                    | DataType::Int16
                    | DataType::Int32
                    | DataType::Int64
                    | DataType::UInt8
                    | DataType::UInt16
                    | DataType::UInt32
                    | DataType::UInt64
                    | DataType::Float32
                    | DataType::Float64
                    | DataType::Utf8
            ) {
                return duckdb::vtab::arrow::flat_vector_to_arrow_array(
                    &mut input.flat_vector(i),
                    input.len(),
                );
            }
            array(v, &ty, input.len())
        })
        .collect()
}
