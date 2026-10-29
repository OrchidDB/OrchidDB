//! Native list-lambda emission. Typed bodies stay in compiler metadata, and
//! captured SQL columns remain ordinary function arguments for demand analysis.
//!
//! SQL audit [L14](../../../../tools/diagnostics/sql-lowering-audit.md#l14-native-sql-list-lambdas): DuckDB/Trino emit native lambdas; PostgreSQL uses ordered
//! UNNEST with materialized predicate evaluation. Eligibility and emission share
//! the same typed capability check; nested PostgreSQL arrays remain fenced.
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
/// The same implementation contract is consulted by eligibility and emission.
pub(super) fn capability(lambda: &Lambda, dialect: SqlDialect) -> Result<(), &'static str> {
    if !matches!(lambda.kind.as_str(), "filter" | "transform" | "any" | "all" | "none" | "single" | "count") {
        return Err("Unknown typed list reduction");
    }
    if dialect == SqlDialect::DuckDb || dialect.name() == "trino" { return Ok(()); }
    if dialect == SqlDialect::Postgres {
        // PostgreSQL UNNEST flattens multidimensional arrays. Nested Cypher
        // lists use a separate JSON representation, so this alternative is
        // only valid for one-dimensional native scalar arrays.
        let scalar = |ty: &DataType| matches!(ty, DataType::Null | DataType::Boolean
            | DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64
            | DataType::Float32 | DataType::Float64 | DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View);
        let flat = |ty: &DataType| matches!(ty, DataType::List(field) if scalar(field.data_type()));
        if flat(&lambda.source) && (scalar(&lambda.result) || flat(&lambda.result)) { return Ok(()); }
        return Err("PostgreSQL native lambdas require flat scalar arrays");
    }
    Err("Dialect has no typed list implementation")
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
    capability(&lambda, dialect).map_err(|why| SqlError::Unsupported(why.into()))?;
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
    if dialect == SqlDialect::Postgres {
        // Bind the collection once. MATERIALIZED also makes predicate
        // evaluation once per element explicit for throwing/volatile bodies.
        let prefix = format!("{}_sql", lambda.parameter);
        let source = format!("{prefix}_source");
        let mapped = format!("{prefix}_mapped");
        let order = format!("{prefix}_ordinal");
        let value = format!("{prefix}_value");
        let parameter = &lambda.parameter;
        let items = format!("unnest({source}.items) WITH ORDINALITY AS {prefix}(\"{parameter}\",{order})");
        let result = match lambda.kind.as_str() {
            "transform" => format!("ARRAY(SELECT __arg1 FROM {items} ORDER BY {order})"),
            "filter" => format!("ARRAY(SELECT \"{parameter}\" FROM {items} WHERE __arg1 ORDER BY {order})"),
            kind => {
                let yes = format!("count(*) FILTER (WHERE {value} IS TRUE)");
                let no = format!("count(*) FILTER (WHERE {value} IS FALSE)");
                let unknown = format!("count(*) FILTER (WHERE {value} IS NULL)");
                let reduction = match kind {
                    "count" => yes.clone(),
                    "any" => format!("CASE WHEN {yes}>0 THEN true WHEN {unknown}>0 THEN NULL ELSE false END"),
                    "none" => format!("CASE WHEN {yes}>0 THEN false WHEN {unknown}>0 THEN NULL ELSE true END"),
                    "all" => format!("CASE WHEN {no}>0 THEN false WHEN {unknown}>0 THEN NULL ELSE true END"),
                    "single" => format!("CASE WHEN {yes}>1 THEN false WHEN {unknown}>0 THEN NULL ELSE {yes}=1 END"),
                    _ => unreachable!(),
                };
                format!("(WITH {mapped} AS MATERIALIZED (SELECT __arg1 AS {value} FROM {items}) SELECT {reduction} FROM {mapped})")
            }
        };
        *expr = super::functions::portable_template(&format!(
            "(WITH {source} AS MATERIALIZED (SELECT __arg0 AS items) SELECT CASE WHEN {source}.items IS NULL THEN NULL ELSE {result} END FROM {source})"
        ), &[args[0].clone(), body], dialect)?;
        return Ok(true);
    }
    if dialect.name() == "trino" && matches!(lambda.kind.as_str(), "count" | "single") {
        // REDUCE binds the mapped predicate once, retaining both true and
        // unknown counts. SINGLE is false for >1 true even with null items.
        let state = format!("{}_state", lambda.parameter);
        let item = format!("{}_truth", lambda.parameter);
        let finish = if lambda.kind == "count" { format!("{state}.yes") } else {
            format!("CASE WHEN {state}.yes>1 THEN false WHEN {state}.unknowns>0 THEN NULL ELSE {state}.yes=1 END")
        };
        *expr = super::functions::portable_template(&format!(
            "reduce(transform(__arg0, \"{}\" -> __arg1), CAST(ROW(0,0) AS ROW(yes BIGINT, unknowns BIGINT)), ({state},{item}) -> CAST(ROW({state}.yes + CASE WHEN {item} THEN 1 ELSE 0 END, {state}.unknowns + CASE WHEN {item} IS NULL THEN 1 ELSE 0 END) AS ROW(yes BIGINT, unknowns BIGINT)), {state} -> {finish})", lambda.parameter
        ), &[args[0].clone(), body], dialect)?;
        return Ok(true);
    }
    if dialect.name() == "trino" {
        let method = match lambda.kind.as_str() {
            "filter" => "filter", "transform" => "transform",
            "any" => "any_match", "all" => "all_match", "none" => "none_match",
            _ => return Err(SqlError::Unsupported(format!("Trino list lambda {} is unsupported", lambda.kind))),
        };
        *expr = super::functions::portable_template(
            &format!("{method}(__arg0, \"{}\" -> __arg1)", lambda.parameter),
            &[args[0].clone(), body], dialect,
        )?;
        return Ok(true);
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
