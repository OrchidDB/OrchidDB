//! Shape-specific adapter requirements shared by placement and final emission.
use super::*;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubqueryKind {
    Scalar,
    Exists,
    In,
    SetComparison,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubqueryRequirements {
    pub kind: SubqueryKind,
    pub correlated: bool,
    pub unnest: bool,
    pub aggregate: bool,
    pub window: bool,
    pub correlated_limit: bool,
    pub output_types: Vec<DataType>,
}
impl SubqueryRequirements {
    pub(crate) fn from_expr(expr: &Expr) -> Option<Self> {
        let (kind, subquery) = match expr {
            Expr::ScalarSubquery(s) => (SubqueryKind::Scalar, s),
            Expr::Exists(s) => (SubqueryKind::Exists, &s.subquery),
            Expr::InSubquery(s) => (SubqueryKind::In, &s.subquery),
            Expr::SetComparison(s) => (SubqueryKind::SetComparison, &s.subquery),
            _ => return None,
        };
        let mut result = Self {
            kind,
            correlated: !subquery.outer_ref_columns.is_empty(),
            unnest: false,
            aggregate: false,
            window: false,
            correlated_limit: false,
            output_types: subquery
                .subquery
                .schema()
                .fields()
                .iter()
                .map(|f| f.data_type().clone())
                .collect(),
        };
        let _ = subquery.subquery.apply(|node| {
            result.unnest |= matches!(node, LogicalPlan::Unnest(_));
            result.aggregate |= matches!(node, LogicalPlan::Aggregate(_));
            result.window |= matches!(node, LogicalPlan::Window(_));
            if matches!(node, LogicalPlan::Limit(_)) {
                for expr in node.expressions() {
                    expr.apply(|expr| {
                        result.correlated_limit |= matches!(expr, Expr::OuterReferenceColumn(..));
                        Ok(TreeNodeRecursion::Continue)
                    })?;
                }
            }
            Ok(TreeNodeRecursion::Continue)
        });
        Some(result)
    }
}
impl SqlDialect {
    pub(crate) fn check_subquery(
        self,
        requirements: &SubqueryRequirements,
    ) -> Result<(), &'static str> {
        match self {
            Self::Custom(adapter) => adapter.check_subquery(requirements),
            Self::DuckDb | Self::Postgres => {
                if requirements.correlated_limit {
                    return Err("engine requires a rewrite for outer references in LIMIT/OFFSET");
                }
                if matches!(
                    requirements.kind,
                    SubqueryKind::Scalar | SubqueryKind::In | SubqueryKind::SetComparison
                ) && requirements.output_types.len() != 1
                {
                    return Err("scalar or membership subquery requires one output column");
                }
                // The engine's typed expression/codec checks remain authoritative
                // for individual calls inside this supported relational shape.
                Ok(())
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/extracted/ir__rel__sql__capabilities__tests.rs"]
mod tests;
