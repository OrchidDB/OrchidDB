//! Native list-lambda emission. Typed bodies stay in compiler metadata, and
//! captured SQL columns remain ordinary function arguments for demand analysis.
use super::super::collection_lambda::{self, Lambda};
use super::*;
use datafusion::sql::sqlparser::ast;
use std::{cell::RefCell, collections::BTreeMap, ops::ControlFlow};
thread_local! {static LAMBDAS:RefCell<BTreeMap<String,Lambda>>=RefCell::new(BTreeMap::new());}
fn collect(expr: &Expr, mappings: &mut BTreeMap<String, Lambda>) -> datafusion::common::Result<()> {
    expr.apply(|expr| {
        if let Expr::ScalarFunction(call) = expr
            && let Some(lambda) = collection_lambda::definition(&call.func)
        {
            mappings.insert(lambda.name.clone(), lambda.clone());
            collect(&lambda.body, mappings)?;
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    Ok(())
}
pub(super) fn with_plan<T>(plan: &LogicalPlan, f: impl FnOnce() -> SqlResult<T>) -> SqlResult<T> {
    let mut mappings = BTreeMap::new();
    plan.apply_with_subqueries(|node| {
        for expr in node.expressions() {
            collect(&expr, &mut mappings)?;
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    struct Restore(Option<BTreeMap<String, Lambda>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            LAMBDAS.with(|m| *m.borrow_mut() = self.0.take().unwrap());
        }
    }
    let _restore = Restore(Some(LAMBDAS.with(|m| m.replace(mappings))));
    f()
}
pub(super) fn adapt(expr: &mut ast::Expr, dialect: SqlDialect) -> SqlResult<bool> {
    let ast::Expr::Function(function) = expr else {
        return Ok(false);
    };
    let Some(lambda) = LAMBDAS.with(|m| {
        m.borrow()
            .get(function.name.to_string().trim_matches('"'))
            .cloned()
    }) else {
        return Ok(false);
    };
    if dialect != SqlDialect::DuckDb {
        return Err(SqlError::Unsupported(
            "Typed list lambdas require DuckDB".into(),
        ));
    }
    let ast::FunctionArguments::List(arguments) = &function.args else {
        return Err(SqlError::Unsupported("Invalid lambda arguments".into()));
    };
    let args = arguments
        .args
        .iter()
        .map(|arg| match arg {
            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(expr)) => Ok(expr.clone()),
            _ => Err(SqlError::Unsupported("Invalid lambda capture".into())),
        })
        .collect::<SqlResult<Vec<_>>>()?;
    let encoded = super::unparse::encode_expression_literals(lambda.body, dialect)?.data;
    let unparser_dialect = dialect.unparser_dialect();
    let mut body = datafusion::sql::unparser::Unparser::new(unparser_dialect.as_ref())
        .expr_to_sql(&encoded)?;
    super::functions::prepare_ast(&mut body, dialect)?;
    if let ControlFlow::Break(error) = ast::visit_expressions_mut(&mut body, |expr| {
        if let ast::Expr::Identifier(name) = expr
            && let Some(index) = lambda
                .captures
                .iter()
                .position(|capture| capture == &name.value)
        {
            let Some(value) = args.get(index + 1) else {
                return ControlFlow::Break(SqlError::Unsupported("Missing lambda capture".into()));
            };
            *expr = value.clone();
        }
        ControlFlow::Continue(())
    }) {
        return Err(error);
    }
    let method = if lambda.kind == "filter" {
        "list_filter"
    } else {
        "list_transform"
    };
    let body = if lambda.kind == "count" {
        super::functions::template("CASE WHEN __arg0 THEN 1 ELSE 0 END", &[body])?
    } else { body };
    let mapped = super::functions::template(
        &format!("{method}(__arg0, lambda \"{}\": __arg1)", lambda.parameter),
        &[args[0].clone(), body],
    )?;
    *expr = match lambda.kind.as_str() {
        "transform" | "filter" => mapped,
        kind => {
            let has_null = "len(__local0) > list_count(__local0)";
            let any = format!(
                "CASE WHEN __local0 IS NULL THEN NULL WHEN list_contains(__local0,true) THEN true WHEN {has_null} THEN NULL ELSE false END"
            );
            let body = match kind {
                "count" => "CASE WHEN __local0 IS NULL THEN NULL ELSE CAST(coalesce(list_sum(__local0),0) AS BIGINT) END".into(),
                "any" => any,
                "none" => format!("NOT ({any})"),
                "all" => format!(
                    "CASE WHEN __local0 IS NULL THEN NULL WHEN list_contains(__local0,false) THEN false WHEN {has_null} THEN NULL ELSE true END"
                ),
                "single" => format!(
                    "CASE WHEN __local0 IS NULL THEN NULL WHEN len(list_filter(__local0, lambda __local1: __local1)) > 1 THEN false WHEN {has_null} THEN NULL ELSE len(list_filter(__local0, lambda __local1: __local1)) = 1 END"
                ),
                _ => return Err(SqlError::Unsupported("Unknown list lambda".into())),
            };
            super::functions::template(
                &format!("list_extract(list_transform([__arg0], lambda __local0: {body}),1)"),
                &[mapped],
            )?
        }
    };
    Ok(true)
}
