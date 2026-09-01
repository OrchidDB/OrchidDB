//! Cypher Ladybug case runner — wired into `cargo test`.
//!
//! Walks every `*.case` file under `cases/cypher/ladybug`, parses the
//! embedded Cypher query, plans it through `CypherPlanner`, executes it
//! against an in-memory `PropertyGraph` built from the case's dataset
//! name, and compares the formatted output to the expected lines.
//!
//! Every case is classified into one of:
//!   - `Correct`     — query parsed, planned, ran, output matches.
//!   - `Incorrect`   — query ran but produced different rows. This is a
//!                     correctness regression and fails the test.
//!   - `ParseError`  — embedded Cypher couldn't be tokenized / parsed.
//!   - `PlanError`   — planner returned an `Unsupported` / `Plan` error.
//!   - `RunError`    — interpreter error (catalog miss, type error, …).
//!   - `Skipped`     — case file unreadable, or its dataset isn't yet
//!                     supported by the harness.
//!
//! `Correct` is the only "data accurate" outcome. The other categories
//! are aggregated as "not accurate", with per-suite totals and the most
//! common error messages so we can see which Cypher features dominate
//! the gap.
//!
//! Cases are additionally partitioned by `cases/cypher/ladybug/tiers.toml`
//! into `core` (headline conformance number), `kuzu-ext` (Kuzu-specific
//! surface, informational) and `broken-import` (machine-tagged: empty
//! dataset + non-empty non-error expectation — the importer dropped the
//! setup statements, so these are structurally unpassable until
//! re-imported). The run prints a three-number tier summary first.
//!
//! Expected lines carrying Kuzu error prose (`Binder exception: …`) are
//! matched by error *category* rather than verbatim text; set
//! `CYPHER_ERROR_CATEGORY=off` for the old verbatim-only behaviour.
//!
//! See output with:
//!
//! ```ignore
//! cargo test --test cypher_ladybug_cases -- --nocapture
//! ```
//!
//! For a Cypher language-focused signal that excludes storage/session
//! engine suites, use:
//!
//! ```ignore
//! CYPHER_PROFILE=language cargo test --test cypher_ladybug_cases -- --nocapture
//! ```
//!
//! The test always succeeds when there are zero `Incorrect` cases —
//! parse/plan/run errors are tracked but expected while the planner is
//! still under construction. Any `Incorrect` row fails the test
//! immediately so a real correctness regression can never sneak past CI.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use num_bigint::BigInt;

use new_graph::ir::catalog::PropertyGraph;
use new_graph::ir::interpreter::{ReturnedBatches, execute};
use new_graph::ir::plan::explain;
use new_graph::ir::value::{STRUCT_ORDER_KEY, Value};
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner;

#[path = "gremlin_case_runner/case_file.rs"]
mod case_file;
#[path = "gremlin_case_runner/compare.rs"]
mod compare;

mod cypher_case_runner;
use cypher_case_runner::dataset;
use cypher_case_runner::format;
use cypher_case_runner::tiers::{Tier, TierManifest};

const CASES_ROOT: &str = "cases/cypher/ladybug";
/// Failure dump dir under `target/`. Cleared at the start of every run so
/// the on-disk picture always reflects the latest pass — easier to grep
/// for "what's still red right now".
const FAILURES_DIR: &str = "target/cypher_case_failures";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaseProfile {
    All,
    Language,
}

impl CaseProfile {
    fn from_env() -> Self {
        match std::env::var("CYPHER_PROFILE") {
            Ok(value) if value.eq_ignore_ascii_case("all") => Self::All,
            Ok(value)
                if value.eq_ignore_ascii_case("language")
                    || value.eq_ignore_ascii_case("core-language")
                    || value.eq_ignore_ascii_case("cypher-language") =>
            {
                Self::Language
            }
            Ok(value) => panic!("unknown CYPHER_PROFILE `{value}`; expected `all` or `language`"),
            Err(_) => Self::All,
        }
    }

    fn includes(self, path: &Path) -> bool {
        match self {
            Self::All => true,
            Self::Language => is_language_profile_case(path),
        }
    }
}

fn is_language_profile_case(path: &Path) -> bool {
    let parts = ladybug_case_parts(path);
    let Some(group) = parts.first().map(String::as_str) else {
        return false;
    };

    if matches!(
        group,
        "acc"
            | "binary_demo"
            | "catalog"
            | "copy"
            | "csv"
            | "ddl"
            | "dml_node"
            | "dml_rel"
            | "export_import"
            | "gds"
            | "glob"
            | "graph"
            | "ldbc"
            | "lsqb"
            | "npy_1d"
            | "parquet"
            | "read_list"
            | "rel_group"
            | "tck"
            | "tensor_list"
            | "transaction"
            | "uint128"
            | "user_defined_types"
    ) {
        return false;
    }

    if group == "exceptions" && parts.get(1).is_some_and(|part| part == "copy") {
        return false;
    }

    if parts.iter().any(|part| {
        matches!(
            part.as_str(),
            "catalog" | "copy" | "csv" | "gds" | "read_list" | "rel_group"
        )
    }) {
        return false;
    }

    !parts.iter().any(|part| {
        part.contains("copy")
            || part.contains("create")
            || part.contains("delete")
            || part.contains("parquet")
            || matches!(
                part.as_str(),
                "hash_large" | "hash_leak" | "hash_ldbc" | "large" | "large_list" | "set_copy"
            )
    })
}

fn ladybug_case_parts(path: &Path) -> Vec<String> {
    let root = Path::new(CASES_ROOT);
    let rel = path.strip_prefix(root).unwrap_or(path);
    let mut parts = rel
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    if parts.last().is_some_and(|part| part.ends_with(".case")) {
        parts.pop();
    }
    parts
}

#[test]
fn cypher_ladybug_cases() {
    let root = PathBuf::from(CASES_ROOT);
    if !root.exists() {
        // Treat a missing case directory as a skip rather than a hard
        // failure — checkouts that don't include the conformance tree
        // shouldn't break `cargo test`.
        eprintln!(
            "[cypher_ladybug_cases] skipping: cases directory `{}` not present",
            root.display()
        );
        return;
    }

    let failures_dir = PathBuf::from(FAILURES_DIR);
    if let Err(err) = reset_failures_dir(&failures_dir) {
        panic!(
            "could not prepare failure dump dir `{}`: {err}",
            failures_dir.display()
        );
    }

    let started = Instant::now();
    let mut summary = Summary::default();
    let manifest = TierManifest::load(&root.join("tiers.toml"));
    let suite_filter = std::env::var("CYPHER_SUITE").ok();
    let profile = CaseProfile::from_env();
    let timeout_ms: u64 = std::env::var("CYPHER_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    spawn_memory_watchdog();
    let extra_quarantine = env_quarantine();
    walk_cases(&root, &mut |path| {
        if !profile.includes(path) {
            return;
        }
        if let Some(filter) = &suite_filter {
            if !path.to_string_lossy().contains(filter.as_str()) {
                return;
            }
        }
        if let Ok(mut current) = CURRENT_CASE.lock() {
            *current = path.display().to_string();
        }
        let run = match is_quarantined(path, &extra_quarantine) {
            Some(reason) => CaseRun::from_outcome(Outcome::Skipped(format!(
                "quarantined (exhausts host memory): {reason}"
            ))),
            None => run_with_timeout(path, timeout_ms),
        };
        if !matches!(run.outcome, Outcome::Correct) {
            if let Err(err) = dump_failure(&failures_dir, path, &run) {
                eprintln!(
                    "[cypher_ladybug_cases] failed to write failure file for `{}`: {err}",
                    path.display()
                );
            }
        }
        // Tier: manifest first (suite dir > subdir > per-case, deepest
        // wins), then the runtime broken-import machine tag. A case that
        // passes anyway (e.g. via a manual graph overlay) is kept in its
        // manifest tier — "broken" only applies to cases that cannot
        // pass and didn't.
        let rel = path
            .strip_prefix(CASES_ROOT)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let mut tier = manifest.lookup(&rel);
        if run.broken_import && !matches!(run.outcome, Outcome::Correct) {
            tier = Tier::BrokenImport;
        }
        summary.record(path, tier, run.outcome);
    });
    summary.print(started.elapsed());
    println!(
        "\nFailure dumps: {} file(s) under `{}`",
        summary.inaccurate() + summary.skipped,
        failures_dir.display()
    );
    summary.assert_no_incorrect();
}

// ============================================================
// Per-case execution
// ============================================================

/// One per-case run carries enough context for the failure-dump to be
/// useful: the original Cypher query string and (when planning got that
/// far) the explain-formatted plan tree.
struct CaseRun {
    query: Option<String>,
    plan_tree: Option<String>,
    outcome: Outcome,
    /// Machine-tag: the case references an empty dataset (no inline
    /// initializer either) yet expects non-empty, non-error output —
    /// its setup statements were dropped at import time, so it is
    /// structurally unpassable. Reported as `broken-import`.
    broken_import: bool,
}

impl CaseRun {
    fn from_outcome(outcome: Outcome) -> Self {
        Self {
            query: None,
            plan_tree: None,
            outcome,
            broken_import: false,
        }
    }
}

/// Cheap textual check for whether a Cypher query contains an `ORDER BY`
/// clause outside of string literals. Used to decide whether ordered
/// comparison applies: a query without `ORDER BY` makes no promises
/// about row order, so we degrade to multiset comparison.
fn query_has_order_by(query: &str) -> bool {
    let bytes = query.as_bytes();
    let mut i = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let lower = query.to_ascii_lowercase();
    let lower_bytes = lower.as_bytes();
    while i < bytes.len() {
        let ch = bytes[i] as char;
        if in_single {
            if ch == '\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        if in_double {
            if ch == '"' {
                in_double = false;
            }
            i += 1;
            continue;
        }
        if in_backtick {
            if ch == '`' {
                in_backtick = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '\'' => {
                in_single = true;
                i += 1;
                continue;
            }
            '"' => {
                in_double = true;
                i += 1;
                continue;
            }
            '`' => {
                in_backtick = true;
                i += 1;
                continue;
            }
            _ => {}
        }
        if i + 8 <= lower_bytes.len()
            && &lower_bytes[i..i + 8] == b"order by"
            && (i == 0 || !is_ident_char(bytes[i - 1] as char))
            && (i + 8 == bytes.len() || !is_ident_char(bytes[i + 8] as char))
        {
            return true;
        }
        i += 1;
    }
    false
}

fn is_ident_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

/// Cases that exhaust host memory before any timeout can fire, matched as
/// path substrings.
///
/// The interpreter materializes intermediate rows eagerly with no spill and
/// no count-only path, so a query whose *intermediate* cardinality is huge
/// allocates without bound even when its result is a single number —
/// `MATCH (a:person), (b:person) RETURN COUNT(*)` builds all 36,000,000
/// cross-product rows to count them. A per-case timeout does not help: tens
/// of gigabytes are gone in a few seconds, and the abandoned thread keeps
/// allocating afterwards because it cannot be cancelled.
///
/// These are reported as `Skipped` (never as passing) so the conformance
/// number stays honest. Removing an entry is the natural regression test for
/// factorized/count-only evaluation when that lands — see the interpreter
/// join-performance item in `docs/handoff_corpus.md`.
const MEMORY_QUARANTINE: &[&str] = &[
    // `MATCH (a:person), (b:person) RETURN COUNT(*)` over 6k persons — 36M
    // intermediate rows for a one-row answer. CSV and parquet twins.
    "read_list/large_adj_list/0006_CrossProduct1",
    "read_list/large_adj_list_parquet/0006_CrossProduct1",
    // Symmetric two-hop join behind a COUNT(*); same missing count-only path.
    "read_list/small_large_property_list_reading/0002_ReadingLargeListSymmetricTwoHop",
    // Variable-length expansion over a very large adjacency list.
    "read_list/var_length_large_adj_list_extend/0004_KnowsVeryLargeAdjListLongPathTest",
];

/// Extra quarantine entries from `CYPHER_SKIP_CASES` (newline- or
/// comma-separated path substrings).
///
/// A memory breach can only be handled by killing the process, which loses
/// the rest of the suite. So the watchdog appends the offender to
/// `CYPHER_BLOWUP_LOG` on its way out and the sweep driver feeds that file
/// back in here on the next attempt, converging on a complete run without
/// anyone hand-enumerating cases. Confirmed offenders graduate into
/// [`MEMORY_QUARANTINE`].
fn env_quarantine() -> Vec<String> {
    std::env::var("CYPHER_SKIP_CASES")
        .unwrap_or_default()
        .split(['\n', ','])
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect()
}

fn is_quarantined(path: &Path, extra: &[String]) -> Option<String> {
    let text = path.to_string_lossy().replace('\\', "/");
    if let Some(hit) = MEMORY_QUARANTINE
        .iter()
        .copied()
        .find(|needle| text.contains(needle))
    {
        return Some(hit.to_string());
    }
    extra
        .iter()
        .find(|needle| text.contains(needle.as_str()))
        .cloned()
}

/// Case currently being executed, for the memory watchdog to name when it
/// trips. Poisoning is irrelevant here — we only ever store a path.
static CURRENT_CASE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Append `case` to `CYPHER_BLOWUP_LOG` so the sweep driver can skip it on
/// the next attempt. Best-effort: a missing or unwritable log is not fatal.
fn record_blowup(case: &str) {
    let Ok(log) = std::env::var("CYPHER_BLOWUP_LOG") else {
        return;
    };
    use std::io::Write;
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(&log) {
        let _ = writeln!(file, "{case}");
    }
}

/// Abort the process if its resident set exceeds `CYPHER_MEM_LIMIT_GB`.
///
/// A per-case *timeout* bounds time, not memory, and that is not enough: a
/// single pathological case (`read_list`, `lsqb`) can allocate tens of
/// gigabytes inside a 20-second window, driving the host into swap and
/// tripping the kernel watchdog long before the timeout would fire. The
/// interpreter materializes intermediate rows eagerly with no spill, so
/// there is no internal backpressure to rely on — this watchdog is the
/// backstop. Set the limit to 0 to disable.
fn spawn_memory_watchdog() {
    let limit_gb: f64 = std::env::var("CYPHER_MEM_LIMIT_GB")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8.0);
    if limit_gb <= 0.0 {
        return;
    }
    let pid = std::process::id();
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_millis(500));
            // `ps` keeps this dependency-free; rss is reported in KiB.
            let Ok(out) = std::process::Command::new("ps")
                .args(["-o", "rss=", "-p", &pid.to_string()])
                .output()
            else {
                return;
            };
            let Ok(text) = String::from_utf8(out.stdout) else {
                return;
            };
            let Ok(rss_kib) = text.trim().parse::<f64>() else {
                return; // process gone, or a platform without this `ps` form
            };
            let rss_gb = rss_kib / 1_048_576.0;
            if rss_gb > limit_gb {
                let case = CURRENT_CASE.lock().map(|c| c.clone()).unwrap_or_default();
                record_blowup(&case);
                eprintln!(
                    "\n[cypher_ladybug_cases] MEMORY LIMIT: resident set reached \
                     {rss_gb:.1} GB (limit {limit_gb:.1} GB) while running `{case}`.\n\
                     Aborting before the host is driven into swap. Quarantine or fix \
                     that case, or raise CYPHER_MEM_LIMIT_GB if this is expected."
                );
                std::process::exit(102);
            }
        }
    });
}

/// Run one case, giving up after `timeout_ms`.
///
/// A timed-out case is **fatal to the process**, deliberately. Rust has no
/// thread cancellation, so the worker we walked away from keeps running and
/// keeps allocating for the rest of the run — and the cases that time out
/// are exactly the ones allocating hardest. Continuing means racing a thread
/// we cannot stop: in practice the OS reached the process first and SIGKILLed
/// it, losing the whole suite's results anyway (and, before the memory
/// watchdog existed, wedging the host badly enough to trip the kernel
/// watchdog).
///
/// So we record the offender to `CYPHER_BLOWUP_LOG` and exit. The sweep
/// driver re-runs the suite with that case in `CYPHER_SKIP_CASES`, which
/// converges on a complete run in as many attempts as there are bad cases —
/// and every skip is visible rather than silently absorbed.
fn run_with_timeout(path: &Path, timeout_ms: u64) -> CaseRun {
    if timeout_ms == 0 {
        return run_one(path);
    }
    let path_buf = path.to_path_buf();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let run = run_one(&path_buf);
        let _ = tx.send(run);
    });
    match rx.recv_timeout(Duration::from_millis(timeout_ms)) {
        Ok(run) => run,
        Err(_) => {
            let case = path.display().to_string();
            record_blowup(&case);
            eprintln!(
                "\n[cypher_ladybug_cases] TIMEOUT after {timeout_ms}ms: `{case}`.\n\
                 Its worker thread cannot be cancelled and is still allocating, so \
                 continuing is unsafe — exiting. Re-run with this case in \
                 CYPHER_SKIP_CASES (the sweep driver does this automatically)."
            );
            std::process::exit(101);
        }
    }
}

fn run_one(path: &Path) -> CaseRun {
    let raw = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) => return CaseRun::from_outcome(Outcome::Skipped(format!("read: {err}"))),
    };

    let case = match case_file::parse(&raw) {
        Ok(case) => case,
        Err(err) => return CaseRun::from_outcome(Outcome::Skipped(format!("case parse: {err}"))),
    };

    if case.metadata.language != "cypher" {
        return CaseRun {
            query: Some(case.query.clone()),
            plan_tree: None,
            outcome: Outcome::Skipped(format!("non-cypher language: {}", case.metadata.language)),
            broken_import: false,
        };
    }

    let graph_initializer = case
        .graph_initializer
        .as_deref()
        .or_else(|| default_graph_initializer(&case.metadata));
    let setup_statements = extract_setup_statements(&raw);
    let broken_import = setup_statements.is_empty() && is_broken_import(&case, graph_initializer);

    let query = case.query.clone();
    let parsed = match parse_query(&query) {
        Ok(q) => q,
        Err(err) => {
            let message = format!("{err}");
            let outcome = if expected_error_matches(
                &case.expected,
                &case.metadata.expected_kind,
                &message,
                ErrorStage::Parse,
            ) {
                Outcome::Correct
            } else {
                Outcome::ParseError(message)
            };
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                outcome,
                broken_import,
            };
        }
    };

    let graph = match dataset::build_with_initializer(&case.metadata.dataset, graph_initializer) {
        Ok(graph) => graph,
        Err(err) => {
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                outcome: Outcome::Skipped(format!(
                    "unsupported dataset `{}`: {err}",
                    case.metadata.dataset
                )),
                broken_import,
            };
        }
    };
    apply_default_graph_overlay(&case.metadata, &graph);

    // Execute any `--- setup_statements` (raw Cypher writes re-extracted
    // from the upstream Kuzu test source) against the graph before the
    // case query runs. A failing setup statement means the fixture can't
    // be reproduced — the case is skipped, never marked incorrect.
    for (idx, stmt) in setup_statements.iter().enumerate() {
        let setup_err = |stage: &str, err: String| {
            Outcome::Skipped(format!(
                "setup statement {} failed at {stage}: {err} (stmt: {stmt})",
                idx + 1
            ))
        };
        let parsed_setup = match parse_query(stmt) {
            Ok(q) => q,
            Err(err) => {
                return CaseRun {
                    query: Some(query),
                    plan_tree: None,
                    outcome: setup_err("parse", format!("{err}")),
                    broken_import,
                };
            }
        };
        let setup_plan = match CypherPlanner::new().plan(&parsed_setup) {
            Ok(plan) => plan,
            Err(err) => {
                return CaseRun {
                    query: Some(query),
                    plan_tree: None,
                    outcome: setup_err("plan", format!("{err}")),
                    broken_import,
                };
            }
        };
        if let Err(err) = execute(&setup_plan, &graph) {
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                outcome: setup_err("run", format!("{err}")),
                broken_import,
            };
        }
    }

    let plan = match CypherPlanner::new().plan(&parsed) {
        Ok(plan) => plan,
        Err(err) => {
            let message = format!("{err}");
            let outcome = if expected_error_matches(
                &case.expected,
                &case.metadata.expected_kind,
                &message,
                ErrorStage::Plan,
            ) {
                Outcome::Correct
            } else {
                Outcome::PlanError(message)
            };
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                outcome,
                broken_import,
            };
        }
    };

    // Render the plan once so RunError / Incorrect dumps can reproduce
    // the same view the interpreter saw, without re-planning.
    let plan_tree = explain(&plan);

    let returned: ReturnedBatches = match execute(&plan, &graph) {
        Ok(returned) => returned,
        Err(err) => {
            let message = format!("{err}");
            let outcome = if expected_error_matches(
                &case.expected,
                &case.metadata.expected_kind,
                &message,
                ErrorStage::Run,
            ) {
                Outcome::Correct
            } else {
                Outcome::RunError(message)
            };
            return CaseRun {
                query: Some(query),
                plan_tree: Some(plan_tree),
                outcome,
                broken_import,
            };
        }
    };

    let actual_lines = format::lines_from_batch(&returned);

    // Cypher's contract: row order is undefined without an explicit
    // `ORDER BY`. The Ladybug metadata still tags every case
    // `ordered=true` because that's how Kuzu records the snapshot it
    // got, but a sortless query that comes back as a row-set should
    // really be compared as a multiset — the snapshot order is an
    // implementation detail of Kuzu, not part of the spec. Detect
    // sortless queries and downgrade to unordered comparison so the
    // accuracy number reflects semantic correctness instead of
    // execution-order coincidences.
    let ordered = if case.metadata.ordered && !query_has_order_by(&query) {
        false
    } else {
        case.metadata.ordered
    };
    let outcome = match compare::matches(
        &actual_lines,
        &case.expected,
        ordered,
        &case.metadata.expected_kind,
    ) {
        compare::Verdict::Match => Outcome::Correct,
        compare::Verdict::Mismatch { reason } => Outcome::Incorrect {
            reason,
            actual: actual_lines,
            expected: case.expected,
        },
    };
    CaseRun {
        query: Some(query),
        plan_tree: Some(plan_tree),
        outcome,
        broken_import,
    }
}

/// Extract the optional `--- setup_statements` section from a raw case
/// file. The shared `case_file` parser (reused verbatim from the gremlin
/// runner) ignores unknown sections, so the cypher runner re-scans the
/// raw text itself. Contract with `tests/import_kuzu_setups.py`:
/// one complete Cypher statement per non-empty line (multi-line
/// statements are joined at import time); `#`-prefixed lines are
/// comments; a trailing `;` is stripped.
fn extract_setup_statements(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_section = false;
    for line in raw.lines() {
        if let Some(header) = line.strip_prefix("--- ") {
            in_section = header.trim() == "setup_statements";
            continue;
        }
        if !in_section {
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let stmt = trimmed.trim_end_matches(';').trim();
        if !stmt.is_empty() {
            out.push(stmt.to_string());
        }
    }
    out
}

/// Machine-detection for structurally unpassable imports: the importer
/// dropped every write/setup statement, so a case whose dataset resolves
/// to the empty fixture but whose expectation contains non-error rows
/// can never pass, regardless of engine quality.
fn is_broken_import(case: &case_file::Case, graph_initializer: Option<&str>) -> bool {
    if graph_initializer.is_some() {
        return false;
    }
    if !dataset::is_empty_dataset(&case.metadata.dataset) {
        return false;
    }
    let first_line = case
        .expected
        .iter()
        .map(|line| line.trim())
        .find(|line| !line.is_empty());
    match first_line {
        None => false, // empty expectation can legitimately pass
        Some(line) => !looks_like_expected_error(line),
    }
}

fn default_graph_initializer(metadata: &case_file::Metadata) -> Option<&'static str> {
    if metadata.source == "recursive_join/semantic_empty.test" {
        return match metadata.source_case.as_str() {
            "TwoCycle" => Some(
                r#"node a:node {ID: 0}
node b:node {ID: 1}
edge a -[:rel]-> b
edge b -[:rel]-> a"#,
            ),
            "ThreeCycle" => Some(
                r#"node a:node {ID: 0}
node b:node {ID: 1}
node c:node {ID: 2}
edge a -[:rel]-> b
edge b -[:rel]-> c
edge c -[:rel]-> a"#,
            ),
            _ => None,
        };
    }

    if metadata.source == "function/uuid.test" && metadata.source_case == "UUIDfunctions" {
        return Some(
            r#"node p0:Person {u: "00000000-0000-0000-0000-000000000000"}
node p1:Person {u: "00000000-0000-0000-0000-000000000000"}
node p2:Person {u: "00000000-0000-0000-0000-000000000000"}
node p3:Person {u: "00000000-0000-0000-0000-000000000001"}
node p4:Person {u: "00000000-0000-0000-0000-000000000001"}
node p5:Person {u: "10203040-5060-7080-0102-030405060708"}
node p6:Person {u: "10203040-5060-7080-0102-030405060708"}
node p7:Person {u: "47183823-2574-4bfd-b411-99ed177d3e43"}
node p8:Person {u: "47183823-2574-4bfd-b411-99ed177d3e43"}
node p9:Person {u: "80000000-0000-0000-8fff-ffffffffffff"}
node p10:Person {u: "80000000-0000-0000-ffff-ffffffffffff"}
node p11:Person {u: "8fffffff-ffff-ffff-0000-000000000000"}
node p12:Person {u: "8fffffff-ffff-ffff-8000-000000000000"}
node p13:Person {u: "8fffffff-ffff-ffff-8fff-ffffffffffff"}
node p14:Person {u: "8fffffff-ffff-ffff-ffff-ffffffffffff"}
node p15:Person {u: "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11"}
node p16:Person {u: "a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a12"}
node p17:Person {u: "ffffffff-ffff-ffff-ffff-ffffffffffff"}"#,
        );
    }
    None
}

fn apply_default_graph_overlay(metadata: &case_file::Metadata, graph: &PropertyGraph) {
    if metadata.source == "agg/hash.test" {
        apply_agg_hash_overlay(metadata, graph);
    }

    if metadata.source != "function/cast.test" {
        return;
    }
    if function_cast_case_needs_null_person(metadata) {
        let mut props = BTreeMap::new();
        props.insert("ID".to_string(), Value::Int(11));
        graph.insert_node("person", props);
    }

    if metadata.id == "cypher.ladybug.function.cast.ListImplicitCast.a65611ea03e1" {
        let mut props = BTreeMap::new();
        props.insert("a".to_string(), Value::Int(0));
        props.insert(
            "b".to_string(),
            Value::List(vec![Value::Int(1), Value::Int(2)]),
        );
        props.insert(
            "c".to_string(),
            Value::List(vec![Value::Float(1.0), Value::Float(2.0)]),
        );
        props.insert(
            "d".to_string(),
            Value::List(vec![Value::Int(3), Value::Int(5)]),
        );
        graph.insert_node("T0", props);
    }

    if metadata.source_case == "NestTypeImplicitCast"
        && metadata
            .id
            .starts_with("cypher.ladybug.function.cast.3248.")
    {
        graph.insert_node("Item", item_props("apple", 2.0, vec![3.1, 4.1]));
        graph.insert_node("Item", item_props("banana", 1.0, vec![5.9, 26.5]));
    }
}

fn apply_agg_hash_overlay(metadata: &case_file::Metadata, graph: &PropertyGraph) {
    if agg_hash_case_needs_extra_people(metadata) {
        graph.insert_node(
            "person",
            person_hash_props(vec![10, 5], vec!["Ein"], vec![vec![1, 2], vec![3, 4]]),
        );
        graph.insert_node(
            "person",
            person_hash_props(vec![1], vec!["Alice"], vec![vec![1, 2], vec![3, 4]]),
        );
        graph.insert_node(
            "person",
            person_hash_props(vec![1, 2, 3], vec!["Carmen"], vec![vec![7, 20]]),
        );
    }

    if matches!(
        metadata.id.as_str(),
        "cypher.ladybug.agg.hash.HashOnBool.6c27419dfd42"
            | "cypher.ladybug.agg.hash.HashOnInterval.1a5c441721f7"
    ) {
        graph.insert_node("payment", payment_props(Some(false), "1 day"));
        graph.insert_node("payment", payment_props(Some(true), "1 year 3 days"));
        graph.insert_node("payment", payment_props(Some(true), "1 year 3 days"));
        graph.insert_node("payment", payment_props(None, "2 years 3 days"));
        graph.insert_node("payment", payment_props(None, "2 years 3 days"));
    }

    if metadata.id == "cypher.ladybug.agg.hash.HashOnInterval.da1ee1fc3a58" {
        insert_agg_hash_orgs(graph, false);
    }

    if matches!(
        metadata.id.as_str(),
        "cypher.ladybug.agg.hash.HashOnListOfStruct.52f927a902fe"
            | "cypher.ladybug.agg.hash.HashOnListOfStruct.ad3dc3920eeb"
    ) {
        insert_agg_hash_orgs(graph, true);
    }

    if matches!(
        metadata.id.as_str(),
        "cypher.ladybug.agg.hash.HashOnInterval.7ebe5c4bc251"
            | "cypher.ladybug.agg.hash.HashOnListOfStruct.9515cc4ecbed"
    ) {
        insert_agg_hash_movies(graph);
    }
}

fn agg_hash_case_needs_extra_people(metadata: &case_file::Metadata) -> bool {
    matches!(
        metadata.id.as_str(),
        "cypher.ladybug.agg.hash.HashOnListOfInt64.2436e9954336"
            | "cypher.ladybug.agg.hash.HashOnListOfListOfInt64.c8ae5dfb8083"
            | "cypher.ladybug.agg.hash.HashOnListOfString.a45ee49a92a5"
            | "cypher.ladybug.agg.hash.HashOnListOfInt32.93c129b8e1cb"
            | "cypher.ladybug.agg.hash.HashOnListOfInt16.b1c327f6a616"
            | "cypher.ladybug.agg.hash.HashOnListOfInt8.d40240ba02c4"
            | "cypher.ladybug.agg.hash.HashOnListOfUint64.65c679469ab4"
            | "cypher.ladybug.agg.hash.HashOnListOfUint32.cf9b0b380682"
            | "cypher.ladybug.agg.hash.HashOnListOfUint16.668ba359c872"
            | "cypher.ladybug.agg.hash.HashOnListOfUint8.ab48f2c02232"
            | "cypher.ladybug.agg.hash.HashOnListOfDouble.08b59cb0f36e"
            | "cypher.ladybug.agg.hash.HashOnListOfFloat.954de144cc70"
    )
}

fn person_hash_props(
    worked_hours: Vec<i64>,
    used_names: Vec<&str>,
    course_scores_per_term: Vec<Vec<i64>>,
) -> BTreeMap<String, Value> {
    let mut props = BTreeMap::new();
    props.insert("workedHours".to_string(), list_int(worked_hours));
    props.insert("usedNames".to_string(), list_string(used_names));
    props.insert(
        "courseScoresPerTerm".to_string(),
        Value::List(course_scores_per_term.into_iter().map(list_int).collect()),
    );
    props
}

fn payment_props(paid: Option<bool>, length: &str) -> BTreeMap<String, Value> {
    let mut props = BTreeMap::new();
    props.insert(
        "paid".to_string(),
        paid.map(Value::Bool).unwrap_or(Value::Null),
    );
    props.insert("length".to_string(), Value::String(length.to_string()));
    props
}

fn insert_agg_hash_orgs(graph: &PropertyGraph, include_null_state: bool) {
    graph.insert_node("organisation", org_hash_props(org_state_revenue_138()));
    graph.insert_node("organisation", org_hash_props(org_state_revenue_55()));
    graph.insert_node("organisation", org_hash_props(org_state_revenue_558()));
    if include_null_state {
        graph.insert_node("organisation", BTreeMap::new());
    }
}

fn org_hash_props(state: Value) -> BTreeMap<String, Value> {
    BTreeMap::from([("state".to_string(), state)])
}

fn org_state_revenue_138() -> Value {
    ordered_map(vec![
        ("revenue", Value::Int(138)),
        ("location", list_string(vec!["'toronto'", "'montr,eal'"])),
        ("stock", stock_map(vec![96, 56], 1000)),
    ])
}

fn org_state_revenue_55() -> Value {
    ordered_map(vec![
        ("revenue", Value::Int(55)),
        ("location", list_string(vec!["'toronto'"])),
        ("stock", stock_map(vec![22, 33], 28)),
    ])
}

fn org_state_revenue_558() -> Value {
    ordered_map(vec![
        ("revenue", Value::Int(558)),
        (
            "location",
            list_string(vec!["'very long city name'", "'new york'"]),
        ),
        ("stock", stock_map(vec![22], 99)),
    ])
}

fn stock_map(price: Vec<i64>, volume: i64) -> Value {
    ordered_map(vec![
        ("price", list_int(price)),
        ("volume", Value::Int(volume)),
    ])
}

fn insert_agg_hash_movies(graph: &PropertyGraph) {
    graph.insert_node(
        "movies",
        movie_hash_props(movie_description_rating_7_stars_10()),
    );
    graph.insert_node(
        "movies",
        movie_hash_props(movie_description_rating_1223_stars_100()),
    );
    graph.insert_node(
        "movies",
        movie_hash_props(movie_description_rating_55_stars_2()),
    );
}

fn movie_hash_props(description: Value) -> BTreeMap<String, Value> {
    BTreeMap::from([("description".to_string(), description)])
}

fn movie_description_rating_7_stars_10() -> Value {
    ordered_map(vec![
        ("rating", Value::Float(7.0)),
        ("stars", Value::Int(10)),
        ("views", Value::Int(982)),
        (
            "release",
            Value::DateTime("2018-11-13 13:33:11".to_string()),
        ),
        (
            "release_ns",
            Value::DateTime("2018-11-13 13:33:11.123456".to_string()),
        ),
        (
            "release_ms",
            Value::DateTime("2018-11-13 13:33:11.123".to_string()),
        ),
        (
            "release_sec",
            Value::DateTime("2018-11-13 13:33:11".to_string()),
        ),
        (
            "release_tz",
            Value::DateTime("2018-11-13 13:33:11.123456+00".to_string()),
        ),
        ("film", Value::DateTime("2014-09-12".to_string())),
        ("u8", Value::Int(12)),
        ("u16", Value::Int(120)),
        ("u32", Value::Int(55)),
        ("u64", big_int("1")),
        ("hugedata", big_int("-1844674407370955161511")),
    ])
}

fn movie_description_rating_1223_stars_100() -> Value {
    ordered_map(vec![
        ("rating", Value::Float(1223.0)),
        ("stars", Value::Int(100)),
        ("views", Value::Int(10003)),
        (
            "release",
            Value::DateTime("2011-02-11 16:44:22".to_string()),
        ),
        (
            "release_ns",
            Value::DateTime("2011-02-11 16:44:22.123456".to_string()),
        ),
        (
            "release_ms",
            Value::DateTime("2011-02-11 16:44:22.123".to_string()),
        ),
        (
            "release_sec",
            Value::DateTime("2011-02-11 16:44:22".to_string()),
        ),
        (
            "release_tz",
            Value::DateTime("2011-02-11 16:44:22.123456+00".to_string()),
        ),
        ("film", Value::DateTime("2013-02-22".to_string())),
        ("u8", Value::Int(1)),
        ("u16", Value::Int(15)),
        ("u32", Value::Int(200)),
        ("u64", big_int("4")),
        ("hugedata", big_int("-15")),
    ])
}

fn movie_description_rating_55_stars_2() -> Value {
    ordered_map(vec![
        ("rating", Value::Float(55.0)),
        ("stars", Value::Int(2)),
        ("views", Value::Int(88)),
        (
            "release",
            Value::DateTime("2022-01-22 00:00:00".to_string()),
        ),
        (
            "release_ns",
            Value::DateTime("2025-01-13 13:33:11.123456".to_string()),
        ),
        (
            "release_ms",
            Value::DateTime("2018-11-13 13:33:11.123".to_string()),
        ),
        (
            "release_sec",
            Value::DateTime("2011-01-11 00:00:00".to_string()),
        ),
        (
            "release_tz",
            Value::DateTime("2011-11-11 00:00:00+00".to_string()),
        ),
        ("film", Value::DateTime("2022-01-11".to_string())),
        ("u8", Value::Int(3)),
        ("u16", Value::Int(22)),
        ("u32", Value::Int(22)),
        ("u64", big_int("56")),
        ("hugedata", big_int("999999")),
    ])
}

fn ordered_map(entries: Vec<(&str, Value)>) -> Value {
    let order = entries
        .iter()
        .map(|(key, _)| Value::String((*key).to_string()))
        .collect();
    let mut map = BTreeMap::new();
    map.insert(STRUCT_ORDER_KEY.to_string(), Value::List(order));
    for (key, value) in entries {
        map.insert(key.to_string(), value);
    }
    Value::Map(map)
}

fn list_int(values: Vec<i64>) -> Value {
    Value::List(values.into_iter().map(Value::Int).collect())
}

fn list_string(values: Vec<&str>) -> Value {
    Value::List(
        values
            .into_iter()
            .map(|value| Value::String(value.to_string()))
            .collect(),
    )
}

fn big_int(value: &str) -> Value {
    Value::BigInt(BigInt::parse_bytes(value.as_bytes(), 10).expect("valid bigint fixture"))
}

fn function_cast_case_needs_null_person(metadata: &case_file::Metadata) -> bool {
    matches!(
        metadata.id.as_str(),
        "cypher.ladybug.function.cast.CastDateToTimestamps.113976020cac"
            | "cypher.ladybug.function.cast.CastStringToTimestamp.6c03a136cb7e"
            | "cypher.ladybug.function.cast.CastTimestampUsToOther.c6f04d5d3e4b"
            | "cypher.ladybug.function.cast.CastTimestampTZToOther.fdf796e28f1c"
            | "cypher.ladybug.function.cast.CastTimestampNsToOther.7c3fe676cf20"
            | "cypher.ladybug.function.cast.CastTimestampMsToOther.0db29090ebd4"
            | "cypher.ladybug.function.cast.CastTimestampSecToOther.e58c1f1afe72"
            | "cypher.ladybug.function.cast.CastListOfIntsToList.1553644d7731"
            | "cypher.ladybug.function.cast.CastListOfListOfIntsToListOfLists.b356f673515c"
            | "cypher.ladybug.function.cast.CastArrayToList.f665fbb2e2f0"
            | "cypher.ladybug.function.cast.CastArrayToArray.a5e46820b0bf"
            | "cypher.ladybug.function.cast.CastInternalIDToString.ffde96984779"
            | "cypher.ladybug.function.cast.CastInternalIDToString.b9ce93133b80"
    )
}

fn item_props(item: &str, price: f64, vector: Vec<f64>) -> BTreeMap<String, Value> {
    let mut props = BTreeMap::new();
    props.insert("item".to_string(), Value::String(item.to_string()));
    props.insert("price".to_string(), Value::Float(price));
    props.insert(
        "vector".to_string(),
        Value::List(vector.into_iter().map(Value::Float).collect()),
    );
    props
}

/// Which stage of the pipeline produced the engine error. Used to map
/// our engine's error surface onto Kuzu's exception categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorStage {
    Parse,
    Plan,
    Run,
}

fn expected_error_matches(
    expected: &[String],
    expected_kind: &str,
    actual: &str,
    stage: ErrorStage,
) -> bool {
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
    let verbatim = expected_lines.iter().any(|expected| {
        actual_candidates.iter().any(|actual| {
            actual == expected || actual.contains(expected) || expected.contains(actual)
        })
    });
    if verbatim {
        return true;
    }
    if error_category_matching_enabled() {
        return expected_lines
            .iter()
            .filter_map(|line| expected_error_category(line))
            .any(|category| engine_error_matches_category(category, stage, actual));
    }
    false
}

/// Error-category matching can be switched off (`CYPHER_ERROR_CATEGORY=off`)
/// to reproduce the old verbatim-prose-only behaviour, e.g. for
/// before/after comparisons.
fn error_category_matching_enabled() -> bool {
    !std::env::var("CYPHER_ERROR_CATEGORY")
        .map(|v| v.eq_ignore_ascii_case("off"))
        .unwrap_or(false)
}

/// Kuzu error categories, extracted from the leading `<Word> exception`
/// token of an expected line (or the TCK-style `SyntaxError` /
/// `RuntimeError` prefixes). Matching the *category* is spec-meaningful;
/// matching Kuzu's exact prose is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ErrorCategory {
    /// `Parser exception: …` / `SyntaxError: …` — query is syntactically
    /// invalid.
    Syntax,
    /// `Binder exception: …` / `Catalog exception: …` — query is
    /// syntactically fine but semantically invalid (unknown property,
    /// table, function, type mismatch discovered at bind time).
    Binder,
    /// `Overflow exception: …` — numeric value outside the target type's
    /// range.
    Overflow,
    /// `Conversion exception: …` — a cast/conversion failed.
    Conversion,
    /// `Runtime exception: …` / `RuntimeError: …` — error surfaced
    /// during execution.
    Runtime,
}

fn expected_error_category(line: &str) -> Option<ErrorCategory> {
    let lower = line.trim().to_ascii_lowercase();
    for (prefix, category) in [
        ("parser exception", ErrorCategory::Syntax),
        ("syntaxerror", ErrorCategory::Syntax),
        ("syntax error", ErrorCategory::Syntax),
        ("binder exception", ErrorCategory::Binder),
        ("catalog exception", ErrorCategory::Binder),
        ("overflow exception", ErrorCategory::Overflow),
        ("conversion exception", ErrorCategory::Conversion),
        ("runtime exception", ErrorCategory::Runtime),
        ("runtimeerror", ErrorCategory::Runtime),
        ("interrupt exception", ErrorCategory::Runtime),
    ] {
        if lower.starts_with(prefix) {
            return Some(category);
        }
    }
    None
}

/// Map our engine's error surface (pipeline stage + message) onto a
/// Kuzu exception category and decide whether it is equivalent.
///
/// Engine error types per stage:
///   - Parse: `CypherParseError::{Parse, Unsupported}`
///   - Plan:  `CypherPlanError::{Unsupported, Invalid}`
///   - Run:   `InterpretError::{Catalog, Type, Runtime, Unbound,
///            Unsupported, ExecutionLimit}`
///
/// `Unsupported` / not-implemented messages are *never* accepted: an
/// engine gap on a valid feature must not masquerade as a conformant
/// rejection of an invalid query.
fn engine_error_matches_category(category: ErrorCategory, stage: ErrorStage, actual: &str) -> bool {
    let lower = actual.to_ascii_lowercase();
    if lower.contains("unsupported") || lower.contains("not implemented") || lower.contains("todo:")
    {
        return false;
    }
    // Timeouts are harness artifacts, not conformant rejections.
    if lower.starts_with("timeout after") {
        return false;
    }
    match category {
        ErrorCategory::Syntax => stage == ErrorStage::Parse,
        ErrorCategory::Binder => match stage {
            // Kuzu's binder rejects semantically invalid queries; our
            // engine surfaces the equivalent rejection either at parse
            // time, at plan time, or lazily at run time as a catalog /
            // type / unbound-variable error.
            ErrorStage::Parse | ErrorStage::Plan => true,
            ErrorStage::Run => {
                lower.starts_with("catalog:")
                    || lower.starts_with("type error:")
                    || lower.starts_with("unbound binding")
            }
        },
        ErrorCategory::Overflow => {
            lower.contains("overflow")
                || lower.contains("out of range")
                || lower.contains("not within")
                || lower.contains("too large")
                || lower.contains("cannot be represented")
        }
        ErrorCategory::Conversion => match stage {
            ErrorStage::Parse => false,
            ErrorStage::Plan | ErrorStage::Run => {
                lower.contains("cast")
                    || lower.contains("convert")
                    || lower.contains("conversion")
                    || lower.starts_with("type error:")
                    || lower.contains("invalid input")
            }
        },
        ErrorCategory::Runtime => stage == ErrorStage::Run,
    }
}

fn looks_like_expected_error(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("exception")
        || lower.starts_with("syntaxerror:")
        || lower.starts_with("syntax error")
        || lower.starts_with("error:")
}

fn error_match_candidates(actual: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in actual
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        push_error_candidate(&mut out, line);
        for prefix in ["invalid cypher plan: ", "parse: ", "plan: ", "run: "] {
            if let Some(stripped) = line.strip_prefix(prefix) {
                push_error_candidate(&mut out, stripped.trim());
            }
        }
    }
    if out.is_empty() {
        push_error_candidate(&mut out, actual.trim());
    }
    out
}

fn push_error_candidate(out: &mut Vec<String>, candidate: &str) {
    if !candidate.is_empty() && !out.iter().any(|existing| existing == candidate) {
        out.push(candidate.to_string());
    }
}

// ============================================================
// Outcome + summary
// ============================================================

#[derive(Debug)]
enum Outcome {
    Correct,
    Incorrect {
        reason: String,
        actual: Vec<String>,
        expected: Vec<String>,
    },
    ParseError(String),
    PlanError(String),
    RunError(String),
    Skipped(String),
}

#[derive(Default)]
struct Summary {
    total: usize,
    correct: usize,
    incorrect: usize,
    parse_errors: usize,
    plan_errors: usize,
    run_errors: usize,
    skipped: usize,
    /// Per-error-message counts for the top-N "what's stopping us" report.
    parse_msg_counts: BTreeMap<String, usize>,
    plan_msg_counts: BTreeMap<String, usize>,
    run_msg_counts: BTreeMap<String, usize>,
    incorrect_examples: Vec<(PathBuf, String, Vec<String>, Vec<String>)>,
    /// Per-suite breakdown (e.g., recursive_join/multi_label, agg/hash, …).
    by_suite: BTreeMap<String, SuiteCounts>,
    /// Per-tier breakdown driving the three-number headline summary.
    by_tier: BTreeMap<Tier, SuiteCounts>,
}

#[derive(Default)]
struct SuiteCounts {
    total: usize,
    correct: usize,
    incorrect: usize,
    parse: usize,
    plan: usize,
    run: usize,
    skipped: usize,
}

impl SuiteCounts {
    fn apply(&mut self, outcome: &Outcome) {
        self.total += 1;
        match outcome {
            Outcome::Correct => self.correct += 1,
            Outcome::Incorrect { .. } => self.incorrect += 1,
            Outcome::ParseError(_) => self.parse += 1,
            Outcome::PlanError(_) => self.plan += 1,
            Outcome::RunError(_) => self.run += 1,
            Outcome::Skipped(_) => self.skipped += 1,
        }
    }

    /// Pass rate over runnable cases (skips are harness-level, not
    /// engine-level, so they're excluded from the denominator).
    fn pass_rate(&self) -> f64 {
        pct(self.correct, self.total.saturating_sub(self.skipped))
    }
}

impl Summary {
    fn record(&mut self, path: &Path, tier: Tier, outcome: Outcome) {
        self.total += 1;
        let suite = suite_label(path);
        self.by_tier.entry(tier).or_default().apply(&outcome);
        let bucket = self.by_suite.entry(suite).or_default();
        bucket.apply(&outcome);
        match outcome {
            Outcome::Correct => {
                self.correct += 1;
            }
            Outcome::Incorrect {
                reason,
                actual,
                expected,
            } => {
                self.incorrect += 1;
                if self.incorrect_examples.len() < 20 {
                    self.incorrect_examples
                        .push((path.to_path_buf(), reason, actual, expected));
                }
            }
            Outcome::ParseError(msg) => {
                self.parse_errors += 1;
                *self.parse_msg_counts.entry(short(&msg)).or_insert(0) += 1;
            }
            Outcome::PlanError(msg) => {
                self.plan_errors += 1;
                *self.plan_msg_counts.entry(short(&msg)).or_insert(0) += 1;
            }
            Outcome::RunError(msg) => {
                self.run_errors += 1;
                *self.run_msg_counts.entry(short(&msg)).or_insert(0) += 1;
            }
            Outcome::Skipped(_) => {
                self.skipped += 1;
            }
        }
    }

    /// Inaccurate = anything that wasn't a clean `Correct`. We exclude
    /// `Skipped` from "inaccurate" because skips are harness-level, not
    /// IR-level shortcomings. Spec callers should still see the totals
    /// in the printed breakdown.
    fn inaccurate(&self) -> usize {
        self.incorrect + self.parse_errors + self.plan_errors + self.run_errors
    }

    fn print(&self, elapsed: std::time::Duration) {
        // --- Three-number tier summary ---
        // core pass rate is the headline conformance number; kuzu-ext is
        // informational; broken-import should trend to zero as re-imports
        // land.
        println!("=== Tier summary ===");
        let empty = SuiteCounts::default();
        let core = self.by_tier.get(&Tier::Core).unwrap_or(&empty);
        let ext = self.by_tier.get(&Tier::KuzuExt).unwrap_or(&empty);
        let broken = self.by_tier.get(&Tier::BrokenImport).unwrap_or(&empty);
        println!(
            "core (headline):  {:>5}/{:<5} passing  ({:.1}%)  [skipped={}]",
            core.correct,
            core.total - core.skipped,
            core.pass_rate(),
            core.skipped,
        );
        println!(
            "kuzu-ext (info):  {:>5}/{:<5} passing  ({:.1}%)  [skipped={}]",
            ext.correct,
            ext.total - ext.skipped,
            ext.pass_rate(),
            ext.skipped,
        );
        println!(
            "broken-import:    {:>5} case(s) excluded (empty dataset + non-error expectation)",
            broken.total,
        );
        println!();

        println!(
            "Accurate:   {:>6}  ({:.1}%)",
            self.correct,
            pct(self.correct, self.total)
        );
        let inaccurate = self.inaccurate();
        println!(
            "Inaccurate: {:>6}  ({:.1}%)  [incorrect={} parse={} plan={} run={}]",
            inaccurate,
            pct(inaccurate, self.total),
            self.incorrect,
            self.parse_errors,
            self.plan_errors,
            self.run_errors,
        );
        println!(
            "Skipped:    {:>6}  ({:.1}%)",
            self.skipped,
            pct(self.skipped, self.total)
        );

        println!("\n--- Per-suite breakdown ---");
        let mut rows: Vec<(&String, &SuiteCounts)> = self.by_suite.iter().collect();
        rows.sort_by(|a, b| b.1.total.cmp(&a.1.total));
        println!(
            "{:<40} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
            "suite", "total", "ok", "wrong", "parse", "plan", "run", "skip"
        );
        for (name, counts) in rows {
            println!(
                "{:<40} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6} {:>6}",
                truncate(name, 40),
                counts.total,
                counts.correct,
                counts.incorrect,
                counts.parse,
                counts.plan,
                counts.run,
                counts.skipped,
            );
        }

        if !self.parse_msg_counts.is_empty() {
            println!("\n--- Top parse errors ---");
            print_top(&self.parse_msg_counts, 10);
        }
        if !self.plan_msg_counts.is_empty() {
            println!("\n--- Top plan errors ---");
            print_top(&self.plan_msg_counts, 10);
        }
        if !self.run_msg_counts.is_empty() {
            println!("\n--- Top run errors ---");
            print_top(&self.run_msg_counts, 10);
        }

        println!("\nElapsed: {elapsed:?}");
    }

    fn assert_no_incorrect(&self) {
        if self.incorrect > 0 {
            panic!(
                "{} Cypher case(s) produced incorrect data — see breakdown above",
                self.incorrect
            );
        }
    }
}

// ============================================================
// Helpers
// ============================================================

fn suite_label(path: &Path) -> String {
    // cases/cypher/ladybug/<group>/<feature>/file.case
    let mut comps = path.components().rev();
    comps.next(); // file.case
    let feature = comps
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_string());
    let group = comps
        .next()
        .map(|c| c.as_os_str().to_string_lossy().to_string());
    match (group, feature) {
        (Some(g), Some(f)) => format!("{g}/{f}"),
        (Some(g), None) => g,
        _ => "?".into(),
    }
}

fn short(message: &str) -> String {
    let trimmed = message.lines().next().unwrap_or("").trim();
    let max = 120;
    if trimmed.chars().count() > max {
        let mut taken: String = trimmed.chars().take(max).collect();
        taken.push('…');
        taken
    } else {
        trimmed.to_string()
    }
}

fn pct(n: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        100.0 * (n as f64) / (total as f64)
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn language_profile_keeps_read_language_suites() {
        assert!(is_language_profile_case(Path::new(
            "cases/cypher/ladybug/function/list/0001.case"
        )));
        assert!(is_language_profile_case(Path::new(
            "cases/cypher/ladybug/recursive_join/range_literal/0001.case"
        )));
    }

    #[test]
    fn language_profile_excludes_storage_and_session_suites() {
        assert!(!is_language_profile_case(Path::new(
            "cases/cypher/ladybug/transaction/ddl/0001.case"
        )));
        assert!(!is_language_profile_case(Path::new(
            "cases/cypher/ladybug/dml_node/create/0001.case"
        )));
        assert!(!is_language_profile_case(Path::new(
            "cases/cypher/ladybug/copy/copy_node/0001.case"
        )));
        assert!(!is_language_profile_case(Path::new(
            "cases/cypher/ladybug/function/gds/basic/0001.case"
        )));
        assert!(!is_language_profile_case(Path::new(
            "cases/cypher/ladybug/demo_db/demo_db_create/0001.case"
        )));
    }

    #[test]
    fn language_profile_excludes_tck_until_scenarios_are_imported() {
        assert!(!is_language_profile_case(Path::new(
            "cases/cypher/ladybug/tck/match/match1/0001.case"
        )));
    }
}

fn print_top(counts: &BTreeMap<String, usize>, limit: usize) {
    let mut entries: Vec<(&String, &usize)> = counts.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    for (msg, count) in entries.into_iter().take(limit) {
        println!("  {count:>4}  {msg}");
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

/// Wipe and recreate the failures dump directory. We blow it away on
/// each run so stale entries from previously-fixed cases don't pile up.
fn reset_failures_dir(dir: &Path) -> std::io::Result<()> {
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    fs::create_dir_all(dir)?;
    Ok(())
}

/// One file per failure (or skip). Filename encodes the case path so it's
/// unique and greppable; contents include the category, reason, the
/// original Cypher query, the IR plan tree (when planning got that far),
/// and any expected/actual rows.
fn dump_failure(dir: &Path, path: &Path, run: &CaseRun) -> std::io::Result<()> {
    let category = match &run.outcome {
        Outcome::Correct => return Ok(()),
        Outcome::Incorrect { .. } => "incorrect",
        Outcome::ParseError(_) => "parse_error",
        Outcome::PlanError(_) => "plan_error",
        Outcome::RunError(_) => "run_error",
        Outcome::Skipped(_) => "skipped",
    };

    let stem = sanitize_path_for_filename(path);
    let filename = format!("{category}__{stem}.txt");
    let target = dir.join(filename);

    let mut body = String::new();
    use std::fmt::Write;
    let _ = writeln!(body, "case:     {}", path.display());
    let _ = writeln!(body, "category: {category}");
    let _ = writeln!(body, "suite:    {}", suite_label(path));
    let _ = writeln!(body);

    let _ = writeln!(body, "--- query ---");
    match &run.query {
        Some(q) => {
            let _ = writeln!(body, "{}", q.trim_end());
        }
        None => {
            let _ = writeln!(body, "(unavailable — case file unreadable)");
        }
    }
    let _ = writeln!(body);

    let _ = writeln!(body, "--- plan ---");
    match &run.plan_tree {
        Some(tree) => {
            // explain() already includes a trailing newline.
            body.push_str(tree);
            if !tree.ends_with('\n') {
                body.push('\n');
            }
        }
        None => {
            let _ = writeln!(body, "(no plan — failed before / during planning)");
        }
    }
    let _ = writeln!(body);

    match &run.outcome {
        Outcome::Correct => unreachable!(),
        Outcome::Incorrect {
            reason,
            actual,
            expected,
        } => {
            let _ = writeln!(body, "--- mismatch ---");
            let _ = writeln!(body, "reason: {reason}");
            let _ = writeln!(body);
            let _ = writeln!(body, "expected ({} lines):", expected.len());
            for line in expected {
                let _ = writeln!(body, "  {line}");
            }
            let _ = writeln!(body);
            let _ = writeln!(body, "actual ({} lines):", actual.len());
            for line in actual {
                let _ = writeln!(body, "  {line}");
            }
        }
        Outcome::ParseError(msg) | Outcome::PlanError(msg) | Outcome::RunError(msg) => {
            let _ = writeln!(body, "--- error ---");
            let _ = writeln!(body, "{msg}");
        }
        Outcome::Skipped(msg) => {
            let _ = writeln!(body, "--- skip reason ---");
            let _ = writeln!(body, "{msg}");
        }
    }

    fs::write(target, body)
}

/// Convert a case path into a filesystem-safe identifier. Strips the
/// `cases/cypher/ladybug/` prefix when present and replaces path
/// separators with `__` so files sort nicely by suite.
fn sanitize_path_for_filename(path: &Path) -> String {
    let stripped = path.strip_prefix(CASES_ROOT).unwrap_or(path);
    let mut s = String::new();
    for (i, comp) in stripped.components().enumerate() {
        if i > 0 {
            s.push_str("__");
        }
        s.push_str(&comp.as_os_str().to_string_lossy());
    }
    if s.ends_with(".case") {
        s.truncate(s.len() - ".case".len());
    }
    // Replace anything that's not safe across mac/linux/windows.
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn walk_cases(root: &Path, on_case: &mut dyn FnMut(&Path)) {
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    let mut sorted: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    sorted.sort();
    for entry in sorted {
        if entry.is_dir() {
            walk_cases(&entry, on_case);
        } else if entry.extension().map(|e| e == "case").unwrap_or(false) {
            on_case(&entry);
        }
    }
}
