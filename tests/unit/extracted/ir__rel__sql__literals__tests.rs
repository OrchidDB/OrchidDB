
    use super::*;

    #[test]
    fn static_text_decodes_literal_encoding_without_evaluating_dynamic_calls() {
        use datafusion::sql::sqlparser::{dialect::GenericDialect, parser::Parser};
        let parse = |sql| Parser::new(&GenericDialect).try_with_sql(sql).unwrap().parse_expr().unwrap();
        for (sql, expected) in [
            ("concat('a', chr(39), 'b')", "a'b"),
            ("('a' || chr(0)) || 'b'", "a\0b"),
            ("CAST(concat('猫',chr(233)) AS TEXT)", "猫é"),
        ] {
            assert_eq!(literal_text(&parse(sql)).as_deref(), Some(expected));
        }
        for sql in ["name", "concat('a',name)", "chr(code)", "chr(1114112)",
            "chr('39')", "random()", "NULL", "42"] {
            assert_eq!(literal_text(&parse(sql)), None, "{sql}");
        }
    }

    #[test]
    fn nested_empty_literals_share_expression_cast_types() {
        let empty = ScalarValue::List(ScalarValue::new_list_nullable(&[], &DataType::Null));
        let inner_type = empty.data_type();
        let null = ScalarValue::try_from(&inner_type).unwrap();
        let value = ScalarValue::List(ScalarValue::new_list_nullable(
            &[empty, null], &inner_type,
        ));
        let sql = sql_literal(SqlDialect::DuckDb, &value).unwrap();
        assert_eq!(sql, "CAST([[], NULL] AS INTEGER[][])");
        assert_eq!(SqlDialect::DuckDb.sql_type(&value.data_type()).unwrap(), "INTEGER[][]");
    }
