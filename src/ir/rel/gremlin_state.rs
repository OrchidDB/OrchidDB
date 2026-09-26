//! Scalar sack operations and seeded side-effect reductions.
use super::*;
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
use std::any::Any;

impl LoweringContext<'_> {
    /// A terminal cap can be reduced in SQL without replaying its input.
    /// Other side-effect placements still require the relational state kernel.
    pub(super) fn lower_terminal_cap(&mut self, labels: &[String], mut node: &Node) -> RelResult<LoweredNode> {
        let [wanted] = labels else { return Err(RelError::Unsupported("multi-label SQL cap".into())); };
        while let Node::GraphSideEffect { reducer, input, .. } = node {
            if reducer != "register" { break; }
            node = input;
        }
        let Node::GraphSideEffect { label, value_input, value, seed, reducer, input, .. } = node else {
            return Err(RelError::Unsupported("SQL cap requires a direct reducer".into()));
        };
        if label != wanted { return Err(RelError::Unsupported("SQL cap has intervening side effects".into())); }
        let source = self.lower_node(input)?;
        let mut values = self.lower_with_correlate(source.plan, value_input)?;
        values.islands.merge(source.islands);
        let arg = self.lower_expr(&values.plan, value)?;
        if reducer == "collect" && matches!(seed, Value::BulkSet(items) if items.is_empty()) {
            // ReturnedBatches expands a terminal BulkSet into its members.
            let plan = LogicalPlanBuilder::from(values.plan.clone()).project(vec![arg.alias("current")])?.build()?;
            return Ok(values.with_plan(plan));
        }
        let kind = arg.get_type(values.plan.schema())?;
        if !kind.is_numeric() { return Err(RelError::Unsupported("SQL side-effect reducer requires numeric values".into())); }
        let seed_kind = infer_property_data_type(&[seed.clone()]);
        let seed_value = lit(value_to_scalar(seed, &seed_kind, self.language)?);
        let aggregate = match reducer.as_str() {
            "sum" => df_sum(arg), "min" => df_min(arg), "max" => df_max(arg),
            _ => return Err(RelError::Unsupported(format!("SQL cap reducer {reducer}"))),
        };
        let reduced = LogicalPlanBuilder::from(values.plan.clone()).aggregate(Vec::<Expr>::new(), vec![aggregate.alias("__reduced")])?.build()?;
        let combined = sack_binary(seed_value.clone(), col_exact("__reduced"), reducer)?;
        let result = df_core::coalesce(vec![combined, seed_value]);
        // Gremlin's reducer retains the numeric seed type when all inputs have
        // that type; DuckDB SUM otherwise widens an integer to HUGEINT.
        let result = if seed_kind == kind { Expr::Cast(datafusion::logical_expr::expr::Cast::new(Box::new(result), kind)) } else { result };
        let plan = LogicalPlanBuilder::from(reduced).project(vec![result.alias("current")])?.build()?;
        Ok(values.with_plan(plan))
    }

    pub(super) fn lower_gremlin_state_call(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        let Some(Value::String(op)) = constant_value_expr(&args[2])? else {
            return Err(RelError::Unsupported("dynamic sack operator".into()));
        };
        if name == "sack_apply" {
            let rhs = self.lower_expr(plan, &args[1])?;
            // Assignment does not require an initialized sack.
            if op == "assign" {
                return Ok(rhs);
            }
            let lhs = self.lower_expr(plan, &args[0])?;
            return sack_binary(lhs, rhs, &op);
        }
        let values = self.lower_native_list(plan, &args[0])?;
        let seed = self.lower_expr(plan, &args[1])?;
        let reduced = match op.as_str() {
            "sum" => ScalarUDF::from(ListSum {
                signature: Signature::any(1, Volatility::Immutable),
            })
            .call(vec![values]),
            "min" => datafusion::functions_nested::expr_fn::array_min(values),
            "max" => datafusion::functions_nested::expr_fn::array_max(values),
            _ => return Err(RelError::Unsupported(format!("fold reducer `{op}`"))),
        };
        // Empty input leaves the seed unchanged.
        let combined = sack_binary(seed.clone(), reduced, &op)?;
        Ok(datafusion::functions::core::expr_fn::coalesce(vec![
            combined, seed,
        ]))
    }
}

fn sack_binary(lhs: Expr, rhs: Expr, op: &str) -> RelResult<Expr> {
    use datafusion::functions::core::expr_fn as core;
    Ok(match op {
        "sum" => binary(lhs, BinaryOp::Add, rhs),
        "minus" => binary(lhs, BinaryOp::Sub, rhs),
        "mult" => binary(lhs, BinaryOp::Mul, rhs),
        "min" | "max" => {
            let missing = Expr::or(lhs.clone().is_null(), rhs.clone().is_null());
            let value = if op == "min" {
                core::least(vec![lhs, rhs])
            } else {
                core::greatest(vec![lhs, rhs])
            };
            super::varlen::case_when(missing, lit(ScalarValue::Null), value)
        }
        "and" => Expr::and(lhs, rhs),
        "or" => Expr::or(lhs, rhs),
        _ => return Err(RelError::Unsupported(format!("sack operator `{op}`"))),
    })
}

/// SQL-native reduction. DuckDB evaluates the list; host code does not
/// interpret the traversers. Decline direct DataFusion execution explicitly.
#[derive(Debug, PartialEq, Eq, Hash)]
struct ListSum {
    signature: Signature,
}
impl ScalarUDFImpl for ListSum {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        "list_sum"
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, args: &[DataType]) -> datafusion::common::Result<DataType> {
        match &args[0] {
            DataType::List(field)
            | DataType::LargeList(field)
            | DataType::FixedSizeList(field, _)
                if field.data_type().is_numeric() =>
            {
                Ok(field.data_type().clone())
            }
            other => Err(DataFusionError::Plan(format!(
                "list_sum requires a numeric list, got {other}"
            ))),
        }
    }
    fn invoke_with_args(&self, _: ScalarFunctionArgs) -> datafusion::common::Result<ColumnarValue> {
        Err(DataFusionError::NotImplemented(
            "list_sum requires the DuckDB SQL backend".into(),
        ))
    }
}
