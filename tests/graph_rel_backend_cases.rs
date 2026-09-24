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
//! GRAPH_REL_PROFILE=read_only GRAPH_REL_STRICT=1 cargo test --test graph_rel_backend_cases -- --ignored --nocapture
//! ```
//!
//! `GRAPH_REL_PROFILE=read_only` excludes only query writes proven by the
//! parsed Cypher clause AST or planned Graph IR effects. Fixture initializers
//! may write while preparing data and do not exclude a read query. The Gremlin
//! traversal AST currently has no source-write steps, so a Gremlin write that
//! fails to parse remains a visible parse failure, not an inferred exclusion.
//! Unknown/opaque effects and missing fixtures also remain strict failures.
//! `metrics.tsv` keeps its original first ten columns and appends
//! `excluded_writes` and `read_total`; `excluded_writes.tsv` gives each
//! exclusion's case path and stable structural reason.
//!
//! With `GRAPH_REL_EXEC=duckdb` the lowered plan is additionally unparsed to
//! DuckDB SQL and executed against a real in-memory DuckDB database (instead
//! of in-process DataFusion), scoring conformance of the generated SQL:
//!
//! ```ignore
//! GRAPH_REL_EXEC=duckdb cargo test --release --test graph_rel_backend_cases -- --ignored --nocapture
//! ```
//!
//! Large corpora can be split into deterministic contiguous shards. Contiguous
//! ranges keep cases from the same suite together, which preserves fixture
//! reuse inside each DuckDB process:
//!
//! ```ignore
//! GRAPH_REL_SHARD_COUNT=4 GRAPH_REL_SHARD_INDEX=0 \
//! GRAPH_REL_OUTPUT_DIR=target/coverage/cypher-0 cargo test --release \
//!   --test graph_rel_backend_cases -- --ignored --nocapture
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use new_graph::ir::analysis::{ReadValidationError, validate_read_only};
use new_graph::ir::catalog::PropertyGraph;
use new_graph::ir::exec::{ExecStats, default_target, plan_with_islands};
use new_graph::ir::interpreter::{ReturnedBatches, execute as interpret};
use new_graph::ir::plan::{GraphPlan, Node, ProcedureMode, explain};
use new_graph::ir::policy::GraphPlanPolicy;
use new_graph::ir::rel::RelBackend;
#[cfg(feature = "duckdb")]
use new_graph::ir::rel::sql::{self, DuckDbExecutor, SqlDialect};
use new_graph::language::cypher::ast::{Clause, Query};
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner;
use new_graph::language::gremlin::planner::GremlinPlanner;

mod cypher_case_runner;
mod gremlin_case_runner;

use gremlin_case_runner::{case_file, format, parse as gremlin_parse};

const CYPHER_ROOT: &str = "cases/cypher/ladybug";
const GREMLIN_ROOT: &str = "cases/gremlin/tinkerpop";
const OUTPUT_DIR: &str = "target/graph_rel_backend_cases";

// Reuse one DuckDB session across the serial corpus run. Prepared setup is
// keyed by materialized table name and contents inside DuckDbExecutor, so
// repeated cases over the same fixture avoid recreating and reinserting tens
// of thousands of rows. PropertyGraph clones are cheap; SQL setup was the
// dominant cost in the slow-case trace.
#[cfg(feature = "duckdb")]
static HARNESS_DUCKDB_EXECUTOR: std::sync::Mutex<Option<DuckDbExecutor>> =
    std::sync::Mutex::new(None);

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
    let config = HarnessConfig::from_env();
    let out_dir = config.output_dir.clone();
    prepare_output_dir(&out_dir).expect("prepare output dir");

    #[cfg(feature = "duckdb")]
    if config.exec == ExecMode::DuckDb {
        *HARNESS_DUCKDB_EXECUTOR.lock().expect("duckdb executor") = None;
    }
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
        "exec mode: {}\nprofile: {}\nshard: {}/{}\n{}{}",
        config.exec.as_str(),
        config.profile.as_str(),
        config.shard_index,
        config.shard_count,
        summary.render(started.elapsed().as_secs_f64(), &out_dir),
        render_island_tally()
    );
    print!("{report}");
    fs::write(out_dir.join("summary.txt"), report).expect("write summary");
    summary
        .write_machine_reports(&out_dir)
        .expect("write machine-readable summary");

    if config.strict && summary.incomplete_cases() > 0 {
        panic!(
            "rel backend strict mode saw {} incomplete cases (including parse, plan, lower, execution, mismatch, and skip outcomes)",
            summary.incomplete_cases()
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecMode {
    DataFusion,
    DuckDb,
    /// Partition the plan into SQL islands, run each on the island target
    /// (DuckDB by default), and let the interpreter handle whatever is left.
    /// Unlike the whole-plan modes, a case that does not lower completely
    /// still produces an answer, so this measures both correctness and how
    /// much of the corpus still needs the interpreter at all.
    Islands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Profile {
    All,
    ReadOnly,
}

impl Profile {
    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::ReadOnly => "read_only",
        }
    }
}

impl ExecMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::DataFusion => "datafusion",
            Self::DuckDb => "duckdb",
            Self::Islands => "islands",
        }
    }
}

/// Corpus-wide island statistics, accumulated across cases.
///
/// A global rather than a threaded-through parameter: the harness is a single
/// test entry point, and this keeps the ~25 `CaseRun` construction sites from
/// having to carry a field none of them care about.
#[derive(Debug, Default)]
struct IslandTally {
    cases: usize,
    fully_pushed_down: usize,
    islands: usize,
    /// Why residual work remained, by reason.
    residual_reasons: BTreeMap<String, usize>,
    /// Cases where the SQL path and the interpreter disagreed on the answer.
    /// The interpreter is the reference, so each of these is a lowering or
    /// execution bug — as opposed to a corpus gap, which makes both wrong
    /// together and shows up in neither this list nor `agreed`.
    divergences: Vec<Divergence>,
    agreed: usize,
}

#[derive(Debug)]
struct Divergence {
    query: String,
    islands: Vec<String>,
    interpreter: Vec<String>,
}

static ISLAND_TALLY: std::sync::Mutex<Option<IslandTally>> = std::sync::Mutex::new(None);

fn record_island_stats(stats: &ExecStats) {
    let mut guard = ISLAND_TALLY.lock().expect("island tally");
    let tally = guard.get_or_insert_with(IslandTally::default);
    tally.cases += 1;
    tally.islands += stats.islands;
    if stats.fully_pushed_down() {
        tally.fully_pushed_down += 1;
    } else {
        // The outermost decline is the one that actually blocks full
        // pushdown; inner ones are consequences of the same gap.
        if let Some(reason) = stats.declined.first() {
            *tally
                .residual_reasons
                .entry(first_line(reason))
                .or_default() += 1;
        } else if stats.islands == 0 {
            *tally
                .residual_reasons
                .entry("no island attempted".to_string())
                .or_default() += 1;
        }
    }
}

fn render_island_tally() -> String {
    let guard = ISLAND_TALLY.lock().expect("island tally");
    let Some(tally) = guard.as_ref() else {
        return String::new();
    };
    let pct = percent(tally.fully_pushed_down, tally.cases);
    let mut out = format!(
        "\nislands: cases={} fully_pushed_down={} ({pct:.1}%) islands={}\n",
        tally.cases, tally.fully_pushed_down, tally.islands
    );
    let mut reasons: Vec<_> = tally.residual_reasons.iter().collect();
    reasons.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    out.push_str("top residual reasons (cases still needing the interpreter):\n");
    for (reason, count) in reasons.iter().take(40) {
        out.push_str(&format!("  {count:>5}  {reason}\n"));
    }
    if crosscheck_enabled() {
        let checked = tally.agreed + tally.divergences.len();
        out.push_str(&format!(
            "crosscheck vs interpreter: checked={} agreed={} ({:.1}%) diverged={}\n",
            checked,
            tally.agreed,
            percent(tally.agreed, checked),
            tally.divergences.len()
        ));
        out.push_str("\n--- divergences (SQL path vs interpreter) ---\n");
        for divergence in &tally.divergences {
            out.push_str(&format!(
                "\nquery: {}\n  sql        : {:?}\n  interpreter: {:?}\n",
                divergence.query,
                truncate_rows(&divergence.islands),
                truncate_rows(&divergence.interpreter),
            ));
        }
    }
    out
}

fn truncate_rows(rows: &[String]) -> Vec<String> {
    rows.iter()
        .take(4)
        .map(|row| row.chars().take(120).collect())
        .collect()
}

#[derive(Debug)]
struct HarnessConfig {
    lang: String,
    suite_filter: Option<String>,
    limit: Option<usize>,
    strict: bool,
    profile: Profile,
    exec: ExecMode,
    case_timeout_ms: u64,
    timeout_ms: u64,
    shard_count: usize,
    shard_index: usize,
    shard_chunk_size: Option<usize>,
    output_dir: PathBuf,
}

impl HarnessConfig {
    fn from_env() -> Self {
        let exec = match std::env::var("GRAPH_REL_EXEC").as_deref() {
            Ok("duckdb") => ExecMode::DuckDb,
            Ok("islands") => ExecMode::Islands,
            _ => ExecMode::DataFusion,
        };
        let shard_count = std::env::var("GRAPH_REL_SHARD_COUNT")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(1)
            .max(1);
        let shard_index = std::env::var("GRAPH_REL_SHARD_INDEX")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        assert!(
            shard_index < shard_count,
            "GRAPH_REL_SHARD_INDEX ({shard_index}) must be less than GRAPH_REL_SHARD_COUNT ({shard_count})"
        );
        Self {
            lang: std::env::var("GRAPH_REL_LANG").unwrap_or_else(|_| "all".into()),
            suite_filter: std::env::var("GRAPH_REL_SUITE").ok(),
            limit: std::env::var("GRAPH_REL_LIMIT")
                .ok()
                .and_then(|value| value.parse().ok()),
            strict: std::env::var("GRAPH_REL_STRICT").is_ok_and(|value| value == "1"),
            profile: match std::env::var("GRAPH_REL_PROFILE").as_deref() {
                Ok("read_only") => Profile::ReadOnly,
                Ok("all") | Err(_) => Profile::All,
                Ok(value) => {
                    panic!("unknown GRAPH_REL_PROFILE={value:?}; expected all or read_only")
                }
            },
            exec,
            case_timeout_ms: std::env::var("GRAPH_REL_CASE_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(60_000),
            timeout_ms: std::env::var("GRAPH_REL_TIMEOUT_MS")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(10_000),
            shard_count,
            shard_index,
            shard_chunk_size: std::env::var("GRAPH_REL_SHARD_CHUNK_SIZE")
                .ok()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|value| *value > 0),
            output_dir: std::env::var_os("GRAPH_REL_OUTPUT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(OUTPUT_DIR)),
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
    if let Some(filter) = &config.suite_filter {
        paths.retain(|path| path.to_string_lossy().contains(filter.as_str()));
    }

    // Paths are sorted by suite and fixture. Small round-robin chunks prevent
    // one worker from receiving every expensive suite while still retaining
    // setup-cache locality across adjacent cases. With no chunk size, retain
    // the original single contiguous range behavior.
    let shard_paths = if let Some(chunk_size) = config.shard_chunk_size {
        paths
            .iter()
            .enumerate()
            .filter(|(position, _)| {
                (position / chunk_size) % config.shard_count == config.shard_index
            })
            .map(|(_, path)| path)
            .collect::<Vec<_>>()
    } else {
        let start = paths.len() * config.shard_index / config.shard_count;
        let end = paths.len() * (config.shard_index + 1) / config.shard_count;
        paths[start..end].iter().collect::<Vec<_>>()
    };
    eprintln!(
        "coverage-shard language={} index={} count={} chunk_size={} cases={} corpus={}",
        lang.as_str(),
        config.shard_index,
        config.shard_count,
        config
            .shard_chunk_size
            .map_or_else(|| "contiguous".into(), |size| size.to_string()),
        shard_paths.len(),
        paths.len(),
    );

    for path in shard_paths.iter().take(config.limit.unwrap_or(usize::MAX)) {
        let case_started = Instant::now();
        // Each case runs on its own thread (with its own runtime) so a
        // panicking case (e.g. from a fixture initializer) records a failure
        // instead of aborting the whole run, and a per-case timeout keeps a
        // single expensive plan (e.g. an unrolled variable-length expand on
        // a large fixture) from stalling the harness.
        let run = {
            let worker_path = path.to_path_buf();
            let backend = backend.clone();
            let exec = config.exec;
            let profile = config.profile;
            let (sender, receiver) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("case runtime");
                let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    runtime.block_on(async {
                        match lang {
                            Language::Cypher => {
                                run_cypher_case(&worker_path, &backend, exec, profile).await
                            }
                            Language::Gremlin => {
                                run_gremlin_case(&worker_path, &backend, exec, profile).await
                            }
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
            // Backstop only. DuckDBExecutor now issues a real database
            // interrupt at the configured deadline, so this thread should
            // return promptly instead of becoming detached CPU work.
            let backstop_ms = if config.exec == ExecMode::DuckDb {
                config.case_timeout_ms
            } else {
                config.timeout_ms.saturating_add(3_000)
            };
            match receiver.recv_timeout(std::time::Duration::from_millis(backstop_ms)) {
                Ok(run) => run,
                Err(_) => CaseRun {
                    query: diagnostic_query(path),
                    plan_tree: None,
                    sql: None,
                    outcome: Outcome::ExecutionError(format!(
                        "case timed out before completion after {backstop_ms}ms"
                    )),
                },
            }
        };
        let elapsed = case_started.elapsed();
        let slow_ms = std::env::var("GRAPH_REL_SLOW_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(500);
        if elapsed.as_millis() >= u128::from(slow_ms) {
            eprintln!(
                "slow-case elapsed_ms={} outcome={} plan_lines={} sql_bytes={} case={} query={}",
                elapsed.as_millis(),
                run.outcome.label(),
                run.plan_tree
                    .as_deref()
                    .map(|plan| plan.lines().count())
                    .unwrap_or(0),
                run.sql.as_deref().map(str::len).unwrap_or(0),
                path.display(),
                run.query
                    .as_deref()
                    .unwrap_or("<unavailable>")
                    .replace('\n', " ")
            );
        }
        if !matches!(run.outcome, Outcome::Matched | Outcome::ExcludedWrite(_)) {
            let _ = dump_case(out_dir, lang, path, &run);
        }
        summary.record(lang, path, run.outcome);
    }
}

fn diagnostic_query(path: &Path) -> Option<String> {
    let raw = fs::read_to_string(path).ok()?;
    case_file::parse(&raw).ok().map(|case| case.query)
}

fn record_slow_phase(path: &Path, phase: &str, started: Instant) {
    let slow_ms = std::env::var("GRAPH_REL_SLOW_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(500);
    let elapsed = started.elapsed();
    if elapsed.as_millis() >= u128::from(slow_ms) {
        eprintln!(
            "slow-phase elapsed_ms={} phase={} case={}",
            elapsed.as_millis(),
            phase,
            path.display()
        );
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
            .unwrap_or(1_000)
    });
    std::time::Duration::from_millis(ms)
}

#[cfg(feature = "duckdb")]
fn setup_timeout() -> std::time::Duration {
    static TIMEOUT_MS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    let ms = *TIMEOUT_MS.get_or_init(|| {
        std::env::var("GRAPH_REL_SETUP_TIMEOUT_MS")
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
        ExecMode::Islands => execute_case_islands(backend, plan, graph).await,
    }
}

/// Partition the plan into SQL islands, then interpret the residual.
///
/// Every case produces an answer here: the islands carry whatever lowers, and
/// the interpreter covers the rest. The tally records how many cases needed
/// the interpreter at all, which is what has to reach zero before it can be
/// deleted.
async fn execute_case_islands(
    backend: &RelBackend,
    plan: &GraphPlan,
    graph: &PropertyGraph,
) -> ExecRun {
    let target = default_target();
    let (hybrid, stats) = plan_with_islands(plan, graph, backend, target.as_ref()).await;
    record_island_stats(&stats);
    let result = interpret(&hybrid, graph).map_err(|err| format!("{err}"));
    if crosscheck_enabled() {
        crosscheck_against_interpreter(plan, graph, &result);
    }
    ExecRun { result, sql: None }
}

/// The case currently executing, so a divergence can name itself. Set by the
/// language runners before execution.
static CURRENT_CASE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

fn set_current_case(query: &str) {
    *CURRENT_CASE.lock().expect("current case") = query.to_string();
}

fn current_case() -> String {
    CURRENT_CASE.lock().expect("current case").clone()
}

fn crosscheck_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("GRAPH_REL_CROSSCHECK").is_ok_and(|v| v == "1"))
}

/// Run the same plan straight through the interpreter and compare.
///
/// A case can fail for two very different reasons: the lowering is wrong, or
/// the fixture/corpus is wrong. Only the first is ours to fix, and only this
/// comparison tells them apart — expected-output mismatches conflate them.
fn crosscheck_against_interpreter(
    plan: &GraphPlan,
    graph: &PropertyGraph,
    islands: &Result<ReturnedBatches, String>,
) {
    let reference = interpret(plan, graph);
    let render = |result: &Result<ReturnedBatches, String>| -> Vec<String> {
        match result {
            Ok(batches) => {
                let mut lines = cypher_case_runner::format::lines_from_batch(batches);
                lines.sort();
                lines
            }
            Err(err) => vec![format!("<error> {}", first_line(err))],
        }
    };
    // The interpreter is only a reference when it produces an answer.
    let Ok(reference) = reference else { return };
    let expected = render(&Ok(reference));
    let actual = render(islands);
    let mut guard = ISLAND_TALLY.lock().expect("island tally");
    let tally = guard.get_or_insert_with(IslandTally::default);
    if expected == actual {
        tally.agreed += 1;
    } else {
        tally.divergences.push(Divergence {
            query: current_case(),
            islands: actual,
            interpreter: expected,
        });
    }
}

#[cfg(feature = "duckdb")]
async fn execute_case_duckdb(
    backend: &RelBackend,
    plan: &GraphPlan,
    graph: &PropertyGraph,
) -> ExecRun {
    let phase_started = Instant::now();
    let lowered = match backend.lower(plan, graph) {
        Ok(lowered) => lowered,
        Err(err) => {
            return ExecRun {
                result: Err(format!("{err}")),
                sql: None,
            };
        }
    };
    record_slow_engine_phase(
        "lower",
        phase_started,
        None,
        count_plan_nodes(&lowered.plan),
    );
    // DuckDB executes synchronously and cannot be cancelled by the case
    // timeout; refuse oversized plans (e.g. deep variable-length unrolls
    // over large fixtures) up front so one query cannot pin a core for the
    // rest of the run.
    let nodes = count_plan_nodes(&lowered.plan);
    if std::env::var("GRAPH_REL_SHOW_PLAN").is_ok_and(|value| value == "1") {
        eprintln!("{}", lowered.plan.display_indent());
    }
    if nodes > 200 {
        return ExecRun {
            result: Err(format!(
                "unsupported relational lowering: plan too large for uncancellable DuckDB execution ({nodes} nodes)"
            )),
            sql: None,
        };
    }
    let phase_started = Instant::now();
    let prepared = match sql::prepare(&lowered, SqlDialect::DuckDb).await {
        Ok(prepared) => prepared,
        Err(err) => {
            return ExecRun {
                result: Err(format!("{err}")),
                sql: None,
            };
        }
    };
    record_slow_engine_phase(
        "prepare_setup_sql",
        phase_started,
        Some(prepared.query.len()),
        nodes,
    );
    let phase_started = Instant::now();
    let mut guard = HARNESS_DUCKDB_EXECUTOR.lock().expect("duckdb executor");
    let executor =
        guard.get_or_insert_with(|| DuckDbExecutor::with_timeouts(case_timeout(), setup_timeout()));
    if std::env::var("GRAPH_REL_EXPLAIN_SQL").is_ok_and(|value| value == "1") {
        use sql::SqlExecutor;
        match executor.run_with_tables(
            &prepared.tables,
            &prepared.setup,
            &format!("EXPLAIN {}", prepared.query),
        ) {
            Ok(rows) => {
                eprintln!("duckdb-explain query={}", current_case());
                for row in rows {
                    for value in row {
                        if let sql::SqlValue::Text(text) = value {
                            eprintln!("{text}");
                        }
                    }
                }
            }
            Err(err) => eprintln!("duckdb-explain failed: {err}"),
        }
    }
    let result = sql::execute_prepared(executor, &prepared).map_err(|err| format!("{err}"));
    record_slow_engine_phase(
        "duckdb_setup_and_query",
        phase_started,
        Some(prepared.query.len()),
        nodes,
    );
    ExecRun {
        result,
        sql: Some(prepared.query),
    }
}

#[cfg(feature = "duckdb")]
fn record_slow_engine_phase(
    phase: &str,
    started: Instant,
    sql_bytes: Option<usize>,
    plan_nodes: usize,
) {
    let slow_ms = std::env::var("GRAPH_REL_SLOW_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(500);
    let elapsed = started.elapsed();
    if elapsed.as_millis() >= u128::from(slow_ms) {
        eprintln!(
            "slow-engine-phase elapsed_ms={} phase={} plan_nodes={} sql_bytes={} query={}",
            elapsed.as_millis(),
            phase,
            plan_nodes,
            sql_bytes.unwrap_or(0),
            current_case().replace('\n', " ")
        );
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
    ExcludedWrite(String),
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

impl Outcome {
    fn label(&self) -> &'static str {
        match self {
            Self::Matched => "matched",
            Self::ExcludedWrite(_) => "excluded_write",
            Self::Mismatch { .. } => "mismatch",
            Self::ParseError(_) => "parse_error",
            Self::PlanError(_) => "plan_error",
            Self::LowerError(_) => "lower_error",
            Self::ExecutionError(_) => "execution_error",
            Self::Skipped(_) => "skipped",
        }
    }
}

/// Only a parsed write is excluded. An unparsed query stays in the read
/// denominator, since text or case names cannot prove its effects.
fn cypher_write_clause(query: &Query) -> Option<&'static str> {
    for clause in &query.clauses {
        match clause {
            Clause::Create(_) => return Some("Cypher CREATE"),
            Clause::Merge(_) => return Some("Cypher MERGE"),
            Clause::Set(_) => return Some("Cypher SET"),
            Clause::Delete(_) => return Some("Cypher DELETE"),
            _ => {}
        }
    }
    query
        .unions
        .iter()
        .find_map(|branch| cypher_write_clause(&branch.query))
}

fn planned_source_mutation(plan: &GraphPlan) -> Option<String> {
    match validate_read_only(plan) {
        Err(ReadValidationError::SourceMutation { operator, .. }) => {
            Some(format!("Graph IR {operator}"))
        }
        // Opaque extensions and unavailable read capabilities are not proof
        // of a write. They remain ordinary failures in strict read coverage.
        _ => None,
    }
}

async fn run_cypher_case(
    path: &Path,
    backend: &RelBackend,
    exec_mode: ExecMode,
    profile: Profile,
) -> CaseRun {
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
    if profile == Profile::ReadOnly
        && let Some(reason) = cypher_write_clause(&parsed)
    {
        return CaseRun {
            query: Some(query),
            plan_tree: None,
            sql: None,
            outcome: Outcome::ExcludedWrite(reason.into()),
        };
    }
    let phase_started = Instant::now();
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
    record_slow_phase(path, "dataset", phase_started);
    let phase_started = Instant::now();
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
    if profile == Profile::ReadOnly
        && let Some(reason) = planned_source_mutation(&plan)
    {
        return CaseRun {
            query: Some(query),
            plan_tree: Some(explain(&plan)),
            sql: None,
            outcome: Outcome::ExcludedWrite(reason),
        };
    }
    record_slow_phase(path, "plan", phase_started);
    let plan_tree = explain(&plan);
    if exec_mode != ExecMode::DuckDb
        && let Err(err) = backend.lower(&plan, &graph)
    {
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
    set_current_case(&query);
    let phase_started = Instant::now();
    let exec = execute_case(backend, &plan, &graph, exec_mode).await;
    record_slow_phase(path, "lower_prepare_execute", phase_started);
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
                outcome: if exec_mode == ExecMode::DuckDb && looks_like_lower_error(&err) {
                    Outcome::LowerError(err)
                } else {
                    Outcome::ExecutionError(err)
                },
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

async fn run_gremlin_case(
    path: &Path,
    backend: &RelBackend,
    exec_mode: ExecMode,
    profile: Profile,
) -> CaseRun {
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
    let phase_started = Instant::now();
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
    record_slow_phase(path, "dataset", phase_started);
    let phase_started = Instant::now();
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
    if profile == Profile::ReadOnly
        && let Some(reason) = planned_source_mutation(&plan)
    {
        return CaseRun {
            query: Some(query),
            plan_tree: Some(explain(&plan)),
            sql: None,
            outcome: Outcome::ExcludedWrite(reason),
        };
    }
    record_slow_phase(path, "plan", phase_started);
    let plan_tree = explain(&plan);
    if exec_mode != ExecMode::DuckDb
        && let Err(err) = backend.lower(&plan, &graph)
    {
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
    set_current_case(&query);
    let phase_started = Instant::now();
    let exec = execute_case(backend, &plan, &graph, exec_mode).await;
    record_slow_phase(path, "lower_prepare_execute", phase_started);
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
                outcome: if exec_mode == ExecMode::DuckDb && looks_like_lower_error(&err) {
                    Outcome::LowerError(err)
                } else {
                    Outcome::ExecutionError(err)
                },
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

fn looks_like_lower_error(error: &str) -> bool {
    error.starts_with("unsupported relational lowering:")
        || error.starts_with("datafusion:")
        || error.starts_with("catalog:")
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
    case_results: Vec<CaseResult>,
}

struct CaseResult {
    language: &'static str,
    path: String,
    outcome: &'static str,
    reason: String,
}

#[derive(Default)]
struct Counts {
    total: usize,
    matched: usize,
    excluded_writes: usize,
    mismatch: usize,
    parse: usize,
    plan: usize,
    lower: usize,
    execution: usize,
    skipped: usize,
}

impl Counts {
    fn read_total(&self) -> usize {
        self.total.saturating_sub(self.excluded_writes)
    }

    fn incomplete_cases(&self) -> usize {
        self.read_total().saturating_sub(self.matched)
    }
}

impl Summary {
    fn record(&mut self, lang: Language, path: &Path, outcome: Outcome) {
        let reason = match &outcome {
            Outcome::Matched => String::new(),
            Outcome::ExcludedWrite(reason) => reason.clone(),
            Outcome::Mismatch { reason, .. }
            | Outcome::ParseError(reason)
            | Outcome::PlanError(reason)
            | Outcome::LowerError(reason)
            | Outcome::ExecutionError(reason)
            | Outcome::Skipped(reason) => first_line(reason),
        };
        self.case_results.push(CaseResult {
            language: lang.as_str(),
            path: path.to_string_lossy().into_owned(),
            outcome: outcome.label(),
            reason,
        });
        let counts = self.by_language.entry(lang.as_str()).or_default();
        counts.total += 1;
        match &outcome {
            Outcome::Matched => counts.matched += 1,
            Outcome::ExcludedWrite(reason) => {
                counts.excluded_writes += 1;
                self.bump_reason("excluded_write", reason);
            }
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

    fn incomplete_cases(&self) -> usize {
        self.by_language
            .values()
            .map(Counts::incomplete_cases)
            .sum()
    }

    fn render(&self, elapsed_secs: f64, out_dir: &Path) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "\nGraph relational backend harness finished in {elapsed_secs:.2}s\n"
        ));
        for (lang, counts) in &self.by_language {
            if counts.total == 0 {
                continue;
            }
            let read_total = counts.read_total();
            let runnable = read_total.saturating_sub(counts.skipped);
            let matched_pct = percent(counts.matched, read_total);
            let executed = counts.matched + counts.mismatch;
            out.push_str(&format!(
                "{lang}: total={} read_total={read_total} excluded_writes={} runnable={} matched={} ({matched_pct:.1}%) parsed={} planned={} lowered={} executed={} mismatches={} skipped={}\n",
                counts.total,
                counts.excluded_writes,
                runnable,
                counts.matched,
                read_total - counts.parse - counts.skipped,
                read_total - counts.parse - counts.plan - counts.skipped,
                read_total - counts.parse - counts.plan - counts.lower - counts.skipped,
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
        out.push_str(&format!("\nFailure files: `{}`\n", out_dir.display()));
        out
    }

    fn write_machine_reports(&self, out_dir: &Path) -> std::io::Result<()> {
        let mut metrics = String::from(
            "language\ttotal\trunnable\tmatched\tparsed\tplanned\tlowered\texecuted\tmismatches\tskipped\texcluded_writes\tread_total\n",
        );
        for (lang, counts) in &self.by_language {
            let read_total = counts.read_total();
            let runnable = read_total.saturating_sub(counts.skipped);
            let parsed = read_total - counts.parse - counts.skipped;
            let planned = read_total - counts.parse - counts.plan - counts.skipped;
            let lowered = read_total - counts.parse - counts.plan - counts.lower - counts.skipped;
            let executed = counts.matched + counts.mismatch;
            metrics.push_str(&format!(
                "{lang}\t{}\t{runnable}\t{}\t{parsed}\t{planned}\t{lowered}\t{executed}\t{}\t{}\t{}\t{read_total}\n",
                counts.total, counts.matched, counts.mismatch, counts.skipped, counts.excluded_writes,
            ));
        }
        fs::write(out_dir.join("metrics.tsv"), metrics)?;

        let mut blockers = String::from("count\treason\n");
        for (reason, count) in &self.reasons {
            blockers.push_str(&format!(
                "{count}\t{}\n",
                reason.replace(['\t', '\n', '\r'], " ")
            ));
        }
        fs::write(out_dir.join("blockers.tsv"), blockers)?;

        let mut cases = String::from("language\tpath\toutcome\treason\n");
        for result in &self.case_results {
            cases.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                result.language,
                result.path.replace(['\t', '\n', '\r'], " "),
                result.outcome,
                result.reason.replace(['\t', '\n', '\r'], " "),
            ));
        }
        fs::write(out_dir.join("cases.tsv"), cases)?;

        let mut excluded = String::from("language\tpath\treason\n");
        for result in self
            .case_results
            .iter()
            .filter(|result| result.outcome == "excluded_write")
        {
            excluded.push_str(&format!(
                "{}\t{}\t{}\n",
                result.language,
                result.path.replace(['\t', '\n', '\r'], " "),
                result.reason.replace(['\t', '\n', '\r'], " "),
            ));
        }
        fs::write(out_dir.join("excluded_writes.tsv"), excluded)
    }
}

fn percent(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        (part as f64 / total as f64) * 100.0
    }
}

#[test]
fn strict_coverage_counts_every_unmatched_case() {
    let mut summary = Summary::default();
    let path = Path::new("fixture.case");
    summary.record(Language::Cypher, path, Outcome::Matched);
    assert_eq!(summary.incomplete_cases(), 0);

    for outcome in [
        Outcome::ParseError("syntax".into()),
        Outcome::PlanError("plan".into()),
        Outcome::LowerError("lower".into()),
        Outcome::ExecutionError("execute".into()),
        Outcome::Skipped("missing fixture".into()),
        Outcome::Mismatch {
            reason: "wrong rows".into(),
            actual: vec![],
            expected: vec!["expected".into()],
        },
    ] {
        summary.record(Language::Cypher, path, outcome);
    }

    assert_eq!(summary.incomplete_cases(), 6);
    summary.record(
        Language::Cypher,
        path,
        Outcome::ExcludedWrite("Cypher CREATE".into()),
    );
    assert_eq!(summary.incomplete_cases(), 6);
    assert_eq!(summary.by_language["cypher"].read_total(), 7);
}

#[test]
fn read_profile_classifies_parsed_cypher_writes_not_keywords_in_literals() {
    for (query, expected) in [
        ("CREATE (n) RETURN n", Some("Cypher CREATE")),
        ("MATCH (n) SET n.x = 1 RETURN n", Some("Cypher SET")),
        ("MATCH (n) DELETE n", Some("Cypher DELETE")),
        ("MERGE (n:X) RETURN n", Some("Cypher MERGE")),
        ("RETURN 'CREATE SET DELETE MERGE' AS text", None),
    ] {
        let parsed = parse_query(query).expect(query);
        assert_eq!(cypher_write_clause(&parsed), expected, "{query}");
    }
}

#[test]
fn read_profile_classifies_planned_procedure_effects() {
    let procedure = |mode| {
        GraphPlan::new(
            GraphPlanPolicy::cypher(),
            Node::GraphProcedureCall {
                name: "example.procedure".into(),
                args: vec![],
                yields: vec![],
                mode,
                input: None,
            },
        )
    };
    assert_eq!(
        planned_source_mutation(&procedure(ProcedureMode::Write)),
        Some("Graph IR GraphProcedureCall".into())
    );
    assert_eq!(
        planned_source_mutation(&procedure(ProcedureMode::Read)),
        None
    );
    let opaque = GraphPlan::new(
        GraphPlanPolicy::cypher(),
        Node::GraphExtension {
            name: "unknown".into(),
            metadata: vec![],
            inputs: vec![Node::GraphOneRow],
        },
    );
    assert_eq!(planned_source_mutation(&opaque), None);
}

#[test]
fn read_profile_reports_excluded_writes_separately() {
    let mut summary = Summary::default();
    summary.record(
        Language::Cypher,
        Path::new("write.case"),
        Outcome::ExcludedWrite("Cypher CREATE".into()),
    );
    summary.record(
        Language::Cypher,
        Path::new("missing.case"),
        Outcome::Skipped("missing fixture".into()),
    );
    let dir = std::env::temp_dir().join(format!(
        "graph-rel-report-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    fs::create_dir(&dir).expect("temporary report directory");
    summary.write_machine_reports(&dir).expect("write reports");
    let metrics = fs::read_to_string(dir.join("metrics.tsv")).expect("metrics");
    assert!(metrics.contains("cypher\t2\t0\t0\t0\t0\t0\t0\t0\t1\t1\t1"));
    let excluded = fs::read_to_string(dir.join("excluded_writes.tsv")).expect("excluded report");
    assert!(excluded.contains("cypher\twrite.case\tCypher CREATE"));
    assert!(!excluded.contains("missing.case"));
    assert_eq!(summary.incomplete_cases(), 1);
    fs::remove_dir_all(dir).expect("remove temporary report directory");
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
