
    use super::*;

    #[test]
    fn permission_constructors_match_wire_defaults_and_allow_quoted_columns() {
        let relation = PermissionRelation::flat("grants", "document", "read");
        let decoded: PermissionRelation = serde_json::from_value(serde_json::json!({
            "table":"grants", "resource_type":"document", "permission":"read"
        })).unwrap();
        assert_eq!(serde_json::to_value(&relation).unwrap(), serde_json::to_value(decoded).unwrap());
        let relation = relation.with_columns("r\"type", "r'rel", "rid", "s.type", "srel", "sid");
        let principal = Authorization::new("user'kind", "alice' OR true --");
        let scope = PermissionScope::new("owner\"id", relation);
        let node: Node = serde_json::from_value(serde_json::json!({
            "label":"Document", "table":"documents", "id":"owner\"id"
        })).unwrap();
        let schemas = BTreeMap::from([("documents".into(), Arc::new(Schema::new(vec![
            Field::new("owner\"id", DataType::Utf8, false),
        ])))]);
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            let sql = permission_scope_filter(dialect, &node, &scope, &principal, 7, &schemas).unwrap();
            assert!(sql.contains("p7.\"r\"\"type\""), "{sql}");
            assert!(sql.contains("'alice'' OR true --'"), "{sql}");
            assert!(sql.contains("'user''kind'"), "{sql}");
            assert!(sql.contains("p7.\"srel\" = ''"), "{sql}");
            assert!(!sql.contains("TRY_CAST"), "{sql}");
        }
        let missing = PermissionScope::new("missing", scope.relation);
        assert!(permission_scope_filter(SqlDialect::DuckDb, &node, &missing, &principal, 0, &schemas)
            .unwrap_err().contains("missing node resource column `documents.missing`"));
    }

    #[cfg(feature = "duckdb")]
    #[test]
    fn integer_permission_membership_requires_canonical_ids_and_exact_principals() {
        for (ty, sql_type) in [
            (DataType::Int8, "TINYINT"), (DataType::Int16, "SMALLINT"),
            (DataType::Int32, "INTEGER"), (DataType::Int64, "BIGINT"),
        ] {
            let connection = duckdb::Connection::open_in_memory().unwrap();
            connection.execute_batch(&format!(
                "CREATE TABLE documents(id {sql_type}); INSERT INTO documents VALUES (1),(2),(3),(4),(5),(6),(7); \
                 CREATE TABLE grants(resource_type VARCHAR, resource_rel VARCHAR, resource_id VARCHAR, \
                 subject_type VARCHAR, subject_rel VARCHAR, subject_id VARCHAR)"
            )).unwrap();
            for (id, permission, subject_relation, subject) in [
                ("1","read","","alice"), ("2","read","","bob"),
                ("03","read","","alice"), ("+4","read","","alice"),
                (" 5","read","","alice"), ("6.0","read","","alice"),
                ("7","write","","alice"), ("2","read","member","alice"),
                ("999999999999999999999999","read","","alice"),
                ("not-a-number","read","","alice"),
            ] {
                connection.execute("INSERT INTO grants VALUES ('document', ?, ?, 'user', ?, ?)",
                    duckdb::params![permission,id,subject_relation,subject]).unwrap();
            }
            let node: Node = serde_json::from_value(serde_json::json!({
                "label":"Document", "table":"documents", "id":"id"
            })).unwrap();
            let schemas = BTreeMap::from([("documents".into(), Arc::new(Schema::new(vec![Field::new("id", ty, false)])))]);
            let scope = PermissionScope::new("id", PermissionRelation::flat("grants", "document", "read"));
            let filter = permission_scope_filter(SqlDialect::DuckDb, &node, &scope,
                &Authorization::new("user", "alice"), 0, &schemas).unwrap();
            let mut statement = connection.prepare(&format!("SELECT CAST(n.id AS BIGINT) FROM documents n WHERE {filter} ORDER BY n.id")).unwrap();
            let rows = statement.query_map([], |row| row.get::<_,i64>(0)).unwrap()
                .collect::<Result<Vec<_>,_>>().unwrap();
            assert_eq!(rows, vec![1], "{sql_type}: {filter}");
        }
    }

    #[test]
    fn table_reference_unquoting_handles_components_and_rejects_incomplete_names() {
        for (input, expected) in [
            ("catalog.schema.table", "catalog.schema.table"),
            ("\"catalog\".\"sch.ema\".\"ta\"\"ble\"", "catalog.sch.ema.ta\"ble"),
            ("\"single\"", "single"),
        ] {
            assert_eq!(unquote_table_reference(input).as_deref(), Some(expected));
        }
        for input in ["", ".table", "schema.", "schema..table", "\"unterminated", "\"\""] {
            assert_eq!(unquote_table_reference(input), None, "{input}");
        }
    }
