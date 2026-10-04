//! Use a subprocess so stack-overflow aborts become ordinary test failures.
use orchiddb::language::gremlin::{parse_query_list, parse_traversal};
#[test]
fn audit_deep_gremlin_parsing_survives_small_stack() {
    if std::env::var_os("ORCHID_AUDIT_PARSER_CHILD").is_some() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let depth = 2048;
                let text = format!(
                    "g.inject(1).{}identity(){}",
                    "map(__.".repeat(depth),
                    ")".repeat(depth)
                );
                let traversal = parse_traversal(&text).unwrap();
                drop(traversal);
                let syntax = parse_query_list(&text).unwrap();
                assert!(syntax.parse_tree.starts_with("(queryList"));
                let chain = format!("g.inject(1){}", ".identity()".repeat(depth));
                assert_eq!(parse_traversal(&chain).unwrap().steps.len(), depth + 1);
                let malformed = format!("{text}.has(");
                assert!(parse_traversal(&malformed).is_err());
            })
            .unwrap()
            .join()
            .unwrap();
        return;
    }
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "audit_deep_gremlin_parsing_survives_small_stack",
            "--nocapture",
        ])
        .env("ORCHID_AUDIT_PARSER_CHILD", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
