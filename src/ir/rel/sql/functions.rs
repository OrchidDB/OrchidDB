//! Target-engine expression adaptation, analogous to Calcite's dialect
//! `unparseCall`/`prepareUnparse` boundary. Operates on syntax nodes, never SQL
//! substrings: a function spelling inside data or an identifier is untouched.
use std::ops::ControlFlow;

use datafusion::common::DFSchema;
use datafusion::logical_expr::Expr;
use datafusion::sql::sqlparser::{ast, dialect::DuckDbDialect, parser::Parser};
use datafusion::sql::unparser::Unparser;

use super::{SqlDialect, SqlError, SqlResult};

/// Render a scalar expression using the same rules as complete queries. The
/// catalog binder uses this after replacing column references by typed NULLs.
pub(crate) fn expression_sql(expr: &Expr, _schema: &DFSchema) -> SqlResult<String> {
    let dialect = SqlDialect::DuckDb.unparser_dialect();
    let encoded =
        super::unparse::encode_expression_literals(expr.clone(), SqlDialect::DuckDb)?.data;
    let mut expression = Unparser::new(dialect.as_ref()).expr_to_sql(&encoded)?;
    prepare_ast(&mut expression, SqlDialect::DuckDb)?;
    Ok(expression.to_string())
}

pub(super) fn prepare_ast<T: ast::VisitMut>(tree: &mut T, dialect: SqlDialect) -> SqlResult<()> {
    struct UnitProjection;
    impl ast::VisitorMut for UnitProjection {
        type Break = SqlError;
        fn post_visit_query(&mut self, query: &mut ast::Query) -> ControlFlow<Self::Break> {
            if let ast::SetExpr::Select(select) = query.body.as_mut() {
                if select.projection.is_empty() {
                    // DataFusion can omit the projection around a limited join.
                    // Retain its input columns; only a FROM-less empty tuple
                    // needs a unit column to preserve cardinality.
                    if select.from.is_empty() {
                        select.projection.push(ast::SelectItem::ExprWithAlias {
                            expr: ast::Expr::Value(ast::Value::Number("1".into(), false).into()),
                            alias: ast::Ident::new("__orchiddb_unit"),
                        });
                    } else {
                        select.projection.push(ast::SelectItem::Wildcard(Default::default()));
                    }
                }
            }
            ControlFlow::Continue(())
        }
    }
    if let ControlFlow::Break(error) = tree.visit(&mut UnitProjection) { return Err(error); }
    match ast::visit_expressions_mut(tree, |expr| match adapt_expression(expr, dialect) {
        Ok(()) => ControlFlow::Continue(()),
        Err(error) => ControlFlow::Break(error),
    }) {
        ControlFlow::Continue(()) => Ok(()),
        ControlFlow::Break(error) => Err(error),
    }
}

fn adapt_expression(expr: &mut ast::Expr, dialect: SqlDialect) -> SqlResult<()> {
    if let ast::Expr::IsNull(operand) | ast::Expr::IsNotNull(operand)
        | ast::Expr::IsTrue(operand) | ast::Expr::IsFalse(operand)
        | ast::Expr::IsNotTrue(operand) | ast::Expr::IsNotFalse(operand) = expr {
        if matches!(operand.as_ref(), ast::Expr::UnaryOp { .. } | ast::Expr::BinaryOp { .. }) {
            **operand = ast::Expr::Nested(Box::new(operand.as_ref().clone()));
        }
    }
    // SQL postfix predicates bind differently from comparisons. The upstream
    // unparser omits these parentheses, changing `(a IS NULL) = (b IS NULL)`
    // into `(a IS NULL = b) IS NULL` when the target parses it again.
    if let ast::Expr::BinaryOp { left, right, .. } = expr {
        for operand in [left, right] {
            if matches!(operand.as_ref(), ast::Expr::IsNull(_) | ast::Expr::IsNotNull(_)
                | ast::Expr::IsTrue(_) | ast::Expr::IsFalse(_)
                | ast::Expr::IsNotTrue(_) | ast::Expr::IsNotFalse(_)) {
                **operand = ast::Expr::Nested(Box::new(operand.as_ref().clone()));
            }
        }
    }
    let ast::Expr::Function(function) = expr else {
        return Ok(());
    };
    let name = function.name.to_string();
    if name == crate::ir::functions::ENGINE_CAST_FUNCTION {
        if dialect != SqlDialect::DuckDb {
            return Err(SqlError::Unsupported(
                "declared DuckDB UDF argument cast on another dialect".into(),
            ));
        }
        let ast::FunctionArguments::List(arguments) = &function.args else {
            return Err(SqlError::Unsupported(
                "invalid declared UDF argument cast".into(),
            ));
        };
        let [
            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(value)),
            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(ast::Expr::Value(type_value))),
        ] = arguments.args.as_slice()
        else {
            return Err(SqlError::Unsupported(
                "invalid declared UDF argument cast".into(),
            ));
        };
        let ast::Value::SingleQuotedString(sql_type) = &type_value.value else {
            return Err(SqlError::Unsupported(
                "invalid declared UDF argument type".into(),
            ));
        };
        *expr = template(&format!("CAST(__arg0 AS {sql_type})"), &[value.clone()])?;
        return Ok(());
    }
    if let Some(native) = name.strip_prefix(crate::ir::functions::ENGINE_FUNCTION_PREFIX) {
        if dialect != SqlDialect::DuckDb {
            return Err(SqlError::Unsupported(format!(
                "DuckDB native function {native} on {}",
                dialect.name()
            )));
        }
        let components = native.split('.').collect::<Vec<_>>();
        if components.iter().any(|part| part.is_empty()) {
            return Err(SqlError::Unsupported(format!(
                "invalid engine function path {native:?}"
            )));
        }
        function.name = ast::ObjectName::from(
            components
                .into_iter()
                .map(|part| ast::Ident::with_quote('"', part))
                .collect::<Vec<_>>(),
        );
        return Ok(());
    }
    let ast::FunctionArguments::List(arguments) = &function.args else {
        return Ok(());
    };
    let args: Option<Vec<_>> = arguments
        .args
        .iter()
        .map(|arg| match arg {
            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(expr)) => Some(expr.clone()),
            _ => None,
        })
        .collect();
    let Some(args) = args else {
        return Ok(());
    };
    if dialect != SqlDialect::DuckDb {
        if name.starts_with("__orchiddb_utf16_") {
            return Err(SqlError::Unsupported("UTF-16 string SQL is currently implemented for DuckDB".into()));
        }
        if name == "array_has" && args.len() == 2 {
            *expr = template("array_position(__arg0, __arg1) IS NOT NULL", &args)?;
        }
        return Ok(());
    }
    match (name.as_str(), args.len()) {
        ("__orchiddb_utf16_length", 1) => {
            *expr = template(r"CAST(length(regexp_replace(__arg0, '[\x{10000}-\x{10FFFF}]', 'xx', 'g')) AS INTEGER)", &args)?;
        }
        ("__orchiddb_utf16_substring", 2 | 3) => {
            let mut args = args;
            if args.len() == 2 {
                args.push(ast::Expr::Value(ast::Value::Number("9223372036854775807".into(), false).into()));
            }
            *expr = template(r"
                (SELECT CASE WHEN __local0.s IS NULL THEN NULL ELSE COALESCE(
                    (SELECT string_agg(CASE WHEN p >= lo AND p + w <= hi THEN ch ELSE '�' END, '' ORDER BY p)
                     FROM (SELECT ch, w, sum(w) OVER (ORDER BY ord ROWS UNBOUNDED PRECEDING) - w AS p
                           FROM (SELECT ch, ord, CASE WHEN unicode(ch) > 65535 THEN 2 ELSE 1 END AS w
                                 FROM unnest(regexp_extract_all(__local0.s, '(?s).')) WITH ORDINALITY AS __local1(ch, ord)) AS __local2) AS __local3
                     WHERE hi > lo AND p < hi AND p + w > lo), '') END
                 FROM (SELECT s,
                         greatest(0, least(n, CASE WHEN a < 0 THEN n + a ELSE a END)) AS lo,
                         greatest(0, least(n, CASE WHEN b < 0 THEN n + b ELSE b END)) AS hi
                       FROM (SELECT s, a, b, length(regexp_replace(s, '[\x{10000}-\x{10FFFF}]', 'xx', 'g')) AS n
                             FROM (SELECT __arg0 AS s, coalesce(__arg1, 0) AS a, coalesce(__arg2, 9223372036854775807) AS b) AS __local2) AS __local1) AS __local0)", &args)?;
        }
        ("array_min", 1) => rename(function, "list_min"),
        ("array_max", 1) => rename(function, "list_max"),
        ("regexp_like", 2 | 3) => rename(function, "regexp_matches"),
        ("nanvl", 2) => {
            *expr = template(
                "list_extract(list_transform([struct_pack(lhs := __arg0, rhs := __arg1)], lambda __local0: CASE WHEN isnan(__local0.lhs) THEN __local0.rhs ELSE __local0.lhs END), 1)",
                &args,
            )?
        }
        ("log", 2) => *expr = template("ln(__arg1) / ln(__arg0)", &args)?,
        // DuckDB sign(NaN)=0, whereas DataFusion preserves NaN. Bind the
        // argument once so volatile calls keep their evaluation count.
        ("signum", 1) => {
            *expr = template(
                "list_extract(list_transform([__arg0], lambda __local0: CASE WHEN isnan(__local0) THEN __local0 ELSE sign(__local0) END), 1)",
                &args,
            )?
        }
        ("trunc", 2) => {
            *expr = template(
                "list_extract(list_transform([power(10.0, __arg1)], lambda __local0: trunc(__arg0 * __local0) / __local0), 1)",
                &args,
            )?
        }
        ("array_distinct", 1) => {
            *expr = template(
                "list_extract(list_transform([__arg0], lambda __local0: list_filter(__local0, lambda __local1, __local2: list_position(__local0, __local1) = __local2)), 1)",
                &args,
            )?
        }
        // DuckDB has no list_replace. Null matches are intentional, as in
        // DataFusion's compare_element_to_list(..., true).
        ("array_replace_all", 3) => {
            *expr = template(
                "list_extract(list_transform([struct_pack(items := __arg0, old := __arg1, new := __arg2)], lambda __local0: list_transform(__local0.items, lambda __local1: CASE WHEN __local1 IS NOT DISTINCT FROM __local0.old THEN __local0.new ELSE __local1 END)), 1)",
                &args,
            )?
        }
        // list_intersect loses NULL elements and reverses order. DataFusion
        // probes the longer list, preserves its order, and includes NULL.
        ("array_intersect", 2) => {
            *expr = template(
                "list_extract(list_transform([struct_pack(lhs := __arg0, rhs := __arg1)], lambda __local0: CASE WHEN __local0.lhs IS NULL OR __local0.rhs IS NULL THEN NULL ELSE list_extract(list_transform([struct_pack(probe := CASE WHEN len(__local0.lhs) < len(__local0.rhs) THEN __local0.rhs ELSE __local0.lhs END, lookup := CASE WHEN len(__local0.lhs) < len(__local0.rhs) THEN __local0.lhs ELSE __local0.rhs END)], lambda __local1: list_filter(__local1.probe, lambda __local2, __local3: list_position(__local1.probe, __local2) = __local3 AND list_position(__local1.lookup, __local2) IS NOT NULL)), 1) END), 1)",
                &args,
            )?
        }
        _ => {}
    }
    Ok(())
}

fn rename(function: &mut ast::Function, name: &str) {
    function.name = ast::ObjectName::from(vec![ast::Ident::new(name)]);
}

/// Parse only trusted rule templates, then substitute argument ASTs. Fresh
/// lambda names prevent capturing outer columns or nested lambdas.
fn template(source: &str, args: &[ast::Expr]) -> SqlResult<ast::Expr> {
    let rendered = args
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let mut source = source.to_owned();
    for local in 0..4 {
        let mut suffix = 0;
        let fresh = loop {
            let name = format!("__graph_dialect_{local}_{suffix}");
            if !rendered.contains(&name) {
                break name;
            }
            suffix += 1;
        };
        source = source.replace(&format!("__local{local}"), &fresh);
    }
    let mut parser = Parser::new(&DuckDbDialect {})
        .try_with_sql(&source)
        .map_err(|err| SqlError::Unsupported(format!("dialect expression template: {err}")))?;
    let mut expression = parser
        .parse_expr()
        .map_err(|err| SqlError::Unsupported(format!("dialect expression template: {err}")))?;
    let _: ControlFlow<()> = ast::visit_expressions_mut(&mut expression, |expr| {
        if let ast::Expr::Identifier(ident) = expr
            && let Some(index) = ident
                .value
                .strip_prefix("__arg")
                .and_then(|s| s.parse::<usize>().ok())
            && let Some(value) = args.get(index)
        {
            *expr = ast::Expr::Nested(Box::new(value.clone()));
        }
        ControlFlow::Continue(())
    });
    Ok(ast::Expr::Nested(Box::new(expression)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rewrite(sql: &str, dialect: SqlDialect) -> String {
        let mut statements = Parser::parse_sql(&DuckDbDialect {}, sql).unwrap();
        prepare_ast(&mut statements[0], dialect).unwrap();
        statements[0].to_string()
    }

    #[test]
    fn function_adaptation_preserves_literals_identifiers_and_other_dialects() {
        let original = "SELECT array_min(xs), 'array_min(xs)', xs AS \"array_min(xs)\" FROM t";
        let duck = rewrite(original, SqlDialect::DuckDb);
        assert!(duck.contains("list_min(xs)"), "{duck}");
        assert!(duck.contains("'array_min(xs)'"), "{duck}");
        assert!(duck.contains("AS \"array_min(xs)\""), "{duck}");
        assert_eq!(rewrite(original, SqlDialect::Postgres), original);
    }

    #[test]
    fn native_calls_bypass_standard_rules_including_aggregates() {
        let sql = rewrite(
            "SELECT __engine_function_log(100), __engine_function_sum(x), log(2, 8) FROM t",
            SqlDialect::DuckDb,
        );
        assert!(sql.contains("\"log\"(100)"), "{sql}");
        assert!(sql.contains("\"sum\"(x)"), "{sql}");
        assert!(sql.contains("ln((8)) / ln((2))"), "{sql}");
        let mut statement =
            Parser::parse_sql(&DuckDbDialect {}, "SELECT __engine_function_log(100)")
                .unwrap()
                .remove(0);
        assert!(prepare_ast(&mut statement, SqlDialect::Postgres).is_err());
    }

    #[test]
    fn native_schema_paths_quote_components_and_keep_sql_data_inert() {
        let sql = rewrite(
            "SELECT __engine_function_app.score('x); DROP TABLE t; --')",
            SqlDialect::DuckDb,
        );
        assert_eq!(sql, "SELECT \"app\".\"score\"('x); DROP TABLE t; --')");
        let mut expression = Parser::new(&DuckDbDialect {})
            .try_with_sql("f('unchanged')")
            .unwrap()
            .parse_expr()
            .unwrap();
        let ast::Expr::Function(function) = &mut expression else {
            unreachable!()
        };
        function.name = ast::ObjectName::from(vec![ast::Ident::new(
            "__engine_function_app.score); DROP TABLE t; --",
        )]);
        prepare_ast(&mut expression, SqlDialect::DuckDb).unwrap();
        assert_eq!(
            expression.to_string(),
            "\"app\".\"score); DROP TABLE t; --\"('unchanged')"
        );
    }

    #[test]
    fn lambda_variables_do_not_capture_outer_columns() {
        let sql = rewrite(
            "SELECT array_replace_all(xs, __graph_dialect_0_0, __graph_dialect_1_0) FROM t",
            SqlDialect::DuckDb,
        );
        assert!(sql.contains("lambda __graph_dialect_0_1"), "{sql}");
        assert!(sql.contains("lambda __graph_dialect_1_1"), "{sql}");
        assert!(sql.contains("old := (__graph_dialect_0_0)"), "{sql}");
    }

    #[cfg(feature = "duckdb")]
    #[test]
    fn expression_binding_preserves_special_literals() {
        use datafusion::prelude::lit;
        let connection = duckdb::Connection::open_in_memory().unwrap();
        let schema = DFSchema::empty();
        for value in [
            "double \" quotes",
            "it''s",
            "a\\'b",
            "nul\0byte",
            "\0''\\'\"",
        ] {
            let sql = expression_sql(&lit(value), &schema).unwrap();
            let returned: String = connection
                .query_row(&format!("SELECT {sql}"), [], |row| row.get(0))
                .unwrap();
            assert_eq!(returned, value, "{sql}");
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let sql = expression_sql(&lit(value), &schema).unwrap();
            let returned: f64 = connection
                .query_row(&format!("SELECT {sql}"), [], |row| row.get(0))
                .unwrap();
            assert!(
                returned == value || returned.is_nan() && value.is_nan(),
                "{sql}: {returned}"
            );
        }
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let sql = expression_sql(&lit(value), &schema).unwrap();
            let returned: f32 = connection
                .query_row(&format!("SELECT {sql}"), [], |row| row.get(0))
                .unwrap();
            assert!(
                returned == value || returned.is_nan() && value.is_nan(),
                "{sql}: {returned}"
            );
        }
    }

    #[cfg(feature = "duckdb")]
    #[tokio::test]
    async fn standard_expressions_match_datafusion_results_in_duckdb() {
        use datafusion::common::ScalarValue;
        use datafusion::prelude::SessionContext;
        let ctx = SessionContext::new();
        let conn = duckdb::Connection::open_in_memory().unwrap();
        let queries = [
            "SELECT log(2.0, 8.0), log(100.0), trunc(-12.345, 2), trunc(123.4, -1), 2.0 / log(2.0, 8.0)",
            "SELECT signum(-0.0), signum(-2.0), signum(3.0), signum(CAST('NaN' AS DOUBLE)), signum(CAST(NULL AS DOUBLE))",
            "SELECT array_replace_all([1, 2, 1, NULL], 1, 9), array_replace_all([1, NULL, 2], CAST(NULL AS INT), 9)",
            "SELECT array_replace_all([1, 2, 1], 1, CAST(NULL AS INT)), array_replace_all(CAST(NULL AS INT[]), 1, 9)",
            "SELECT array_intersect([1, NULL, 2, 2], [NULL, 2]), array_intersect([2, 1], [1, 2, 3]), array_intersect(CAST(NULL AS INT[]), [1])",
            "SELECT array_distinct([2, NULL, 1, 2, NULL]), array_distinct(CAST(NULL AS INT[])), array_distinct(CAST([] AS INT[]))",
            "SELECT array_min([2, NULL, 1]), array_max([2, NULL, 1]), array_min(CAST([] AS INT[]))",
            "SELECT nanvl(CAST('NaN' AS DOUBLE), 9.0), nanvl(2.0, 9.0), nanvl(CAST(NULL AS DOUBLE), 9.0)",
            "SELECT regexp_like('alphabet', 'pha'), regexp_like('ABC', 'abc', 'i'), regexp_like(CAST(NULL AS VARCHAR), 'x')",
        ];
        for query in queries {
            let frame = ctx.sql(query).await.unwrap();
            let plan = frame.logical_plan();
            let dialect = SqlDialect::DuckDb.unparser_dialect();
            // DF53 cannot unparse explicit list casts. Exercise those
            // boundary cases directly through the identical SQL AST adapter.
            let mut statement = if query.contains("AS INT[]") {
                Parser::parse_sql(&DuckDbDialect {}, query)
                    .unwrap()
                    .remove(0)
            } else {
                Unparser::new(dialect.as_ref()).plan_to_sql(plan).unwrap()
            };
            prepare_ast(&mut statement, SqlDialect::DuckDb).unwrap();
            let sql = statement.to_string();
            let expected = frame.collect().await.unwrap();
            let mut prepared = conn
                .prepare(&sql)
                .unwrap_or_else(|err| panic!("{query}\n{sql}\n{err}"));
            let actual: Vec<_> = prepared.query_arrow([]).unwrap().collect();
            for col in 0..expected[0].num_columns() {
                let actual_column = arrow::compute::cast(
                    actual[0].column(col),
                    expected[0].column(col).data_type(),
                )
                .unwrap();
                let a = ScalarValue::try_from_array(&actual_column, 0).unwrap();
                let e = ScalarValue::try_from_array(expected[0].column(col), 0).unwrap();
                // SQL engines choose different integer widths for constants;
                // displayed scalar values compare values including list order.
                assert_eq!(a.to_string(), e.to_string(), "{query}\n{sql}\ncolumn {col}");
            }
        }
    }
}
