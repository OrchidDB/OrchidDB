//! Render unevaluated calls for the owning DuckDB binder.
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::common::{Column, DFSchema, DataFusionError, Result};
use datafusion::logical_expr::{Expr, ExprSchemable};
use std::collections::BTreeMap;
fn error(e: impl std::fmt::Display) -> DataFusionError {
    DataFusionError::Plan(e.to_string())
}
pub fn call_sql(name: &str, args: &[Expr], schema: &DFSchema) -> Result<String> {
    let arguments = args
        .iter()
        .map(|arg| {
            // Keep actual literals: functions such as date_part and quantile
            // need their constant arguments during binding. Replace only row
            // references with typed NULLs; user functions are never evaluated.
            let mut placeholders = BTreeMap::new();
            let expr = arg
                .clone()
                .transform_up(|expr| {
                    if let Expr::Column(_) = &expr {
                        let data_type = expr.get_type(schema)?;
                        let marker = format!("__graph_bind_column_{}", placeholders.len());
                        let sql = super::types::typed_null(&data_type)?;
                        placeholders.insert(marker.clone(), sql);
                        return Ok(Transformed::yes(Expr::Column(Column::new_unqualified(
                            marker,
                        ))));
                    }
                    Ok(Transformed::no(expr))
                })?
                .data;
            let rendered = crate::ir::rel::sql::expression_sql(&expr, schema).map_err(error)?;
            use datafusion::sql::sqlparser::{ast, dialect::DuckDbDialect, parser::Parser};
            let mut sql_expr = Parser::new(&DuckDbDialect {})
                .try_with_sql(&rendered)
                .map_err(error)?
                .parse_expr()
                .map_err(error)?;
            let result = ast::visit_expressions_mut(&mut sql_expr, |expr| {
                if let ast::Expr::Identifier(ident) = expr {
                    if let Some(sql) = placeholders.get(&ident.value) {
                        match Parser::new(&DuckDbDialect {})
                            .try_with_sql(sql)
                            .and_then(|mut parser| parser.parse_expr())
                        {
                            Ok(replacement) => *expr = replacement,
                            Err(err) => return std::ops::ControlFlow::Break(error(err)),
                        }
                    }
                }
                std::ops::ControlFlow::Continue(())
            });
            if let std::ops::ControlFlow::Break(err) = result {
                return Err(err);
            }
            Ok(sql_expr.to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let name = name
        .to_ascii_lowercase()
        .split('.')
        .map(|part| format!("\"{}\"", part.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(".");
    Ok(format!("{name}({})", arguments.join(", ")))
}
