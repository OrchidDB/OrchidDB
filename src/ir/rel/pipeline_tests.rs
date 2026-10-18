//! Behavioral integration tests across managed and terminal preparation routes.
use super::*;
use crate::engine::{GraphEngine, ReadMode};
use crate::ir::rules::{EngineIdentity, RuleContext, StringToFloatPolicy};
use arrow::array::Float64Array;

fn native_context() -> RuleContext {
    let mut context = RuleContext::default();
    context.profile.string_to_float = StringToFloatPolicy::DuckDbNative;
    context.owner = Some(EngineIdentity {
        name: "duckdb".into(),
        version: "live-session".into(),
    });
    context.trace_limit = 128;
    context
}

#[tokio::test]
async fn managed_native_policy_survives_literal_parameter_and_nested_routes() {
    for mode in [ReadMode::Hybrid, ReadMode::SqlOnly] {
        let mut engine = GraphEngine::in_memory().unwrap();
        engine.set_read_mode(mode);
        engine.set_rule_context(native_context());
        for query in [
            "RETURN toFloat('NaN') AS x",
            "RETURN toFloat($value) AS x",
            "WITH 'NaN' AS value RETURN toFloat(value) AS x",
            "WITH toFloat('NaN') AS value RETURN value AS x",
        ] {
            let result = engine
                .cypher_with_params(
                    query,
                    &BTreeMap::from([("value".into(), Value::String("NaN".into()))]),
                )
                .await
                .unwrap_or_else(|e| panic!("{mode:?} {query}: {e}"));
            let values = result
                .returned
                .batch
                .column(0)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap();
            assert!(values.value(0).is_nan(), "{mode:?} {query}");
            assert!(
                result
                    .stats
                    .rule_trace
                    .iter()
                    .any(|trace| trace.rule_id == "semantic.cypher.string_to_float"
                        && trace.evidence.contains("DuckDB-native")),
                "{mode:?}: {:?}",
                result.stats.rule_trace
            );
        }
    }
}

#[tokio::test]
async fn managed_helper_optout_retains_compatible_parser() {
    crate::ir::diagnostics::without_constant_folding(async {
        for mode in [ReadMode::Hybrid, ReadMode::SqlOnly] {
            let mut engine = GraphEngine::in_memory().unwrap();
            engine.set_read_mode(mode);
            let mut context = RuleContext::default();
            context.trace_limit = 128;
            engine.set_rule_context(context);
            // Exercise an already initialized session, then explicitly opt out.
            engine.cypher("RETURN 1").await.unwrap();
            engine.set_language_functions(false).unwrap();
            let result = engine
                .cypher("WITH '__1__2__' AS value RETURN toFloat(value) AS x")
                .await
                .unwrap();
            let values = result
                .returned
                .batch
                .column(0)
                .as_any()
                .downcast_ref::<Float64Array>()
                .unwrap();
            assert_eq!(values.value(0), 12.0);
            assert!(
                result
                    .stats
                    .sql_queries
                    .iter()
                    .all(|sql| !sql.contains(language_functions::CYPHER_TO_FLOAT))
            );
            assert!(
                result
                    .stats
                    .rule_trace
                    .iter()
                    .any(|trace| trace.rule_id == "semantic.cypher.string_to_float"
                        && trace.evidence.contains("conformant guarded SQL")),
                "{mode:?}: {:?}",
                result.stats.rule_trace
            );
        }
    })
    .await;
}

#[tokio::test]
async fn required_validation_covers_compact_and_ordinary_emission_without_optimization() {
    for (query, compact) in [
        ("WITH [1,2,3] AS list UNWIND list AS x RETURN x", true),
        ("UNWIND [1,2,3] AS x RETURN x + 1 AS y", false),
    ] {
        let graph =
            crate::language::cypher::preparation::prepare(query, &BTreeMap::new(), None).unwrap();
        let mut options = RelBackendOptions::default();
        options.rule_context.trace_limit = 128;
        options.rule_context.optimize = false;
        let lowered = RelBackend::with_options(options)
            .lower(&graph, &PropertyGraph::new())
            .unwrap();
        let text = sql::unparse(&lowered, sql::SqlDialect::DuckDb).unwrap();
        assert!(!text.is_empty());
        let trace = lowered.rule_context.trace();
        let validation = trace
            .iter()
            .position(|t| t.rule_id == "adapter.validate_contract" && t.outcome == "validated")
            .expect("required validation cannot be bypassed");
        let emission = trace
            .iter()
            .position(|t| t.rule_id == "adapter.compact_unwind");
        assert_eq!(emission.is_some(), compact, "{query}: {trace:?}");
        if let Some(emission) = emission {
            assert!(validation < emission);
        }
    }
}

#[test]
fn layout_filter_preparation_preserves_native_evaluation_ownership() {
    let graph = crate::language::cypher::preparation::prepare(
        "RETURN toFloat('NaN') AS x", &BTreeMap::new(), None,
    ).unwrap();
    let mut options = RelBackendOptions::default();
    options.rule_context = native_context();
    let mut lowered = RelBackend::with_options(options).lower(&graph, &PropertyGraph::new()).unwrap();
    lowered.plan = super::layout::push_filters(lowered.plan).unwrap();
    assert!(sql::unparse(&lowered, sql::SqlDialect::DuckDb).unwrap().contains("TRY_CAST"));
}

#[test]
fn required_placement_budget_failure_is_not_an_unsupported_fallback() {
    let graph = crate::language::cypher::preparation::prepare(
        "UNWIND [1,2,3] AS x RETURN x + 1 AS y", &BTreeMap::new(), None,
    ).unwrap();
    let mut lowered = RelBackend::default().lower(&graph, &PropertyGraph::new()).unwrap();
    // Allow required entry validation, then exhaust the query before placement.
    lowered.rule_context.budget.query_visits = lowered.rule_context.visits() + 1;
    let error = sql::unparse(&lowered, sql::SqlDialect::DuckDb).unwrap_err();
    assert!(matches!(error, sql::SqlError::RulePipeline(_)), "{error}");
    assert!(error.to_string().contains("placement.representation"));
}

#[test]
fn reused_backend_starts_a_fresh_bounded_query() {
    let graph =
        crate::language::cypher::preparation::prepare("RETURN 1 AS x", &BTreeMap::new(), None)
            .unwrap();
    let mut options = RelBackendOptions::default();
    options.rule_context.budget.query_visits = 64;
    options.rule_context.budget.stage_visits = 64;
    options.rule_context.trace_limit = 8;
    let backend = RelBackend::with_options(options);
    let mut expected_visits = None;
    for _ in 0..128 {
        let lowered = backend.lower(&graph, &PropertyGraph::new()).unwrap();
        sql::unparse(&lowered, sql::SqlDialect::DuckDb).unwrap();
        let visits = lowered.rule_context.visits();
        assert!(visits > 0 && visits <= 64);
        assert_eq!(*expected_visits.get_or_insert(visits), visits);
        assert!(lowered.rule_context.trace().len() <= 8);
    }
}

#[tokio::test]
async fn managed_native_policy_survives_error_and_session_recreation() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.set_read_mode(ReadMode::Hybrid);
    engine.set_rule_context(native_context());
    assert!(engine.cypher("RETURN 1 / 0 AS x").await.is_err());
    let result = engine.cypher("RETURN toFloat('NaN') AS x").await.unwrap();
    assert!(
        result
            .returned
            .batch
            .column(0)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(0)
            .is_nan()
    );
    engine.set_sql_timeout(std::time::Duration::from_secs(10));
    let result = engine.cypher("RETURN toFloat('NaN') AS x").await.unwrap();
    assert!(
        result
            .returned
            .batch
            .column(0)
            .as_any()
            .downcast_ref::<Float64Array>()
            .unwrap()
            .value(0)
            .is_nan()
    );
    assert!(
        result
            .stats
            .rule_trace
            .iter()
            .any(|trace| trace.rule_id == "semantic.cypher.string_to_float"
                && trace.evidence.contains("DuckDB-native"))
    );
}

#[tokio::test]
async fn correlated_execution_keeps_one_bounded_trace() {
    let mut engine = GraphEngine::in_memory().unwrap();
    engine.cypher("CREATE (:P {k:1})").await.unwrap();
    let mut context = RuleContext::default();
    context.trace_limit = 8;
    engine.set_rule_context(context);
    let result = engine
        .cypher("UNWIND range(1,32) AS x RETURN EXISTS { MATCH (n:P) WHERE n.k = x } AS found")
        .await
        .unwrap();
    assert_eq!(result.returned.batch.num_rows(), 32);
    assert!(!result.stats.rule_trace.is_empty());
    assert!(
        result.stats.rule_trace.len() <= 8,
        "{:?}",
        result.stats.rule_trace
    );
}
