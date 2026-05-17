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
//! See output with:
//!
//! ```ignore
//! cargo test --test cypher_ladybug_cases -- --nocapture
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

use new_graph::ir::interpreter::{ReturnedBatches, execute};
use new_graph::ir::plan::explain;
use new_graph::language::cypher::parser::parse_query;
use new_graph::language::cypher::planner::CypherPlanner;

#[path = "gremlin_case_runner/case_file.rs"]
mod case_file;
#[path = "gremlin_case_runner/compare.rs"]
mod compare;

mod cypher_case_runner;
use cypher_case_runner::dataset;
use cypher_case_runner::format;

const CASES_ROOT: &str = "cases/cypher/ladybug";
/// Failure dump dir under `target/`. Cleared at the start of every run so
/// the on-disk picture always reflects the latest pass — easier to grep
/// for "what's still red right now".
const FAILURES_DIR: &str = "target/cypher_case_failures";

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
    let suite_filter = std::env::var("CYPHER_SUITE").ok();
    let timeout_ms: u64 = std::env::var("CYPHER_TIMEOUT_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    walk_cases(&root, &mut |path| {
        if let Some(filter) = &suite_filter {
            if !path.to_string_lossy().contains(filter.as_str()) {
                return;
            }
        }
        let run = run_with_timeout(path, timeout_ms);
        if !matches!(run.outcome, Outcome::Correct) {
            if let Err(err) = dump_failure(&failures_dir, path, &run) {
                eprintln!(
                    "[cypher_ladybug_cases] failed to write failure file for `{}`: {err}",
                    path.display()
                );
            }
        }
        summary.record(path, run.outcome);
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
}

impl CaseRun {
    fn from_outcome(outcome: Outcome) -> Self {
        Self {
            query: None,
            plan_tree: None,
            outcome,
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
        Err(_) => CaseRun {
            query: None,
            plan_tree: None,
            outcome: Outcome::RunError(format!("timeout after {timeout_ms}ms")),
        },
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
        };
    }

    let query = case.query.clone();
    let parsed = match parse_query(&query) {
        Ok(q) => q,
        Err(err) => {
            let message = format!("{err}");
            let outcome =
                if expected_error_matches(&case.expected, &case.metadata.expected_kind, &message) {
                    Outcome::Correct
                } else {
                    Outcome::ParseError(message)
                };
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                outcome,
            };
        }
    };

    let graph = match dataset::build_with_initializer(
        &case.metadata.dataset,
        case.graph_initializer.as_deref(),
    ) {
        Ok(graph) => graph,
        Err(err) => {
            return CaseRun {
                query: Some(query),
                plan_tree: None,
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
            let message = format!("{err}");
            let outcome =
                if expected_error_matches(&case.expected, &case.metadata.expected_kind, &message) {
                    Outcome::Correct
                } else {
                    Outcome::PlanError(message)
                };
            return CaseRun {
                query: Some(query),
                plan_tree: None,
                outcome,
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
            let outcome =
                if expected_error_matches(&case.expected, &case.metadata.expected_kind, &message) {
                    Outcome::Correct
                } else {
                    Outcome::RunError(message)
                };
            return CaseRun {
                query: Some(query),
                plan_tree: Some(plan_tree),
                outcome,
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
    }
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

impl Summary {
    fn record(&mut self, path: &Path, outcome: Outcome) {
        self.total += 1;
        let suite = suite_label(path);
        let bucket = self.by_suite.entry(suite).or_default();
        bucket.total += 1;
        match outcome {
            Outcome::Correct => {
                self.correct += 1;
                bucket.correct += 1;
            }
            Outcome::Incorrect {
                reason,
                actual,
                expected,
            } => {
                self.incorrect += 1;
                bucket.incorrect += 1;
                if self.incorrect_examples.len() < 20 {
                    self.incorrect_examples
                        .push((path.to_path_buf(), reason, actual, expected));
                }
            }
            Outcome::ParseError(msg) => {
                self.parse_errors += 1;
                bucket.parse += 1;
                *self.parse_msg_counts.entry(short(&msg)).or_insert(0) += 1;
            }
            Outcome::PlanError(msg) => {
                self.plan_errors += 1;
                bucket.plan += 1;
                *self.plan_msg_counts.entry(short(&msg)).or_insert(0) += 1;
            }
            Outcome::RunError(msg) => {
                self.run_errors += 1;
                bucket.run += 1;
                *self.run_msg_counts.entry(short(&msg)).or_insert(0) += 1;
            }
            Outcome::Skipped(_) => {
                self.skipped += 1;
                bucket.skipped += 1;
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
