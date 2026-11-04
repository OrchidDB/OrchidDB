
    use super::*;

    #[test]
    fn nullable_predicates_and_date_inputs_keep_sql_types() {
        let connection = Connection::open_in_memory().unwrap();
        register(&connection).unwrap();
        let mut statement = connection
            .prepare(
                r#"SELECT __orchiddb_predicate('{"Binding":"value"}', struct_pack(value := flag))
            FROM (VALUES (NULL::BOOLEAN), (false), (true)) AS input(flag)"#,
            )
            .unwrap();
        let actual = statement
            .query_map([], |row| row.get::<_, Option<bool>>(0))
            .unwrap()
            .collect::<duckdb::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(actual, vec![None, Some(false), Some(true)]);
        let encoded: String = connection
            .query_row(
                r#"SELECT (__orchiddb_value('{"Binding":"value"}',
            struct_pack(value := DATE '2024-02-29'))).__orchiddb_value_v1"#,
                [],
                |row| row.get(0),
            )
            .unwrap();
        use base64::Engine;
        let value = crate::ir::catalog::snapshot::binary::decode_value_bytes(
            &base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            value,
            crate::ir::value::Value::Temporal(
                crate::ir::temporal::parse("date", "2024-02-29").unwrap()
            )
        );
    }
