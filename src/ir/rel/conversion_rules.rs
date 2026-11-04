//! Typed conversion implementation selection. Policy is chosen before expansion;
//! capabilities choose only between implementations of that selected contract.
//!
//! SQL audit [L10](../../../tools/diagnostics/sql-lowering-audit.md#l10-conversions-and-semantic-ownership): Semantic policy is chosen before helper availability. Neither folding
//! nor fallback may substitute another engine's conversion contract.
use super::*;
use crate::ir::rules::{
    RuleDescriptor, RuleKind, RuleSession, Stage, StringToFloatPolicy, Traversal,
};
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
use std::any::Any;

const STRING_TO_FLOAT: RuleDescriptor = RuleDescriptor {
    id: "semantic.cypher.string_to_float",
    version: 1,
    stage: Stage::SemanticResolve,
    kind: RuleKind::ImplementationLowering,
    traversal: Traversal::Once,
    required: true,
    required_facts: &[
        "bound string operand",
        "semantic profile",
        "session function capability",
    ],
    invalidated_facts: &["expression effects", "placement"],
};

/// The supported operation descriptor is deliberately narrow: nullable text in,
/// nullable double out, invalid syntax yields NULL, evaluation of the operand
/// remains observable. Native policy changes only parsing, never input effects.
const CONTRACT: &str =
    "cypher.string_to_float/v1; nullable string -> nullable double; invalid syntax -> null";

pub(crate) const NATIVE_FLOAT: &str = "__orchiddb_duckdb_string_to_float_v1";

/// Conservative recursive policy dependency used before *any* scalar folding or
/// native-value fallback, including folds of an enclosing CASE/list/comparison.
pub(crate) fn contains_float_conversion(expr: &IrExpr) -> bool {
    let mut found = false;
    crate::ir::rules::graph::visit_expr(expr, &Default::default(), &mut |expr, _scope| {
        found |= matches!(expr, IrExpr::Call { name, .. } if name == "cypher_convert.tofloat");
    });
    found
}

/// Engine ownership is metadata on the logical implementation, not volatility.
/// A scalar simplifier without that evaluator must leave the operation intact.
pub(crate) fn can_fold_plan(plan: &LogicalPlan) -> bool {
    use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
    let mut allowed = true;
    let _ = plan.apply_with_subqueries(|node| {
        for expr in node.expressions() {
            expr.apply(|expr| {
                if matches!(expr, Expr::ScalarFunction(call) if call.func.inner().as_any().is::<NativeFloat>()
                    || call.func.inner().as_any().downcast_ref::<crate::ir::functions::logical::LogicalFunction>()
                        .is_some_and(|f| f.source_evaluation_only)) {
                    allowed = false;
                    return Ok(TreeNodeRecursion::Stop);
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
            if !allowed { return Ok(TreeNodeRecursion::Stop); }
        }
        Ok(TreeNodeRecursion::Continue)
    });
    allowed
}

impl LoweringContext<'_> {
    pub(super) fn native_float_dependency(&self, expr: &IrExpr) -> bool {
        self.language == Language::Cypher
            && self.options.rule_context.profile.string_to_float
                == StringToFloatPolicy::DuckDbNative
            && contains_float_conversion(expr)
    }

    pub(super) fn select_string_to_float(
        &self,
        value: Expr,
        arg: &IrExpr,
        input_type: &DataType,
    ) -> RelResult<Expr> {
        let context = &self.options.rule_context;
        let typed_text = |value: Expr| {
            if input_type == &DataType::Utf8 {
                value
            } else {
                Expr::Cast(Cast::new(Box::new(value), DataType::Utf8))
            }
        };
        let mut session = RuleSession::new(context.clone());
        session
            .visit(STRING_TO_FLOAT)
            .map_err(RelError::RulePipeline)?;
        let implementation = match context.profile.string_to_float {
            StringToFloatPolicy::DuckDbNative => {
                "explicit DuckDB-native parser; owning engine required"
            }
            StringToFloatPolicy::Orchid if context.supports_float_helper() => {
                "conformant registered typed helper"
            }
            StringToFloatPolicy::Orchid => "helper unavailable or disabled; conformant guarded SQL",
        };
        let result = (|| match context.profile.string_to_float {
            StringToFloatPolicy::DuckDbNative => {
                if !context
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owner.name.eq_ignore_ascii_case("duckdb"))
                {
                    return Err(RelError::Unsupported(
                        "native string-to-float requires its owning DuckDB engine".into(),
                    ));
                }
                Ok(ScalarUDF::from(NativeFloat {
                    // Determinism and evaluation ownership are independent.
                    // Optimizer fold eligibility inspects this typed operation;
                    // only DuckDB may evaluate it, including constant inputs.
                    signature: Signature::exact(vec![DataType::Utf8], Volatility::Immutable),
                })
                .call(vec![typed_text(value)]))
            }
            StringToFloatPolicy::Orchid if context.supports_float_helper() => Ok(
                super::language_functions::cypher_to_float(typed_text(value)),
            ),
            StringToFloatPolicy::Orchid => {
                // Aggregate normalization may spell a mapped property as a
                // subscript. Once binding resolves it to a column, rereading
                // that value cannot repeat evaluation of the original syntax.
                if !matches!(value, Expr::Column(_) | Expr::Literal(..))
                    && !super::scalar_semantics::stable_scalar_input(arg) {
                    return Err(RelError::Unsupported("conformant string-to-float SQL requires a repeatable operand when a typed helper is unavailable".into()));
                }
                super::scalar_semantics::conformant_string_to_float(value)
            }
        })();
        match &result {
            Ok(_) => session.changed(STRING_TO_FLOAT, || format!("{CONTRACT}; {implementation}")),
            Err(error) => session.record(STRING_TO_FLOAT, "blocked", || error.to_string()),
        }
        result
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NativeFloat {
    signature: Signature,
}
impl ScalarUDFImpl for NativeFloat {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        NATIVE_FLOAT
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, _: &[DataType]) -> datafusion::common::Result<DataType> {
        Ok(DataType::Float64)
    }
    fn invoke_with_args(&self, _: ScalarFunctionArgs) -> datafusion::common::Result<ColumnarValue> {
        Err(DataFusionError::Execution("native string-to-float requires execution by its owning DuckDB engine; compatible host fallback is forbidden".into()))
    }
}

#[cfg(all(test, feature = "duckdb"))]
#[path = "../../../tests/unit/extracted/ir__rel__conversion_rules__tests.rs"]
mod tests;
