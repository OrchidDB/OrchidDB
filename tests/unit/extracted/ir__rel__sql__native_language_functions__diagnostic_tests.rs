
    use super::*;
    #[test]
    fn runtime_diagnosis_survives_duckdb_scalar_error_transport() {
        let connection = Connection::open_in_memory().unwrap();
        register(&connection).unwrap();
        let spec =
            json!({"Call":{"name":"cypher_slice_bound","args":[{"Binding":"value"}]}}).to_string();
        let error = connection
            .query_row::<String, _, _>(
                "SELECT __orchiddb_value(?, struct_pack(value := -1))",
                [spec],
                |row| row.get(0),
            )
            .unwrap_err();
        let error = crate::ir::diagnostics::QueryExecutionError::from_error(error);
        assert_eq!(
            error.diagnosis,
            Some(crate::ir::diagnostics::RuntimeDiagnosis::NegativeIntegerArgument)
        );
        assert!(error.message.contains("non-negative"));
    }
