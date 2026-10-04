//! Bridge graph mutation expressions to the same typed JSON kernels used by plans.
use super::*;
use crate::ir::value::Value as GraphValue;

fn scalar(value: &GraphValue) -> Result<ScalarValue> {
    Ok(match value {
        GraphValue::Scalar(v) => v.clone(),
        GraphValue::Null => ScalarValue::Null,
        GraphValue::Bool(v) => ScalarValue::Boolean(Some(*v)),
        GraphValue::Byte(v) => ScalarValue::Int8(Some(*v)),
        GraphValue::UInt8(v) => ScalarValue::UInt8(Some(*v)),
        GraphValue::Short(v) => ScalarValue::Int16(Some(*v)),
        GraphValue::UInt16(v) => ScalarValue::UInt16(Some(*v)),
        GraphValue::Int(v) | GraphValue::Long(v) => ScalarValue::Int64(Some(*v)),
        GraphValue::UInt32(v) => ScalarValue::UInt32(Some(*v)),
        GraphValue::UInt64(v) => ScalarValue::UInt64(Some(*v)),
        GraphValue::Float32(v) => ScalarValue::Float32(Some(*v)),
        GraphValue::Float(v) => ScalarValue::Float64(Some(*v)),
        GraphValue::String(v) => ScalarValue::Utf8(Some(v.clone())),
        GraphValue::List(items) => list(
            items
                .iter()
                .map(|v| output(&to_json(&scalar(v)?)?))
                .collect::<Result<_>>()?,
            &domain::json_type(),
        )?,
        GraphValue::Map(items) => {
            let items = items
                .iter()
                .filter(|(key, _)| {
                    !matches!(
                        key.as_str(),
                        crate::ir::value::STRUCT_ORDER_KEY | crate::ir::value::STRUCT_TYPES_KEY
                    )
                })
                .collect::<Vec<_>>();
            let values = items
                .iter()
                .map(|(_, value)| scalar(value))
                .collect::<Result<Vec<_>>>()?;
            let ty = DataType::Struct(
                items
                    .iter()
                    .map(|(key, _)| *key)
                    .zip(&values)
                    .map(|(key, value)| Field::new(key, value.data_type(), true))
                    .collect::<Vec<_>>()
                    .into(),
            );
            struct_value(&ty, values)?
        }
        value => {
            return Err(error(format!(
                "cannot pass {} to JSON function",
                value.type_name()
            )));
        }
    })
}

pub(crate) fn call(name: &str, args: &[GraphValue]) -> Result<GraphValue> {
    if name.eq_ignore_ascii_case("json.literal") {
        let [GraphValue::String(text)] = args else {
            return Err(error("JSON literal requires constant text"));
        };
        return Ok(GraphValue::Scalar(domain::json_scalar(text)?));
    }
    let name = name.to_ascii_lowercase();
    let op = operation(&name).ok_or_else(|| error("unknown JSON function"))?;
    let args = args.iter().map(scalar).collect::<Result<Vec<_>>>()?;
    let types = args.iter().map(ScalarValue::data_type).collect::<Vec<_>>();
    validate(op, &types)?;
    let schema = if op == "value" {
        args.get(2)
    } else if op == "transform" {
        args.get(1)
    } else {
        None
    };
    let schema = schema
        .map(|v| {
            if domain::is_json(&v.data_type()) {
                domain::json_text(v)
            } else {
                text(v)
            }
        })
        .transpose()?
        .flatten();
    let ty = return_type(op, &types, schema.as_deref())?;
    let value = evaluate(op, &args, &ty)?;
    let array = value.to_array_of_size(1)?;
    Ok(crate::ir::catalog::array_value(array.as_ref(), 0, None))
}
