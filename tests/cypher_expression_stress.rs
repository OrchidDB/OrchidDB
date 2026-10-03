//! Deterministic expression stress cases, evaluated locally without DuckDB builds.
use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

async fn rows(query: &str) -> Vec<Vec<String>> {
    let ast = parse_query(query).unwrap_or_else(|e| panic!("{query}: {e}"));
    let plan = CypherPlanner::new()
        .plan(&ast)
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    let (result, _) = orchiddb::ir::rel::runtime::execute(&plan, &PropertyGraph::new(), None)
        .await
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    (0..result.batch.num_rows())
        .map(|row| {
            result
                .batch
                .columns()
                .iter()
                .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                .collect()
        })
        .collect()
}

#[tokio::test]
async fn arithmetic_collections_and_three_valued_logic() {
    for (expression, expected) in [
        ("CASE WHEN true THEN 1 ELSE 2 END", "1"),
        ("CASE 2 WHEN 1 THEN 3 WHEN 2 THEN 4 END", "4"),
        ("CASE WHEN false THEN 1 END IS NULL", "true"),
        ("-9223372036854775808", "-9223372036854775808"),
        ("0x7fffffffffffffff", "9223372036854775807"),
        ("0o77", "63"),
        ("2 ^ 3 ^ 2", "64.0"),
        ("-2 ^ 2", "4.0"),
        ("1 + 2 * 3", "7"),
        ("7 / 2", "3"),
        ("-7 % 3", "-1"),
        ("[1,2,3][-1]", "3"),
        ("[1,2,3][null] IS NULL", "true"),
        ("[1,2,3][1..null] IS NULL", "true"),
        ("[x IN [1,2,3] WHERE x > 1 | x * 2]", "[4,6]"),
        ("any(x IN [null,1] WHERE x = 2)", ""),
        ("all(x IN [] WHERE false)", "true"),
        ("single(x IN [1,null] WHERE x=1)", ""),
        ("null IN []", "false"),
        ("1 IN [null,2]", ""),
        ("coalesce(null, null, 3)", "3"),
        ("head([]) IS NULL", "true"),
        ("last([]) IS NULL", "true"),
        ("size('aé😀')", "3"),
        ("substring('aé😀',1,2)", "é😀"),
        ("reverse('aé😀')", "😀éa"),
        ("{a: 1, b: [2,3]}.b[1]", "3"),
        ("{a: 1}['a']", "1"),
        ("[1, null] = [1, null]", ""),
        ("null AND false", "false"),
        ("null OR true", "true"),
        ("null XOR true", ""),
        ("NOT null", ""),
    ] {
        let query = format!("RETURN {expression} AS value");
        assert_eq!(rows(&query).await, vec![vec![expected]], "{query}");
    }
}

#[tokio::test]
async fn generated_parameter_arithmetic_and_nested_collections() {
    use orchiddb::ir::value::Value;
    use orchiddb::language::cypher::parameters::bind_parameters;
    use std::collections::BTreeMap;

    let query = "WITH $a AS a, $b AS b RETURN (a + b) * (a - b) AS arithmetic, \
                 CASE a WHEN b THEN 1 ELSE 0 END AS equal, \
                 [x IN [a,b] | x * x][1] AS square";
    for a in -4..=4 {
        for b in -4..=4 {
            let mut ast = parse_query(query).unwrap();
            bind_parameters(
                &mut ast,
                &BTreeMap::from([
                    ("a".to_string(), Value::Int(a)),
                    ("b".to_string(), Value::Int(b)),
                ]),
            )
            .unwrap();
            let plan = CypherPlanner::new().plan(&ast).unwrap();
            let (result, _) =
                orchiddb::ir::rel::runtime::execute(&plan, &PropertyGraph::new(), None)
                    .await
                    .unwrap_or_else(|error| panic!("a={a}, b={b}: {error}"));
            assert_eq!(result.batch.num_rows(), 1);
            let actual = result
                .batch
                .columns()
                .iter()
                .map(|column| arrow::util::display::array_value_to_string(column, 0).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                actual,
                [
                    ((a + b) * (a - b)).to_string(),
                    i64::from(a == b).to_string(),
                    (b * b).to_string()
                ],
                "a={a}, b={b}"
            );
        }
    }
}

#[tokio::test]
async fn simple_case_uses_cypher_equality_in_scalar_and_aggregate_projections() {
    for (left, right, expected) in [
        ("null", "null", "2"),
        ("1", "null", "2"),
        ("null", "1", "2"),
        ("[1,null]", "[1,null]", "2"),
        ("{a:null}", "{a:null}", "2"),
        ("1", "1.0", "1"),
        ("[1,2]", "[1.0,2.0]", "1"),
        ("{a:1}", "{a:1.0}", "1"),
        ("'1'", "1", "2"),
    ] {
        for prefix in ["", "UNWIND [1,2] AS n "] {
            let then = if prefix.is_empty() { "1" } else { "count(*)" };
            let otherwise = if prefix.is_empty() {
                "2"
            } else {
                "count(*) + 1"
            };
            let query = format!(
                "{prefix}RETURN CASE {left} WHEN {right} THEN {then} ELSE {otherwise} END AS result"
            );
            let expected = if prefix.is_empty() {
                expected
            } else if expected == "1" {
                "2"
            } else {
                "3"
            };
            assert_eq!(rows(&query).await, vec![vec![expected]], "{query}");
        }
    }
}

#[tokio::test]
async fn unicode_escape_width_and_surrogate_pairs_preserve_values() {
    for (literal, expected) in [
        (r"'\u0041'", "A"),
        (r"'\uD83D\uDE00'", "😀"),
        (r"'\U0001F600'", "😀"),
        (r"'\u0001F600'", "\u{1}F600"),
        (r"'\uD800\uDC00'", "𐀀"),
        (r"'\uDBFF\uDFFF'", "\u{10ffff}"),
        (r"'a\uD83D\uDE00z'", "a😀z"),
        (r"'\\uD83D\\uDE00'", r"\uD83D\uDE00"),
    ] {
        let query = format!("RETURN {literal} AS value");
        assert_eq!(rows(&query).await, vec![vec![expected]], "{query}");
    }
    for literal in [
        r"'\uD800'",
        r"'\uDC00'",
        r"'\uD800\u0041'",
        r"'\U00110000'",
        r"'\U0000D800'",
    ] {
        let error = parse_query(&format!("RETURN {literal}")).unwrap_err();
        assert_eq!(
            error.classification(),
            Some(("SyntaxError", "InvalidUnicodeLiteral")),
            "{literal}: {error}"
        );
    }
}

#[tokio::test]
async fn case_regressions_compile_to_both_sql_dialects() {
    let mut exported = Vec::new();
    for dialect in ["duckdb", "postgres"] {
        for (query, expected) in [
            ("RETURN CASE null WHEN null THEN 1 ELSE 2 END AS result", 2),
            (
                "RETURN CASE [1,null] WHEN [1,null] THEN 1 ELSE 2 END AS result",
                2,
            ),
            (
                "RETURN CASE {a:null} WHEN {a:null} THEN 1 ELSE 2 END AS result",
                2,
            ),
            (
                "MATCH (n:Number) RETURN CASE null WHEN null THEN count(*) ELSE count(*) + 1 END AS result",
                3,
            ),
            (
                "MATCH (n:Number) RETURN CASE WHEN null THEN count(*) ELSE count(*) + 1 END AS result",
                3,
            ),
        ] {
            let request = serde_json::json!({"version": 1, "dialect": dialect, "language": "cypher", "query": query, "tables": [{"name":"expression_numbers","columns":[{"name":"id","data_type":"int64"}]}], "nodes": [{"label":"Number","table":"expression_numbers","id":"id"}], "edges": []});
            let compiled = orchiddb::compiler::compile_json(&request.to_string())
                .await
                .unwrap_or_else(|e| panic!("{dialect}: {query}: {e}"));
            let compiled: serde_json::Value = serde_json::from_str(&compiled).unwrap();
            let sql = compiled["sql"].as_str().unwrap();
            assert!(!sql.is_empty());
            if dialect == "postgres" && query.starts_with("MATCH") {
                assert!(
                    sql.contains("AS BOOLEAN"),
                    "null CASE condition lost its boolean type: {sql}"
                );
                assert!(
                    !sql.contains("AS VARCHAR"),
                    "null CASE condition became a string: {sql}"
                );
            }
            exported.push(serde_json::json!({"dialect":dialect,"query":query,"sql":compiled["sql"],"expected":expected}));
        }
    }
    if let Ok(path) = std::env::var("ORCHIDDB_EXPRESSION_SQL_EXPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&exported).unwrap()).unwrap();
    }
}

async fn typed_rows(query: &str) -> serde_json::Value {
    let ast = parse_query(query).unwrap();
    let plan = CypherPlanner::new().plan(&ast).unwrap();
    let (result, _) = orchiddb::ir::rel::runtime::execute(&plan, &PropertyGraph::new(), None)
        .await
        .unwrap_or_else(|error| panic!("{query}: {error}"));
    let mut rows: serde_json::Value =
        serde_json::from_str(&result.batch.schema().metadata()["orchiddb.cypher.typed_rows.v1"])
            .unwrap();
    fn normalize_integer_width(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(fields) => {
                if fields.get("type").and_then(|v| v.as_str()) == Some("long") {
                    fields.insert("type".into(), "int".into());
                }
                for value in fields.values_mut() {
                    normalize_integer_width(value);
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    normalize_integer_width(value);
                }
            }
            _ => {}
        }
    }
    normalize_integer_width(&mut rows);
    rows
}

#[tokio::test]
async fn case_preserves_branch_types_without_sql_coercion() {
    use serde_json::json;
    let integer = json!({"type":"int", "value":9007199254740993_i64});
    let string = json!({"type":"string", "value":"text"});
    let boolean = json!({"type":"boolean", "value":true});
    let float = json!({"type":"double", "value":"0"});
    let null = json!({"type":"null", "value":null});
    for (left, right, expected_left, expected_right) in [
        (
            "9007199254740993",
            "'text'",
            integer.clone(),
            string.clone(),
        ),
        ("9007199254740993", "true", integer.clone(), boolean),
        ("9007199254740993", "0.0", integer.clone(), float),
        ("9007199254740993", "null", integer.clone(), null),
        (
            "[9007199254740993]",
            "['text']",
            json!({"type":"list","value":[integer.clone()]}),
            json!({"type":"list","value":[string]}),
        ),
    ] {
        for condition in ["WHEN flag", "flag WHEN true"] {
            for tail in ["RETURN value", "WITH value RETURN value"] {
                let query = format!(
                    "UNWIND [true,false] AS flag WITH CASE {condition} THEN {left} ELSE {right} END AS value {tail}"
                );
                assert_eq!(
                    typed_rows(&query).await,
                    json!([[expected_left], [expected_right]]),
                    "{query}"
                );
            }
        }
    }
}

#[tokio::test]
async fn mixed_integer_float_comparisons_do_not_round_integer_variables() {
    for (integer, float, expected) in [
        (
            9007199254740993_i64,
            "9007199254740992.0",
            [false, true, false, false, true, true],
        ),
        (
            -9007199254740993_i64,
            "-9007199254740992.0",
            [false, true, true, true, false, false],
        ),
        (
            i64::MAX,
            "9223372036854775808.0",
            [false, true, true, true, false, false],
        ),
        (
            i64::MIN,
            "-9223372036854775808.0",
            [true, false, false, true, false, true],
        ),
        (1, "1.0", [true, false, false, true, false, true]),
    ] {
        let operators = ["=", "<>", "<", "<=", ">", ">="];
        for (op, expected) in operators.into_iter().zip(expected) {
            // Literal folding and relational variable evaluation must agree.
            for query in [
                format!("RETURN {integer} {op} {float} AS result"),
                format!("UNWIND [{integer}] AS x RETURN x {op} {float} AS result"),
                format!("UNWIND [{float}] AS x RETURN {integer} {op} x AS result"),
            ] {
                assert_eq!(
                    rows(&query).await,
                    vec![vec![expected.to_string()]],
                    "{query}"
                );
            }
        }
    }
    assert_eq!(rows("UNWIND [9007199254740993] AS x RETURN x IN [9007199254740992.0], CASE x WHEN 9007199254740992.0 THEN 1 ELSE 2 END").await,
        vec![vec!["false", "2"]]);
}

#[tokio::test]
async fn scientific_literals_accept_explicit_positive_exponents() {
    for (literal, expected) in [
        ("1e+3", "1000.0"),
        ("1E+0", "1.0"),
        ("1.25e+3", "1250.0"),
        ("-1.25e+3", "-1250.0"),
        (".5e+2", "50.0"),
        ("0e+0", "0.0"),
        ("1e-2", "0.01"),
    ] {
        assert_eq!(
            rows(&format!("RETURN {literal} AS value")).await,
            vec![vec![expected]],
            "{literal}"
        );
    }
    let tokens = orchiddb::language::cypher::parser::tokenize("RETURN 1.25e+3 AS n").unwrap();
    assert!(
        tokens
            .iter()
            .any(|token| token.symbolic_name == Some("ExponentDecimalReal")
                && token.text == "1.25e+3")
    );
    for literal in ["1e+", "1e++2", "1e+-2", "1e+ 3", ".e+3", "1e+2tail"] {
        assert!(
            parse_query(&format!("RETURN {literal}")).is_err(),
            "{literal}"
        );
    }
    assert_eq!(
        rows("WITH '1e+3' AS `1e+3` RETURN `1e+3`").await,
        vec![vec!["1e+3"]]
    );
    assert_eq!(
        rows("WITH 3 AS x1e RETURN x1e + + 2").await,
        vec![vec!["5"]]
    );
    assert_eq!(rows("RETURN 1e+2 + + 3").await, vec![vec!["103.0"]]);
}
