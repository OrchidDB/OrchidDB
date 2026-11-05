
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

#[test]
fn json_transform_rejects_dynamic_invalid_and_unrepresentable_schemas() {
    for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
        for args in [vec![], vec![expression("payload")], vec![expression("payload"), expression("schema_column")], vec![expression("payload"), expression("'not json'")]] {
            assert!(lower(&args, dialect).is_err(), "{dialect:?}: {args:?}");
        }
        for schema in ["[]", "[1,2]", "null", "true", "7", "\"unknown\""] {
            assert!(lower(&[expression("payload"), expression(&string(schema))], dialect).is_err(), "{dialect:?}: {schema}");
        }
    }
    assert!(lower(&[expression("payload"), expression("'{}'")], SqlDialect::DuckDb).is_err());
}

#[cfg(feature = "duckdb")]
#[test]
fn json_transform_scalar_types_execute_without_numeric_or_boolean_coercion() {
    let db = duckdb::Connection::open_in_memory().unwrap();
    for (document, schema, expected) in [
        ("true", "boolean", "true"), ("false", "boolean", "false"),
        ("1.25", "float", "1.25"), ("-2.5", "double", "-2.5"),
        ("9223372036854775807", "int64", "9223372036854775807"),
        ("true", "string", "\"true\""), ("42", "string", "\"42\""),
        ("null", "double", "null"), ("null", "json", "null"),
    ] {
        let schema = serde_json::to_string(schema).unwrap();
        let args = [expression(&format!("CAST({} AS JSON)", string(document))), expression(&string(&schema))];
        let lowered = lower(&args, SqlDialect::DuckDb).unwrap();
        let sql = format!("SELECT coalesce(CAST(to_json({lowered}) AS VARCHAR),'null')");
        let actual: String = db.query_row(&sql, [], |row| row.get(0)).unwrap_or_else(|error| panic!("{document}/{schema}: {error}: {sql}"));
        assert_eq!(serde_json::from_str::<serde_json::Value>(&actual).unwrap(), serde_json::from_str::<serde_json::Value>(expected).unwrap());
    }
    for (document, schema) in [("1", "boolean"), ("\"1\"", "int64"), ("1.5", "int64"), ("{}", "string"), ("[]", "string"), ("1e100", "float"), ("1e999", "double")] {
        let schema = serde_json::to_string(schema).unwrap();
        let args = [expression(&format!("CAST({} AS JSON)", string(document))), expression(&string(&schema))];
        let sql = format!("SELECT {}", lower(&args, SqlDialect::DuckDb).unwrap());
        let result = db.prepare(&sql).and_then(|mut statement| {
            let batches = statement.query_arrow([])?.collect::<Vec<_>>();
            Ok(batches)
        });
        assert!(result.is_err(), "{document}/{schema}");
    }
}

#[test]
#[ignore = "requires PostgreSQL"]
fn postgres_json_transform_nested_lists_preserve_json_null_carriers() {
    let pg = std::env::var("GRAPH_PG_URL").expect("GRAPH_PG_URL");
    for (document, schema, expected) in [
        ("[[1,null,2],[],null]", "[[\"int64\"]]", serde_json::json!([[1,null,2],[],null])),
        ("[[null,7],[],null]", "[[\"json\"]]", serde_json::json!([["null","7"],[],null])),
        ("[true,false,null]", "[\"boolean\"]", serde_json::json!([true,false,null])),
        ("[1.25,-2.5,null]", "[\"double\"]", serde_json::json!([1.25,-2.5,null])),
        ("[1.25,-2.5,null]", "[\"float\"]", serde_json::json!([1.25,-2.5,null])),
        ("[true,42,null]", "[\"string\"]", serde_json::json!(["true","42",null])),
    ] {
        let args = [expression(&format!("CAST({} AS JSONB)", string(document))), expression(&string(schema))];
        let sql = format!("SELECT to_jsonb({})::text", lower(&args, SqlDialect::Postgres).unwrap());
        let output = std::process::Command::new("psql").args([&pg,"-X","-A","-t","-v","ON_ERROR_STOP=1","-c",&sql]).output().unwrap();
        assert!(output.status.success(), "{schema}: {}: {sql}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(), expected, "{schema}");
    }
    for (document, schema) in [("1", "boolean"), ("\"1\"", "int64"), ("1.5", "int64"), ("{}", "string"), ("1e100", "float"), ("1e999", "double")] {
        let schema = serde_json::to_string(schema).unwrap();
        let args = [expression(&format!("CAST({} AS JSONB)", string(document))), expression(&string(&schema))];
        let sql = format!("SELECT {}", lower(&args, SqlDialect::Postgres).unwrap());
        let output = std::process::Command::new("psql").args([&pg,"-X","-A","-t","-v","ON_ERROR_STOP=1","-c",&sql]).output().unwrap();
        assert!(!output.status.success(), "invalid conversion accepted: {document}/{schema}");
        assert!(!output.stderr.is_empty());
    }

}
