//! Read JSON-encoded query strings, one per line, and emit results as JSON lines.
//! Also accepts {"query": "...", "setup": ["..."], "parameters": {"name": value}}.
//! Each request gets a fresh graph; setup statements share that graph with its query.
//! External corpora and output belong outside the repository.
//! Build with `cargo build --example cypher_corpus_probe`, then pipe JSON lines
//! to `target/debug/examples/cypher_corpus_probe`. Set ORCHIDDB_EXPLAIN_DAG to
//! print logical/physical plans to stderr without altering the JSON output.
use futures::FutureExt;
use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::ir::value::Value;
use orchiddb::language::cypher::parameters::bind_parameters;
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;
use std::io::{self, BufRead};
use std::panic::AssertUnwindSafe;

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum Request {
    Query(String),
    Parameterized {
        query: String,
        #[serde(default)]
        setup: Vec<String>,
        #[serde(default)]
        parameters: BTreeMap<String, serde_json::Value>,
    },
}

fn parameter(value: serde_json::Value) -> Value {
    match value {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(v) => Value::Bool(v),
        serde_json::Value::Number(v) => v
            .as_i64()
            .map(Value::Int)
            .unwrap_or_else(|| Value::Float(v.as_f64().expect("numeric parameter"))),
        serde_json::Value::String(v) => Value::String(v),
        serde_json::Value::Array(v) => Value::List(v.into_iter().map(parameter).collect()),
        serde_json::Value::Object(v) => {
            Value::Map(v.into_iter().map(|(k, v)| (k, parameter(v))).collect())
        }
    }
}

// Match GraphEngine's frontend preparation, including procedure resolution both
// before parameter binding and after it (for argument type validation).
fn plan_query(
    query: &str,
    parameters: &BTreeMap<String, Value>,
) -> Result<orchiddb::ir::plan::GraphPlan, String> {
    let mut ast = parse_query(query).map_err(|e| format!("parse: {e}"))?;
    let catalog = Default::default();
    orchiddb::language::cypher::procedures::prepare(&mut ast, &catalog)
        .map_err(|e| format!("prepare: {e}"))?;
    bind_parameters(&mut ast, parameters).map_err(|e| format!("parameters: {e}"))?;
    orchiddb::language::cypher::procedures::prepare(&mut ast, &catalog)
        .map_err(|e| format!("prepare: {e}"))?;
    CypherPlanner::new()
        .plan(&ast)
        .map_err(|e| format!("plan: {e}"))
}

#[tokio::main]
async fn main() {
    for line in io::stdin().lock().lines() {
        let request: Request =
            serde_json::from_str(&line.expect("read query")).expect("JSON query");
        let (query, setup, parameters) = match request {
            Request::Query(query) => (query, Vec::new(), BTreeMap::new()),
            Request::Parameterized {
                query,
                setup,
                parameters,
            } => (
                query,
                setup,
                parameters
                    .into_iter()
                    .map(|(name, value)| (name, parameter(value)))
                    .collect(),
            ),
        };
        let result = AssertUnwindSafe(async move {
            let graph = PropertyGraph::new();
            for statement in setup {
                let plan = plan_query(&statement, &parameters).map_err(|e| format!("setup {e}"))?;
                orchiddb::ir::rel::runtime::execute(
                    &plan,
                    &graph,
                    Some(std::time::Duration::from_secs(10)),
                )
                .await
                .map_err(|e| format!("setup execution: {e}"))?;
            }
            let plan = plan_query(&query, &parameters)?;
            let (result, stats) = orchiddb::ir::rel::runtime::execute(
                &plan,
                &graph,
                Some(std::time::Duration::from_secs(10)),
            )
            .await?;
            if std::env::var_os("ORCHIDDB_EXPLAIN_DAG").is_some() {
                eprintln!(
                    "Logical: {}\nPhysical: {}",
                    stats.logical_plan, stats.physical_plan
                );
            }
            let typed: Option<serde_json::Value> = result
                .batch
                .schema()
                .metadata()
                .get("orchiddb.cypher.typed_rows.v1")
                .map(|text| serde_json::from_str(text).expect("typed result metadata"));
            let rows = (0..result.batch.num_rows())
                .map(|row| {
                    result
                        .batch
                        .columns()
                        .iter()
                        .map(|column| {
                            if column.is_null(row) {
                                None
                            } else {
                                Some(
                                    arrow::util::display::array_value_to_string(column, row)
                                        .unwrap(),
                                )
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            Ok::<_, String>(serde_json::json!({"rows": rows, "typed": typed}))
        })
        .catch_unwind()
        .await;
        let output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => serde_json::json!({"error": error}),
            Err(_) => serde_json::json!({"panic": "query panicked"}),
        };
        println!("{output}");
    }
}
