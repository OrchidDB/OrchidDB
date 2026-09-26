#[path = "common/execution.rs"]
mod datafusion_test;
use orchiddb::{
    ir::{catalog::PropertyGraph,  value::Value},
    language::gremlin::{GremlinPlanner, parse_traversal},
};

fn run(query: &str) -> Result<Vec<Value>, String> {
    let traversal = parse_traversal(query).map_err(|e| e.to_string())?;
    let plan = GremlinPlanner::new().plan(&traversal).map_err(|e| e.to_string())?;
    execute_rows(&plan, &PropertyGraph::new())
        .map(|rows| rows.into_iter().map(|r| r.bindings["current"].clone()).collect())
        .map_err(|e| e.to_string())
}

#[test]
fn regex_preserves_sequence_anchors_and_character_classes() {
    let cases = [
        ("g.inject('ab','ba','axb').is(TextP.regex('a.*b'))", vec!["ab", "axb"]),
        ("g.inject('xa1z','a12','none').is(TextP.regex('a[0-9]+'))", vec!["xa1z", "a12"]),
        ("g.inject('a1','xa1','a12').is(TextP.regex('^a[0-9]$'))", vec!["a1"]),
        ("g.inject('ab','ac','bc').is(TextP.notRegex('a[bc]'))", vec!["bc"]),
    ];
    for (query, expected) in cases {
        assert_eq!(run(query).unwrap(), expected.into_iter().map(|v|Value::String(v.into())).collect::<Vec<_>>(), "{query}");
    }
}

#[test]
fn unsupported_native_regex_syntax_is_an_error_in_predicates_and_search() {
    for query in [
        "g.inject('ab').is(TextP.regex('a(?=b)'))",
        r"g.inject('aa').is(TextP.regex('(a)\\1'))",
        "g.call('tinker.search',['regex':'a(?=b)'])",
        r"g.call('tinker.search',['regex':'(a)\\1'])",
    ] {
        let error = run(query).unwrap_err();
        assert!(error.contains("lookaround") || error.contains("backreference"), "{query}: {error}");
    }
}

use crate::datafusion_test::execute_rows;
