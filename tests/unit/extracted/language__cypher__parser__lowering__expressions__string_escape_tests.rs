
    use super::*;

    #[test]
    fn string_escapes_preserve_controls_unicode_and_unknown_escape_payloads() {
        for (input, expected) in [
            (r#"'\b\B\f\F\n\N\r\R\t\T'"#, "\u{8}\u{8}\u{c}\u{c}\n\n\r\r\t\t"),
            (r#"'\x00\X41\xff'"#, "\0Aÿ"),
            (r#"'\xZ1\x1'"#, "xZ1x1"),
            (r#"'\u0041\U0001F600\uD83D\uDE00'"#, "A😀😀"),
            (r#"'\q\"\'\\'"#, "q\"'\\"),
            ("'雪'", "雪"),
            ("plain", "plain"),
            ("'trailing\\'", "trailing"),
        ] {
            assert_eq!(unquote_string(input).unwrap(), expected, "{input}");
        }
    }

    #[test]
    fn invalid_unicode_is_rejected_without_consuming_the_input() {
        for (input, width) in [
            ("123", 4), ("12z4", 4), ("é12", 4), ("00110000", 8),
            ("0000D800", 8), ("DC00", 4), ("D800", 4),
            (r"D800xDC00", 4), (r"D800\UDC00", 4),
            (r"D800\uDC0", 4), (r"D800\uDC0z", 4), (r"D800\u0041", 4),
        ] {
            let mut chars = input.chars();
            assert_eq!(read_unicode_escape(&mut chars, width), None, "{input}");
            assert_eq!(chars.as_str(), input, "failed decoding consumed {input}");
            let prefix = if width == 4 { 'u' } else { 'U' };
            assert!(matches!(unquote_string(&format!("'\\{prefix}{input}'")), Err(CypherParseError::InvalidUnicodeLiteral)));
        }
        for (input, width, expected) in [
            ("0000rest", 4, '\0'), ("FFFFrest", 4, '\u{ffff}'),
            (r"DBFF\uDFFFrest", 4, '\u{10ffff}'), ("0010FFFFrest", 8, '\u{10ffff}'),
        ] {
            let mut chars = input.chars();
            assert_eq!(read_unicode_escape(&mut chars, width), Some(expected));
            assert_eq!(chars.as_str(), "rest");
        }
    }
