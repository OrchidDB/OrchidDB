
    use super::*;
    fn expression(text: &str) -> ast::Expr {
        datafusion::sql::sqlparser::parser::Parser::new(
            &datafusion::sql::sqlparser::dialect::DuckDbDialect {},
        )
        .try_with_sql(text)
        .unwrap()
        .parse_expr()
        .unwrap()
    }
    #[test]
    fn nested_transform_is_an_engine_owned_typed_expression() {
        let args = [
            expression("payload"),
            expression(r#"'{"id":"int64","tags":["string"],"nested":[{"value":"json"}]}'"#),
        ];
        let sql = lower(&args, SqlDialect::DuckDb).unwrap().to_string();
        assert!(
            sql.contains("struct_pack")
                && sql.contains("list_transform")
                && sql.contains("regexp_full_match"),
            "{sql}"
        );
        assert!(!sql.contains("__arg0"));
        assert!(lower(&args, SqlDialect::Postgres).is_err());
    }
    #[test]
    fn postgres_scalar_and_list_transforms_do_not_require_anonymous_records() {
        let args = [expression("payload"), expression("'[\"int64\"]'")];
        let sql = lower(&args, SqlDialect::Postgres).unwrap().to_string();
        assert!(
            sql.contains("jsonb_array_elements") && sql.contains("WITH ORDINALITY"),
            "{sql}"
        );
    }
    #[cfg(feature = "duckdb")]
    #[test]
    fn nested_duckdb_transform_executes_and_preserves_missing_vs_json_null() {
        use arrow::array::Array;
        let db = duckdb::Connection::open_in_memory().unwrap();
        db.execute_batch("SET lambda_syntax='ENABLE_SINGLE_ARROW'")
            .unwrap();
        let args = [
            expression(
                r#"CAST('{"id":1,"id":7,"tags":["a",null],"nested":[{"value":null},{}]}' AS JSON)"#,
            ),
            expression(r#"'{"id":"int64","tags":["string"],"nested":[{"value":"json"}]}'"#),
        ];
        let sql = format!(
            "SELECT {} AS result",
            lower(&args, SqlDialect::DuckDb).unwrap()
        );
        let mut stmt = db.prepare(&sql).unwrap();
        let batches = stmt.query_arrow([]).unwrap().collect::<Vec<_>>();
        assert_eq!(batches[0].num_rows(), 1);
        let result = batches[0]
            .column(0)
            .as_any()
            .downcast_ref::<arrow::array::StructArray>()
            .unwrap();
        assert_eq!(
            result
                .column_by_name("id")
                .unwrap()
                .as_any()
                .downcast_ref::<arrow::array::Int64Array>()
                .unwrap()
                .value(0),
            7
        );
        let nested = result
            .column_by_name("nested")
            .unwrap()
            .as_any()
            .downcast_ref::<arrow::array::ListArray>()
            .unwrap()
            .value(0);
        let nested = nested
            .as_any()
            .downcast_ref::<arrow::array::StructArray>()
            .unwrap();
        let values = nested.column_by_name("value").unwrap();
        assert!(!values.is_null(0));
        assert!(values.is_null(1));
        let bad = [
            expression(r#"CAST('{"id":"7"}' AS JSON)"#),
            expression(r#"'{"id":"int64"}'"#),
        ];
        let sql = format!("SELECT {}", lower(&bad, SqlDialect::DuckDb).unwrap());
        assert!(db.prepare(&sql).unwrap().query_arrow([]).is_err());
    }
