
    use super::*;

    #[test]
    fn malformed_lambdas_remain_available_for_parser_diagnostics() {
        for source in [
            "list_transform", "list_transform [1]", "list_transform([1], x -> x",
            "list_transform([1])", "list_transform([1], x -> x, 3)",
            "list_transform([1], -> 1)", "list_transform([1], (x,y) -> x)",
            "list_filter([1])", "list_filter([1], x)",
            "list_reduce([1])", "list_reduce([1], x -> x)",
            "reduce(x=0)", "reduce(x, y IN [1] | y)",
            "reduce(x=0, y)", "reduce(x=0, y OF [1] | y)",
            "reduce(x=0, y IN [1])", "reduce(=0, y IN [1] | y)",
            "reduce(x=, y IN [1] | y)", "reduce(x.y=0, y IN [1] | y)",
            "reduce(x=0, y.z IN [1] | y)",
        ] { assert_eq!(normalize_lambda_list_functions(source),source,"{source}"); }
    }

    #[test]
    fn nested_lambdas_and_quoted_separators_preserve_scope_and_text() {
        assert_eq!(normalize_lambda_list_functions("list_transform([1], (x) -> list_filter([x], y -> y>0))"),
            "__list_transform([1], 'x', __list_filter([x], 'y', y>0))");
        assert_eq!(normalize_lambda_list_functions("list_reduce([1,2], (a,x) -> a+x)"),
            "__list_reduce([1,2], 'a', 'x', a+x)");
        assert_eq!(normalize_lambda_list_functions("reduce(a=0, x IN [] | a+x)"),
            "__list_reduce([0] + ([]), 'a', 'x', a+x)");
        for source in ["'list_transform([1], x -> x)'", r#""list_filter([1], x -> 'a\\b')""#,
            "mylist_filter([1],x -> x)","list_filter_suffix([1],x -> x)"] {
            assert_eq!(normalize_lambda_list_functions(source),source);
        }
        assert_eq!(split_top_level_args("'a,b', f(1,2), [3,4], {x:1,y:2}, 'é'"),
            ["'a,b'","f(1,2)","[3,4]","{x:1,y:2}","'é'"]);
        assert_eq!(top_level_arrow("['->'], {k:'->'}, ('->')"),None);
        assert_eq!(split_single_lambda("(x) -> 'a,b'"),Some(("x".into()," 'a,b'")));
        assert_eq!(strip_wrapping_parens(" (x)+(y) "),"(x)+(y)");
        assert_eq!(find_matching("('escaped\\\' )', (2))",0,'(',')'),Some(19));
        assert_eq!(match_keyword_at("xlist_filter",1,"list_filter"),None);
        assert_eq!(match_keyword_at("list_filter",0,"list_filter"),Some(11));
    }

    #[test]
    fn colon_slices_do_not_rewrite_maps_patterns_or_quoted_strings() {
        for (source, expected) in [
            ("xs[1:3]","xs[1..3]"), ("xs[:3]","xs[..3]"), ("xs[1:]","xs[1..]"),
            ("xs[ys[1:2]:3]","xs[ys[1..2]..3]"),
            ("[{x:1}]","[{x:1}]"), ("['a:b']","['a:b']"),
            ("`xs[1:2]`","`xs[1:2]`"), ("xs[1:2:3]","xs[1:2:3]"),
            ("[x IN xs | {k:x}]","[x IN xs | {k:x}]"), ("xs[1:","xs[1:"),
        ] { assert_eq!(normalize_colon_slices(source),expected,"{source}"); }
    }
