//! UTF-16 string expressions with matching DataFusion and DuckDB semantics.
use super::*;
use datafusion::common::DFSchema;
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
use std::any::Any;

pub(super) fn call(name: &str, args: Vec<Expr>, schema: &DFSchema) -> RelResult<Expr> {
    let length = name == "gremlin_string_length";
    if !matches!(
        args[0].get_type(schema)?,
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View | DataType::Null
    ) {
        return Err(RelError::Unsupported(
            "Gremlin string operation requires a string".into(),
        ));
    }
    Ok(ScalarUDF::from(Utf16 {
        length,
        signature: Signature::any(args.len(), Volatility::Immutable),
    })
    .call(args))
}
#[derive(Debug, PartialEq, Eq, Hash)]
struct Utf16 {
    length: bool,
    signature: Signature,
}
impl ScalarUDFImpl for Utf16 {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        if self.length {
            "__orchiddb_utf16_length"
        } else {
            "__orchiddb_utf16_substring"
        }
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, _: &[DataType]) -> datafusion::common::Result<DataType> {
        Ok(if self.length {
            DataType::Int32
        } else {
            DataType::Utf8
        })
    }
    fn invoke_with_args(
        &self,
        args: ScalarFunctionArgs,
    ) -> datafusion::common::Result<ColumnarValue> {
        let arrays = args
            .args
            .iter()
            .map(|arg| arg.to_array(args.number_rows))
            .collect::<datafusion::common::Result<Vec<_>>>()?;
        let mut strings = arrow::array::StringBuilder::new();
        let mut lengths = arrow::array::Int32Builder::new();
        for row in 0..args.number_rows {
            let values: Vec<_> = arrays
                .iter()
                .map(|a| crate::ir::catalog::array_value(a.as_ref(), row, None))
                .collect();
            let name = if self.length { "length" } else { "substring" };
            let value = crate::ir::runtime::scalar::gremlin_string_call(name, &values)
                .map_err(|e| DataFusionError::Execution(e.to_string()))?;
            match value {
                Value::Int(n) => lengths.append_value(n as i32),
                Value::String(s) => strings.append_value(s),
                Value::Null if self.length => lengths.append_null(),
                Value::Null => strings.append_null(),
                _ => {
                    return Err(DataFusionError::Execution(
                        "invalid UTF-16 scalar result".into(),
                    ));
                }
            }
        }
        Ok(ColumnarValue::Array(if self.length {
            Arc::new(lengths.finish())
        } else {
            Arc::new(strings.finish())
        }))
    }
}
