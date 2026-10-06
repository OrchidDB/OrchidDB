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
        .cypher("CREATE (:P {x: 1}), (:P {x: 1}), (:P {x: 2})")
        .await
        .unwrap();
    for q in [
        "MATCH (p:P) RETURN p.x, count(*)",
        "MATCH (p:P) RETURN DISTINCT p.x",
        "MATCH (p:P) RETURN count(DISTINCT p.x)",
    ] {
        engine.set_language_functions(false).unwrap();
        let before = engine.cypher(q).await.unwrap();
        engine.set_language_functions(true).unwrap();
        let after = engine.cypher(q).await.unwrap();
        assert_eq!(rows(&before), rows(&after), "{q}");
        assert!(
            after
                .stats
                .sql_queries
                .iter()
                .any(|s| s.contains("__orchiddb_lang_cypher_key")),
            "{q}: {:?}",
            after.stats
        );
        println!(
            "{q}\nbefore={} after={}\n{}",
            before.stats.datafusion_ops,
            after.stats.datafusion_ops,
            after.stats.sql_queries.join("\n")
        );
    }
}
#[tokio::test]
async fn gremlin_functions_move_into_duckdb_without_changing_results() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:P {name: '  Ab😀 cd  '}), (:P {name: 'Other'})")
        .await
        .unwrap();
    for q in [
        "g.V().values('name').toUpper()",
        "g.V().values('name').trim()",
        "g.V().values('name').reverse()",
        "g.V().values('name').split(' ')",
        "g.V().values('name').length()",
        "g.V().values('name').substring(1,4)",
    ] {
        engine.set_language_functions(false).unwrap();
        let before = engine.gremlin(q).await.unwrap();
        engine.set_language_functions(true).unwrap();
        let after = engine.gremlin(q).await.unwrap();
        assert_eq!(rows(&before), rows(&after), "{q}");
        assert!(
            after
                .stats
                .sql_queries
                .iter()
                .any(|s| s.contains("__orchiddb_lang_")),
            "{q}: {:?}",
            after.stats
        );
        println!(
            "{q}: before={} after={}",
            before.stats.datafusion_ops, after.stats.datafusion_ops
        );
    }
}
#[tokio::test]
async fn sparql_functions_use_the_optional_registry() {
    use orchiddb::{
        ir::rel::{mapping::GraphMapping, rdf::RdfDatasetMapping},
        language::sparql::SparqlPlanner,
    };
    use std::sync::Arc;
    let mapping = GraphMapping::new().with_rdf_mapping(RdfDatasetMapping::new());
    let mut engine = GraphEngine::mapped(
        duckdb::Connection::open_in_memory().unwrap(),
        Arc::new(mapping),
    )
    .unwrap();
    for q in [
        "SELECT (ENCODE_FOR_URI(?v) AS ?x) WHERE { VALUES ?v { \"a b\" \"é\" } }",
        "SELECT (REPLACE(?v, \"a\", \"X\") AS ?x) WHERE { VALUES ?v { \"abc\" \"aaa\" } }",
    ] {
        let plan = SparqlPlanner::new("default").plan_str(q).unwrap();
        engine.set_language_functions(false).unwrap();
        let before = engine.execute_plan(&plan).await.unwrap();
        engine.set_language_functions(true).unwrap();
        let after = engine.execute_plan(&plan).await.unwrap();
        assert_eq!(rows(&before), rows(&after));
        assert!(
            after
                .stats
                .sql_queries
                .iter()
                .any(|s| s.contains("__orchiddb_lang_sparql")),
            "{:?}",
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

#[tokio::test]
async fn nulls_empty_groups_and_unsupported_shapes_preserve_results() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:P {x: 1}), (:P), (:P {x: 2})")
        .await
        .unwrap();
    for q in [
        "MATCH (p:P) RETURN p.x, count(*)",
        "MATCH (p:P) WHERE p.x > 10 RETURN p.x, count(*)",
        "MATCH (p:P) WHERE p.x > 10 RETURN count(DISTINCT p.x)",
        "MATCH (p:P) RETURN count(DISTINCT p)",
        "UNWIND [1, 1.0] AS x RETURN x, count(*)",
        "UNWIND [[1], [1.0]] AS x RETURN count(DISTINCT x)",
        "UNWIND [1, 1.0] AS x RETURN collect(DISTINCT x)",
        "RETURN 1 AS n UNION RETURN 1.0 AS n",
    ] {
        engine.set_language_functions(false).unwrap();
        let before = engine.cypher(q).await.unwrap();
        engine.set_language_functions(true).unwrap();
        let after = engine.cypher(q).await.unwrap();
        assert_eq!(rows(&before), rows(&after), "{q}");
    }
    for q in [
        "g.V().values('x').path()",
        "g.V().as('p').values('x').select('p')",
    ] {
        engine.set_language_functions(false).unwrap();
        let before = engine.gremlin(q).await.unwrap();
        engine.set_language_functions(true).unwrap();
        let after = engine.gremlin(q).await.unwrap();
        assert_eq!(rows(&before), rows(&after), "{q}");
    }
    engine.set_sql_timeout(std::time::Duration::from_secs(10));
    let after_reset = engine
        .cypher("MATCH (p:P) RETURN count(DISTINCT p.x)")
        .await
        .unwrap();
    assert!(
        after_reset
            .stats
            .sql_queries
            .iter()
            .any(|s| s.contains("__orchiddb_lang_cypher_key"))
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
        .cypher("CREATE (:P {x: 1}), (:P {x: 2})")
        .await
        .unwrap();
    let query = "MATCH (p:P) RETURN count(DISTINCT p.x)";
    let enabled = engine.cypher(query).await.unwrap();
    assert!(
        enabled
            .stats
            .sql_queries
            .iter()
            .any(|sql| sql.contains("__orchiddb_lang_cypher_key"))
    );
    engine.set_language_functions(false).unwrap();
    let disabled = engine.cypher(query).await.unwrap();
    assert_eq!(rows(&enabled), rows(&disabled));
    assert!(
        disabled
            .stats
            .sql_queries
            .iter()
            .all(|sql| !sql.contains("__orchiddb_lang_cypher_key"))
    );
}
