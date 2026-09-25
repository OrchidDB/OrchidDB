//! openCypher conversion input domains, separate from shared lenient casts.
use crate::ir::diagnostics::RuntimeDiagnosis;
use crate::ir::interpreter::{InterpretError, IrResult};
use crate::ir::value::Value;

pub(super) fn validate(name: &str, value: &Value) -> IrResult<()> {
    let integer = matches!(
        value,
        Value::Byte(_)
            | Value::UInt8(_)
            | Value::Short(_)
            | Value::UInt16(_)
            | Value::Int(_)
            | Value::Long(_)
            | Value::UInt32(_)
            | Value::UInt64(_)
            | Value::BigInt(_)
            | Value::UInt128(_)
    );
    let numeric = integer
        || matches!(
            value,
            Value::Float32(_) | Value::Float(_) | Value::BigDecimal(_)
        );
    let valid = matches!(value, Value::Null | Value::String(_))
        || match name {
            "toboolean" => integer || matches!(value, Value::Bool(_)),
            "tointeger" => numeric || matches!(value, Value::Bool(_)),
            "tofloat" => numeric,
            "tostring" => {
                numeric
                    || matches!(
                        value,
                        Value::Bool(_) | Value::Temporal(_) | Value::DateTime(_)
                    )
            }
            _ => false,
        };
    if valid {
        Ok(())
    } else {
        Err(InterpretError::Diagnosed {
            code: RuntimeDiagnosis::InvalidValue,
            message: format!("{name} cannot convert {}", value.type_name()),
        })
    }
}
