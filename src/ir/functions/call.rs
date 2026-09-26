//! Typed engine calls are planning objects, never host-language emulations.
use super::{FunctionKind, OperatorTable};
use arrow::datatypes::DataType;
use datafusion::common::{DFSchema, DataFusionError, Result};
use datafusion::logical_expr::{
    Accumulator, AggregateUDF, AggregateUDFImpl, ColumnarValue, Expr, ExprFunctionExt,
    ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility, function::AccumulatorArgs,
};
use std::{any::Any, sync::Arc};

/// Internal identity prevents a native `log` being rewritten as a standard
/// language `log`. Only dialect SQL rendering removes this marker.
pub const ENGINE_FUNCTION_PREFIX: &str = "__engine_function_";

#[derive(Debug, PartialEq, Eq, Hash)]
struct EngineCall {
    name: String,
    return_type: DataType,
    signature: Signature,
}

fn execution_error() -> DataFusionError {
    DataFusionError::NotImplemented("engine function requires the DuckDB SQL backend".into())
}

impl ScalarUDFImpl for EngineCall {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, _: &[DataType]) -> Result<DataType> {
        Ok(self.return_type.clone())
    }
    fn invoke_with_args(&self, _: ScalarFunctionArgs) -> Result<ColumnarValue> {
        Err(execution_error())
    }
}
impl AggregateUDFImpl for EngineCall {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, _: &[DataType]) -> Result<DataType> {
        Ok(self.return_type.clone())
    }
    fn accumulator(&self, _: AccumulatorArgs) -> Result<Box<dyn Accumulator>> {
        Err(execution_error())
    }
}

fn bind(
    catalog: &dyn OperatorTable,
    name: &str,
    kind: FunctionKind,
    args: &[Expr],
    schema: &DFSchema,
) -> Result<EngineCall> {
    if catalog.engine() != "duckdb" {
        return Err(DataFusionError::NotImplemented(format!(
            "function SQL adapter for engine `{}`",
            catalog.engine()
        )));
    }
    let return_type = catalog.bind(name, kind, args, schema)?;
    let target = catalog.target_name(name);
    // Conservative: native calls must never be folded by DataFusion or the
    // host scalar evaluator. The engine applies its own volatility and null semantics.
    Ok(EngineCall {
        name: format!("{ENGINE_FUNCTION_PREFIX}{target}"),
        return_type,
        signature: if args.is_empty() {
            Signature::exact(vec![], Volatility::Volatile)
        } else {
            Signature::any(args.len(), Volatility::Volatile)
        },
    })
}

pub fn native_scalar(name: &str, args: Vec<Expr>, schema: &DFSchema) -> Result<Expr> {
    let catalog = super::selected_operator_table()?;
    let args = catalog.prepare_args(name, FunctionKind::Scalar, args, schema)?;
    Ok(ScalarUDF::new_from_impl(bind(
        catalog.as_ref(),
        name,
        FunctionKind::Scalar,
        &args,
        schema,
    )?)
    .call(args))
}

pub fn native_aggregate(
    name: &str,
    args: Vec<Expr>,
    schema: &DFSchema,
    distinct: bool,
) -> Result<Expr> {
    let catalog = super::selected_operator_table()?;
    let args = catalog.prepare_args(name, FunctionKind::Aggregate, args, schema)?;
    let function = AggregateUDF::new_from_impl(bind(
        catalog.as_ref(),
        name,
        FunctionKind::Aggregate,
        &args,
        schema,
    )?);
    let call = Arc::new(function).call(args);
    if distinct {
        call.distinct().build()
    } else {
        Ok(call)
    }
}
