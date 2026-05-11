//! GraphReturn boundary — Row stream → Arrow RecordBatch.
//!
//! Extracted from `interpreter.rs` lines 3313..3463.

use std::sync::Arc;

use arrow::array::{
    ArrayRef, BooleanBuilder, Float64Builder, Int64Builder, RecordBatch, StringBuilder,
};
use arrow::datatypes::{DataType, Field, Schema, SchemaRef};

use crate::ir::policy::ResultForm;
use crate::ir::value::Value;

use super::runtime::display_for_concat;
use super::{InterpretError, IrResult, ReturnedBatches, Row};

pub(crate) fn finalize_return(
    fields: &[String],
    result_form: ResultForm,
    rows: Vec<Row>,
) -> IrResult<ReturnedBatches> {
    if fields.is_empty() {
        let schema: SchemaRef = Arc::new(Schema::empty());
        let batch = RecordBatch::new_empty(schema);
        return Ok(ReturnedBatches {
            fields: Vec::new(),
            result_form,
            batch,
        });
    }

    let mut columns: Vec<ColumnBuilder> = fields.iter().map(|_| ColumnBuilder::new()).collect();

    for row in &rows {
        for (idx, field) in fields.iter().enumerate() {
            let value = row.bindings.get(field).cloned().unwrap_or(Value::Null);
            columns[idx].push(value);
        }
    }

    let mut arrow_fields = Vec::with_capacity(fields.len());
    let mut arrays: Vec<ArrayRef> = Vec::with_capacity(fields.len());
    for (name, builder) in fields.iter().zip(columns.into_iter()) {
        let (data_type, array) = builder.finish();
        arrow_fields.push(Field::new(name, data_type, true));
        arrays.push(array);
    }
    let schema: SchemaRef = Arc::new(Schema::new(arrow_fields));
    let batch = RecordBatch::try_new(schema, arrays)
        .map_err(|err| InterpretError::Type(format!("record batch: {err}")))?;
    Ok(ReturnedBatches {
        fields: fields.to_vec(),
        result_form,
        batch,
    })
}

/// Type-promoting column builder: scans values to pick a primitive Arrow
/// type. Mixed types fall back to Utf8.
pub(crate) struct ColumnBuilder {
    values: Vec<Value>,
}

impl ColumnBuilder {
    fn new() -> Self {
        Self { values: Vec::new() }
    }

    fn push(&mut self, value: Value) {
        self.values.push(value);
    }

    fn finish(self) -> (DataType, ArrayRef) {
        let kind = infer_kind(&self.values);
        match kind {
            ColumnKind::Int => {
                let mut builder = Int64Builder::with_capacity(self.values.len());
                for v in self.values {
                    match v {
                        Value::Byte(n) => builder.append_value(n as i64),
                        Value::Short(n) => builder.append_value(n as i64),
                        Value::Int(n) | Value::Long(n) => builder.append_value(n),
                        Value::Null => builder.append_null(),
                        Value::Float32(f) => builder.append_value(f as i64),
                        Value::Float(f) => builder.append_value(f as i64),
                        _ => builder.append_null(),
                    }
                }
                (DataType::Int64, Arc::new(builder.finish()) as ArrayRef)
            }
            ColumnKind::Float => {
                let mut builder = Float64Builder::with_capacity(self.values.len());
                for v in self.values {
                    match v {
                        Value::Float(f) => builder.append_value(f),
                        Value::Float32(f) => builder.append_value(f as f64),
                        Value::Byte(n) => builder.append_value(n as f64),
                        Value::Short(n) => builder.append_value(n as f64),
                        Value::Int(n) | Value::Long(n) => builder.append_value(n as f64),
                        Value::Null => builder.append_null(),
                        _ => builder.append_null(),
                    }
                }
                (DataType::Float64, Arc::new(builder.finish()) as ArrayRef)
            }
            ColumnKind::Bool => {
                let mut builder = BooleanBuilder::with_capacity(self.values.len());
                for v in self.values {
                    match v {
                        Value::Bool(b) => builder.append_value(b),
                        Value::Null => builder.append_null(),
                        _ => builder.append_null(),
                    }
                }
                (DataType::Boolean, Arc::new(builder.finish()) as ArrayRef)
            }
            ColumnKind::Utf8 => {
                let mut builder = StringBuilder::new();
                for v in self.values {
                    match v {
                        Value::String(s) => builder.append_value(s),
                        Value::Null => builder.append_null(),
                        Value::Byte(n) => builder.append_value(format!("d[{n}].b")),
                        Value::Short(n) => builder.append_value(format!("d[{n}].s")),
                        Value::Int(n) => builder.append_value(n.to_string()),
                        Value::Long(n) => builder.append_value(format!("d[{n}].l")),
                        Value::Float32(f) => builder.append_value(format!("d[{}].f", f as f64)),
                        Value::Float(f) => builder.append_value(f.to_string()),
                        Value::Bool(b) => builder.append_value(b.to_string()),
                        // Tag arbitrary-precision values with TinkerPop's
                        // `d[N].m` / `d[N].i` shape so the harness can
                        // compare against the expected gherkin lines.
                        Value::BigDecimal(d) => builder.append_value(format!("d[{d}].m")),
                        Value::BigInt(n) => builder.append_value(format!("d[{n}].n")),
                        Value::DateTime(s) => builder.append_value(format!("dt[{s}]")),
                        Value::Node { label, id } => {
                            builder.append_value(format!("{label}#{id}"));
                        }
                        Value::Edge { rel_type, id, .. } => {
                            builder.append_value(format!("{rel_type}#{id}"));
                        }
                        other => builder.append_value(display_for_concat(&other)),
                    }
                }
                (DataType::Utf8, Arc::new(builder.finish()) as ArrayRef)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColumnKind {
    Int,
    Float,
    Bool,
    Utf8,
}

pub(crate) fn infer_kind(values: &[Value]) -> ColumnKind {
    let mut kind: Option<ColumnKind> = None;
    for v in values {
        let candidate = match v {
            Value::Null => continue,
            Value::Byte(_) | Value::Short(_) | Value::Int(_) | Value::Long(_) => ColumnKind::Int,
            Value::Float32(_) | Value::Float(_) => ColumnKind::Float,
            Value::Bool(_) => ColumnKind::Bool,
            _ => ColumnKind::Utf8,
        };
        kind = Some(match kind {
            None => candidate,
            Some(existing) if existing == candidate => existing,
            Some(ColumnKind::Int) if matches!(candidate, ColumnKind::Float) => ColumnKind::Float,
            Some(ColumnKind::Float) if matches!(candidate, ColumnKind::Int) => ColumnKind::Float,
            _ => ColumnKind::Utf8,
        });
    }
    kind.unwrap_or(ColumnKind::Utf8)
}
