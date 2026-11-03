//! Request-scoped portable function adaptation, including order-key polarity.
use super::*;
use crate::ir::functions::logical::{SqlFunctionMapping, definition};
use std::{cell::RefCell, collections::BTreeMap};
thread_local! { static MAPPINGS: RefCell<BTreeMap<(String, usize), SqlFunctionMapping>> = RefCell::new(BTreeMap::new()); }
pub(super) fn mapping(name: &str, arity: usize) -> Option<SqlFunctionMapping> {
    MAPPINGS.with(|m| {
        m.borrow()
            .get(&(name.trim_matches('"').to_owned(), arity))
            .cloned()
    })
}
pub(super) fn with_plan<T>(
    plan: &LogicalPlan,
    dialect: SqlDialect,
    f: impl FnOnce() -> SqlResult<T>,
) -> SqlResult<T> {
    let mut mappings = BTreeMap::new();
    plan.apply_with_subqueries(|node| {
        let mut schema = datafusion::common::DFSchema::empty();
        for input in node.inputs() {
            schema.merge(input.schema());
        }
        schema.merge(node.schema());
        for expr in node.expressions() {
            expr.apply(|expr| {
                if let Expr::ScalarFunction(call) = expr {
                    if let Some(function) = definition(&call.func) {
                        // Relational lowering can consume a scoring expression
                        // entirely (for example, an ANN table function). Require
                        // scalar SQL support only if the expression survives in
                        // the emitted AST, where adapt_expression reports it.
                        let override_mapping = match dialect {
                            SqlDialect::Custom(adapter) => {
                                adapter.function_mapping(function.logical_name())
                            }
                            _ => None,
                        };
                        let Some(implementation) = (if override_mapping.is_some() {
                            override_mapping
                        } else {
                            function.sql_mapping_for_call(dialect.name(), &call.args, &schema)?
                        }) else {
                            return Ok(TreeNodeRecursion::Continue);
                        };
                        if let Some(previous) = mappings.insert(
                            (call.func.name().to_owned(), call.args.len()),
                            implementation.clone(),
                        ) {
                            if previous != implementation {
                                return Err(DataFusionError::Plan(
                                    "conflicting logical function definitions".into(),
                                ));
                            }
                        }
                    }
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    struct Restore(Option<BTreeMap<(String, usize), SqlFunctionMapping>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            MAPPINGS.with(|m| *m.borrow_mut() = self.0.take().unwrap());
        }
    }
    let _restore = Restore(Some(MAPPINGS.with(|m| m.replace(mappings))));
    f()
}

/// Lambda bodies are emitted after the enclosing plan has been prepared.
/// Include mappings introduced by literal encoding while retaining the outer
/// mapping scope for captured expressions and nested lambda calls.
pub(super) fn with_expression<T>(
    expr: &Expr, schema: &datafusion::common::DFSchema, dialect: SqlDialect,
    f: impl FnOnce() -> SqlResult<T>,
) -> SqlResult<T> {
    let mut mappings = MAPPINGS.with(|m| m.borrow().clone());
    expr.apply(|expr| {
        if let Expr::ScalarFunction(call) = expr
            && let Some(function) = definition(&call.func) {
            let override_mapping = match dialect {
                SqlDialect::Custom(adapter) => adapter.function_mapping(function.logical_name()),
                _ => None,
            };
            let implementation = match override_mapping {
                Some(value) => Some(value),
                None => function.sql_mapping_for_call(dialect.name(), &call.args, schema)?,
            };
            if let Some(implementation) = implementation {
                let key = (call.func.name().to_owned(), call.args.len());
                if let Some(previous) = mappings.insert(key, implementation.clone())
                    && previous != implementation {
                    return Err(DataFusionError::Plan("conflicting logical function definitions".into()));
                }
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    struct Restore(Option<BTreeMap<(String, usize), SqlFunctionMapping>>);
    impl Drop for Restore {
        fn drop(&mut self) { MAPPINGS.with(|m| *m.borrow_mut() = self.0.take().unwrap()); }
    }
    let _restore = Restore(Some(MAPPINGS.with(|m| m.replace(mappings))));
    f()
}

/// Adapt only bare logical scoring calls used as ordering keys. Arithmetic
/// around a score retains its ordinary ordering; no monotonicity is guessed.
pub(super) fn adapt_ordering<T: datafusion::sql::sqlparser::ast::VisitMut>(
    tree: &mut T,
    dialect: SqlDialect,
) -> SqlResult<()> {
    use datafusion::sql::sqlparser::ast;
    use std::ops::ControlFlow;
    fn order(items: &mut [ast::OrderByExpr], dialect: SqlDialect) -> SqlResult<()> {
        for item in items {
            let ast::Expr::Function(function) = &item.expr else {
                continue;
            };
            let ast::FunctionArguments::List(arguments) = &function.args else {
                continue;
            };
            let Some(implementation) = mapping(&function.name.to_string(), arguments.args.len())
            else {
                continue;
            };
            let Some(order) = implementation.ordering else {
                continue;
            };
            let args = arguments
                .args
                .iter()
                .map(|arg| match arg {
                    ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(e)) => Ok(e.clone()),
                    _ => Err(SqlError::Unsupported(
                        "invalid logical ordering arguments".into(),
                    )),
                })
                .collect::<SqlResult<Vec<_>>>()?;
            item.expr = super::functions::portable_template(&order.expression, &args, dialect)?;
            if order.reverse {
                item.options.asc = Some(!item.options.asc.unwrap_or(true));
            }
        }
        Ok(())
    }
    struct Adapt(SqlDialect);
    impl ast::VisitorMut for Adapt {
        type Break = SqlError;
        fn pre_visit_query(&mut self, query: &mut ast::Query) -> ControlFlow<SqlError> {
            if let Some(ast::OrderBy {
                kind: ast::OrderByKind::Expressions(items),
                ..
            }) = &mut query.order_by
            {
                if let Err(e) = order(items, self.0) {
                    return ControlFlow::Break(e);
                }
            }
            ControlFlow::Continue(())
        }
        fn pre_visit_expr(&mut self, expr: &mut ast::Expr) -> ControlFlow<SqlError> {
            if let ast::Expr::Function(f) = expr {
                if let Some(ast::WindowType::WindowSpec(spec)) = &mut f.over {
                    if let Err(e) = order(&mut spec.order_by, self.0) {
                        return ControlFlow::Break(e);
                    }
                }
            }
            ControlFlow::Continue(())
        }
    }
    if let ControlFlow::Break(e) = tree.visit(&mut Adapt(dialect)) {
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod expression_scope_tests {
    use super::*;

    fn list_literal(value: i64) -> (Expr, String) {
        let scalar = ScalarValue::List(ScalarValue::new_list_nullable(
            &[ScalarValue::Int64(Some(value))], &DataType::Int64,
        ));
        let expr = super::super::structured::literal_expr(&scalar, SqlDialect::DuckDb).unwrap();
        let Expr::ScalarFunction(call) = &expr else { panic!("logical literal expected") };
        let name = call.func.name().to_owned();
        (expr, name)
    }

    #[test]
    fn nested_lambda_literal_mappings_restore_outer_scope() {
        let schema = datafusion::common::DFSchema::empty();
        let (outer, outer_name) = list_literal(71);
        let (inner, inner_name) = list_literal(93);
        assert!(mapping(&outer_name, 0).is_none());
        with_expression(&outer, &schema, SqlDialect::DuckDb, || {
            let expected = mapping(&outer_name, 0).unwrap();
            with_expression(&inner, &schema, SqlDialect::DuckDb, || {
                assert_eq!(mapping(&outer_name, 0), Some(expected.clone()));
                assert!(mapping(&inner_name, 0).is_some());
                Ok(())
            })?;
            assert_eq!(mapping(&outer_name, 0), Some(expected));
            assert!(mapping(&inner_name, 0).is_none());
            Ok(())
        }).unwrap();
        assert!(mapping(&outer_name, 0).is_none());
    }

    #[test]
    fn failed_lambda_emission_does_not_leak_literal_mappings() {
        let schema = datafusion::common::DFSchema::empty();
        let (expr, name) = list_literal(59);
        let error = with_expression(&expr, &schema, SqlDialect::DuckDb, || {
            assert!(mapping(&name, 0).is_some());
            Err::<(), _>(SqlError::Unsupported("failed body".into()))
        }).unwrap_err();
        assert!(error.to_string().contains("failed body"));
        assert!(mapping(&name, 0).is_none());
    }
}
