//! DataFusion relational-backend progress harness.
//!
//! This intentionally reuses the existing Cypher and Gremlin conformance case
//! loaders, but executes planned Graph IR through `ir::rel::RelBackend`
//! instead of the interpreter. It is ignored by default because it is a
//! progress gauge, not yet a correctness gate.
//!
//! Run examples:
//!
//! ```ignore
//! cargo test --test graph_rel_backend_cases -- --ignored --nocapture
//! GRAPH_REL_LANG=cypher GRAPH_REL_LIMIT=100 cargo test --test graph_rel_backend_cases -- --ignored --nocapture
//! GRAPH_REL_SUITE=filter cargo test --test graph_rel_backend_cases -- --ignored --nocapture
//! ```
//!
//! With `GRAPH_REL_EXEC=duckdb` the lowered plan is additionally unparsed to
//! DuckDB SQL and executed against a real in-memory DuckDB database (instead
//! of in-process DataFusion), scoring conformance of the generated SQL:
//!
//! ```ignore
//! GRAPH_REL_EXEC=duckdb cargo test --release --test graph_rel_backend_cases -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use new_graph::ir::catalog::PropertyGraph;
use new_graph::ir::interpreter::ReturnedBatches;
use new_graph::ir::plan::{GraphPlan, explain};
use new_graph::ir::rel::RelBackend;
#[cfg(feature = "duckdb")]
use new_graph::ir::rel::sql::{self, DuckDbExecutor, SqlDialect};
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner;
use new_graph::language::gremlin::planner::GremlinPlanner;

mod cypher_case_runner;
mod gremlin_case_runner;

use gremlin_case_runner::{case_file, format, parse as gremlin_parse};

const CYPHER_ROOT: &str = "cases/cypher/ladybug";
const GREMLIN_ROOT: &str = "cases/gremlin/tinkerpop";
const OUTPUT_DIR: &str = "target/graph_rel_backend_cases";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    Cypher,
    Gremlin,
}

impl Language {
    fn as_str(self) -> &'static str {
        match self {
            Self::Cypher => "cypher",
            Self::Gremlin => "gremlin",
        }
    }
}

#[tokio::test]
#[ignore]
async fn graph_rel_backend_cases() {
    let started = Instant::now();
    let out_dir = PathBuf::from(OUTPUT_DIR);
    prepare_output_dir(&out_dir).expect("prepare output dir");

    let config = HarnessConfig::from_env();
    let backend = RelBackend::new();
    let mut summary = Summary::default();

    if config.includes(Language::Cypher) {
        run_language(
            Language::Cypher,
            Path::new(CYPHER_ROOT),
            &config,
            &backend,
            &out_dir,
            &mut summary,
        )
        .await;
    }
    if config.includes(Language::Gremlin) {
        run_language(
            Language::Gremlin,
            Path::new(GREMLIN_ROOT),
            &config,
            &backend,
            &out_dir,
            &mut summary,
        )
        .await;
    }

    let report = format!(
        "exec mode: {}\n{}",
        config.exec.as_str(),
        summary.render(started.elapsed().as_secs_f64())
    );
    print!("{report}");
    fs::write(out_dir.join("summary.txt"), report).expect("write summary");

    if config.strict && summary.failed_cases() > 0 {
        panic!(
            "rel backend strict mode saw {} failed cases",
            summary.failed_cases()
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecMode {
    DataFusion,
    DuckDb,
}

impl ExecMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::DataFusion => "datafusion",
            Self::DuckDb => "duckdb",
        }
    }
}

#[derive(Debug)]
struct HarnessConfig {
    lang: String,
    suite_filter: Option<String>,
    limit: Option<usize>,
    strict: bool,
    exec: ExecMode,
    timeout_ms: u64,
}

impl HarnessConfig {
    fn from_env() -> Self {
        let exec = match std::env::var("GRAPH_REL_EXEC").as_deref() {
            Ok("duckdb") => ExecMode::DuckDb,
            _ => ExecMode::DataFusion,
        };
        Self {
            lang: std::env::var("GRAPH_REL_LANG").unwrap_or_else(|_| "all".into()),
            suite_filter: std::env::var("GRAPH_REL_SUITE").ok(),
            limit: std::env::var("GRAPH_REL_LIMIT")
                .ok()
                .and_then(|value| value.parse().ok()),
            strict: std::env::var("GRAPH_REL_STRICT").is_ok_and(|value| value == "1"),
            exec,
            timeout_ms: std::env::var("GRAPH_REL_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(10_000),
        }
    }

    fn includes(&self, lang: Language) -> bool {
        self.lang.eq_ignore_ascii_case("all") || self.lang.eq_ignore_ascii_case(lang.as_str())
    }
}

async fn run_language(
    lang: Language,
    root: &Path,
    config: &HarnessConfig,
    backend: &RelBackend,
    out_dir: &Path,
    summary: &mut Summary,
) {
    if !root.exists() {
        summary.record(
            lang,
            root,
            Outcome::Skipped(format!("case root `{}` is not present", root.display())),
        );
        return;
    }

    let mut paths = Vec::new();
    walk_cases(root, &mut |path| paths.push(path.to_path_buf()));
    paths.sort();

    let mut seen = 0usize;
    for path in paths {
        if let Some(filter) = &config.suite_filter {
            if !path.to_string_lossy().contains(filter.as_str()) {
                continue;
            }
        }
        if config.limit.is_some_and(|limit| seen >= limit) {
            break;
        }
        seen += 1;
        // Each case runs on its own thread (with its own runtime) so a
        // panicking case (e.g. from a fixture initializer) records a failure
        // instead of aborting the whole run, and a per-case timeout keeps a
        // single expensive plan (e.g. an unrolled variable-length expand on
        // a large fixture) from stalling the harness. Timed-out threads are
        // detached; their result is discarded when it eventually arrives.
        let run = {
            let path = path.clone();
            let backend = backend.clone();
            let exec = config.exec;
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("case runtime");
                let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runtime.block_on(async {
                        match lang {
                            Language::Cypher => run_cypher_case(&path, &backend, exec).await,
                            Language::Gremlin => run_gremlin_case(&path, &backend, exec).await,
                        }
                    })
                }))
                .unwrap_or_else(|panic| CaseRun {
                    query: None,
                    plan_tree: None,
                    sql: None,
                    outcome: Outcome::ExecutionError(format!(
                        "case panicked: {}",
                        panic_message(panic.as_ref())
                    )),
                });
                let _ = sender.send(run);
            });
            // Backstop only: the cooperative tokio timeout inside the case
            // normally fires first, letting the thread exit cleanly.
            match receiver.recv_timeout(std::time::Duration::from_millis(
                config.timeout_ms.saturating_mul(3).saturating_add(5_000),
            )) {
                Ok(run) => run,
                Err(_) => CaseRun {
                    query: None,
                    plan_tree: None,
                    sql: None,
                    outcome: Outcome::ExecutionError(format!(
                        "case timed out after {}ms",
                        config.timeout_ms
                    )),
                },
            }
        };
        if !matches!(run.outcome, Outcome::Matched) {
            let _ = dump_case(out_dir, lang, &path, &run);
        }
        summary.record(lang, &path, run.outcome);
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

struct CaseRun {
    query: Option<String>,
    plan_tree: Option<String>,
    sql: Option<String>,
    outcome: Outcome,
}

impl CaseRun {
    fn skipped(message: impl Into<String>) -> Self {
        Self {
            query: None,
            plan_tree: None,
            sql: None,
            outcome: Outcome::Skipped(message.into()),
        }
    }
}

/// Result of running a case's plan through the configured execution engine.
struct ExecRun {
    result: Result<ReturnedBatches, String>,
    /// Generated SQL text when running through a real database.
    sql: Option<String>,
}

#[cfg(feature = "duckdb")]
fn count_plan_nodes(plan: &datafusion::logical_expr::LogicalPlan) -> usize {
    let mut count = 0usize;
    let mut stack = vec![plan];
    while let Some(node) = stack.pop() {
        count += 1;
        stack.extend(node.inputs());
    }
    count
}

fn case_timeout() -> std::time::Duration {
    static TIMEOUT_MS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let ms = *TIMEOUT_MS.get_or_init(|| {
        std::env::var("GRAPH_REL_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(10_000)
    });
    std::time::Duration::from_millis(ms)
}

async fn execute_case(
    backend: &RelBackend,
    plan: &GraphPlan,
    graph: &PropertyGraph,
    mode: ExecMode,
) -> ExecRun {
    match mode {
        // A cooperative timeout drops the DataFusion future so a single
        // expensive plan cannot pin memory/CPU for the rest of the run.
        ExecMode::DataFusion => {
            match tokio::time::timeout(case_timeout(), backend.execute(plan, graph)).await {
                Ok(result) => ExecRun {
                    result: result.map_err(|err| format!("{err}")),
                    sql: None,
                },
                Err(_) => ExecRun {
                    result: Err(format!(
                        "execution timed out after {}ms",
                        case_timeout().as_millis()
                    )),
                    sql: None,
                },
            }
        }
        ExecMode::DuckDb => execute_case_duckdb(backend, plan, graph).await,
    }
}

#[cfg(feature = "duckdb")]
async fn execute_case_duckdb(
    backend: &RelBackend,
    plan: &GraphPlan,
    graph: &PropertyGraph,
) -> ExecRun {
    let lowered = match backend.lower(plan, graph) {
        Ok(lowered) => lowered,
        Err(err) => {
            return ExecRun {
                result: Err(format!("{err}")),
                sql: None,
            };
        }
    };
    // DuckDB executes synchronously and cannot be cancelled by the case
    // timeout; refuse oversized plans (e.g. deep variable-length unrolls
    // over large fixtures) up front so one query cannot pin a core for the
    // rest of the run.
    let nodes = count_plan_nodes(&lowered.plan);
    if nodes > 200 {
        return ExecRun {
            result: Err(format!(
                "unsupported relational lowering: plan too large for uncancellable DuckDB execution ({nodes} nodes)"
            )),
            sql: None,
        };
    }
    let prepared = match sql::prepare(&lowered, SqlDialect::DuckDb).await {
        Ok(prepared) => prepared,
        Err(err) => {
            return ExecRun {
                result: Err(format!("{err}")),
                sql: None,
            };
        }
    };
    let mut executor = DuckDbExecutor::new();
    let result = sql::execute_prepared(&mut executor, &prepared).map_err(|err| format!("{err}"));
    ExecRun {
        result,
        sql: Some(prepared.query),
    }
}

#[cfg(not(feature = "duckdb"))]
async fn execute_case_duckdb(
    _backend: &RelBackend,
    _plan: &GraphPlan,
    _graph: &PropertyGraph,
) -> ExecRun {
    ExecRun {
        result: Err("GRAPH_REL_EXEC=duckdb requires the `duckdb` feature".into()),
        sql: None,
    }
}

#[derive(Debug)]
enum Outcome {
    Matched,
    Mismatch {
        reason: String,
        actual: Vec<String>,
        expected: Vec<String>,
    },
    ParseError(String),
    PlanError(String),
    LowerError(String),
    ExecutionError(String),
    Skipped(String),
}

async fn run_cypher_case(path: &Path, backend: &RelBackend, exec_mode: ExecMode) -> CaseRun {
    let case = match read_case(path) {
        Ok(case) => case,
        Err(run) => return run,
    };
    if case.metadata.language != "cypher" {
        return CaseRun::skipped(format!("non-cypher language: {}", case.metadata.language));
    }

    let query = case.query.clone();
    let parsed = match parse_query(&query) {
        Ok(parsed) => parsed,
        Err(err) => {
            if let Some(outcome) = expected_error_outcome(
                &case.expected,
                &case.metadata.expected_kind,
                &format!("{err}"),
            ) {
                return CaseRun {
                    query: Some(query),
                    plan_tree: None,
                    sql: None,
                    outcome,
                };
            }
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                sql: None,
                outcome: Outcome::ParseError(format!("{err}")),
            };
        }
    };
    let graph = match cypher_case_runner::dataset::build_with_initializer(
        &case.metadata.dataset,
        case.graph_initializer.as_deref(),
    ) {
        Ok(graph) => graph,
        Err(err) => {
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                sql: None,
                outcome: Outcome::Skipped(format!(
                    "unsupported dataset `{}`: {err}",
                    case.metadata.dataset
                )),
            };
        }
    };
    let plan = match CypherPlanner::new().plan(&parsed) {
        Ok(plan) => plan,
        Err(err) => {
            if let Some(outcome) = expected_error_outcome(
                &case.expected,
                &case.metadata.expected_kind,
                &format!("{err}"),
            ) {
                return CaseRun {
                    query: Some(query),
                    plan_tree: None,
                    sql: None,
                    outcome,
                };
            }
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                sql: None,
                outcome: Outcome::PlanError(format!("{err}")),
            };
        }
    };
    let plan_tree = explain(&plan);
    if let Err(err) = backend.lower(&plan, &graph) {
        if let Some(outcome) = expected_error_outcome(
            &case.expected,
            &case.metadata.expected_kind,
            &format!("{err}"),
        ) {
            return CaseRun {
                query: Some(query),
                plan_tree: Some(plan_tree),
                sql: None,
                outcome,
            };
        }
        return CaseRun {
            query: Some(query),
            plan_tree: Some(plan_tree),
            sql: None,
            outcome: Outcome::LowerError(format!("{err}")),
        };
    }
    let exec = execute_case(backend, &plan, &graph, exec_mode).await;
    let returned = match exec.result {
        Ok(returned) => returned,
        Err(err) => {
            if let Some(outcome) =
                expected_error_outcome(&case.expected, &case.metadata.expected_kind, &err)
            {
                return CaseRun {
                    query: Some(query),
                    plan_tree: Some(plan_tree),
                    sql: exec.sql,
                    outcome,
                };
            }
            return CaseRun {
                query: Some(query),
                plan_tree: Some(plan_tree),
                sql: exec.sql,
                outcome: Outcome::ExecutionError(err),
            };
        }
    };
    let actual = cypher_case_runner::format::lines_from_batch(&returned);
    let ordered = case.metadata.ordered && query_has_order_by(&query);
    finish_compare(
        Language::Cypher,
        query,
        plan_tree,
        exec.sql,
        actual,
        case.expected,
        ordered,
        &case.metadata.expected_kind,
    )
}

async fn run_gremlin_case(path: &Path, backend: &RelBackend, exec_mode: ExecMode) -> CaseRun {
    let case = match read_case(path) {
        Ok(case) => case,
        Err(run) => return run,
    };
    if case.metadata.language != "gremlin" {
        return CaseRun::skipped(format!("non-gremlin language: {}", case.metadata.language));
    }

    let query = case.query.clone();
    let traversal = match gremlin_parse::gremlin_with_case(&query, &case.metadata.source_case) {
        Ok(traversal) => traversal,
        Err(err) => {
            if let Some(outcome) =
                expected_error_outcome(&case.expected, &case.metadata.expected_kind, &err)
            {
                return CaseRun {
                    query: Some(query),
                    plan_tree: None,
                    sql: None,
                    outcome,
                };
            }
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                sql: None,
                outcome: Outcome::ParseError(err),
            };
        }
    };
    let graph = match gremlin_case_runner::dataset::build_with_initializer(
        &case.metadata.dataset,
        case.graph_initializer.as_deref(),
    ) {
        Ok(graph) => graph,
        Err(err) => {
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                sql: None,
                outcome: Outcome::Skipped(format!(
                    "unsupported dataset `{}`: {err}",
                    case.metadata.dataset
                )),
            };
        }
    };
    let plan = match GremlinPlanner::new().plan(&traversal) {
        Ok(plan) => plan,
        Err(err) => {
            if let Some(outcome) = expected_error_outcome(
                &case.expected,
                &case.metadata.expected_kind,
                &format!("{err}"),
            ) {
                return CaseRun {
                    query: Some(query),
                    plan_tree: None,
                    sql: None,
                    outcome,
                };
            }
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                sql: None,
                outcome: Outcome::PlanError(format!("{err}")),
            };
        }
    };
    let plan_tree = explain(&plan);
    if let Err(err) = backend.lower(&plan, &graph) {
        if let Some(outcome) = expected_error_outcome(
            &case.expected,
            &case.metadata.expected_kind,
            &format!("{err}"),
        ) {
            return CaseRun {
                query: Some(query),
                plan_tree: Some(plan_tree),
                sql: None,
                outcome,
            };
        }
        return CaseRun {
            query: Some(query),
            plan_tree: Some(plan_tree),
            sql: None,
            outcome: Outcome::LowerError(format!("{err}")),
        };
    }
    let exec = execute_case(backend, &plan, &graph, exec_mode).await;
    let returned = match exec.result {
        Ok(returned) => returned,
        Err(err) => {
            if let Some(outcome) =
                expected_error_outcome(&case.expected, &case.metadata.expected_kind, &err)
            {
                return CaseRun {
                    query: Some(query),
                    plan_tree: Some(plan_tree),
                    sql: exec.sql,
                    outcome,
                };
            }
            return CaseRun {
                query: Some(query),
                plan_tree: Some(plan_tree),
                sql: exec.sql,
                outcome: Outcome::ExecutionError(err),
            };
        }
    };
    let actual = gremlin_case_runner::format::lines_from_batch(&returned);
    finish_compare(
        Language::Gremlin,
        query,
        plan_tree,
        exec.sql,
        actual,
        case.expected,
        case.metadata.ordered,
        &case.metadata.expected_kind,
    )
}

fn read_case(path: &Path) -> Result<case_file::Case, CaseRun> {
    let raw = fs::read_to_string(path)
        .map_err(|err| CaseRun::skipped(format!("read `{}`: {err}", path.display())))?;
    case_file::parse(&raw).map_err(|err| CaseRun::skipped(format!("case parse: {err}")))
}

fn finish_compare(
    lang: Language,
    query: String,
    plan_tree: String,
    sql: Option<String>,
    actual: Vec<String>,
    expected: Vec<String>,
    ordered: bool,
    expected_kind: &str,
) -> CaseRun {
    let outcome = match compare_lines(lang, &actual, &expected, ordered, expected_kind) {
        Ok(()) => Outcome::Matched,
        Err(reason) => Outcome::Mismatch {
            reason,
            actual,
            expected,
        },
    };
    CaseRun {
        query: Some(query),
        plan_tree: Some(plan_tree),
        sql,
        outcome,
    }
}

fn expected_error_outcome(
    expected: &[String],
    expected_kind: &str,
    actual: &str,
) -> Option<Outcome> {
    expected_error_matches(expected, expected_kind, actual).then_some(Outcome::Matched)
}

fn expected_error_matches(expected: &[String], expected_kind: &str, actual: &str) -> bool {
    if expected_kind != "rows" {
        return false;
    }
    let expected_lines = expected
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    if expected_lines.is_empty() || !looks_like_expected_error(expected_lines[0]) {
        return false;
    }
    let actual_candidates = error_match_candidates(actual);
    expected_lines.iter().any(|expected| {
        actual_candidates.iter().any(|actual| {
            actual == expected || actual.contains(expected) || expected.contains(actual)
        })
    })
}

fn looks_like_expected_error(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("exception")
        || lower.starts_with("syntaxerror:")
        || lower.starts_with("syntax error")
        || lower.starts_with("error:")
}

fn error_match_candidates(actual: &str) -> Vec<String> {
    let mut out = vec![actual.trim().to_string()];
    for needle in [
        "Runtime exception:",
        "Binder exception:",
        "Parser exception:",
        "Conversion exception:",
        "Overflow exception:",
        "Unsupported exception:",
        "Catalog exception:",
        "RuntimeError:",
        "SyntaxError:",
        "Error:",
    ] {
        if let Some(idx) = actual.find(needle) {
            out.push(actual[idx..].trim().trim_matches('"').to_string());
        }
    }
    out
}

fn compare_lines(
    lang: Language,
    actual: &[String],
    expected: &[String],
    ordered: bool,
    expected_kind: &str,
) -> Result<(), String> {
    if expected_kind == "count" {
        let expected_count = expected
            .iter()
            .find_map(|line| line.trim().parse::<usize>().ok())
            .ok_or_else(|| "count expectation had no numeric count".to_string())?;
        return if actual.len() == expected_count {
            Ok(())
        } else {
            Err(format!(
                "row count: actual {}, expected {expected_count}",
                actual.len()
            ))
        };
    }

    let mut actual = actual
        .iter()
        .map(|line| normalize_line(lang, line))
        .collect::<Vec<_>>();
    let mut expected = expected
        .iter()
        .map(|line| normalize_line(lang, line))
        .collect::<Vec<_>>();

    if actual.len() != expected.len() {
        return Err(format!(
            "row count: actual {}, expected {}",
            actual.len(),
            expected.len()
        ));
    }
    if !ordered {
        actual.sort();
        expected.sort();
    }
    if actual == expected {
        Ok(())
    } else {
        Err(format!("actual={actual:?}, expected={expected:?}"))
    }
}

fn normalize_line(lang: Language, line: &str) -> String {
    let stripped = match lang {
        Language::Cypher => cypher_case_runner::format::strip_expected_tags(line),
        Language::Gremlin => gremlin_case_runner::format::strip_expected_tags(line),
    };
    normalize_numbers(stripped.trim())
}

fn normalize_numbers(input: &str) -> String {
    let mut out = input.to_string();
    while out.contains(".0") {
        out = out.replace(".0", "");
    }
    out
}

#[derive(Default)]
struct Summary {
    by_language: BTreeMap<&'static str, Counts>,
    reasons: BTreeMap<String, usize>,
}

#[derive(Default)]
struct Counts {
    total: usize,
    matched: usize,
    mismatch: usize,
    parse: usize,
    plan: usize,
    lower: usize,
    execution: usize,
    skipped: usize,
}

impl Summary {
    fn record(&mut self, lang: Language, _path: &Path, outcome: Outcome) {
        let counts = self.by_language.entry(lang.as_str()).or_default();
        counts.total += 1;
        match &outcome {
            Outcome::Matched => counts.matched += 1,
            Outcome::Mismatch { reason, .. } => {
                counts.mismatch += 1;
                self.bump_reason("mismatch", reason);
            }
            Outcome::ParseError(err) => {
                counts.parse += 1;
                self.bump_reason("parse", err);
            }
            Outcome::PlanError(err) => {
                counts.plan += 1;
                self.bump_reason("plan", err);
            }
            Outcome::LowerError(err) => {
                counts.lower += 1;
                self.bump_reason("lower", err);
            }
            Outcome::ExecutionError(err) => {
                counts.execution += 1;
                self.bump_reason("execute", err);
            }
            Outcome::Skipped(err) => {
                counts.skipped += 1;
                self.bump_reason("skip", err);
            }
        }
    }

    fn bump_reason(&mut self, stage: &str, reason: &str) {
        let reason = first_line(reason);
        *self
            .reasons
            .entry(format!("{stage}: {reason}"))
            .or_default() += 1;
    }

    fn failed_cases(&self) -> usize {
        self.by_language
            .values()
            .map(|counts| counts.mismatch + counts.execution)
            .sum()
    }

    fn render(&self, elapsed_secs: f64) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "\nGraph relational backend harness finished in {elapsed_secs:.2}s\n"
        ));
        for (lang, counts) in &self.by_language {
            if counts.total == 0 {
                continue;
            }
            let runnable = counts.total.saturating_sub(counts.skipped);
            let matched_pct = percent(counts.matched, counts.total);
            let executed = counts.matched + counts.mismatch;
            out.push_str(&format!(
                "{lang}: total={} runnable={} matched={} ({matched_pct:.1}%) parsed={} planned={} lowered={} executed={} mismatches={} skipped={}\n",
                counts.total,
                runnable,
                counts.matched,
                counts.total - counts.parse - counts.skipped,
                counts.total - counts.parse - counts.plan - counts.skipped,
                counts.total - counts.parse - counts.plan - counts.lower - counts.skipped,
                executed,
                counts.mismatch,
                counts.skipped,
            ));
        }
        out.push_str("\nTop blockers:\n");
        let mut reasons = self
            .reasons
            .iter()
            .map(|(reason, count)| (reason, *count))
            .collect::<Vec<_>>();
        reasons.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        for (idx, (reason, count)) in reasons.into_iter().take(20).enumerate() {
            out.push_str(&format!("{:>2}. {:>5} {reason}\n", idx + 1, count));
        }
        out.push_str(&format!("\nFailure files: `{OUTPUT_DIR}`\n"));
        out
    }
}

fn percent(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (part as f64 / total as f64) * 100.0
    }
}

fn first_line(input: &str) -> String {
    input
        .lines()
        .next()
        .unwrap_or(input)
        .chars()
        .take(240)
        .collect()
}

fn dump_case(out_dir: &Path, lang: Language, path: &Path, run: &CaseRun) -> std::io::Result<()> {
    let rel = path
        .file_stem()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "case".into());
    let file = out_dir.join(format!("{}_{}.txt", lang.as_str(), sanitize_filename(&rel)));
    let mut text = String::new();
    text.push_str(&format!("case: {}\n", path.display()));
    text.push_str(&format!("outcome: {:?}\n\n", run.outcome));
    if let Some(query) = &run.query {
        text.push_str("--- query\n");
        text.push_str(query);
        text.push_str("\n\n");
    }
    if let Some(plan) = &run.plan_tree {
        text.push_str("--- plan\n");
        text.push_str(plan);
        text.push('\n');
    }
    if let Some(sql) = &run.sql {
        text.push_str("\n--- sql\n");
        text.push_str(sql);
        text.push('\n');
    }
    if let Outcome::Mismatch {
        reason,
        actual,
        expected,
    } = &run.outcome
    {
        text.push_str("\n--- mismatch\n");
        text.push_str(reason);
        text.push_str("\n\nactual:\n");
        text.push_str(&actual.join("\n"));
        text.push_str("\n\nexpected:\n");
        text.push_str(&expected.join("\n"));
        text.push('\n');
    }
    fs::write(file, text)
}

fn sanitize_filename(input: &str) -> String {
    input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

fn prepare_output_dir(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)
}

fn walk_cases(root: &Path, f: &mut impl FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk_cases(&path, f);
        } else if path.extension().is_some_and(|ext| ext == "case") {
            f(&path);
        }
    }
}

fn query_has_order_by(query: &str) -> bool {
    query.to_ascii_lowercase().contains("order by")
}
