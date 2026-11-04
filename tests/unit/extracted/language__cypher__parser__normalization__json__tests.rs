
use super::*;
#[test]
fn literals_preserve_json_escapes_and_sql_quotes() {
    let result = normalize_json(r#"RETURN JSON '{"s":"a\nb","quote":"it''s"}'"#);
    assert_eq!(
        result,
        r#"RETURN json.`literal`("{\"s\":\"a\\nb\",\"quote\":\"it's\"}")"#
    );
}
#[test]
fn strings_comments_and_identifiers_are_not_literals() {
    let input = "RETURN \"JSON 'null'\", `JSON 'null'` // JSON 'null'\n/* JSON 'null' */";
    assert_eq!(normalize_json(input), input);
    assert_eq!(
        normalize_json("RETURN json.contains(a,b)"),
        "RETURN json.`contains`(a,b)"
    );
}

#[test]
fn incomplete_and_quoted_json_tokens_remain_byte_exact() {
    for input in [
        "RETURN JSON 'unterminated",
        "RETURN json.",
        "RETURN json. (1)",
        "RETURN json.member",
        "RETURN json.member_name  ",
        "RETURN json'null'",
        "RETURN ns.json 'null'",
        "RETURN my_json 'null'",
        "RETURN 'a''JSON null'",
        "RETURN `a``JSON null`",
        "RETURN \"a\"\"JSON null\"",
        "RETURN 'escaped\\' JSON null'",
        "RETURN 'trailing\\",
        "/* JSON 'null'",
        "// JSON 'null'",
        "RETURN 日本語",
    ] {
        assert_eq!(normalize_json(input), input, "{input}");
    }
}

#[test]
fn multiple_json_rewrites_preserve_unicode_payloads_and_method_whitespace() {
    let input = "RETURN JSON '日本語', JsOn .get_2 \n ('x'), JSON 'null'";
    assert_eq!(
        normalize_json(input),
        "RETURN json.`literal`(\"日本語\"), JsOn .`get_2` \n ('x'), json.`literal`(\"null\")"
    );
    assert_eq!(
        normalize_json("RETURN json.123 (x)"),
        "RETURN json.`123` (x)"
    );
}
