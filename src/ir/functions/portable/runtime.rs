//! Scalar boundary calls reuse typed function implementations, not a DataFusion
//! physical expression, SessionContext, TaskContext, or execution environment.
use crate::ir::value::Value;
use datafusion::{
    common::{DFSchema, DataFusionError, Result, ScalarValue},
    logical_expr::{
        ColumnarValue, Expr, ReturnFieldArgs, ScalarFunctionArgs, lit, simplify::SimplifyContext,
    },
    optimizer::simplify_expressions::ExprSimplifier,
};
use std::sync::Arc;

pub(crate) fn call(name: &str, args: &[Value]) -> Result<Value> {
    let function = super::function(name)
        .ok_or_else(|| DataFusionError::Plan(format!("unknown portable function {name}")))?;
    let batch = crate::ir::rel::host::function_arguments(args.len(), &[args.to_vec()])
        .map_err(DataFusionError::Execution)?;
    let arguments = batch
        .columns()
        .iter()
        .map(|column| ScalarValue::try_from_array(column.as_ref(), 0).map(lit))
        .collect::<Result<Vec<_>>>()?;
    let schema = DFSchema::empty();
    // Coercion and constant simplification are compiler operations. Conditional
    // calls with literal boundary inputs reduce here without an executable plan.
    let simplifier = ExprSimplifier::new(
        SimplifyContext::default()
            .with_schema(Arc::new(schema.clone()))
            .with_query_execution_start_time(Some(chrono::Utc::now())),
    );
    let expression = simplifier.coerce(function.call(arguments), &schema)?;
    let scalar = evaluate_scalar(simplifier.simplify(expression)?)?;
    Ok(crate::ir::value::scalar_semantic_value(&scalar).unwrap_or(Value::Scalar(scalar)))
}

fn evaluate_scalar(expression: Expr) -> Result<ScalarValue> {
    match expression {
        Expr::Literal(value, _) => Ok(value),
        Expr::Alias(alias) => evaluate_scalar(*alias.expr),
        Expr::Cast(cast) => evaluate_scalar(*cast.expr)?.cast_to(&cast.data_type),
        Expr::TryCast(cast) => evaluate_scalar(*cast.expr)?
            .cast_to(&cast.data_type)
            .or_else(|_| ScalarValue::try_from(&cast.data_type)),
        Expr::ScalarFunction(call) => {
            let values = call
                .args
                .into_iter()
                .map(evaluate_scalar)
                .collect::<Result<Vec<_>>>()?;
            let fields = values
                .iter()
                .map(|v| Arc::new(arrow::datatypes::Field::new("", v.data_type(), v.is_null())))
                .collect::<Vec<_>>();
            let scalar_arguments = values.iter().map(Some).collect::<Vec<_>>();
            let return_field = call.func.return_field_from_args(ReturnFieldArgs {
                arg_fields: &fields,
                scalar_arguments: &scalar_arguments,
            })?;
            let result = call.func.invoke_with_args(ScalarFunctionArgs {
                args: values.into_iter().map(ColumnarValue::Scalar).collect(),
                arg_fields: fields,
                number_rows: 1,
                return_field,
                config_options: Default::default(),
            })?;
            match result {
                ColumnarValue::Scalar(value) => Ok(value),
                ColumnarValue::Array(array) => ScalarValue::try_from_array(array.as_ref(), 0),
            }
        }
        other => Err(DataFusionError::Plan(format!(
            "portable boundary expression requires SQL lowering: {other}"
        ))),
    }
}
