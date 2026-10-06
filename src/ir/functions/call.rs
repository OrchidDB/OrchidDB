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
    DataFusionError::NotImplemented("engine function requires its SQL backend".into())
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
    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        super::host_execution::scalar(
            self.name
                .strip_prefix(ENGINE_FUNCTION_PREFIX)
                .unwrap_or(&self.name),
            args,
        )
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
    if !matches!(catalog.engine(), "duckdb" | "postgres") {
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
    if name == "json.literal" {
        let [Expr::Literal(datafusion::common::ScalarValue::Utf8(Some(text)), _)] = args.as_slice()
        else {
            return Err(DataFusionError::Plan(
                "JSON literal requires constant text".into(),
            ));
        };
        return Ok(datafusion::logical_expr::lit(super::domain::json_scalar(
            text,
        )?));
    }
    if let Some(function) = super::json::function(name) {
        return Ok(function.call(args));
    }
    if let Some(function) = super::search::function(name) {
        return Ok(function.call(args));
    }
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
    if let Some(function) = super::json::aggregate(name) {
        let call = function.call(args);
        return if distinct {
            call.distinct().build()
        } else {
            Ok(call)
        };
    }
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

/// SQL mapping expressions are parsed before argument types are known. This
/// lightweight UDF asks DuckDB to bind once DataFusion supplies fields/literals.
#[derive(Debug, PartialEq, Eq, Hash)]
struct CatalogCall {
    name: String,
    signature: Signature,
}
impl ScalarUDFImpl for CatalogCall {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn coerce_types(&self, types: &[DataType]) -> Result<Vec<DataType>> {
        Ok(types.to_vec())
    }
    fn return_type(&self, types: &[DataType]) -> Result<DataType> {
        let fields = types
            .iter()
            .map(|t| Arc::new(arrow::datatypes::Field::new("arg", t.clone(), true)))
            .collect::<Vec<_>>();
        self.return_field_from_args(datafusion::logical_expr::ReturnFieldArgs {
            arg_fields: &fields,
            scalar_arguments: &vec![None; types.len()],
        })
        .map(|f| f.data_type().clone())
    }
    fn return_field_from_args(
        &self,
        args: datafusion::logical_expr::ReturnFieldArgs,
    ) -> Result<arrow::datatypes::FieldRef> {
        let arguments = args
            .arg_fields
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let scalar = args
                    .scalar_arguments
                    .get(i)
                    .and_then(|v| *v)
                    .cloned()
                    .map(Ok)
                    .unwrap_or_else(|| datafusion::common::ScalarValue::try_from(f.data_type()))?;
                Ok(datafusion::logical_expr::lit(scalar))
            })
            .collect::<Result<Vec<_>>>()?;
        let catalog = super::selected_operator_table()?;
        let name = self.name.strip_prefix(ENGINE_FUNCTION_PREFIX).unwrap();
        let ty = catalog.bind(name, FunctionKind::Scalar, &arguments, &DFSchema::empty())?;
        Ok(Arc::new(arrow::datatypes::Field::new(&self.name, ty, true)))
    }
    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        super::host_execution::scalar(
            self.name.strip_prefix(ENGINE_FUNCTION_PREFIX).unwrap(),
            args,
        )
    }
}
pub fn catalog_scalar(name: &str) -> Option<Arc<ScalarUDF>> {
    if super::active_operator_table().is_some_and(|t| {
        t.overloads(name)
            .iter()
            .any(|f| matches!(f.kind, FunctionKind::Scalar | FunctionKind::Macro))
    }) {
        return Some(Arc::new(ScalarUDF::new_from_impl(CatalogCall {
            name: format!("{ENGINE_FUNCTION_PREFIX}{name}"),
            signature: Signature::user_defined(Volatility::Volatile),
        })));
    }
    // Standalone compiler callers without a host still need the upstream SQL
    // parser's intrinsic expressions. There is no Orchid inventory or mapping.
    datafusion::functions::all_default_functions()
        .into_iter()
        .chain(datafusion::functions_nested::all_default_nested_functions())
        .find(|f| f.name() == name || f.aliases().iter().any(|a| a == name))
}
