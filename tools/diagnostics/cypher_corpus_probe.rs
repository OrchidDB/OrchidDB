//! Read JSON-encoded query strings, one per line, and emit results as JSON lines.
//! Also accepts {"query": "...", "setup": ["..."], "parameters": {"name": value}}.
//! Each request gets a fresh graph; setup statements share that graph with its query.
//! External corpora and output belong outside the repository.
//! Build with `cargo build --example cypher_corpus_probe`, then pipe JSON lines
//! to `target/debug/examples/cypher_corpus_probe`. Set ORCHIDDB_EXPLAIN_DAG to
//! print logical/physical plans to stderr without altering the JSON output.
//! Pass `--sql` to compile queries without executing them (setup still runs).
//! `--conformance-sql` uses the conformance runner's real relational fixture
//! and execution preparation path (requires `duckdb`). Supply `scenario_queries`
//! to infer the same schema from all scenario statements as the runner does.
//! It exports prepared SQL regions and residual operator counts without executing
//! the target query; only fixture setup is executed.
//! `--mapped-sql` compiles against declared physical SQL tables and exports
//! `fixture_sql` (CREATE/INSERT) separately. A diagnostic request may include
//! `schema: {nodes: [...], edges: [...]}`; otherwise fixture types are inferred.
//! `--describe-fixture` returns that inferred declaration without compilation.
//! Unsupported fixture representations fail explicitly, without snapshot fallback.
//! `--no-constant-folding` (with any SQL export mode) disables optional compiler
//! constant evaluation for structural SQL review. Fixture setup runs normally.
//! Literal encoding, type proofs, structural rewrites, and DuckDB's own bind/
//! execution optimization are retained; this is not a no-optimization mode.
//! `--rule-metrics` adds lowering/preparation time, visits, revision and SQL size.
//! `--rule-trace` additionally enables the bounded shared rule trace (256 entries).
//! Both are diagnostic-only and leave ordinary export records unchanged.
mod cypher_fixture_schema;
mod cypher_noop;
#[cfg(feature = "duckdb")]
#[path = "../../conformance/runner/src/relational_fixture.rs"]
mod relational_fixture;
use futures::FutureExt;
use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::ir::value::Value;
use std::collections::BTreeMap;
use std::io::{self, BufRead};
use std::panic::AssertUnwindSafe;

#[derive(serde::Deserialize)]
struct Procedure {
    name: String,
    inputs: Vec<orchiddb::ir::procedures::ProcedureField>,
    outputs: Vec<orchiddb::ir::procedures::ProcedureField>,
    rows: Vec<Vec<serde_json::Value>>,
}

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
        #[serde(default)]
        procedures: Vec<Procedure>,
        #[serde(default)]
        schema: Option<cypher_fixture_schema::Declaration>,
        #[serde(default)]
        scenario_queries: Vec<String>,
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

// Reuse GraphEngine's frontend preparation, including semantic validation,
// parameter binding and procedure resolution.
fn plan_query(
    query: &str,
    parameters: &BTreeMap<String, Value>,
    catalog: &orchiddb::ir::procedures::ProcedureCatalog,
) -> Result<orchiddb::ir::plan::GraphPlan, String> {
    orchiddb::language::cypher::preparation::prepare(query, parameters, Some(catalog))
        .map_err(|error| error.message)
}

// Bind against a real DuckDB catalog with the same language functions as
// execution. Only fixture DDL/DML runs: preparation does not run the query.
#[cfg(feature = "duckdb")]
fn validate_mapped_sql(output: &mut serde_json::Value) {
    use orchiddb::ir::rel::sql::DuckDbExecutor;
    if output["execution_kind"] == "no_op" {
        output["validation"] = serde_json::json!("no_execution_required");
        output["fixture_loaded"] = serde_json::json!(false);
        return;
    }
    let setup = output["fixture_sql"].as_array().unwrap().iter()
        .map(|value| value.as_str().unwrap().to_owned()).collect::<Vec<_>>();
    let query = output["sql"].as_str().map(str::to_owned);
    output["fixture_loaded"] = serde_json::json!(false);
    let mut executor = DuckDbExecutor::new();
    let loaded = (|| -> Result<(), String> {
        executor.set_language_functions(true).map_err(|e| e.to_string())?;
        let connection = executor.connection().map_err(|e| e.to_string())?;
        for statement in &setup {
            connection.execute_batch(statement).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(error) = loaded {
        output["validation"] = serde_json::json!("fixture_error");
        output["validation_error"] = serde_json::json!(error);
        return;
    }
    output["fixture_loaded"] = serde_json::json!(true);
    let Some(query) = query else {
        output["validation"] = serde_json::json!("not_lowered");
        return;
    };
    let bound = (|| -> Result<(), String> {
        let connection = executor.connection().map_err(|e| e.to_string())?;
        let _statement = connection.prepare(&query).map_err(|e| e.to_string())?;
        Ok(())
    })();
    match bound {
        Ok(()) => output["validation"] = serde_json::json!("bound_to_real_tables"),
        Err(error) => {
            output["validation"] = serde_json::json!("binding_error");
            output["validation_error"] = serde_json::json!(error);
        }
    }
}

#[tokio::main]
async fn main() {
    let rule_trace = std::env::args().any(|arg| arg == "--rule-trace");
    let rule_metrics = rule_trace || std::env::args().any(|arg| arg == "--rule-metrics");
    let conformance_sql = std::env::args().any(|arg| arg == "--conformance-sql");
    let mapped_sql = std::env::args().any(|arg| arg == "--mapped-sql");
    let describe_fixture = std::env::args().any(|arg| arg == "--describe-fixture");
    let sql_only = conformance_sql || mapped_sql || std::env::args().any(|arg| arg == "--sql");
    let no_constant_folding = std::env::args().any(|arg| arg == "--no-constant-folding");
    assert!(!no_constant_folding || sql_only, "--no-constant-folding requires --sql or --mapped-sql");
    for line in io::stdin().lock().lines() {
        let request: Request =
            serde_json::from_str(&line.expect("read query")).expect("JSON query");
        let (query, setup, parameters, procedures, schema, scenario_queries) = match request {
            Request::Query(query) => (query, Vec::new(), BTreeMap::new(), Vec::new(), None, Vec::new()),
            Request::Parameterized {
                query,
                setup,
                parameters,
                procedures,
                schema,
                scenario_queries,
            } => (
                query,
                setup,
                parameters
                    .into_iter()
                    .map(|(name, value)| (name, parameter(value)))
                    .collect(),
                procedures,
                schema,
                scenario_queries,
            ),
        };
        let result = AssertUnwindSafe(async move {
            let mut graph = PropertyGraph::new();
            let mut catalog = orchiddb::ir::procedures::ProcedureCatalog::new();
            for procedure in procedures {
                use orchiddb::ir::procedures::{TableProcedure, ProcedureSignature};
                let table = TableProcedure {
                    signature: ProcedureSignature { inputs: procedure.inputs, outputs: procedure.outputs },
                    rows: procedure.rows.into_iter().map(|row| row.into_iter().map(parameter).collect()).collect(),
                };
                table.validate()?;
                graph.register_table_procedure(procedure.name.clone(), table.clone())?;
                catalog.insert(procedure.name, table);
            }
            #[cfg(feature = "duckdb")]
            if conformance_sql {
                let mut fixture = relational_fixture::RelationalFixture::new()?;
                for (name, procedure) in &catalog {
                    fixture.engine.register_table_procedure(name.clone(), procedure.clone())?;
                }
                let target = if no_constant_folding {
                    orchiddb::ir::diagnostics::without_constant_folding(async { plan_query(&query, &parameters, &catalog) }).await?
                } else { plan_query(&query, &parameters, &catalog)? };
                let setup_plans = setup.iter().map(|statement| plan_query(statement, &parameters, &catalog))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut all_plans = setup_plans.clone(); all_plans.push(target.clone());
                if !scenario_queries.is_empty() {
                    all_plans = scenario_queries.iter().filter_map(|q| plan_query(q, &parameters, &catalog).ok()).collect();
                }
                fixture.configure_cypher(&all_plans)?;
                for plan in setup_plans {
                    fixture.prepare(&plan)?;
                    fixture.engine.execute_plan(&plan).await?;
                }
                if cypher_noop::is_noop(&target, &catalog) {
                    return Ok(cypher_noop::output());
                }
                let work = fixture.diagnostic_sql(&target);
                return if no_constant_folding {
                    orchiddb::ir::diagnostics::without_constant_folding(work).await
                } else { work.await };
            }
            for statement in setup {
                let plan = plan_query(&statement, &parameters, &catalog).map_err(|e| format!("setup {e}"))?;
                orchiddb::ir::rel::runtime::execute(
                    &plan,
                    &graph,
                    Some(std::time::Duration::from_secs(10)),
                )
                .await
                .map_err(|e| format!("setup execution: {e}"))?;
            }
            let query_work = async {
            if describe_fixture || mapped_sql {
                let declaration = match schema {
                    Some(schema) => schema,
                    None => match cypher_fixture_schema::discover(&graph) {
                        Ok(schema) => schema,
                        Err(error) => return Ok(serde_json::json!({"error":error,"schema_mode":"declared_physical_tables","fixture_loaded":false,"validation":"fixture_error","validation_error":error})),
                    },
                };
                if describe_fixture { return Ok(serde_json::json!({"fixture_schema": declaration})); }
                let (mapping, fixture_sql) = match cypher_fixture_schema::materialize(&graph, &declaration) {
                    Ok(value) => value,
                    Err(error) => return Ok(serde_json::json!({"fixture_schema":declaration,"error":error,"schema_mode":"declared_physical_tables","fixture_loaded":false,"validation":"fixture_error","validation_error":error})),
                };
                let mut output = serde_json::json!({"fixture_schema": declaration, "fixture_sql": fixture_sql, "schema_mode": "declared_physical_tables"});
                let plan = plan_query(&query, &parameters, &catalog)?;
                if cypher_noop::is_noop(&plan, &catalog) {
                    output.as_object_mut().unwrap().extend(cypher_noop::output().as_object().unwrap().clone());
                    output["validation"] = serde_json::json!("no_execution_required");
                    output["fixture_loaded"] = serde_json::json!(false);
                    return Ok(output);
                }
                use orchiddb::ir::rel::{RelBackend, RelBackendOptions, sql::{self,SqlDialect}};
                let mut options = RelBackendOptions {mapping:Some(mapping.clone()),language_functions:true,..Default::default()};
                options.rule_context.trace_limit = if rule_trace {256} else {0};
                let backend = RelBackend::with_options(options);
                let started = std::time::Instant::now();
                match backend.lower(&plan, &graph) {
                    Ok(lowered) => match sql::prepare_with_external(&lowered,SqlDialect::DuckDb,&mapping.physical_table_names()).await {
                        Ok(prepared) => {
                            let mut setup = output["fixture_sql"].as_array().unwrap().clone();
                            for table in &prepared.tables { setup.extend(sql::table_setup_sql(SqlDialect::DuckDb,table).map_err(|e|e.to_string())?.into_iter().map(serde_json::Value::String)); }
                            setup.extend(prepared.setup.into_iter().map(serde_json::Value::String));
                            output["fixture_sql"]=serde_json::json!(setup);
                            output["sql"]=serde_json::json!(prepared.query);
                            output["execution_kind"]=serde_json::json!("sql");
                            output["requires_sql_execution"]=serde_json::json!(true);
                            if rule_metrics {
                                output["rule_metrics"] = serde_json::json!({
                                    "lower_prepare_micros": started.elapsed().as_micros(),
                                    "visits":lowered.rule_context.visits(),
                                    "plan_revision":lowered.rule_context.revision(),
                                    "sql_bytes":output["sql"].as_str().unwrap().len(),
                                    "trace_enabled":rule_trace,
                                    "trace":lowered.rule_context.trace(),
                                });
                            }
                        },
                        Err(error) => output["lowering_error"]=serde_json::json!(error.to_string()),
                    },
                    Err(error) => output["lowering_error"]=serde_json::json!(error.to_string()),
                }
                #[cfg(feature = "duckdb")]
                validate_mapped_sql(&mut output);
                return Ok(output);
            }
            let plan = plan_query(&query, &parameters, &catalog)?;
            if sql_only {
                if cypher_noop::is_noop(&plan, &catalog) { return Ok(cypher_noop::output()); }
                use orchiddb::ir::rel::{RelBackend, RelBackendOptions, sql::{self, SqlDialect}};
                let mut options = RelBackendOptions {language_functions:true,..Default::default()};
                options.rule_context.trace_limit = if rule_trace {256} else {0};
                let backend = RelBackend::with_options(options);
                let started = std::time::Instant::now();
                let mut metrics = None;
                let compiled = match backend.lower(&plan, &graph) {
                    Ok(lowered) => {
                        let prepared = sql::prepare(&lowered, SqlDialect::DuckDb).await;
                        if rule_metrics {
                            metrics = Some(serde_json::json!({
                                "lower_prepare_micros":started.elapsed().as_micros(),
                                "visits":lowered.rule_context.visits(),
                                "plan_revision":lowered.rule_context.revision(),
                                "sql_bytes":prepared.as_ref().ok().map(|p|p.query.len()),
                                "trace_enabled":rule_trace,"trace":lowered.rule_context.trace(),
                            }));
                        }
                        prepared.map(|prepared|prepared.query).map_err(|e|e.to_string())
                    },
                    Err(error) => Err(error.to_string()),
                };
                let mut output = match compiled {
                    Ok(sql) => serde_json::json!({"sql": sql, "execution_kind": "sql", "requires_sql_execution": true}),
                    Err(error) => serde_json::json!({"lowering_error": error, "ir": orchiddb::ir::plan::explain(&plan)}),
                };
                if let Some(metrics) = metrics { output["rule_metrics"] = metrics; }
                // Inspect the actual hybrid compiler too: failing whole-query
                // lowering does not mean its read subtrees cannot run in SQL.
                use orchiddb::ir::rel::runtime::{compile_for_host, CompileOptions, CompiledKernel};
                match compile_for_host(&plan, &graph, CompileOptions {
                    language_functions: true, native_values: false, ..Default::default()
                }) {
                    Ok(dag) => {
                        let mut pending = vec![&dag];
                        let mut regions = Vec::new();
                        let mut kernels = Vec::new();
                        while let Some(node) = pending.pop() {
                            if let Some(kernel) = CompiledKernel::from_plan(node) {
                                if let Some(fields) = kernel.relational_fields() {
                                    for input in kernel.inputs() {
                                        let lowered = orchiddb::ir::rel::LoweredPlan {
            rule_context: Default::default(),
                                            plan: input.clone(), fields: fields.to_vec(),
                                            result_form: plan.policy.result_form, islands: Default::default(),
                                        };
                                        regions.push(match sql::prepare(&lowered, SqlDialect::DuckDb).await {
                                            Ok(prepared) => serde_json::json!({"sql": prepared.query}),
                                            Err(error) => serde_json::json!({"error": error.to_string()}),
                                        });
                                    }
                                    continue;
                                }
                                kernels.push(kernel.name().to_owned());
                            }
                            pending.extend(node.inputs());
                        }
                        output["regions"] = serde_json::json!(regions);
                        output["kernels"] = serde_json::json!(kernels);
                        output["dag"] = serde_json::json!(dag.display_indent().to_string());
                    }
                    Err(error) => output["compile_error"] = serde_json::json!(error.to_string()),
                }
                return Ok(output);
            }
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
            };
            if no_constant_folding {
                orchiddb::ir::diagnostics::without_constant_folding(query_work).await
            } else { query_work.await }
        })
        .catch_unwind()
        .await;
        let mut output = match result {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => serde_json::json!({"error": error}),
            Err(_) => serde_json::json!({"panic": "query panicked"}),
        };
        if no_constant_folding {
            output["diagnostic_mode"] = serde_json::json!("no_constant_folding");
            output["constant_folding"] = serde_json::json!({
                "enabled": false,
                "scope": "frontend closed quantifiers; relational scalar/quantifier evaluation; finite temporal evaluation; DataFusion SimplifyExpressions",
                "retained": "literal/type encoding; schema validation and type/domain proofs; structural rewrites including literal-condition CASE pruning; backend bind/execution optimization",
                "fixture_setup": "normal"
            });
        }
        println!("{output}");
    }
}
