//! Reproducible enabled/disabled comparison, including result parity.
use arrow::datatypes::{DataType, Field, Schema};
use orchiddb::{
    engine::{GraphEngine, QueryResult},
    ir::{
        plan::GraphPlan,
        rel::{
            mapping::{GraphMapping, NodeMapping},
            rdf::RdfDatasetMapping,
        },
    },
    language::{
        cypher::{parse_query, planner::CypherPlanner},
        sparql::SparqlPlanner,
    },
};
use std::{sync::Arc, time::Instant};

fn values(result: &QueryResult) -> Vec<Vec<String>> {
    let batch = &result.returned.batch;
    let mut rows = (0..batch.num_rows())
        .map(|r| {
            batch
                .columns()
                .iter()
                .map(|a| arrow::util::display::array_value_to_string(a, r).unwrap())
                .collect()
        })
        .collect::<Vec<Vec<_>>>();
    rows.sort();
    rows
}
#[tokio::main]
async fn main() {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("SET threads=1; CREATE TABLE people AS SELECT i::BIGINT AS id, (i % 100)::BIGINT AS x, ' hello 😀 '::VARCHAR AS name FROM range(10000) t(i)").unwrap();
    let mut mapping = GraphMapping::new().with_rdf_mapping(RdfDatasetMapping::new());
    mapping
        .register_table_schema(
            "people",
            Arc::new(Schema::new(vec![
                Field::new("id", DataType::Int64, false),
                Field::new("x", DataType::Int64, true),
                Field::new("name", DataType::Utf8, true),
            ])),
        )
        .map_node(
            NodeMapping::table("P", "people", "id")
                .property("x", "x")
                .property("name", "name"),
        );
    let mut engine = GraphEngine::mapped(connection, Arc::new(mapping)).unwrap();
    let mut comparisons = Vec::new();
    for (language, query) in [
        ("cypher", "MATCH (p:P) RETURN p.x, count(*)"),
        ("cypher", "MATCH (p:P) RETURN count(DISTINCT p.x)"),
        ("gremlin", "g.V().values('name').toUpper().trim()"),
        (
            "sparql",
            "SELECT (ENCODE_FOR_URI(?v) AS ?x) WHERE { VALUES ?v { \"a b\" \"é\" } }",
        ),
    ] {
        let plan: GraphPlan = match language {
            "cypher" => CypherPlanner::new()
                .plan(&parse_query(query).unwrap())
                .unwrap(),
            "sparql" => SparqlPlanner::new("default").plan_str(query).unwrap(),
            _ => {
                // Use the engine parser while collecting the same public stats.
                engine.set_language_functions(false).unwrap();
                let before = engine.gremlin(query).await.unwrap();
                engine.set_language_functions(true).unwrap();
                let after = engine.gremlin(query).await.unwrap();
                assert_eq!(values(&before), values(&after));
                let mut timings = [Vec::new(), Vec::new()];
                for enabled in [false, true] {
                    engine.set_language_functions(enabled).unwrap();
                    for _ in 0..5 {
                        let t = Instant::now();
                        let r = engine.gremlin(query).await.unwrap();
                        timings[enabled as usize].push(t.elapsed().as_secs_f64() * 1000.0);
                        assert_eq!(values(&before), values(&r));
                    }
                }
                comparisons.push(report(language, query, &before, &after, timings));
                continue;
            }
        };
        engine.set_language_functions(false).unwrap();
        let before = engine.execute_plan(&plan).await.unwrap();
        engine.set_language_functions(true).unwrap();
        let after = engine.execute_plan(&plan).await.unwrap();
        assert_eq!(values(&before), values(&after));
        let mut timings = [Vec::new(), Vec::new()];
        for enabled in [false, true] {
            engine.set_language_functions(enabled).unwrap();
            for _ in 0..5 {
                let t = Instant::now();
                let r = engine.execute_plan(&plan).await.unwrap();
                timings[enabled as usize].push(t.elapsed().as_secs_f64() * 1000.0);
                assert_eq!(values(&before), values(&r));
            }
        }
        comparisons.push(report(language, query, &before, &after, timings));
    }
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"profile":"unoptimized dev", "rows":10000,"duckdb_threads":1,"trials":5,"result_parity":true,"comparisons":comparisons})).unwrap());
}
fn report(
    language: &str,
    query: &str,
    before: &QueryResult,
    after: &QueryResult,
    timings: [Vec<f64>; 2],
) -> serde_json::Value {
    let phases = [before,after].into_iter().zip(timings).map(|(r,mut times)|{times.sort_by(f64::total_cmp);serde_json::json!({"median_ms":times[2],"trials_ms":times,"residual_operators":r.stats.datafusion_ops,"native_kernel_calls":r.stats.cost.native_kernel_calls,"sql_executions":r.stats.cost.sql_executions,"result_rows":r.returned.batch.num_rows(),"sql":r.stats.sql_queries})}).collect::<Vec<_>>();
    serde_json::json!({"language":language,"query":query,"before":phases[0],"after":phases[1]})
}
