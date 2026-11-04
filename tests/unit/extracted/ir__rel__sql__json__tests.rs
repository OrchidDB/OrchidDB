
    use super::*;
    use datafusion::sql::sqlparser::parser::Parser;
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    fn expr(sql: &str, dialect: SqlDialect) -> ast::Expr {
        Parser::new(dialect.parser_dialect().as_ref())
            .try_with_sql(sql)
            .unwrap()
            .parse_expr()
            .unwrap()
    }
    fn render(name: &str, args: &[&str], dialect: SqlDialect) -> String {
        let args = args.iter().map(|s| expr(s, dialect)).collect::<Vec<_>>();
        lower(&format!("__orchiddb_json_{name}"), &args, dialect)
            .unwrap()
            .unwrap()
            .to_string()
    }
    fn execute(sql: &str, pg: bool) -> Result<String, String> {
        let mut command = if pg {
            let mut c = Command::new("psql");
            c.arg(std::env::var("GRAPH_PG_URL").unwrap_or_else(|_| "dbname=postgres".into()));
            c.args(["-X", "-A", "-t", "-v", "ON_ERROR_STOP=1"]);
            c
        } else {
            let mut c = Command::new(
                std::env::var("ORCHIDDB_DUCKDB_BIN").unwrap_or_else(|_| "duckdb".into()),
            );
            c.args(["-list", "-noheader"]);
            c
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sql.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().into())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into())
        }
    }
    #[test]
    fn render_json_families() {
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            for (name, args) in [
                ("parse", vec!["text_column"]),
                ("query", vec!["doc", "'$.a[0]'"]),
                ("value", vec!["doc", "'$.a'", "'int64'"]),
                ("type", vec!["doc"]),
                ("keys", vec!["doc"]),
                ("array_length", vec!["doc"]),
                ("object", vec!["'a'", "1"]),
                ("array", vec!["1", "NULL"]),
                ("equals", vec!["doc", "other"]),
                ("contains", vec!["doc", "other"]),
            ] {
                let sql = render(name, &args, dialect);
                assert!(!sql.contains("__arg"), "{sql}");
            }
        }
    }
    #[test]
    #[ignore = "requires PostgreSQL and DuckDB 1.5.2 CLI"]
    fn live_json_scalar_semantics() {
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            let pg = dialect == SqlDialect::Postgres;
            let ty = if pg { "JSONB" } else { "JSON" };
            let cases = [
                ("[1]", "1", true, false),
                ("[[1]]", "[1]", false, false),
                ("{\"a\":[1]}", "{\"a\":1}", false, false),
                ("{\"x\":{\"a\":1}}", "{\"a\":1}", false, false),
                ("{\"a\":1,\"a\":2}", "{\"a\":2}", true, true),
                ("{\"a\":{\"x\":0},\"a\":2}", "{\"a\":2}", true, true),
                ("[1,2]", "[2,1]", true, false),
                ("[1,1]", "[1]", true, false),
                ("1", "1.0", true, true),
                ("9007199254740993", "9007199254740992", false, false),
                ("{\"b\":2,\"a\":1}", "{\"a\":1,\"b\":2}", true, true),
            ];
            for (left, right, contains, equals) in cases {
                for (name, expected) in [("contains", contains), ("equals", equals)] {
                    let expression = render(name, &["left_doc", "right_doc"], dialect);
                    let sql = format!(
                        "SELECT CASE WHEN {expression} THEN 'yes' ELSE 'no' END FROM (SELECT CAST({} AS {ty}) left_doc,CAST({} AS {ty}) right_doc) docs;",
                        string(left),
                        string(right)
                    );
                    assert_eq!(
                        execute(&sql, pg)
                            .unwrap_or_else(|e| panic!("{dialect:?} {name} {left} {right}: {e}")),
                        if expected { "yes" } else { "no" },
                        "{dialect:?} {name} {left} {right}"
                    );
                }
            }
            for (doc, path, expected) in [
                ("{\"a\":1,\"a\":2}", "$.a", "2"),
                ("[7]", "$['0']", "missing"),
                ("{\"0\":8}", "$[0]", "missing"),
                ("[7]", "/0", "7"),
                ("{\"0\":8}", "/0", "8"),
            ] {
                let expression = render("value", &["doc", &string(path)], dialect);
                let sql = format!(
                    "SELECT coalesce({expression},'missing') FROM (SELECT CAST({} AS {ty}) doc) docs;",
                    string(doc)
                );
                assert_eq!(
                    execute(&sql, pg).unwrap(),
                    expected,
                    "{dialect:?} {doc} {path}"
                );
            }
            for doc in ["{\"n\":\"7\"}", "{\"n\":1.5}"] {
                let expression = render("value", &["doc", "'$.n'", "'int64'"], dialect);
                let sql = format!(
                    "SELECT {expression} FROM (SELECT CAST({} AS {ty}) doc) docs;",
                    string(doc)
                );
                assert!(execute(&sql, pg).is_err(), "{dialect:?} must reject {doc}");
            }
        }
    }
    #[test]
    #[ignore = "requires DuckDB 1.5.2 CLI"]
    fn live_duck_strict_parse() {
        for text in ["NaN", "Infinity", "-Infinity", "[1,]", "{\"x\":1,}"] {
            let valid = render("valid", &["txt"], SqlDialect::DuckDb);
            assert_eq!(
                execute(
                    &format!("SELECT {valid} FROM (SELECT {} txt) s;", string(text)),
                    false
                )
                .unwrap(),
                "false"
            );
            let parse = render("parse", &["txt"], SqlDialect::DuckDb);
            assert!(
                execute(
                    &format!("SELECT {parse} FROM (SELECT {} txt) s;", string(text)),
                    false
                )
                .is_err()
            );
        }
        let valid = render("valid", &["txt"], SqlDialect::DuckDb);
        assert_eq!(
            execute(
                &format!("SELECT {valid} FROM (SELECT '\"NaN,]Infinity\"' txt) s;"),
                false
            )
            .unwrap(),
            "true"
        );
    }
    #[test]
    #[ignore = "requires PostgreSQL and DuckDB 1.5.2 CLI"]
    fn live_json_aggregates_and_mutations() {
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            let pg = dialect == SqlDialect::Postgres;
            for (call, expected) in [
                (
                    "__orchiddb_json_array_agg(v ORDER BY i DESC) FILTER (WHERE i>0)",
                    serde_json::json!([2, null, 1]),
                ),
                (
                    "__orchiddb_json_object_agg(k,v ORDER BY i) FILTER (WHERE i>0)",
                    serde_json::json!({"a":2,"b":null}),
                ),
                (
                    "__orchiddb_json_array_agg(DISTINCT v)",
                    serde_json::json!([1, 2, null]),
                ),
            ] {
                let ast::Expr::Function(call) = expr(call, dialect) else {
                    panic!()
                };
                let lowered = lower_function(&call, dialect).unwrap().unwrap();
                assert!(lowered.to_string().contains(if call.filter.is_some() {
                    "FILTER"
                } else {
                    "DISTINCT"
                }));
                let sql = format!(
                    "SELECT {lowered} FROM (VALUES (1,'a',1),(2,'b',CAST(NULL AS INTEGER)),(3,'a',2)) t(i,k,v);"
                );
                let actual: serde_json::Value =
                    serde_json::from_str(&execute(&sql, pg).unwrap()).unwrap();
                if call.to_string().contains("DISTINCT") {
                    assert_eq!(actual.as_array().unwrap().len(), 3);
                    assert!(
                        actual
                            .as_array()
                            .unwrap()
                            .contains(&serde_json::Value::Null)
                    );
                } else {
                    assert_eq!(actual, expected, "{dialect:?}");
                }
                let empty = format!("SELECT {lowered} FROM (SELECT 1 i,'a' k,1 v WHERE false) t;");
                assert_eq!(execute(&empty, pg).unwrap(), if pg { "" } else { "NULL" });
            }
            let ty = if pg { "JSONB" } else { "JSON" };
            for (left, right, expected) in [
                (
                    "{\"a\":{\"b\":1,\"c\":2},\"x\":1}",
                    "{\"a\":{\"b\":null,\"d\":3},\"x\":[]}",
                    serde_json::json!({"a":{"c":2,"d":3},"x":[]}),
                ),
                ("[1]", "{\"a\":{\"b\":2}}", serde_json::json!({"a":{"b":2}})),
                ("{}", "null", serde_json::Value::Null),
            ] {
                let lowered = render("merge_patch", &["a", "b"], dialect);
                let sql = format!(
                    "SELECT {lowered} FROM (SELECT CAST({} AS {ty}) a,CAST({} AS {ty}) b) t;",
                    string(left),
                    string(right)
                );
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&execute(&sql, pg).unwrap()).unwrap(),
                    expected,
                    "{dialect:?}"
                );
            }
            {
                for (op, doc, path, value, expected) in [
                    (
                        "set",
                        "{\"a\":1}",
                        "$.a",
                        "'hello'",
                        serde_json::json!({"a":"hello"}),
                    ),
                    ("set", "[1]", "$['0']", "2", serde_json::json!([1])),
                    (
                        "insert",
                        "{\"a\":1}",
                        "$.a",
                        "2",
                        serde_json::json!({"a":1}),
                    ),
                    (
                        "replace",
                        "{\"a\":1}",
                        "$.b",
                        "2",
                        serde_json::json!({"a":1}),
                    ),
                    ("set", "[1]", "$[9]", "2", serde_json::json!([1, 2])),
                ] {
                    let lowered = render(op, &["doc", &string(path), value], dialect);
                    let sql = format!(
                        "SELECT {lowered} FROM (SELECT CAST({} AS {ty}) doc) t;",
                        string(doc)
                    );
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(&execute(&sql, pg).unwrap())
                            .unwrap(),
                        expected,
                        "{op} {doc} {path}"
                    );
                }
            }
        }
    }
