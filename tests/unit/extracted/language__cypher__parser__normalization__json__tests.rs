
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
