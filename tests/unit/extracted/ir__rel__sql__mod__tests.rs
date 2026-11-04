
    use super::*;

    #[test]
    fn quotes_and_escapes_identifiers() {
        assert_eq!(SqlDialect::DuckDb.quote_ident("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn renders_literals() {
        assert_eq!(
            sql_literal(
                SqlDialect::DuckDb,
                &ScalarValue::Utf8(Some("o'hare".into()))
            )
            .unwrap(),
            "'o''hare'"
        );
        assert_eq!(
            sql_literal(SqlDialect::DuckDb, &ScalarValue::Float64(Some(2.0))).unwrap(),
            "2.0"
        );
        assert_eq!(
            sql_literal(SqlDialect::Postgres, &ScalarValue::Float64(Some(f64::NAN))).unwrap(),
            "CAST('NaN' AS DOUBLE PRECISION)"
        );
        assert_eq!(
            sql_literal(SqlDialect::DuckDb, &ScalarValue::Int64(None)).unwrap(),
            "NULL"
        );
        let nul = sql_literal(SqlDialect::DuckDb, &ScalarValue::Utf8(Some("a\0b".into()))).unwrap();
        assert_eq!(nul, "'a' || chr(0) || 'b'");
        assert!(!nul.contains('\0'));
    }

    #[test]
    fn rejects_unsupported_ddl_types() {
        let err = SqlDialect::Postgres
            .ddl_type(&DataType::Duration(arrow::datatypes::TimeUnit::Microsecond))
            .unwrap_err();
        assert!(matches!(err, SqlError::Unsupported(_)));
    }

    #[test]
    fn duckdb_fixups_alias_unnest_outputs() {
        let sql = SqlDialect::DuckDb.fixup_query(
            "SELECT UNNEST(\"items\"), UNNEST(\"items\") AS \"copy\", array_min(\"items\"), array_intersect(\"items\", \"other\")".into(),
        );
        assert_eq!(
            sql,
            "SELECT UNNEST(\"items\") AS \"items\", UNNEST(\"items\") AS \"copy\", array_min(\"items\"), array_intersect(\"items\", \"other\")"
        );
    }
