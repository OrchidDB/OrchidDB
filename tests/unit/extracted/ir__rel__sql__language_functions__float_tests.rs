
    use super::*;

    #[test]
    fn current_helpers_satisfy_the_registration_probe() {
        let connection = Connection::open_in_memory().unwrap();
        assert!(connection.prepare(REGISTRATION_PROBE).is_err());
        register(&connection).unwrap();
        connection.prepare(REGISTRATION_PROBE).unwrap();
        // Borrowed connections share installed functions and must also take
        // the fast path rather than enumerating the catalog for each helper.
        let clone = connection.try_clone().unwrap();
        clone.prepare(REGISTRATION_PROBE).unwrap();
        register(&clone).unwrap();
        clone.prepare(REGISTRATION_PROBE).unwrap();
    }

    #[test]
    fn typed_nullable_helper_matches_reference_and_verifies_registration() {
        let connection = Connection::open_in_memory().unwrap();
        assert!(float_capability(&connection, true).unwrap().is_none());
        register(&connection).unwrap();
        let capability = float_capability(&connection, true).unwrap().unwrap();
        assert_eq!(capability.operation(), "cypher.to_float");
        assert_eq!(capability.signature(), "varchar->double");
        assert_eq!(capability.contract_version(), 1);
        assert!(float_capability(&connection, false).unwrap().is_none());
        connection.execute_batch("CREATE TABLE input(raw VARCHAR)").unwrap();
        let inputs = [None, Some(""), Some("foo"), Some("1.25"), Some("-2e2"),
            Some("__1__2__"), Some("\u{a0}1.5\u{3000}"), Some("NaN"), Some("Infinity"),
            Some("1e999"), Some("1e-999"), Some("-0"), Some("1 2"), Some("0x10")];
        for input in inputs { connection.execute("INSERT INTO input VALUES (?)", [input]).unwrap(); }
        let mut query = connection.prepare("SELECT __orchiddb_cypher_to_float_v1(raw) FROM input").unwrap();
        let actual = query.query_map([], |r| r.get::<_, Option<f64>>(0)).unwrap().collect::<duckdb::Result<Vec<_>>>().unwrap();
        let expected: Vec<_> = inputs.into_iter().map(|s| s.and_then(|s| crate::ir::functions::cypher_float::parse_string(s).ok())).collect();
        assert_eq!(actual.iter().map(|v| v.map(f64::to_bits)).collect::<Vec<_>>(), expected.iter().map(|v| v.map(f64::to_bits)).collect::<Vec<_>>());
        // A clone can use the function, but must not reuse another session's
        // witness or cached plan. It safely chooses the helper-free alternative.
        let clone = connection.try_clone().unwrap();
        assert!(float_capability(&clone, true).unwrap().is_none());
        register(&connection).unwrap();
        assert_eq!(float_capability(&connection, true).unwrap().unwrap(), capability);
    }

    #[test]
    fn mismatched_or_untrusted_registration_never_authorizes_helper() {
        for definition in [
            "CREATE MACRO __orchiddb_cypher_to_float_v1(x) AS TRY_CAST(x AS DOUBLE)",
            "CREATE MACRO __orchiddb_cypher_to_float_v1(x) AS 7",
        ] {
            let connection = Connection::open_in_memory().unwrap();
            connection.execute_batch(definition).unwrap();
            register(&connection).unwrap();
            assert!(float_capability(&connection, true).unwrap().is_none());
        }
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("BEGIN").unwrap();
        register(&connection).unwrap();
        assert!(float_capability(&connection, true).unwrap().is_some());
        connection.execute_batch("ROLLBACK").unwrap();
        assert!(float_capability(&connection, true).unwrap().is_none());
    }
    #[tokio::test]
    async fn mapped_helper_and_expansion_share_the_selected_contract() {
        use crate::ir::rel::{RelBackend, RelBackendOptions, mapping::{GraphMapping, NodeMapping}};
        use crate::ir::catalog::PropertyGraph;
        use arrow::datatypes::{Field, Schema, DataType};
        use std::{collections::BTreeMap, sync::Arc};
        let connection = Connection::open_in_memory().unwrap();
        register(&connection).unwrap();
        connection.execute_batch("CREATE TABLE conversion_values (id BIGINT, raw VARCHAR)").unwrap();
        let inputs = [None, Some(""), Some("foo"), Some("2.9"), Some("__1__2__"), Some("NaN"), Some("Infinity"), Some("1e999"), Some("1e-999"), Some("\u{a0}1.5\u{3000}")];
        for (id, input) in inputs.iter().enumerate() {
            connection.execute("INSERT INTO conversion_values VALUES (?, ?)", duckdb::params![id as i64, input]).unwrap();
        }
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema("conversion_values", Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false), Field::new("raw", DataType::Utf8, true)])));
        mapping.map_node(NodeMapping::table("Conversion", "conversion_values", "id").property("raw", "raw").property("id", "id"));
        let plan = crate::language::cypher::preparation::prepare("MATCH (n:Conversion) RETURN toFloat(n.raw) AS x ORDER BY n.id", &BTreeMap::new(), None).unwrap();
        for enabled in [true, false] {
            let mut options = RelBackendOptions::default();
            options.language_functions = enabled;
            options.rule_context.capabilities.language_functions_enabled = enabled;
            // Preserve the verified registration even when disabled: explicit
            // opt-out must win over availability in an already-used session.
            options.rule_context.capabilities.float_helper = float_capability(&connection, true).unwrap();
            options.mapping = Some(Arc::new(mapping.clone()));
            let lowered = RelBackend::with_options(options).lower(&plan, &PropertyGraph::new()).unwrap();
            let prepared = super::super::prepare_with_external(&lowered, super::super::SqlDialect::DuckDb, &mapping.physical_table_names()).await.unwrap();
            assert_eq!(prepared.query.contains(kernels::CYPHER_TO_FLOAT), enabled, "{}", prepared.query);
            let mut query = connection.prepare(&prepared.query).unwrap();
            let values = query.query_map([], |r| r.get::<_, Option<f64>>(0)).unwrap().collect::<duckdb::Result<Vec<_>>>().unwrap();
            let expected: Vec<_> = inputs.into_iter().map(|v| v.and_then(|v| crate::ir::functions::cypher_float::parse_string(v).ok())).collect();
            assert_eq!(values, expected);
            for op in ["=", "<>", "<", "<=", ">", ">="] {
                for shape in 0..3 {
                    let query = match shape {
                        0 => format!("MATCH (n:Conversion) RETURN toFloat(n.raw) {op} 5.0 AS result ORDER BY n.id"),
                        1 => format!("MATCH (n:Conversion) WITH n.id AS id, toFloat(n.raw) AS x RETURN x {op} 5.0 AS result ORDER BY id"),
                        _ => format!("MATCH (n:Conversion) RETURN (CASE WHEN n.id = 1 THEN 0.0 ELSE toFloat(n.raw) END) {op} 5.0 AS result ORDER BY n.id"),
                    };
                    let plan = crate::language::cypher::preparation::prepare(&query, &BTreeMap::new(), None).unwrap();
                    let mut options = RelBackendOptions::default();
                    options.mapping = Some(Arc::new(mapping.clone()));
                    options.rule_context.capabilities.language_functions_enabled = enabled;
                    options.rule_context.capabilities.float_helper = float_capability(&connection, true).unwrap();
                    let lowered = RelBackend::with_options(options).lower(&plan, &PropertyGraph::new()).unwrap();
                    let prepared = super::super::prepare_with_external(&lowered, super::super::SqlDialect::DuckDb, &mapping.physical_table_names()).await.unwrap();
                    assert!(!prepared.query.to_lowercase().contains("isnan"), "{}", prepared.query);
                    if enabled {
                        assert_eq!(prepared.query.matches(kernels::CYPHER_TO_FLOAT).count(), 1, "{}", prepared.query);
                    }
                    let mut statement = connection.prepare(&prepared.query).unwrap();
                    let actual = statement.query_map([], |r| r.get::<_, Option<bool>>(0)).unwrap().collect::<duckdb::Result<Vec<_>>>().unwrap();
                    let expected: Vec<_> = expected.iter().enumerate().map(|(id, v)| {
                        let value = if shape == 2 && id == 1 { Some(0.0) } else { *v };
                        value.map(|v| match op {
                        "=" => v == 5.0, "<>" => v != 5.0, "<" => v < 5.0,
                        "<=" => v <= 5.0, ">" => v > 5.0, ">=" => v >= 5.0, _ => unreachable!(),
                    })}).collect();
                    assert_eq!(actual, expected, "{query}: {}", prepared.query);
                }
            }
        }
    }

    #[test]
    fn cached_helper_plan_is_bound_to_live_enabled_session() {
        use super::super::{PreparedSql, DuckDbExecutor, execute_prepared};
        use arrow::datatypes::{Schema, Field, DataType};
        use std::sync::Arc;
        let connection = Connection::open_in_memory().unwrap();
        register(&connection).unwrap();
        let mut prepared = PreparedSql::query_only("SELECT __orchiddb_cypher_to_float_v1('1.25') AS x".into(), Arc::new(Schema::new(vec![Field::new("x", DataType::Float64, true)])));
        prepared.float_helper = float_capability(&connection, true).unwrap();
        let mut executor = DuckDbExecutor::from_connection(connection);
        assert_eq!(execute_prepared(&mut executor, &prepared).unwrap().batch.num_rows(), 1);
        executor.set_language_functions(false).unwrap();
        assert!(execute_prepared(&mut executor, &prepared).unwrap_err().to_string().contains("reprepare"));
        assert!(executor.execute_prepared_arrow(&prepared).unwrap_err().to_string().contains("reprepare"));
        executor.set_language_functions(true).unwrap();
        assert!(execute_prepared(&mut executor, &prepared).is_ok());
        let mut other = DuckDbExecutor::new();
        other.prepare_float_helper_capability().unwrap();
        assert!(execute_prepared(&mut other, &prepared).unwrap_err().to_string().contains("reprepare"));
    }

