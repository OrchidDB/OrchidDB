//! Command-line interface for the orchiddb graph engine.
//!
//! Executes a single Cypher or Gremlin query against a durable,
//! DuckDB-backed [`GraphEngine`], printing the Arrow result as
//! tab-separated headers and rows. Errors go to stderr with a nonzero
//! exit status; the chosen execution backend is reported on stderr.

use std::collections::BTreeMap;
use std::io::Read;

use orchiddb::engine::{GraphEngine, QueryResult, ReadMode};
use orchiddb::ir;
use orchiddb::language::{cypher, gremlin};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    Cypher,
    Gremlin,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Config {
    database: Option<String>,
    language: Language,
    query: Option<String>,
    file: Option<String>,
    sql_only: bool,
    explain: bool,
    help: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            database: None,
            language: Language::Cypher,
            query: None,
            file: None,
            sql_only: false,
            explain: false,
            help: false,
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = run(&args).await {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

async fn run(args: &[String]) -> Result<(), String> {
    let config = parse_args(args)?;
    if config.help {
        print_help();
        return Ok(());
    }

    let query = resolve_query(&config)?;

    if config.explain {
        print!("{}", explain_query(config.language, &query)?);
        return Ok(());
    }

    let mut engine = match &config.database {
        Some(path) => GraphEngine::open(path)?,
        None => GraphEngine::in_memory()?,
    };
    if config.sql_only {
        engine.set_read_mode(ReadMode::SqlOnly);
    }

    let result = match config.language {
        Language::Cypher => engine.cypher(&query).await?,
        Language::Gremlin => engine.gremlin(&query).await?,
    };

    print_result(&result);
    eprintln!("backend: {:?}", result.backend);
    Ok(())
}

/// Parse a single query into a `GraphPlan` and render the Graph IR without
/// executing it. Mirrors the parse + plan half of `GraphEngine::cypher` and
/// `GraphEngine::gremlin`.
fn explain_query(language: Language, query: &str) -> Result<String, String> {
    match language {
        Language::Cypher => {
            let mut parsed = cypher::parse_query(query).map_err(|error| error.to_string())?;
            cypher::parameters::bind_parameters(&mut parsed, &BTreeMap::new())?;
            let plan = cypher::CypherPlanner::new()
                .plan(&parsed)
                .map_err(|error| error.to_string())?;
            Ok(ir::explain(&plan))
        }
        Language::Gremlin => {
            let parsed = gremlin::parse_traversal(query).map_err(|error| error.to_string())?;
            let plan = gremlin::GremlinPlanner::new()
                .plan(&parsed)
                .map_err(|error| error.to_string())?;
            Ok(ir::explain(&plan))
        }
    }
}

fn print_result(result: &QueryResult) {
    println!("{}", result.returned.fields.join("\t"));
    let batch = &result.returned.batch;
    for row in 0..batch.num_rows() {
        let cells: Vec<String> = batch
            .columns()
            .iter()
            .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
            .collect();
        println!("{}", cells.join("\t"));
    }
}

fn parse_args(args: &[String]) -> Result<Config, String> {
    let mut config = Config::default();
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].as_str();
        match arg {
            "-h" | "--help" => config.help = true,
            "--sql-only" => config.sql_only = true,
            "--explain" => config.explain = true,
            "--database" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--database requires a PATH argument".to_string())?;
                config.database = Some(value.clone());
            }
            "--language" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--language requires a value (cypher or gremlin)".to_string())?;
                config.language = parse_language(value)?;
            }
            "--query" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--query requires a TEXT argument".to_string())?;
                config.query = Some(value.clone());
            }
            "--file" => {
                index += 1;
                let value = args
                    .get(index)
                    .ok_or_else(|| "--file requires a PATH argument".to_string())?;
                config.file = Some(value.clone());
            }
            _ => {
                if let Some((flag, value)) = arg.split_once('=') {
                    match flag {
                        "--database" => config.database = Some(value.to_string()),
                        "--language" => config.language = parse_language(value)?,
                        "--query" => config.query = Some(value.to_string()),
                        "--file" => config.file = Some(value.to_string()),
                        _ => return Err(format!("unknown argument `{arg}`")),
                    }
                } else {
                    return Err(format!("unknown argument `{arg}`"));
                }
            }
        }
        index += 1;
    }
    Ok(config)
}

fn parse_language(value: &str) -> Result<Language, String> {
    match value {
        "cypher" => Ok(Language::Cypher),
        "gremlin" => Ok(Language::Gremlin),
        other => Err(format!(
            "unsupported language `{other}` (expected `cypher` or `gremlin`)"
        )),
    }
}

fn resolve_query(config: &Config) -> Result<String, String> {
    if let Some(query) = &config.query {
        return Ok(query.clone());
    }
    if let Some(path) = &config.file {
        return std::fs::read_to_string(path)
            .map_err(|error| format!("failed to read query file `{path}`: {error}"));
    }
    let mut buffer = String::new();
    std::io::stdin()
        .lock()
        .read_to_string(&mut buffer)
        .map_err(|error| format!("failed to read query from stdin: {error}"))?;
    Ok(buffer)
}

fn print_help() {
    println!(
        "orchiddb — durable graph query CLI

USAGE:
    orchiddb [OPTIONS]

OPTIONS:
    --database PATH     DuckDB storage file (default: in-memory)
    --language LANG     query language: `cypher` (default) or `gremlin`
    --query TEXT        run a single query supplied on the command line
    --file PATH         read the query from a file
    --sql-only          enable strict SQL-only read execution
    --explain           print the Graph IR plan without executing
    -h, --help          print this help and exit

The query is read from --query, --file, or standard input, in that order."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn defaults_to_cypher_in_memory_with_no_query() {
        let config = parse_args(&args(&[])).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn parses_database_path() {
        let config = parse_args(&args(&["--database", "graph.duckdb"])).unwrap();
        assert_eq!(config.database.as_deref(), Some("graph.duckdb"));

        let config = parse_args(&args(&["--database=graph.duckdb"])).unwrap();
        assert_eq!(config.database.as_deref(), Some("graph.duckdb"));
    }

    #[test]
    fn parses_language() {
        let config = parse_args(&args(&["--language", "gremlin"])).unwrap();
        assert_eq!(config.language, Language::Gremlin);

        let config = parse_args(&args(&["--language=cypher"])).unwrap();
        assert_eq!(config.language, Language::Cypher);
    }

    #[test]
    fn rejects_unknown_language() {
        let error = parse_args(&args(&["--language", "sparql"])).unwrap_err();
        assert!(error.contains("sparql"));
    }

    #[test]
    fn parses_query_and_file() {
        let config = parse_args(&args(&["--query", "RETURN 1"])).unwrap();
        assert_eq!(config.query.as_deref(), Some("RETURN 1"));

        let config = parse_args(&args(&["--file", "q.cypher"])).unwrap();
        assert_eq!(config.file.as_deref(), Some("q.cypher"));
    }

    #[test]
    fn query_text_may_contain_equals_and_quotes() {
        let config =
            parse_args(&args(&["--query", "MATCH (a) WHERE a.name = 'x' RETURN a"])).unwrap();
        assert_eq!(
            config.query.as_deref(),
            Some("MATCH (a) WHERE a.name = 'x' RETURN a")
        );
    }

    #[test]
    fn parses_boolean_flags() {
        let config = parse_args(&args(&["--sql-only", "--explain", "--help"])).unwrap();
        assert!(config.sql_only);
        assert!(config.explain);
        assert!(config.help);

        let config = parse_args(&args(&["-h"])).unwrap();
        assert!(config.help);
    }

    #[test]
    fn rejects_unknown_argument() {
        let error = parse_args(&args(&["--frobnicate"])).unwrap_err();
        assert!(error.contains("--frobnicate"));
    }

    #[test]
    fn missing_value_is_an_error() {
        assert!(parse_args(&args(&["--database"])).is_err());
        assert!(parse_args(&args(&["--language"])).is_err());
        assert!(parse_args(&args(&["--query"])).is_err());
        assert!(parse_args(&args(&["--file"])).is_err());
    }
}
