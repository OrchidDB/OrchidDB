//! Explicit declared-UDF argument types must survive SQL generation. A normal
//! DataFusion cast can disappear as a same-type no-op even though SQL literals
//! (e.g. `4`) will be rebound by DuckDB as a different integer width.
use arrow::datatypes::DataType;
use datafusion::common::{DataFusionError, Result};
use datafusion::logical_expr::{
    ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
use datafusion::prelude::lit;
use std::any::Any;

pub(crate) const ENGINE_CAST_FUNCTION: &str = "__engine_declared_argument_cast";

#[derive(Debug, PartialEq, Eq, Hash)]
struct DeclaredCast {
    data_type: DataType,
    signature: Signature,
}
impl ScalarUDFImpl for DeclaredCast {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        ENGINE_CAST_FUNCTION
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, _: &[DataType]) -> Result<DataType> {
        Ok(self.data_type.clone())
    }
    fn invoke_with_args(&self, _: ScalarFunctionArgs) -> Result<ColumnarValue> {
        Err(DataFusionError::NotImplemented(
            "declared UDF argument casts require the SQL backend".into(),
        ))
    }
}
pub(crate) fn typed_argument_cast(expr: Expr, data_type: DataType) -> Result<Expr> {
    let sql_type = super::types::sql_type(&data_type)?;
    Ok(ScalarUDF::new_from_impl(DeclaredCast {
        data_type,
        signature: Signature::any(2, Volatility::Volatile),
    })
    .call(vec![expr, lit(sql_type)]))
}
