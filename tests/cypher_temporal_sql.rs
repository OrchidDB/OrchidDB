#![cfg(feature = "duckdb")]
use orchiddb::engine::GraphEngine;
use serde_json::{Value, json};

fn typed(result: &orchiddb::engine::QueryResult) -> Value {
    serde_json::from_str(
        &result.returned.batch.schema().metadata()["orchiddb.cypher.typed_rows.v1"],
    )
    .unwrap()
}

#[tokio::test]
async fn constant_temporal_results_use_lossless_sql_transport() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for (expression, kind, text) in [
        ("date('1984-10-11')", "date", "1984-10-11"),
        (
            "localtime('12:31:14.645876123')",
            "localtime",
            "12:31:14.645876123",
        ),
        (
            "time('12:31:14.645876123+01:00')",
            "time",
            "12:31:14.645876123+01:00",
        ),
        (
            "localdatetime('1984-10-11T12:31:14.645876123')",
            "localdatetime",
            "1984-10-11T12:31:14.645876123",
        ),
        (
            "datetime('1984-10-11T12:31:14.645876123+01:00[Europe/Stockholm]')",
            "datetime",
            "1984-10-11T12:31:14.645876123+01:00[Europe/Stockholm]",
        ),
        (
            "date({year: 1000000, month: 1, day: 1})",
            "date",
            "+1000000-01-01",
        ),
    ] {
        let query = format!("RETURN {expression} AS value");
        // Exercise the portable SQL backend with native scalar evaluation
        // disabled, as well as the production scheduler below.
        let plan = orchiddb::language::cypher::planner::CypherPlanner::new()
            .plan(&orchiddb::language::cypher::parser::parse_query(&query).unwrap())
            .unwrap();
        let lowered = orchiddb::ir::rel::RelBackend::new()
            .with_language_functions(true)
            .lower(&plan, &orchiddb::ir::catalog::PropertyGraph::new())
            .unwrap();
        let sql_result = orchiddb::ir::rel::sql::execute_lowered_sql(
            &mut orchiddb::ir::rel::sql::DuckDbExecutor::default(),
            &lowered,
        )
        .await
        .unwrap();
        let scalar =
            datafusion::common::ScalarValue::try_from_array(sql_result.batch.column(0), 0).unwrap();
        let payload = orchiddb::ir::functions::domain::storage(&scalar).unwrap();
        if kind == "date" && text == "1984-10-11" {
            assert!(matches!(payload, datafusion::common::ScalarValue::Date32(Some(_))));
        } else {
            assert_eq!(payload, datafusion::common::ScalarValue::Utf8(Some(format!("{kind}|{text}"))), "{query}");
        }
        let result = engine.cypher(&query).await.unwrap();
        assert_eq!(
            typed(&result),
            json!([[{"type":"cypher_temporal", "value":{"kind":kind,"text":text}}]]),
            "{query}"
        );
        assert_eq!(
            result.stats.cost.native_kernel_calls, 0,
            "{query}\n{}",
            result.stats.physical_plan
        );
        assert!(!result.stats.sql_queries.is_empty());
    }
}

#[tokio::test]
async fn transported_temporals_survive_aliases_lists_and_runtime_consumers() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in [
        "WITH date('1984-10-11') AS d RETURN d.year AS year",
        "UNWIND [date('1984-10-11')] AS d RETURN d.year AS year",
    ] {
        let result = engine.cypher(query).await.unwrap();
        assert_eq!(
            typed(&result)[0][0]["value"],
            json!(1984),
            "{query}: {}",
            typed(&result)
        );
    }
    let result = engine
        .cypher("UNWIND [date('1984-10-11'),date('1910-05-06')] AS d RETURN d ORDER BY d")
        .await
        .unwrap();
    assert_eq!(typed(&result)[0][0]["value"]["text"], "1910-05-06");
    assert_eq!(typed(&result)[1][0]["value"]["text"], "1984-10-11");
    assert_eq!(result.stats.cost.native_kernel_calls, 0);
    let result = engine
        .cypher("WITH date('1984-10-11') AS d RETURN d AS copy")
        .await
        .unwrap();
    assert_eq!(typed(&result)[0][0]["value"]["kind"], "date");
    assert_eq!(result.stats.cost.native_kernel_calls, 0);
}

#[tokio::test]
async fn temporal_alias_folding_is_portable_and_keeps_empty_inputs_empty() {
    use orchiddb::ir::{
        catalog::PropertyGraph,
        rel::{
            RelBackend,
            sql::{DuckDbExecutor, execute_lowered_sql},
        },
    };
    use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
    for (query, expected) in [
        ("WITH date('1984-10-11') AS d RETURN d.year AS year", "1984"),
        (
            "WITH datetime('1984-10-11T12:31+01:00') AS other RETURN date({date:other, ordinalDay:28}) AS result",
            "1984-01-28",
        ),
        (
            "RETURN duration({months:1, days:2, seconds:3, nanoseconds:4}) AS result",
            "duration|1|2|3|4",
        ),
        (
            "WITH duration({months:1, days:2, seconds:3, nanoseconds:4}) AS d RETURN d.nanosecondsOfSecond AS n",
            "4",
        ),
    ] {
        let plan = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        let lowered = RelBackend::new()
            .with_language_functions(true)
            .lower(&plan, &PropertyGraph::new())
            .unwrap();
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await
            .unwrap();
        let scalar =
            datafusion::common::ScalarValue::try_from_array(result.batch.column(0), 0).unwrap();
        let scalar = if orchiddb::ir::functions::domain::descriptor(&scalar.data_type()).is_some() {
            orchiddb::ir::functions::domain::storage(&scalar).unwrap()
        } else {
            scalar
        };
        assert_eq!(arrow::util::display::array_value_to_string(scalar.to_array().unwrap().as_ref(), 0).unwrap(), expected, "{query}");
    }
    let query = "UNWIND [] AS x RETURN date('1984-10-11') AS d";
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    let lowered = RelBackend::new()
        .lower(&plan, &PropertyGraph::new())
        .unwrap();
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await
        .unwrap();
    assert_eq!(result.batch.num_rows(), 0);
}

#[tokio::test]
async fn temporal_transport_never_uses_payload_order_for_temporal_operations() {
    use orchiddb::ir::{catalog::PropertyGraph, rel::RelBackend};
    use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
    for query in [
        "UNWIND [date('1984-10-11'),date('1910-05-06')] AS d RETURN min(d)",
        "UNWIND [date('1984-10-11'),date('1910-05-06')] AS d RETURN DISTINCT d",
        "RETURN datetime.realtime()",
        "UNWIND [1984,1985] AS y RETURN date({year:y})",
    ] {
        let plan = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        // Row-dependent operations and realtime clocks retain their typed evaluator.
        let lowered = RelBackend::new()
            .with_language_functions(true)
            .lower(&plan, &PropertyGraph::new());
        assert!(lowered.is_err(), "{query}");
    }
}

#[tokio::test]
async fn temporal_transport_has_a_postgres_sql_mapping() {
    use orchiddb::ir::{
        catalog::PropertyGraph,
        rel::{
            RelBackend,
            sql::{self, SqlDialect},
        },
    };
    use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
    let plan = CypherPlanner::new()
        .plan(&parse_query("RETURN localtime('12:31:14.645876123') AS t").unwrap())
        .unwrap();
    let lowered = RelBackend::new()
        .lower(&plan, &PropertyGraph::new())
        .unwrap();
    let prepared = sql::prepare(&lowered, SqlDialect::Postgres).await.unwrap();
    assert!(prepared.query.contains("localtime|12:31:14.645876123"));
    assert!(prepared.query.to_ascii_lowercase().contains("text"));
}

#[tokio::test]
async fn week_date_is_native_duckdb_date_and_preserves_cypher_identity() {
    let query = "RETURN date({year: 1816, week: 1}) AS d";
    let mut engine = GraphEngine::in_memory().unwrap();
    let result = engine.cypher(query).await.unwrap();
    assert_eq!(typed(&result), json!([[{"type":"cypher_temporal", "value":{"kind":"date","text":"1816-01-01"}}]]));
    let plan = orchiddb::language::cypher::planner::CypherPlanner::new()
        .plan(&orchiddb::language::cypher::parser::parse_query(query).unwrap()).unwrap();
    let lowered = orchiddb::ir::rel::RelBackend::new().with_language_functions(true)
        .lower(&plan, &orchiddb::ir::catalog::PropertyGraph::new()).unwrap();
    let sql = orchiddb::ir::rel::sql::unparse(&lowered, orchiddb::ir::rel::sql::SqlDialect::DuckDb).unwrap();
    assert!(!sql.contains("date|"), "{sql}");
    let conn = duckdb::Connection::open_in_memory().unwrap();
    let (ty, text): (String, String) = conn.query_row(
        &format!("SELECT typeof(d), CAST(d AS VARCHAR) FROM ({sql})"), [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    ).unwrap();
    assert_eq!((ty, text), ("DATE".into(), "1816-01-01".into()), "{sql}");
}

#[tokio::test]
async fn native_dates_keep_null_lists_and_mixed_temporal_values() {
    let mut engine = GraphEngine::in_memory().unwrap();
    for query in [
        "UNWIND [date('1816-01-01'), null, date('2000-02-29')] AS d RETURN d",
        "UNWIND [date('1816-01-01'), localtime('12:00')] AS d RETURN d",
        "UNWIND [date('1816-01-01'), date({year:1000000})] AS d RETURN d",
        "RETURN [[date('1816-01-01'), null], []] AS d",
    ] {
        let result = engine.cypher(query).await.unwrap();
        assert!(!typed(&result).as_array().unwrap().is_empty(), "{query}");
        assert_eq!(result.stats.cost.native_kernel_calls, 0, "{query}");
    }
}
