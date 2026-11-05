
    use super::*;

    #[test]
    fn postgres_empty_arrays_use_the_declared_outer_type() {
        for value in ["ARRAY[]", "(ARRAY[])", "((ARRAY[]))", "(ARRAY[NULL,NULL])", "(CAST('{}' AS INTEGER[]))", "(CAST('{NULL,NULL}' AS INTEGER[]))"] {
            let sql=rewrite(&format!("SELECT CAST({value} AS JSONB[])"),SqlDialect::Postgres);
            assert!(sql.contains("AS JSONB[]"),"{sql}");
            assert!(!sql.contains("INTEGER[]"),"{sql}");
            assert_eq!(sql.matches("CAST(").count(),1,"{sql}");
        }
    }

    #[test]
    fn set_operation_arms_repair_derived_aliases_in_separate_scopes() {
        let arm="SELECT x,y FROM (SELECT 1 AS x) derived_projection JOIN (SELECT 2 AS y) derived_projection ON x<y";
        for operator in ["UNION", "UNION ALL", "INTERSECT", "EXCEPT"] {
            let sql=rewrite(&format!("{arm} {operator} {arm}"),SqlDialect::Postgres);
            assert!(!sql.contains("derived_projection"),"{sql}");
            for index in 1..=4 { assert!(sql.contains(&format!("__orchiddb_derived_{index}")),"{sql}"); }
        }
    }

    #[test]
    fn unit_rows_are_preserved_inside_union_operands() {
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres, SqlDialect::Custom(&crate::ir::rel::sql::trino::TRINO)] {
            let sql = rewrite("SELECT count(*) FROM (SELECT * UNION ALL SELECT *) AS units", dialect);
            assert!(!sql.contains("SELECT *"), "{sql}");
            assert_eq!(sql.matches("1 AS __orchiddb_unit").count(), 2, "{sql}");
        }
    }

    #[test]
    fn postgres_first_value_aggregate_preserves_order_filter_and_nulls() {
        let sql = rewrite("SELECT first_value(v ORDER BY k) FILTER (WHERE keep) FROM source", SqlDialect::Postgres);
        assert!(sql.contains("array_agg(v ORDER BY k)") && sql.contains("FILTER (WHERE keep)") && sql.contains("[1]"), "{sql}");
        let window = rewrite("SELECT first_value(v) OVER (ORDER BY k) FROM source", SqlDialect::Postgres);
        assert!(window.contains("first_value(v) OVER") && !window.contains("array_agg"), "{window}");
    }

    #[test]
    fn unordered_non_null_representatives_use_native_aggregates() {
        for (dialect, name) in [(SqlDialect::DuckDb, "any_value"), (SqlDialect::Postgres, "any_value"), (SqlDialect::Custom(&crate::ir::rel::sql::trino::TRINO), "arbitrary")] {
            let sql = rewrite("SELECT first_value(v) IGNORE NULLS FROM source", dialect);
            assert!(sql.contains(&format!("{name}(v)")), "{sql}");
            assert!(!sql.contains("IGNORE NULLS"), "{sql}");
        }
    }

    #[test]
    fn postgres_polymorphic_aggregates_type_only_bare_string_literals() {
        for call in ["first_value('Person') IGNORE NULLS", "any_value('Person')", "array_agg('Person')",
            "first_value('Person') OVER (ORDER BY k)", "last_value('Person') OVER (ORDER BY k)"] {
            let sql = rewrite(&format!("SELECT {call} FROM source"), SqlDialect::Postgres);
            assert!(sql.contains("AS TEXT") && sql.contains("'Person'") && sql.matches("CAST(").count() == 1, "{sql}");
        }
        for call in ["any_value(v)", "any_value(1)", "any_value(CAST('Person' AS TEXT))"] {
            let sql = rewrite(&format!("SELECT {call} FROM source"), SqlDialect::Postgres);
            assert_eq!(sql.matches("CAST(").count(), usize::from(call.contains("CAST(")), "{sql}");
        }
    }

    #[test]
    fn templates_preserve_argument_identifiers_and_reject_missing_arguments() {
        let column = ast::Expr::Identifier(ast::Ident::with_quote('"', "__arg1"));
        assert_eq!(template("__arg0", &[column]).unwrap().to_string(), "((\"__arg1\"))");
        assert!(template("__arg1", &[]).is_err());
        assert!(template("__arg0; SELECT 1", &[ast::Expr::Identifier(ast::Ident::new("x"))]).is_err());
        let sql = template("(SELECT '__local12' AS \"__local12\" FROM (SELECT __arg0 AS value) AS __local12)", &[ast::Expr::Identifier(ast::Ident::new("payload"))]).unwrap().to_string();
        assert!(sql.contains("'__local12'") && sql.contains("\"__local12\"") && sql.contains("AS __graph_dialect_12_0"), "{sql}");
    }

    fn rewrite(sql: &str, dialect: SqlDialect) -> String {
        let mut statements = Parser::parse_sql(&DuckDbDialect {}, sql).unwrap();
        prepare_ast(&mut statements[0], dialect).unwrap();
        statements[0].to_string()
    }

    #[test]
    fn correlated_exists_keeps_outer_key_when_inner_projects_it_under_an_alias() {
        let sql = rewrite("SELECT l.id FROM source l WHERE EXISTS (SELECT 1 FROM (SELECT l.id AS right_id FROM source l WHERE l.id = 1) derived_1 WHERE l.id = derived_1.right_id)", SqlDialect::Postgres);
        assert!(sql.contains("WHERE l.id = right_id"), "{sql}");
        assert!(!sql.contains("WHERE right_id = right_id"), "{sql}");
    }

    #[test]
    fn derived_wildcard_does_not_correlate_across_non_lateral_boundary() {
        let sql = rewrite("SELECT ranked.id FROM ranked JOIN (SELECT ranked.id FROM (SELECT * FROM ranked ORDER BY ranked.ordinal) sorted) selected ON ranked.id = selected.id", SqlDialect::Postgres);
        assert!(sql.contains("JOIN (SELECT id FROM (SELECT * FROM ranked ORDER BY ranked.ordinal)"), "{sql}");
        assert!(sql.contains("ON ranked.id = selected.id"), "{sql}");
    }

    #[test]
    fn postgres_preserves_reused_scalar_projection_boundary() {
        let sql = rewrite("SELECT x + x FROM (SELECT TRY_CAST(v AS DOUBLE) AS x FROM source) t", SqlDialect::Postgres);
        assert!(sql.contains("FROM source OFFSET 0"), "{sql}");
    }

    #[test]
    fn function_adaptation_preserves_literals_and_identifiers_in_both_dialects() {
        let original = "SELECT array_min(xs), 'array_min(xs)', xs AS \"array_min(xs)\" FROM t";
        let duck = rewrite(original, SqlDialect::DuckDb);
        assert!(duck.contains("list_min(xs)"), "{duck}");
        assert!(duck.contains("'array_min(xs)'"), "{duck}");
        assert!(duck.contains("AS \"array_min(xs)\""), "{duck}");
        let postgres = rewrite(original, SqlDialect::Postgres);
        assert!(
            postgres.contains("SELECT min(v) FROM unnest((xs))"),
            "{postgres}"
        );
        assert!(postgres.contains("'array_min(xs)'"), "{postgres}");
        assert!(postgres.contains("AS \"array_min(xs)\""), "{postgres}");
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
        prepare_ast(&mut statement, SqlDialect::Postgres).unwrap();
        assert!(statement.to_string().contains("\"log\"(100)"));
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
    async fn standard_expressions_match_constant_folded_reference_in_duckdb() {
        use datafusion::common::ScalarValue;
        let mapping = crate::ir::rel::mapping::GraphMapping::new();
        let compiler = crate::ir::rel::optimizer::session();
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
            let plan = mapping.plan_sql(query).unwrap();
            let dialect = SqlDialect::DuckDb.unparser_dialect();
            // DF53 cannot unparse explicit list casts. Exercise those
            // boundary cases directly through the identical SQL AST adapter.
            let mut statement = if query.contains("AS INT[]") {
                Parser::parse_sql(&DuckDbDialect {}, query)
                    .unwrap()
                    .remove(0)
            } else {
                Unparser::new(dialect.as_ref()).plan_to_sql(&plan).unwrap()
            };
            prepare_ast(&mut statement, SqlDialect::DuckDb).unwrap();
            let sql = statement.to_string();
            // Invoke the compiler's real constant-folding implementations as
            // an independent scalar oracle, with no physical plan or executor.
            let reference = compiler.state().optimize(&plan).unwrap();
            let datafusion::logical_expr::LogicalPlan::Projection(projection) = reference else { panic!("expected constant projection: {reference:?}") };
            fn constant(expr: &datafusion::logical_expr::Expr) -> ScalarValue {
                match expr {
                    datafusion::logical_expr::Expr::Alias(alias) => constant(&alias.expr),
                    datafusion::logical_expr::Expr::Literal(value, _) => value.clone(),
                    other => panic!("constant oracle did not fold {other:?}"),
                }
            }
            let expected = projection.expr.iter().map(constant).collect::<Vec<_>>();
            let mut prepared = conn
                .prepare(&sql)
                .unwrap_or_else(|err| panic!("{query}\n{sql}\n{err}"));
            let actual: Vec<_> = prepared.query_arrow([]).unwrap().collect();
            for col in 0..expected.len() {
                let actual_column = arrow::compute::cast(
                    actual[0].column(col),
                    &expected[col].data_type(),
                )
                .unwrap();
                let a = ScalarValue::try_from_array(&actual_column, 0).unwrap();
                let e = &expected[col];
                // SQL engines choose different integer widths for constants;
                // displayed scalar values compare values including list order.
                assert_eq!(a.to_string(), e.to_string(), "{query}\n{sql}\ncolumn {col}");
            }
        }
    }

#[test]
fn declared_function_casts_validate_shape_and_keep_one_cast() {
    let parse=|source: &str| Parser::new(&DuckDbDialect {}).try_with_sql(source).unwrap().parse_expr().unwrap();
    let name=crate::ir::functions::ENGINE_CAST_FUNCTION;
    for dialect in [SqlDialect::DuckDb,SqlDialect::Postgres,SqlDialect::Custom(&crate::ir::rel::sql::trino::TRINO)] {
        for (arguments,message) in [("()","invalid declared UDF argument cast"),("(x)","invalid declared UDF argument cast"),("(x,y)","invalid declared UDF argument cast"),("(x,42)","invalid declared UDF argument type")] {
            let mut expression=parse(&format!("{name}{arguments}"));
            let error=adapt_expression(&mut expression,dialect).unwrap_err().to_string();
            assert!(error.contains(message),"{error}");
        }
        let mut expression=parse(&format!("{name}(payload,'BIGINT')"));
        adapt_expression(&mut expression,dialect).unwrap();
        let sql=expression.to_string();
        assert_eq!(sql.matches("CAST(").count(),1,"{sql}");
        assert!(sql.contains("payload") && sql.contains("AS BIGINT"),"{sql}");
        let mut expression=parse(&format!("{name}(payload,'BIGINT')"));
        let ast::Expr::Function(function)=&mut expression else {panic!()};
        function.args=ast::FunctionArguments::None;
        assert!(adapt_expression(&mut expression,dialect).unwrap_err().to_string().contains("invalid declared UDF argument cast"));
    }
}

#[test]
fn native_function_paths_reject_empty_components_and_quote_names() {
    let parse=|source: &str| Parser::new(&DuckDbDialect {}).try_with_sql(source).unwrap().parse_expr().unwrap();
    for dialect in [SqlDialect::DuckDb,SqlDialect::Postgres,SqlDialect::Custom(&crate::ir::rel::sql::trino::TRINO)] {
        for path in ["", ".f", "catalog.", "catalog..f", "catalog.schema.f"] {
            let mut expression=parse("placeholder(payload)");
            let ast::Expr::Function(function)=&mut expression else {panic!()};
            function.name=ast::ObjectName::from(vec![ast::Ident::new(format!("{}{path}",crate::ir::functions::ENGINE_FUNCTION_PREFIX))]);
            let result=adapt_expression(&mut expression,dialect);
            if path=="catalog.schema.f" {
                result.unwrap();
                let ast::Expr::Function(function)=expression else {panic!()};
                assert_eq!(function.name.0.len(),3);
                assert!(function.to_string().contains("payload"));
            } else {
                assert!(result.unwrap_err().to_string().contains("invalid engine function path"));
            }
        }
    }
}

#[test]
fn native_float_kernel_requires_duckdb_and_one_positional_argument() {
    let parse=|source: &str| Parser::new(&DuckDbDialect {}).try_with_sql(source).unwrap().parse_expr().unwrap();
    let name=super::super::super::conversion_rules::NATIVE_FLOAT;
    for dialect in [SqlDialect::Postgres,SqlDialect::Custom(&crate::ir::rel::sql::trino::TRINO)] {
        let mut expression=parse(&format!("{name}(payload)"));
        assert!(adapt_expression(&mut expression,dialect).unwrap_err().to_string().contains("owning DuckDB adapter"));
    }
    for arguments in ["()","(x,y)","(*)"] {
        let mut expression=parse(&format!("{name}{arguments}"));
        assert!(adapt_expression(&mut expression,SqlDialect::DuckDb).unwrap_err().to_string().contains("native string-to-float signature"));
    }
    let mut expression=parse(&format!("{name}(payload)"));
    let ast::Expr::Function(function)=&mut expression else {panic!()};
    function.args=ast::FunctionArguments::None;
    assert!(adapt_expression(&mut expression,SqlDialect::DuckDb).unwrap_err().to_string().contains("native string-to-float signature"));
}

#[test]
fn variadic_templates_preserve_argument_asts_and_quoted_placeholder_identifiers() {
    for dialect in [SqlDialect::DuckDb,SqlDialect::Postgres,SqlDialect::Custom(&super::super::trino::TRINO)] {
        for args in [vec![],vec![ast::Expr::Identifier(ast::Ident::with_quote('"',"__arg9"))],
            vec![ast::Expr::Identifier(ast::Ident::new("__args")),ast::Expr::Value(ast::Value::SingleQuotedString("'); SELECT 2; --".into()).into())]] {
            let expression=portable_template("coalesce(__args)",&args,dialect).unwrap();
            let ast::Expr::Nested(inner)=expression else {panic!()};
            let ast::Expr::Function(call)=*inner else {panic!()};
            let ast::FunctionArguments::List(actual)=call.args else {panic!()};
            let expected=args.iter().cloned().map(|arg|ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(ast::Expr::Nested(Box::new(arg))))).collect::<Vec<_>>();
            assert_eq!(actual.args,expected);
            let array=portable_template("ARRAY[__args]",&args,dialect).unwrap();
            let ast::Expr::Nested(inner)=array else {panic!()};
            let ast::Expr::Array(array)=*inner else {panic!()};
            assert_eq!(array.elem,args.iter().cloned().map(|arg|ast::Expr::Nested(Box::new(arg))).collect::<Vec<_>>());
            let quoted=portable_template("coalesce(\"__args\", '__args')",&args,dialect).unwrap().to_string();
            assert!(quoted.contains("\"__args\"") && quoted.contains("'__args'"),"{quoted}");
        }
    }
}

#[test]
fn generated_template_names_avoid_both_caller_and_template_collisions() {
    for dialect in [SqlDialect::DuckDb,SqlDialect::Postgres,SqlDialect::Custom(&super::super::trino::TRINO)] {
        let arg=ast::Expr::Identifier(ast::Ident::new("__graph_dialect_0_0"));
        let sql=portable_template("(SELECT __arg0 AS value, '__graph_dialect_0_1' AS marker FROM input AS __local0)",&[arg],dialect).unwrap().to_string();
        assert!(sql.contains("AS __graph_dialect_0_2"),"{sql}");
        assert!(sql.contains("'__graph_dialect_0_1'") && sql.contains("(__graph_dialect_0_0)"),"{sql}");
        for malformed in ["'unterminated","1 +","1 2","__arg2","__arg0; SELECT 2"] {
            assert!(portable_template(malformed,&[],dialect).is_err(),"accepted malformed mapping: {malformed}");
        }
    }
}
