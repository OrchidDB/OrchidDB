use super::*;
use datafusion::sql::sqlparser::{dialect::PostgreSqlDialect, parser::Parser};

fn expression(sql: &str) -> ast::Expr {
    Parser::new(&PostgreSqlDialect {})
        .try_with_sql(sql)
        .unwrap()
        .parse_expr()
        .unwrap()
}

#[test]
fn unsupported_safe_casts_report_capability_errors() {
    for ty in [
        "DATE",
        "STRUCT(a BIGINT)",
        "DECIMAL(5)",
        "DECIMAL(x,2)",
        "NUMERIC(5,x)",
    ] {
        assert!(safe_cast(ty).is_err(), "{ty}");
    }
    let mut untouched = expression("unknown_function(1)");
    let expected = untouched.clone();
    adapt(&mut untouched, "unknown_function", &[expression("1")]).unwrap();
    assert_eq!(untouched, expected);
}

#[test]
#[ignore = "requires local PostgreSQL fixture and GRAPH_PG_URL"]
fn postgres_scalar_adapters_execute_boundary_values() {
    let cases: Vec<(&str, Vec<&str>, &str)> = vec![
        ("encode", vec!["'é'::text", "'hex'"], "c3a9"),
        ("encode", vec!["decode('00ff','hex')", "'hex'"], "00ff"),
        ("__orchiddb_utf16_length", vec!["'a😀b'"], "4"),
        ("__orchiddb_utf16_length", vec!["''"], "0"),
        ("__orchiddb_utf16_substring", vec!["'a😀b'", "1"], "😀b"),
        ("__orchiddb_utf16_substring", vec!["'a😀b'", "1", "2"], "�"),
        ("__orchiddb_utf16_substring", vec!["'a😀b'", "2", "3"], "�"),
        ("__orchiddb_utf16_substring", vec!["'a😀b'", "-1"], "b"),
        (
            "__orchiddb_utf16_substring",
            vec!["NULL::text", "0"],
            "NULL",
        ),
        (
            "strftime",
            vec![
                "TIMESTAMP '2024-02-03 04:05:06.123456'",
                "'%Y-%m-%dT%H:%M:%S.%fZ'",
            ],
            "2024-02-03T04:05:06.123456Z",
        ),
        (
            "generate_series",
            vec!["3::bigint", "1::bigint", "-1::bigint"],
            "{3,2,1}",
        ),
        (
            "range",
            vec![
                "'-9223372036854775808'::bigint",
                "'-9223372036854775808'::bigint",
                "1",
            ],
            "{}",
        ),
        ("regexp_full_match", vec!["'AbC'", "'abc'", "'i'"], "true"),
        ("regexp_extract", vec!["'abc123'", "'([0-9]+)'", "1"], "123"),
        ("regexp_extract", vec!["'abc'", "'([0-9]+)'", "1"], ""),
        ("hex", vec!["'é'"], "C3A9"),
        ("array_has", vec!["ARRAY[1,NULL,2]", "2"], "true"),
        (
            "array_has",
            vec!["ARRAY[1,NULL,2]", "NULL::integer"],
            "NULL",
        ),
        ("array_min", vec!["ARRAY[3,NULL,1]"], "1"),
        ("array_max", vec!["ARRAY[3,NULL,1]"], "3"),
        ("array_sum", vec!["ARRAY[3,NULL,1]"], "4"),
        (
            "array_distinct",
            vec!["ARRAY[3,NULL,1,3,NULL]"],
            "{3,NULL,1}",
        ),
        ("array_reverse", vec!["ARRAY[3,NULL,1]"], "{1,NULL,3}"),
        ("isfinite", vec!["'Infinity'::double precision"], "false"),
        ("isfinite", vec!["'NaN'::double precision"], "false"),
        ("nanvl", vec!["'NaN'::double precision", "7.0"], "7"),
        ("regexp_like", vec!["'ABC'", "'abc'", "NULL::text"], "NULL"),
        ("quantile_disc", vec!["7", "0.5"], "7"),
        ("quantile_cont", vec!["7", "0.5"], "7"),
        ("list_extract", vec!["ARRAY[5,9]", "2"], "9"),
        ("regexp_full_match", vec!["'abcd'", "'abc'"], "false"),
        (
            "epoch_us",
            vec!["TIMESTAMP '1970-01-01 00:00:00.000001'"],
            "1",
        ),
        ("make_timestamp", vec!["1"], "1970-01-01 00:00:00.000001"),
        ("array_length", vec!["ARRAY[]::integer[]"], "0"),
        ("array_element", vec!["ARRAY[5,9]", "2"], "9"),
        ("array_slice", vec!["ARRAY[5,9,2]", "2", "3"], "{9,2}"),
        ("array_empty", vec!["ARRAY[]::integer[]"], "true"),
        (
            "array_concat",
            vec!["ARRAY[5,NULL]", "ARRAY[9]"],
            "{5,NULL,9}",
        ),
        (
            "array_intersect",
            vec!["ARRAY[3,1,3]", "ARRAY[1,3]"],
            "{3,1}",
        ),
        (
            "array_intersect",
            vec!["ARRAY[1,3]", "ARRAY[3,1,3]"],
            "{3,1}",
        ),
        (
            "array_replace_all",
            vec!["ARRAY[1,NULL,2]", "NULL::integer", "7"],
            "{1,7,2}",
        ),
        ("array_to_string", vec!["ARRAY[1,NULL,2]", "':'"], "1:2"),
        ("contains", vec!["'abc'", "'b'"], "true"),
        ("starts_with", vec!["'abc'", "'b'"], "false"),
        ("ends_with", vec!["'abc'", "'bc'"], "true"),
        ("strpos", vec!["'abc'", "'b'"], "2"),
        ("isnan", vec!["'NaN'::double precision"], "true"),
        ("iszero", vec!["'-0'::double precision"], "true"),
        ("signum", vec!["-3::double precision"], "-1"),
        ("trunc", vec!["12.345", "2"], "12.34"),
        ("round", vec!["12.345", "2"], "12.35"),
        ("regexp_like", vec!["'abc'", "'b'"], "true"),
    ];
    let mut script = String::new();
    let mut expected = Vec::new();
    for (index, (name, args, value)) in cases.into_iter().enumerate() {
        let args = args.into_iter().map(expression).collect::<Vec<_>>();
        let mut result = expression("NULL");
        adapt(&mut result, name, &args).unwrap();
        script.push_str(&format!(
            "SELECT '{index}:' || coalesce(CAST(({result}) AS TEXT),'NULL');\n"
        ));
        expected.push(format!("{index}:{value}"));
    }
    use std::io::Write;
    use std::process::Stdio;
    let mut child = std::process::Command::new("psql")
        .args(["-X", "-qAt", "-v", "ON_ERROR_STOP=1"])
        .arg(std::env::var("GRAPH_PG_URL").expect("GRAPH_PG_URL is required"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("psql must be installed");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(script.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim_end(),
        expected.join("\n")
    );
}
