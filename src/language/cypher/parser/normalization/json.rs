//! Token-aware JSON literals. The existing grammar parses the internal call;
//! planning validates its constant payload and creates a typed JSON literal.
pub(super) fn normalize_json(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut output = String::with_capacity(input.len());
    let mut copied = 0;
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i..].starts_with(b"//") {
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i + 1 < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if matches!(bytes[i], b'\'' | b'"' | b'`') {
            let quote = bytes[i];
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == quote {
                    i += 1;
                    if bytes.get(i) == Some(&quote) {
                        i += 1;
                    } else {
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            continue;
        }
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if !input[start..i].eq_ignore_ascii_case("json")
                || (start > 0 && bytes[start - 1] == b'.')
            {
                continue;
            }
            let mut next = i;
            while next < bytes.len() && bytes[next].is_ascii_whitespace() {
                next += 1;
            }
            if next > i && bytes.get(next) == Some(&b'\'') {
                let mut end = next + 1;
                let mut text = String::new();
                let mut segment = end;
                let mut closed = false;
                while end < bytes.len() {
                    if bytes[end] == b'\'' {
                        text.push_str(&input[segment..end]);
                        if bytes.get(end + 1) == Some(&b'\'') {
                            text.push('\'');
                            end += 2;
                            segment = end;
                        } else {
                            end += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        end += 1;
                    }
                }
                if closed {
                    output.push_str(&input[copied..start]);
                    output.push_str("json.`literal`(");
                    output.push_str(&serde_json::to_string(&text).expect("string serialization"));
                    output.push(')');
                    copied = end;
                    i = end;
                }
            } else if bytes.get(next) == Some(&b'.') {
                let method_start = next + 1;
                let mut end = method_start;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                let mut paren = end;
                while paren < bytes.len() && bytes[paren].is_ascii_whitespace() {
                    paren += 1;
                }
                if end > method_start && bytes.get(paren) == Some(&b'(') {
                    output.push_str(&input[copied..method_start]);
                    output.push('`');
                    output.push_str(&input[method_start..end]);
                    output.push('`');
                    copied = end;
                    i = end;
                }
            }
        } else {
            i += 1;
        }
    }
    output.push_str(&input[copied..]);
    output
}

#[cfg(test)]
mod tests {
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
}
