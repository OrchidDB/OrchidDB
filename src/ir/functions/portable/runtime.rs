//! Evaluate residual graph calls using the same typed portable UDFs as plans.
use crate::ir::value::Value;
use datafusion::{
    common::{DFSchema, DataFusionError, Result, ScalarValue},
    logical_expr::{lit, simplify::SimplifyContext},
    optimizer::simplify_expressions::ExprSimplifier,
    prelude::SessionContext,
};

pub(crate) fn call(name: &str, args: &[Value]) -> Result<Value> {
    let function = super::function(name)
        .ok_or_else(|| DataFusionError::Plan(format!("unknown portable function {name}")))?;
    let batch = crate::ir::rel::host::function_arguments(args.len(), &[args.to_vec()])
        .map_err(DataFusionError::Execution)?;
    // Preserve literals for functions whose result type depends on an argument,
    // and let the shared planner apply each UDF's ordinary argument coercions.
    let arguments = batch.columns().iter()
        .map(|column| ScalarValue::try_from_array(column.as_ref(), 0).map(lit))
        .collect::<Result<Vec<_>>>()?;
    let context = SessionContext::new();
    let schema = DFSchema::empty();
    let simplifier = ExprSimplifier::new(SimplifyContext::default()
        .with_schema(std::sync::Arc::new(schema.clone()))
        .with_query_execution_start_time(context.state().execution_props().query_execution_start_time));
    // Conditional and query-time functions require their normal planner rewrites
    // (for example, coalesce becomes CASE) before physical evaluation.
    let call = simplifier.coerce(function.call(arguments), &schema)?;
    let expression = context.create_physical_expr(simplifier.simplify(call)?, &schema)?;
    let output = expression.evaluate(&batch)?.into_array(1)?;
    let scalar = ScalarValue::try_from_array(output.as_ref(), 0)?;
    Ok(crate::ir::value::scalar_semantic_value(&scalar).unwrap_or(Value::Scalar(scalar)))
}
