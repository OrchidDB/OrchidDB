//! Measures Crabgraph parser and planner coverage over `.rq` files.
//!
//! Pass one or more W3C test-suite directories. Syntax-only collections
//! should be measured separately from query-evaluation collections.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use new_graph::ir::explain;
use new_graph::language::sparql::{SparqlPlanner, parse_query_with_base};

fn main() {
    let roots: Vec<PathBuf> = env::args_os().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: cargo run --example sparql_coverage -- <corpus-dir>...");
        std::process::exit(2);
    }

    let mut files = Vec::new();
    for root in &roots {
        collect_queries(root, &mut files).unwrap_or_else(|error| {
            eprintln!("{}: {error}", root.display());
            std::process::exit(2);
        });
    }
    files.sort();
    files.dedup();

    let planner = SparqlPlanner::default();
    let mut parsed = 0usize;
    let mut planned = 0usize;
    let mut planned_without_extensions = 0usize;
    let mut parse_errors = BTreeMap::<String, usize>::new();
    let mut plan_errors = BTreeMap::<String, usize>::new();

    for path in &files {
        let source = fs::read_to_string(path).unwrap_or_else(|error| {
            eprintln!("{}: {error}", path.display());
            std::process::exit(2);
        });
        let base_iri = format!("file://{}", path.display());
        match parse_query_with_base(&source, &base_iri) {
            Ok(query) => {
                parsed += 1;
                match planner.plan(&query) {
                    Ok(plan) => {
                        planned += 1;
                        if !explain(&plan).contains("GraphExtension") {
                            planned_without_extensions += 1;
                        }
                    }
                    Err(error) => *plan_errors.entry(error.to_string()).or_default() += 1,
                }
            }
            Err(error) => *parse_errors.entry(error.to_string()).or_default() += 1,
        }
    }

    println!("SPARQL coverage benchmark");
    println!("query files: {}", files.len());
    println!(
        "parsed: {parsed}/{} ({:.1}%)",
        files.len(),
        percent(parsed, files.len())
    );
    println!(
        "planned: {planned}/{parsed} ({:.1}% of parsed)",
        percent(planned, parsed)
    );
    println!(
        "planned overall: {planned}/{} ({:.1}%)",
        files.len(),
        percent(planned, files.len())
    );
    println!(
        "planned without extension nodes: {planned_without_extensions}/{parsed} ({:.1}% of parsed)",
        percent(planned_without_extensions, parsed)
    );
    print_groups("planner gaps", &plan_errors);
    print_groups("parse errors", &parse_errors);
}

fn collect_queries(path: &Path, output: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if path.is_file() {
        if path.extension().is_some_and(|extension| extension == "rq") {
            output.push(path.to_path_buf());
        }
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        collect_queries(&entry?.path(), output)?;
    }
    Ok(())
}

fn percent(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 * 100.0 / denominator as f64
    }
}

fn print_groups(title: &str, groups: &BTreeMap<String, usize>) {
    println!("{title}:");
    let mut groups: Vec<_> = groups.iter().collect();
    groups.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    for (message, count) in groups.into_iter().take(12) {
        println!("  {count:>4}  {message}");
    }
}
