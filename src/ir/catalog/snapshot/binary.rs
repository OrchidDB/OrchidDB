//! Primitive, Value and Arrow IPC codecs for the snapshot wire format.

use super::*;

// Value tags.
const V_NULL: u8 = 0;
const V_BOOL: u8 = 1;
const V_BYTE: u8 = 2;
const V_UINT8: u8 = 3;
const V_SHORT: u8 = 4;
const V_UINT16: u8 = 5;
const V_INT: u8 = 6;
const V_UINT32: u8 = 7;
const V_LONG: u8 = 8;
const V_UINT64: u8 = 9;
const V_FLOAT32: u8 = 10;
const V_FLOAT: u8 = 11;
const V_BIGINT: u8 = 12;
const V_UINT128: u8 = 13;
const V_BIGDECIMAL: u8 = 14;
const V_DATETIME: u8 = 15;
const V_INTERNAL_ID: u8 = 16;
const V_STRING: u8 = 17;
const V_NODE: u8 = 18;
const V_EDGE: u8 = 19;
const V_LIST: u8 = 20;
const V_MAP: u8 = 21;
const V_PATH: u8 = 22;

// ---------------- primitive writers ----------------

fn put_u8(out: &mut Vec<u8>, v: u8) {
    out.push(v);
}

fn put_u16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_i16(out: &mut Vec<u8>, v: i16) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(super) fn put_u32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(super) fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(super) fn put_i64(out: &mut Vec<u8>, v: i64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f64(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(&v.to_le_bytes());
}

pub(super) fn put_str(out: &mut Vec<u8>, s: &str) {
    put_u64(out, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

pub(super) fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    put_u64(out, b.len() as u64);
    out.extend_from_slice(b);
}

pub(super) fn put_str_list(out: &mut Vec<u8>, list: &[String]) {
    put_u64(out, list.len() as u64);
    for s in list {
        put_str(out, s);
    }
}

pub(super) fn put_map(out: &mut Vec<u8>, map: &BTreeMap<String, Value>) {
    put_u64(out, map.len() as u64);
    for (key, value) in map {
        put_str(out, key);
        encode_value(out, value);
    }
}

fn put_values(out: &mut Vec<u8>, items: &[Value]) {
    put_u64(out, items.len() as u64);
    for item in items {
        encode_value(out, item);
    }
}

pub(super) fn write_section(out: &mut Vec<u8>, tag: u8, payload: &[u8]) {
    put_u8(out, tag);
    put_u64(out, payload.len() as u64);
    out.extend_from_slice(payload);
}

// ---------------- Value codec ----------------

fn encode_value(out: &mut Vec<u8>, value: &Value) {
    match value {
        Value::Null => put_u8(out, V_NULL),
        Value::Bool(b) => {
            put_u8(out, V_BOOL);
            put_u8(out, *b as u8);
        }
        Value::Byte(v) => {
            put_u8(out, V_BYTE);
            put_u8(out, *v as u8);
        }
        Value::UInt8(v) => {
            put_u8(out, V_UINT8);
            put_u8(out, *v);
        }
        Value::Short(v) => {
            put_u8(out, V_SHORT);
            put_i16(out, *v);
        }
        Value::UInt16(v) => {
            put_u8(out, V_UINT16);
            put_u16(out, *v);
        }
        Value::Int(v) => {
            put_u8(out, V_INT);
            put_i64(out, *v);
        }
        Value::UInt32(v) => {
            put_u8(out, V_UINT32);
            put_u32(out, *v);
        }
        Value::Long(v) => {
            put_u8(out, V_LONG);
            put_i64(out, *v);
        }
        Value::UInt64(v) => {
            put_u8(out, V_UINT64);
            put_u64(out, *v);
        }
        Value::Float32(v) => {
            put_u8(out, V_FLOAT32);
            put_f32(out, *v);
        }
        Value::Float(v) => {
            put_u8(out, V_FLOAT);
            put_f64(out, *v);
        }
        Value::BigInt(v) => {
            put_u8(out, V_BIGINT);
            put_str(out, &v.to_string());
        }
        Value::UInt128(v) => {
            put_u8(out, V_UINT128);
            put_str(out, &v.to_string());
        }
        Value::BigDecimal(v) => {
            put_u8(out, V_BIGDECIMAL);
            put_str(out, &v.to_string());
        }
        Value::DateTime(v) => {
            put_u8(out, V_DATETIME);
            put_str(out, v);
        }
        Value::InternalId { table, offset } => {
            put_u8(out, V_INTERNAL_ID);
            put_i64(out, *table);
            put_i64(out, *offset);
        }
        Value::String(v) => {
            put_u8(out, V_STRING);
            put_str(out, v);
        }
        Value::Node { label, id } => {
            put_u8(out, V_NODE);
            put_str(out, label);
            put_i64(out, *id);
        }
        Value::Edge {
            rel_type,
            id,
            src_label,
            src_id,
            dst_label,
            dst_id,
            projected_properties,
        } => {
            put_u8(out, V_EDGE);
            put_str(out, rel_type);
            put_i64(out, *id);
            put_str(out, src_label);
            put_i64(out, *src_id);
            put_str(out, dst_label);
            put_i64(out, *dst_id);
            match projected_properties {
                None => put_u8(out, 0),
                Some(keys) => {
                    put_u8(out, 1);
                    put_str_list(out, keys);
                }
            }
        }
        Value::List(items) => {
            put_u8(out, V_LIST);
            put_values(out, items);
        }
        Value::Map(map) => {
            put_u8(out, V_MAP);
            put_map(out, map);
        }
        Value::Path(items) => {
            put_u8(out, V_PATH);
            put_values(out, items);
        }
    }
}

fn decode_value(r: &mut Reader) -> Result<Value, String> {
    let tag = r.u8()?;
    Ok(match tag {
        V_NULL => Value::Null,
        V_BOOL => Value::Bool(r.u8()? != 0),
        V_BYTE => Value::Byte(r.u8()? as i8),
        V_UINT8 => Value::UInt8(r.u8()?),
        V_SHORT => Value::Short(r.i16()?),
        V_UINT16 => Value::UInt16(r.u16()?),
        V_INT => Value::Int(r.i64()?),
        V_UINT32 => Value::UInt32(r.u32()?),
        V_LONG => Value::Long(r.i64()?),
        V_UINT64 => Value::UInt64(r.u64()?),
        V_FLOAT32 => Value::Float32(r.f32()?),
        V_FLOAT => Value::Float(r.f64()?),
        V_BIGINT => {
            Value::BigInt(BigInt::from_str(&r.str()?).map_err(|e| format!("invalid BigInt: {e}"))?)
        }
        V_UINT128 => Value::UInt128(
            BigInt::from_str(&r.str()?).map_err(|e| format!("invalid UInt128: {e}"))?,
        ),
        V_BIGDECIMAL => Value::BigDecimal(
            BigDecimal::from_str(&r.str()?).map_err(|e| format!("invalid BigDecimal: {e}"))?,
        ),
        V_DATETIME => Value::DateTime(r.str()?),
        V_INTERNAL_ID => Value::InternalId {
            table: r.i64()?,
            offset: r.i64()?,
        },
        V_STRING => Value::String(r.str()?),
        V_NODE => Value::Node {
            label: r.str()?,
            id: r.i64()?,
        },
        V_EDGE => Value::Edge {
            rel_type: r.str()?,
            id: r.i64()?,
            src_label: r.str()?,
            src_id: r.i64()?,
            dst_label: r.str()?,
            dst_id: r.i64()?,
            projected_properties: if r.u8()? != 0 {
                Some(decode_str_list(r)?)
            } else {
                None
            },
        },
        V_LIST => Value::List(decode_values(r)?),
        V_MAP => Value::Map(decode_map(r)?),
        V_PATH => Value::Path(decode_values(r)?),
        other => return Err(format!("unknown value tag {other}")),
    })
}

fn decode_values(r: &mut Reader) -> Result<Vec<Value>, String> {
    let n = r.count()?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(decode_value(r)?);
    }
    Ok(out)
}

pub(super) fn decode_map(r: &mut Reader) -> Result<BTreeMap<String, Value>, String> {
    let n = r.count()?;
    let mut map = BTreeMap::new();
    for _ in 0..n {
        map.insert(r.str()?, decode_value(r)?);
    }
    Ok(map)
}

pub(super) fn decode_str_list(r: &mut Reader) -> Result<Vec<String>, String> {
    let n = r.count()?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(r.str()?);
    }
    Ok(out)
}

pub(super) fn encode_str_list(list: &[String]) -> Vec<u8> {
    let mut b = Vec::new();
    put_u64(&mut b, list.len() as u64);
    for s in list {
        put_str(&mut b, s);
    }
    b
}

/// Encode a single [`Value`] into a standalone byte buffer using the
/// snapshot value codec. Exposed to the incremental overlay codec so it can
/// reuse the exact same tag-complete (including NaN bit patterns) encoding.
#[cfg(any(feature = "duckdb", test))]
pub(in crate::ir::catalog) fn encode_value_bytes(value: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    encode_value(&mut out, value);
    out
}

/// Decode exactly one [`Value`] from `data`, rejecting any trailing bytes.
/// Exposed to the incremental overlay codec.
#[cfg(any(feature = "duckdb", test))]
pub(in crate::ir::catalog) fn decode_value_bytes(data: &[u8]) -> Result<Value, String> {
    let mut r = Reader::new(data);
    let value = decode_value(&mut r)?;
    finish(&r)?;
    Ok(value)
}

// ---------------- Arrow IPC ----------------

pub(super) fn encode_batch(batch: &RecordBatch) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    {
        let mut writer = StreamWriter::try_new(&mut buf, batch.schema().as_ref())
            .map_err(|e| format!("IPC schema write failed: {e}"))?;
        writer
            .write(batch)
            .map_err(|e| format!("IPC batch write failed: {e}"))?;
        writer
            .finish()
            .map_err(|e| format!("IPC finish failed: {e}"))?;
    }
    Ok(buf)
}

pub(super) fn decode_batch(data: &[u8]) -> Result<RecordBatch, String> {
    let reader = StreamReader::try_new(std::io::Cursor::new(data), None)
        .map_err(|e| format!("IPC schema read failed: {e}"))?;
    let mut batches = reader
        .collect::<Result<Vec<RecordBatch>, _>>()
        .map_err(|e| format!("IPC batch read failed: {e}"))?;
    match batches.len() {
        1 => Ok(batches.remove(0)),
        0 => Err("IPC stream contained no record batch".to_string()),
        _ => Err("IPC stream contained multiple record batches".to_string()),
    }
}

// ---------------- reader ----------------

pub(super) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub(super) fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub(super) fn is_empty(&self) -> bool {
        self.pos >= self.buf.len()
    }

    pub(super) fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if n > self.remaining() {
            return Err("unexpected end of snapshot".to_string());
        }
        let slice = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    pub(super) fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16, String> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub(super) fn i16(&mut self) -> Result<i16, String> {
        let b = self.take(2)?;
        Ok(i16::from_le_bytes([b[0], b[1]]))
    }

    pub(super) fn u32(&mut self) -> Result<u32, String> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(super) fn u64(&mut self) -> Result<u64, String> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub(super) fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }

    pub(super) fn f32(&mut self) -> Result<f32, String> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(super) fn f64(&mut self) -> Result<f64, String> {
        let b = self.take(8)?;
        Ok(f64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ]))
    }

    pub(super) fn str(&mut self) -> Result<String, String> {
        let len = self.u64()? as usize;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| "invalid UTF-8 in snapshot".to_string())
    }

    pub(super) fn blob(&mut self) -> Result<&'a [u8], String> {
        let len = self.u64()? as usize;
        self.take(len)
    }

    /// Read a length prefix, rejecting counts that cannot possibly fit in the
    /// remaining bytes. This turns a corrupt length into a clean error instead
    /// of an oversized allocation.
    pub(super) fn count(&mut self) -> Result<usize, String> {
        let n = self.u64()? as usize;
        if n > self.remaining() {
            return Err("element count exceeds remaining data".to_string());
        }
        Ok(n)
    }
}

pub(super) fn finish(r: &Reader) -> Result<(), String> {
    if r.is_empty() {
        Ok(())
    } else {
        Err("trailing bytes after section".to_string())
    }
}
