#![cfg(feature = "duckdb")]
use orchiddb::{
    engine::GraphEngine,
    ir::rel::sql::{DuckDbExecutor, SqlExecutor},
};

#[test]
fn registration_is_optional_idempotent_and_preserves_keys() {
    let mut e = DuckDbExecutor::new();
    assert!(e.language_functions_enabled());
    e.set_language_functions(false).unwrap();
    assert!(e.run(&[], "SELECT __orchiddb_lang_cypher_key(1)").is_err());
    e.set_language_functions(true).unwrap();
    e.set_language_functions(true).unwrap();
    for (a, b, same) in [
        ("1", "1.0", true),
        ("[1]", "[1.0]", true),
        ("{'a': 1}", "{'a': 1.0}", true),
        ("1", "'1'", false),
        ("0", "-0.0", true),
        ("[NULL,1]", "[NULL,1.0]", true),
        ("CAST('NaN' AS DOUBLE)", "CAST('NaN' AS DOUBLE)", true),
        (
            "CAST(9007199254740993 AS BIGINT)",
            "CAST(9007199254740993 AS DOUBLE)",
            false,
        ),
        ("{'b': 2, 'a': 1}", "{'a': 1.0, 'b': 2.0}", true),
    ] {
        let sql =
            format!("SELECT __orchiddb_lang_cypher_key({a}) = __orchiddb_lang_cypher_key({b})");
        assert_eq!(
            e.run(&[], &sql).unwrap(),
            vec![vec![orchiddb::ir::rel::sql::SqlValue::Bool(same)]],
            "{sql}"
        );
    }
    assert_eq!(e.run(&[], "SELECT count(DISTINCT __orchiddb_lang_cypher_key(v)) FROM (VALUES (1), (1), (NULL)) t(v)").unwrap(), vec![vec![orchiddb::ir::rel::sql::SqlValue::Int(1)]]);
    e.set_language_functions(false).unwrap();
    assert!(!e.language_functions_enabled());
}
fn rows(r: &orchiddb::engine::QueryResult) -> Vec<Vec<String>> {
    let b = &r.returned.batch;
    let mut out = (0..b.num_rows())
        .map(|r| {
            b.columns()
                .iter()
                .map(|a| arrow::util::display::array_value_to_string(a, r).unwrap())
                .collect()
        })
        .collect::<Vec<Vec<_>>>();
    out.sort();
    out
}
#[tokio::test]
async fn cypher_grouping_and_distinct_move_into_duckdb() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:P {x: 1.5}), (:P {x: 1.5}), (:P {x: 2.5})")
        .await
        .unwrap();
    for q in [
        "MATCH (p:P) RETURN p.x, count(*)",
        "MATCH (p:P) RETURN DISTINCT p.x",
        "MATCH (p:P) RETURN count(DISTINCT p.x)",
    ] {
        engine.set_language_functions(false).unwrap();
        let disabled = engine.cypher(q).await.unwrap_err();
        assert!(disabled.contains("language functions disabled"), "{disabled}");
        engine.set_language_functions(true).unwrap();
        let after = engine.cypher(q).await.unwrap();
        let expected = if q.contains("count(DISTINCT") { vec![vec!["2"]] }
            else if q.contains("DISTINCT") { vec![vec!["1.5"], vec!["2.5"]] }
            else { vec![vec!["1.5", "2"], vec!["2.5", "1"]] };
        assert_eq!(rows(&after), expected, "{q}");
        assert_eq!(after.stats.cost.native_relational_kernel_calls, 0);
        assert!(
            after
                .stats
                .sql_queries
                .iter()
                .any(|s| s.contains("orchid_cypher_key")),
            "{q}: {:?}",
            after.stats
        );

    }
}



#[test]
fn batched_nested_values_and_gremlin_null_contracts() {
    use orchiddb::ir::rel::sql::SqlValue as S;
    let mut e = DuckDbExecutor::new();
    e.set_language_functions(true).unwrap();
    assert_eq!(e.run(&[], "SELECT count(DISTINCT __orchiddb_lang_cypher_key([i % 17, NULL])) FROM range(10000) t(i)").unwrap(), vec![vec![S::Int(17)]]);
    for (sql, expected) in [
        (
            "SELECT __orchiddb_lang_text('concat', {'a0': NULL::VARCHAR, 'a1': 'yes'})",
            S::Text("yes".into()),
        ),
        (
            "SELECT __orchiddb_lang_text('concat', {'a0': NULL::VARCHAR, 'a1': NULL::VARCHAR})",
            S::Null,
        ),
        (
            "SELECT __orchiddb_lang_text('replace', {'a0': 'abc', 'a1': '', 'a2': 'X'})",
            S::Text("abc".into()),
        ),
        (
            "SELECT __orchiddb_lang_texts('split', {'a0': ' a  b ', 'a1': NULL::VARCHAR})",
            S::List(vec![S::Text("a".into()), S::Text("b".into())]),
        ),
        (
            "SELECT __orchiddb_lang_ints('local_length', {'a0': ['😀', NULL, 'a']})",
            S::List(vec![S::Int(2), S::Null, S::Int(1)]),
        ),
        (
            "SELECT __orchiddb_lang_texts('local_ucase', {'a0': ['a', NULL, 'ß']})",
            S::List(vec![S::Text("A".into()), S::Null, S::Text("SS".into())]),
        ),
    ] {
        assert_eq!(e.run(&[], sql).unwrap(), vec![vec![expected]], "{sql}");
    }
    assert!(
        e.run(
            &[],
            "SELECT __orchiddb_lang_text('conjoin', {'a0': NULL::VARCHAR, 'a1': ','})"
        )
        .is_err()
    );
    assert_eq!(e.run(&[], "SELECT 42").unwrap(), vec![vec![S::Int(42)]]);
    let mut other = DuckDbExecutor::new();
    assert!(other.language_functions_enabled());
    other.set_language_functions(false).unwrap();
    assert!(
        other
            .run(&[], "SELECT __orchiddb_lang_cypher_key(1)")
            .is_err()
    );
}



#[test]
fn registration_survives_transaction_rollback() {
    let mut e = DuckDbExecutor::new();
    e.begin().unwrap();
    e.set_language_functions(true).unwrap();
    e.rollback().unwrap();
    assert!(e.run(&[], "SELECT __orchiddb_lang_cypher_key(42)").is_ok());
    e.begin().unwrap();
    assert!(e.run(&[], "SELECT CAST('invalid' AS INTEGER)").is_err());
    e.rollback().unwrap();
    assert!(e.run(&[], "SELECT __orchiddb_lang_cypher_key(42)").is_ok());
}

#[test]
fn executor_defaults_include_borrowed_active_transactions() {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("BEGIN").unwrap();
    let mut executor = DuckDbExecutor::from_connection(connection);
    assert!(executor.language_functions_enabled());
    executor
        .run(&[], "SELECT __orchiddb_lang_cypher_key(1)")
        .unwrap();
    executor.rollback().unwrap();
    executor
        .run(&[], "SELECT __orchiddb_lang_cypher_key(1)")
        .unwrap();
    assert!(DuckDbExecutor::default().language_functions_enabled());
    assert!(
        DuckDbExecutor::with_timeout(std::time::Duration::from_secs(1))
            .language_functions_enabled()
    );
}

#[tokio::test]
async fn graph_engine_uses_language_functions_by_default_with_explicit_opt_out() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:P {x: 1.5}), (:P {x: 2.5})")
        .await
        .unwrap();
    let query = "MATCH (p:P) RETURN count(DISTINCT p.x)";
    let enabled = engine.cypher(query).await.unwrap();
    assert!(
        enabled
            .stats
            .sql_queries
            .iter()
            .any(|sql| sql.contains("orchid_cypher_key"))
    );
    engine.set_language_functions(false).unwrap();
    let disabled = engine.cypher(query).await.unwrap_err();
    assert!(disabled.contains("language functions disabled"), "{disabled}");
    assert_eq!(rows(&engine.cypher("RETURN 1 AS n").await.unwrap()), vec![vec!["1"]]);
}

#[test]
fn distinct_key_excludes_untyped_and_typed_nulls() {
    use orchiddb::ir::rel::sql::SqlValue;
    let mut executor = DuckDbExecutor::new();
    for expression in ["NULL", "CAST(NULL AS BIGINT)", "CAST(NULL AS VARCHAR)"] {
        let sql = format!("SELECT count(DISTINCT __orchiddb_lang_cypher_key({expression})) FROM (VALUES (1), (2)) AS t(x)");
        assert_eq!(executor.run(&[], &sql).unwrap(), vec![vec![SqlValue::Int(0)]], "{sql}");
    }
    // A non-null list containing null is still a value, unlike a null list.
    assert_eq!(executor.run(&[], "SELECT count(DISTINCT __orchiddb_lang_cypher_key(v)) FROM (VALUES ([NULL]), ([NULL]), (NULL)) AS t(v)").unwrap(), vec![vec![SqlValue::Int(1)]]);
}

#[tokio::test]
async fn distinct_count_ignores_missing_properties_and_nulls() {
    let mut engine = GraphEngine::in_memory().unwrap();
    let query = "MATCH (a) RETURN count(DISTINCT a.name) AS n";
    assert_eq!(rows(&engine.cypher(query).await.unwrap()), vec![vec!["0"]]);
    engine.cypher("CREATE (), (), ({name: null})").await.unwrap();
    assert_eq!(rows(&engine.cypher(query).await.unwrap()), vec![vec!["0"]]);
    engine.cypher("CREATE ({name: 'a'}), ({name: 'a'}), ({name: 'b'})").await.unwrap();
    assert_eq!(rows(&engine.cypher(query).await.unwrap()), vec![vec!["2"]]);
}
