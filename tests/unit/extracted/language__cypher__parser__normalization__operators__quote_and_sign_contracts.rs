
    use super::*;
    #[test]
    fn signs_preserve_exponents_binary_operators_and_string_contents() {
        for (source, expected) in [
            ("RETURN - - 2", "RETURN 2"), ("RETURN + - + 2", "RETURN -2"),
            ("RETURN 1e-2", "RETURN 1e-2"), ("RETURN 1e++2", "RETURN 1e++2"),
            ("RETURN 1 - 2", "RETURN 1 - 2"),
            ("RETURN '- - 2'", "RETURN '- - 2'"),
            ("RETURN '\\' - - 2'", "RETURN '\\' - - 2'"),
            ("RETURN [ - - 2, - + 3]", "RETURN [ 2, -3]"),
            ("RETURN value + - - 1", "RETURN value + 1"),
        ] { assert_eq!(normalize_spaced_unary_signs(source),expected,"{source}"); }
        for prefix in ["", "WITH ", "WHERE ", "THEN ", "ELSE ", "(", "["] {
            assert!(unary_sign_context(prefix),"{prefix}");
        }
        for prefix in ["1e", ".5E", "1.5e+ "] { assert!(exponent_sign_context(prefix),"{prefix}"); }
        for prefix in ["value", "foo1e", "1.2.3e", "e", "1.e"] { assert!(!exponent_sign_context(prefix),"{prefix}"); }
    }
    #[test]
    fn operator_search_uses_utf8_byte_offsets_and_ignores_escaped_quotes() {
        assert_eq!(find_operator_outside_quotes("'&' é & 1","&"),Some(7));
        assert_eq!(find_operator_outside_quotes("'a\\'&b'","&"),None);
        assert_eq!(find_projection_keyword("myRETURN x RETURN 1",0),Some((11,18)));
        assert!(!contains_bitwise_operator("'&|<<>>'"));
        assert!(contains_bitwise_operator("1 << 2"));
        assert!(is_clause_boundary(" ORDER BY x"));
        assert!(!is_clause_boundary(" ORDERING x"));
        assert!(!is_clause_boundary("ORDER BY x"));
    }

#[test]
fn projection_scanners_keep_escaped_quote_payloads_and_nested_delimiters_intact() {
    let first=r"'a\', LIMIT 1 & b'";
    let source=format!("{first}, [1,2], {{v:3}} LIMIT 4");
    let boundary=source.find(" LIMIT 4").unwrap();
    assert_eq!(projection_body_len(&source),boundary);
    assert_eq!(split_top_level_commas(&source[..boundary]),vec![first," [1,2]"," {v:3}"]);
    let bits=format!("{first} & 7");
    assert_eq!(find_top_level_bitwise(&bits,&["&"]),Some((first.len()+1,1,"bitwise_and")));
    assert_eq!(top_level_token("[1,2] LAST AS result",|s|keyword_at(s,"AS")),Some(11));
    assert_eq!(top_level_token("[1,2",|s|keyword_at(s,"AS")),None);
}

#[test]
fn incomplete_postfix_and_unary_operators_remain_invalid_for_the_parser() {
    for source in ["!", " )!", "RETURN 1 != 2"] {
        assert_eq!(normalize_postfix_factorial(source),source);
    }
    assert_eq!(normalize_spaced_unary_signs("RETURN - + "),"RETURN -");
    assert_eq!(matching_open_paren(")",0),None);
}
