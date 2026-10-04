//! Portable JSON functions with native execution and explicit document types.
mod aggregate;
mod runtime;
pub(crate) use runtime::call as runtime_call;
pub mod path;
#[cfg(test)]
mod tests;
use super::domain;
pub use aggregate::{aggregate, aggregate_names};
use arrow::{
    array::{Array, ArrayRef, StructArray},
    datatypes::{DataType, Field, FieldRef},
};
use datafusion::{
    common::{DataFusionError, Result, ScalarValue},
    logical_expr::{
        ColumnarValue, ReturnFieldArgs, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
        TypeSignature, Volatility,
    },
};
use serde_json::{Map, Value};
use std::{
    any::Any,
    cmp::Ordering,
    collections::BTreeMap,
    str::FromStr,
    sync::{Arc, LazyLock},
};

pub const NAMES: &[&str] = &[
    "parse",
    "stringify",
    "valid",
    "query",
    "value",
    "exists",
    "type",
    "keys",
    "array_length",
    "elements",
    "entries",
    "tree",
    "object",
    "array",
    "transform",
    "set",
    "insert",
    "replace",
    "remove",
    "merge_patch",
    "equals",
    "contains",
];
pub fn names() -> impl Iterator<Item = String> {
    NAMES.iter().map(|n| format!("json.{n}"))
}
pub fn operation(name: &str) -> Option<&str> {
    let name = name
        .strip_prefix("__orchiddb_json_")
        .or_else(|| name.strip_prefix("json."))?;
    (NAMES.contains(&name) || matches!(name, "array_agg" | "object_agg")).then_some(name)
}
pub fn function(name: &str) -> Option<Arc<ScalarUDF>> {
    static FUNCTIONS: LazyLock<BTreeMap<String, Arc<ScalarUDF>>> = LazyLock::new(|| {
        NAMES
            .iter()
            .map(|name| {
                let function = ScalarUDF::new_from_impl(JsonFunction {
                    operation: (*name).into(),
                    name: format!("__orchiddb_json_{name}"),
                    aliases: vec![format!("json.{name}")],
                    signature: Signature::one_of(
                        vec![TypeSignature::Exact(vec![]), TypeSignature::VariadicAny],
                        Volatility::Immutable,
                    ),
                });
                (format!("json.{name}"), Arc::new(function))
            })
            .collect()
    });
    let name = name.to_ascii_lowercase();
    FUNCTIONS
        .get(&name)
        .or_else(|| {
            name.strip_prefix("__orchiddb_json_")
                .and_then(|n| FUNCTIONS.get(&format!("json.{n}")))
        })
        .cloned()
}
pub fn error(message: impl std::fmt::Display) -> DataFusionError {
    DataFusionError::Execution(format!("JSON: {message}"))
}
#[derive(Debug, PartialEq, Eq, Hash)]
struct JsonFunction {
    operation: String,
    name: String,
    aliases: Vec<String>,
    signature: Signature,
}
impl ScalarUDFImpl for JsonFunction {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn aliases(&self) -> &[String] {
        &self.aliases
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, args: &[DataType]) -> Result<DataType> {
        validate(&self.operation, args)?;
        return_type(&self.operation, args, None)
    }
    fn return_field_from_args(&self, args: ReturnFieldArgs) -> Result<FieldRef> {
        let types = args
            .arg_fields
            .iter()
            .map(|f| f.data_type().clone())
            .collect::<Vec<_>>();
        validate(&self.operation, &types)?;
        let literal = if self.operation == "value" {
            args.scalar_arguments.get(2)
        } else if self.operation == "transform" {
            args.scalar_arguments.get(1)
        } else {
            None
        };
        let literal = literal
            .and_then(|v| *v)
            .map(|v| {
                if self.operation == "transform" && domain::is_json(&v.data_type()) {
                    domain::json_text(v)
                } else {
                    text(v)
                }
            })
            .transpose()?
            .flatten();
        let ty = return_type(&self.operation, &types, literal.as_deref())?;
        Ok(Arc::new(Field::new(self.name(), ty, true)))
    }
    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        let scalar = args
            .args
            .iter()
            .all(|v| matches!(v, ColumnarValue::Scalar(_)));
        let arrays = ColumnarValue::values_to_arrays(&args.args)?;
        let count = arrays.first().map(|a| a.len()).unwrap_or(args.number_rows);
        let mut result = Vec::with_capacity(count);
        for row in 0..count {
            let input = arrays
                .iter()
                .map(|a| ScalarValue::try_from_array(a.as_ref(), row))
                .collect::<Result<Vec<_>>>()?;
            result.push(evaluate(&self.operation, &input, args.return_type())?);
        }
        if scalar && count > 0 {
            return Ok(ColumnarValue::Scalar(result.remove(0)));
        }
        let array = if result.is_empty() {
            arrow::array::new_empty_array(args.return_type())
        } else {
            ScalarValue::iter_to_array(result)?
        };
        Ok(ColumnarValue::Array(array))
    }
}
fn validate(op: &str, args: &[DataType]) -> Result<()> {
    let n = args.len();
    let arity = match op {
        "array" => true,
        "object" => n % 2 == 0,
        "type" | "keys" | "array_length" | "elements" | "entries" | "tree" => matches!(n, 1 | 2),
        "value" => matches!(n, 2 | 3),
        "remove" => n >= 2,
        "parse" | "stringify" | "valid" => n == 1,
        "set" | "insert" | "replace" => n == 3,
        _ => n == 2,
    };
    if !arity {
        return Err(error(format!("invalid argument count for json.{op}")));
    }
    let string = |t: &DataType| {
        matches!(
            t,
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View | DataType::Null
        )
    };
    if matches!(op, "parse" | "valid") {
        if !string(&args[0]) {
            return Err(error("parse/valid require text"));
        }
    } else if !matches!(op, "array" | "object")
        && !domain::is_json(&args[0])
        && args[0] != DataType::Null
    {
        return Err(error(
            "document must have JSON type; use json.parse for text",
        ));
    }
    if matches!(op, "equals" | "contains" | "merge_patch")
        && !domain::is_json(&args[1])
        && args[1] != DataType::Null
    {
        return Err(error("second document must have JSON type"));
    }
    if matches!(
        op,
        "query" | "value" | "exists" | "set" | "insert" | "replace"
    ) && !string(&args[1])
    {
        return Err(error("path/schema must be text"));
    }
    if matches!(
        op,
        "type" | "keys" | "array_length" | "elements" | "entries" | "tree"
    ) && n == 2
        && !string(&args[1])
    {
        return Err(error("path must be text"));
    }
    if op == "transform" && !string(&args[1]) && !domain::is_json(&args[1]) {
        return Err(error("transform schema must be constant text or JSON"));
    }
    if op == "remove" && args[1..].iter().any(|t| !string(t)) {
        return Err(error("paths must be text"));
    }
    if op == "object" && args.iter().step_by(2).any(|t| !string(t)) {
        return Err(error("object keys must be text"));
    }
    Ok(())
}
fn return_type(op: &str, args: &[DataType], literal: Option<&str>) -> Result<DataType> {
    Ok(match op {
        "stringify" | "type" => DataType::Utf8,
        "valid" | "exists" | "equals" | "contains" => DataType::Boolean,
        "value" => {
            if args.len() == 2 {
                DataType::Utf8
            } else {
                scalar_type(
                    literal.ok_or_else(|| error("json.value type must be a constant string"))?,
                )?
            }
        }
        "transform" => schema_type(
            &serde_json::from_str::<Value>(
                literal
                    .ok_or_else(|| error("json.transform schema must be a constant JSON string"))?,
            )
            .map_err(error)?,
        )?,
        "keys" => DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
        "array_length" => DataType::UInt64,
        "elements" | "entries" | "tree" => {
            DataType::List(Arc::new(Field::new("item", row_type(), true)))
        }
        _ => domain::json_type(),
    })
}
pub fn row_type() -> DataType {
    DataType::Struct(
        vec![
            Field::new("value", domain::json_type(), true),
            Field::new("index", DataType::Int64, true),
            Field::new("key", DataType::Utf8, true),
            Field::new("path", DataType::Utf8, true),
            Field::new("parent_path", DataType::Utf8, true),
            Field::new("depth", DataType::Int64, true),
        ]
        .into(),
    )
}
pub fn scalar_type(name: &str) -> Result<DataType> {
    match name.to_ascii_lowercase().as_str() {
        "json" => Ok(domain::json_type()),
        "varchar" | "text" | "string" => Ok(DataType::Utf8),
        "bool" | "boolean" => Ok(DataType::Boolean),
        "bigint" | "int64" => Ok(DataType::Int64),
        "integer" | "int" | "int32" => Ok(DataType::Int32),
        "double" | "double precision" | "float64" => Ok(DataType::Float64),
        "real" | "float" | "float32" => Ok(DataType::Float32),
        name => crate::compiler::data_type(name).map_err(error),
    }
}
pub fn schema_type(schema: &Value) -> Result<DataType> {
    match schema {
        Value::String(name) => scalar_type(name),
        Value::Array(items) if items.len() == 1 => Ok(DataType::List(Arc::new(Field::new(
            "item",
            schema_type(&items[0])?,
            true,
        )))),
        Value::Object(fields) => Ok(DataType::Struct(
            fields
                .iter()
                .map(|(name, schema)| Ok(Field::new(name, schema_type(schema)?, true)))
                .collect::<Result<Vec<_>>>()?
                .into(),
        )),
        _ => Err(error(
            "schema must be a type name, one-element array, or object of field schemas",
        )),
    }
}
fn text(value: &ScalarValue) -> Result<Option<String>> {
    if value.is_null() {
        return Ok(None);
    }
    match value {
        ScalarValue::Utf8(v) | ScalarValue::LargeUtf8(v) | ScalarValue::Utf8View(v) => {
            Ok(v.clone())
        }
        _ => Err(error("expected text")),
    }
}
pub fn document(value: &ScalarValue) -> Result<Option<Value>> {
    if value.is_null() {
        return Ok(None);
    }
    domain::json_text(value)?
        .map(|s| serde_json::from_str(&s).map_err(error))
        .transpose()
}
fn output(value: &Value) -> Result<ScalarValue> {
    domain::json_scalar(&value.to_string())
}
fn null(ty: &DataType) -> Result<ScalarValue> {
    ScalarValue::try_from(ty)
}
fn list(values: Vec<ScalarValue>, ty: &DataType) -> Result<ScalarValue> {
    let array = if values.is_empty() {
        arrow::array::new_empty_array(ty)
    } else {
        ScalarValue::iter_to_array(values)?
    };
    Ok(ScalarValue::List(Arc::new(
        arrow::array::ListArray::try_new(
            Arc::new(Field::new("item", ty.clone(), true)),
            arrow::buffer::OffsetBuffer::from_lengths([array.len()]),
            array,
            None,
        )?,
    )))
}
fn struct_value(ty: &DataType, values: Vec<ScalarValue>) -> Result<ScalarValue> {
    let DataType::Struct(fields) = ty else {
        return Err(error("expected struct type"));
    };
    if fields.is_empty() {
        return Ok(ScalarValue::Struct(Arc::new(
            StructArray::new_empty_fields(1, None),
        )));
    }
    Ok(ScalarValue::Struct(Arc::new(StructArray::try_new(
        fields.clone(),
        values
            .into_iter()
            .map(|v| v.to_array_of_size(1))
            .collect::<Result<Vec<_>>>()?,
        None,
    )?)))
}
pub fn to_json(value: &ScalarValue) -> Result<Value> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    if domain::is_json(&value.data_type()) {
        return Ok(document(value)?.unwrap());
    }
    Ok(match value {
        ScalarValue::Utf8(Some(s))
        | ScalarValue::LargeUtf8(Some(s))
        | ScalarValue::Utf8View(Some(s)) => Value::String(s.clone()),
        ScalarValue::Boolean(Some(b)) => Value::Bool(*b),
        ScalarValue::List(a) => Value::Array(
            (0..a.value(0).len())
                .map(|i| to_json(&ScalarValue::try_from_array(a.value(0).as_ref(), i)?))
                .collect::<Result<_>>()?,
        ),
        ScalarValue::LargeList(a) => Value::Array(
            (0..a.value(0).len())
                .map(|i| to_json(&ScalarValue::try_from_array(a.value(0).as_ref(), i)?))
                .collect::<Result<_>>()?,
        ),
        ScalarValue::FixedSizeList(a) => Value::Array(
            (0..a.value(0).len())
                .map(|i| to_json(&ScalarValue::try_from_array(a.value(0).as_ref(), i)?))
                .collect::<Result<_>>()?,
        ),
        ScalarValue::Struct(a) => Value::Object(
            a.fields()
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    Ok((
                        f.name().clone(),
                        to_json(&ScalarValue::try_from_array(a.column(i), 0)?)?,
                    ))
                })
                .collect::<Result<_>>()?,
        ),
        value if value.data_type().is_numeric() => serde_json::from_str(&value.to_string())
            .map_err(|_| error("JSON numbers must be finite and representable"))?,
        _ => {
            return Err(error(format!(
                "no JSON conversion for {}",
                value.data_type()
            )));
        }
    })
}
// Compare decimal JSON numbers without f64 rounding or expanding large powers.
fn compare_numbers(a: &str, b: &str) -> Ordering {
    fn normalized(input: &str) -> (bool, String, num_bigint::BigInt) {
        let negative = input.starts_with('-');
        let input = input.strip_prefix('-').unwrap_or(input);
        let (mantissa, exponent) = input.split_once(['e', 'E']).unwrap_or((input, "0"));
        let fraction = mantissa.split_once('.').map(|(_, s)| s.len()).unwrap_or(0);
        let digits = mantissa.replace('.', "");
        let digits = digits.trim_start_matches('0');
        if digits.is_empty() {
            return (false, "0".into(), 0.into());
        }
        let trimmed = digits.trim_end_matches('0');
        let exponent = num_bigint::BigInt::from_str(exponent).expect("valid JSON exponent")
            - num_bigint::BigInt::from(fraction)
            + num_bigint::BigInt::from(digits.len() - trimmed.len());
        (negative, trimmed.into(), exponent)
    }
    let (an, ad, ae) = normalized(a);
    let (bn, bd, be) = normalized(b);
    if an != bn {
        return if an {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }
    let magnitude = if ad == "0" || bd == "0" {
        match (ad == "0", bd == "0") {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            _ => Ordering::Greater,
        }
    } else {
        let order = (ae + num_bigint::BigInt::from(ad.len()))
            .cmp(&(be + num_bigint::BigInt::from(bd.len())));
        if order != Ordering::Equal {
            order
        } else {
            ad.bytes()
                .chain(std::iter::repeat(b'0'))
                .zip(bd.bytes().chain(std::iter::repeat(b'0')))
                .take(ad.len().max(bd.len()))
                .map(|(a, b)| a.cmp(&b))
                .find(|o| *o != Ordering::Equal)
                .unwrap_or(Ordering::Equal)
        }
    };
    if an { magnitude.reverse() } else { magnitude }
}
pub fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => {
            Some(compare_numbers(&a.to_string(), &b.to_string()))
        }
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        (Value::Null, Value::Null) => Some(Ordering::Equal),
        (Value::Array(a), Value::Array(b))
            if a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(a, b)| compare(a, b) == Some(Ordering::Equal)) =>
        {
            Some(Ordering::Equal)
        }
        (Value::Object(a), Value::Object(b))
            if a.len() == b.len()
                && a.iter().all(|(k, a)| {
                    b.get(k)
                        .is_some_and(|b| compare(a, b) == Some(Ordering::Equal))
                }) =>
        {
            Some(Ordering::Equal)
        }
        _ => None,
    }
}
pub fn contains(a: &Value, b: &Value) -> bool {
    contains_inner(a, b, true)
}
fn contains_inner(a: &Value, b: &Value, root: bool) -> bool {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => b
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|a| contains_inner(a, v, false))),
        (Value::Array(a), Value::Array(b)) => b
            .iter()
            .all(|b| a.iter().any(|a| contains_inner(a, b, false))),
        (Value::Array(a), b) if root && !b.is_array() && !b.is_object() => {
            a.iter().any(|a| compare(a, b) == Some(Ordering::Equal))
        }
        _ => compare(a, b) == Some(Ordering::Equal),
    }
}
fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
pub fn convert(value: &Value, ty: &DataType) -> Result<ScalarValue> {
    if domain::is_json(ty) {
        return output(value);
    }
    if value.is_null() {
        return null(ty);
    }
    match (value, ty) {
        (Value::Object(object), DataType::Struct(fields)) => struct_value(
            ty,
            fields
                .iter()
                .map(|f| match object.get(f.name()) {
                    Some(value) => convert(value, f.data_type()),
                    None => null(f.data_type()),
                })
                .collect::<Result<_>>()?,
        ),
        (Value::Array(values), DataType::List(field)) => list(
            values
                .iter()
                .map(|v| convert(v, field.data_type()))
                .collect::<Result<_>>()?,
            field.data_type(),
        ),
        (_, DataType::Utf8) if !value.is_array() && !value.is_object() => {
            Ok(ScalarValue::Utf8(Some(if let Value::String(s) = value {
                s.clone()
            } else {
                value.to_string()
            })))
        }
        (Value::Bool(b), DataType::Boolean) => Ok(ScalarValue::Boolean(Some(*b))),
        (Value::Number(n), ty) if ty.is_numeric() => {
            let text = n.to_string();
            // Arrow's decimal/string casts preserve exact integers and enforce overflow.
            let string = ScalarValue::Utf8(Some(text)).to_array_of_size(1)?;
            let cast = arrow::compute::cast_with_options(
                &string,
                ty,
                &arrow::compute::CastOptions {
                    safe: false,
                    ..Default::default()
                },
            )
            .map_err(|e| error(format!("JSON numeric conversion: {e}")))?;
            let result = ScalarValue::try_from_array(cast.as_ref(), 0)?;
            if matches!(result, ScalarValue::Float32(Some(v)) if !v.is_finite())
                || matches!(result, ScalarValue::Float64(Some(v)) if !v.is_finite())
            {
                return Err(error("JSON numeric conversion overflow"));
            }
            Ok(result)
        }
        _ => Err(error(format!(
            "cannot convert JSON {} to {ty}",
            kind(value)
        ))),
    }
}
pub fn evaluate(op: &str, args: &[ScalarValue], return_type: &DataType) -> Result<ScalarValue> {
    if op == "array" {
        return output(&Value::Array(
            args.iter().map(to_json).collect::<Result<_>>()?,
        ));
    }
    if op == "object" {
        let mut map = Map::new();
        for pair in args.chunks_exact(2) {
            let key = text(&pair[0])?.ok_or_else(|| error("object key cannot be SQL NULL"))?;
            map.insert(key, to_json(&pair[1])?);
        }
        return output(&Value::Object(map));
    }
    if matches!(op, "parse" | "valid") {
        let Some(s) = text(&args[0])? else {
            return null(return_type);
        };
        return if op == "valid" {
            Ok(ScalarValue::Boolean(Some(
                serde_json::from_str::<Value>(&s).is_ok(),
            )))
        } else {
            domain::json_scalar(&s)
        };
    }
    let rows = matches!(op, "elements" | "entries" | "tree");
    let Some(mut doc) = document(&args[0])? else {
        return if rows {
            list(vec![], &row_type())
        } else {
            null(return_type)
        };
    };
    if op == "stringify" {
        return Ok(ScalarValue::Utf8(Some(doc.to_string())));
    }
    if matches!(op, "equals" | "contains" | "merge_patch") {
        let Some(other) = document(&args[1])? else {
            return null(return_type);
        };
        return match op {
            "equals" => Ok(ScalarValue::Boolean(Some(
                compare(&doc, &other) == Some(Ordering::Equal),
            ))),
            "contains" => Ok(ScalarValue::Boolean(Some(contains(&doc, &other)))),
            _ => {
                merge_patch(&mut doc, &other);
                output(&doc)
            }
        };
    }
    if op == "transform" {
        return convert(&doc, return_type);
    }
    if matches!(op, "set" | "insert" | "replace" | "remove") {
        for p in if op == "remove" {
            &args[1..]
        } else {
            &args[1..2]
        } {
            let Some(p) = text(p)? else {
                return null(return_type);
            };
            let steps = path::parse(&p)?;
            if !path::singular(&steps) {
                return Err(error("mutation requires a definite field/index path"));
            }
            let replacement = if op == "remove" {
                None
            } else {
                Some(to_json(&args[2])?)
            };
            mutate(&mut doc, &steps, op, replacement)?;
        }
        return output(&doc);
    }
    let p = if args.len() > 1 {
        let Some(p) = text(&args[1])? else {
            return if rows {
                list(vec![], &row_type())
            } else {
                null(return_type)
            };
        };
        p
    } else {
        "$".into()
    };
    let steps = path::parse(&p)?;
    let hits = path::select_steps(&doc, &steps)?;
    if op == "exists" {
        return Ok(ScalarValue::Boolean(Some(!hits.is_empty())));
    }
    if rows {
        let mut result = vec![];
        for hit in hits {
            match op {
                "elements" if !hit.value.is_array() => {
                    return Err(error("json.elements requires an array"));
                }
                "entries" if !hit.value.is_object() => {
                    return Err(error("json.entries requires an object"));
                }
                "tree" => walk(&hit, &mut result)?,
                _ => {
                    for child in path::children(&hit) {
                        result.push(row(&child)?);
                    }
                }
            }
        }
        return list(result, &row_type());
    }
    if hits.is_empty() {
        return null(return_type);
    }
    if op == "query" {
        return if path::singular(&steps) {
            output(hits[0].value)
        } else {
            output(&Value::Array(
                hits.iter().map(|m| m.value.clone()).collect(),
            ))
        };
    }
    if hits.len() != 1 {
        return Err(error(format!("json.{op} requires one matched value")));
    }
    let value = hits[0].value;
    match op {
        "value" => {
            if value.is_null() || value.is_array() || value.is_object() {
                null(return_type)
            } else {
                convert(value, return_type)
            }
        }
        "type" => Ok(ScalarValue::Utf8(Some(kind(value).into()))),
        "keys" => {
            let object = value
                .as_object()
                .ok_or_else(|| error("json.keys requires an object"))?;
            list(
                object
                    .keys()
                    .map(|k| ScalarValue::Utf8(Some(k.clone())))
                    .collect(),
                &DataType::Utf8,
            )
        }
        "array_length" => Ok(ScalarValue::UInt64(Some(
            value
                .as_array()
                .ok_or_else(|| error("json.array_length requires an array"))?
                .len() as u64,
        ))),
        _ => Err(error(format!("unknown JSON operation {op}"))),
    }
}
fn row(m: &path::Match) -> Result<ScalarValue> {
    struct_value(
        &row_type(),
        vec![
            output(m.value)?,
            ScalarValue::Int64(m.index),
            ScalarValue::Utf8(m.key.clone()),
            ScalarValue::Utf8(Some(m.path.clone())),
            ScalarValue::Utf8(m.parent_path.clone()),
            ScalarValue::Int64(Some(m.depth)),
        ],
    )
}
fn walk(m: &path::Match, out: &mut Vec<ScalarValue>) -> Result<()> {
    out.push(row(m)?);
    for c in path::children(m) {
        walk(&c, out)?;
    }
    Ok(())
}
fn merge_patch(doc: &mut Value, patch: &Value) {
    if let Value::Object(patch) = patch {
        if !doc.is_object() {
            *doc = Value::Object(Map::new());
        }
        let object = doc.as_object_mut().unwrap();
        for (k, v) in patch {
            if v.is_null() {
                object.remove(k);
            } else {
                merge_patch(object.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
    } else {
        *doc = patch.clone();
    }
}
fn mutate(doc: &mut Value, steps: &[path::Step], op: &str, value: Option<Value>) -> Result<()> {
    if steps.is_empty() {
        if op == "remove" {
            return Err(error("cannot remove the document root"));
        }
        if op != "insert" {
            *doc = value.unwrap();
        }
        return Ok(());
    }
    let (last, parents) = steps.split_last().unwrap();
    let mut target = doc;
    for step in parents {
        let next = match step {
            path::Step::Field(key) | path::Step::Pointer(key) => {
                if matches!(step, path::Step::Pointer(_)) && target.is_array() {
                    key.parse::<i64>()
                        .ok()
                        .filter(|i| *i >= 0 && i.to_string() == *key)
                        .and_then(|i| array_item_mut(target, i))
                } else {
                    target.get_mut(key)
                }
            }
            path::Step::Index(i) => array_item_mut(target, *i),
            _ => None,
        };
        let Some(next) = next else {
            return Ok(());
        };
        target = next;
    }
    if let (Value::Object(object), path::Step::Field(key) | path::Step::Pointer(key)) =
        (&mut *target, last)
    {
        match op {
            "remove" => {
                object.remove(key);
            }
            "insert" if object.contains_key(key) => {}
            "replace" if !object.contains_key(key) => {}
            _ => {
                object.insert(key.clone(), value.unwrap());
            }
        }
        return Ok(());
    }
    let index = match last {
        path::Step::Index(i) => Some(*i),
        path::Step::Pointer(k) => k
            .parse::<i64>()
            .ok()
            .filter(|i| *i >= 0 && i.to_string() == *k),
        _ => None,
    };
    if let (Some(array), Some(index)) = (target.as_array_mut(), index) {
        let index = if index < 0 {
            array.len() as i64 + index
        } else {
            index
        };
        let valid = index >= 0 && (index as usize) < array.len();
        match op {
            "remove" if valid => {
                array.remove(index as usize);
            }
            "replace" if !valid => {}
            "remove" => {}
            "insert" => array.insert(
                index.max(0).min(array.len() as i64) as usize,
                value.unwrap(),
            ),
            _ if valid => array[index as usize] = value.unwrap(),
            _ if index < 0 => array.insert(0, value.unwrap()),
            _ => array.push(value.unwrap()),
        }
    }
    Ok(())
}
fn array_item_mut(value: &mut Value, index: i64) -> Option<&mut Value> {
    let a = value.as_array_mut()?;
    let index = if index < 0 {
        a.len() as i64 + index
    } else {
        index
    };
    if index < 0 {
        None
    } else {
        a.get_mut(index as usize)
    }
}
